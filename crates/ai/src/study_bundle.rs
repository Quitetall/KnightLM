//! The `.rbcstudy` bundle: a study as one portable JSON document.
//!
//! The shape is the one the design fixes, so a bundle written by one build can be
//! read by another (and by a person with an editor):
//!
//! ```json
//! {"schema":"rbc-study/1", "id":"…", "title":"…", "description_md":"…",
//!  "tags":[], "lineage":{"from":"…","version":3},
//!  "chapters":[{"id":"…","ordinal":1,"title":"…","primary_board_id":"…",
//!               "boards":[{"id":"…","pgn":"…","fen":"…","arrows":[],"notes_md":"…"}],
//!               "notes_md":"…"}],
//!  "lua":null}
//! ```
//!
//! Three rules make it a format rather than a dump:
//!
//! - **Round-tripping is lossless for what we know.** Export then import produces
//!   the same chapters, the same boards and the same notes.
//! - **What we do not know survives.** A bundle from a newer build carries fields
//!   this one has never heard of; they are kept under `other` and written back out,
//!   so an old build editing a bundle does not silently delete a new field.
//! - **Nothing here is authoritative.** The database is; a bundle is a copy for
//!   moving a study between installations.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde_json::{Map, Value, json};

use rbc_db::Db;

/// What this build writes.
pub const SCHEMA: &str = "rbc-study/1";

/// A parsed bundle.
#[derive(Debug, Clone, PartialEq)]
pub struct Bundle {
    pub schema: String,
    pub id: Option<String>,
    pub title: String,
    pub description_md: String,
    pub tags: Vec<String>,
    pub lineage: Option<Lineage>,
    pub chapters: Vec<BundleChapter>,
    /// Fields this build does not understand, kept so an edit does not drop them.
    pub other: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lineage {
    pub from: String,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BundleChapter {
    pub id: Option<String>,
    pub ordinal: i64,
    pub title: String,
    pub primary_board_id: Option<String>,
    pub boards: Vec<BundleBoard>,
    pub notes_md: String,
    pub other: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BundleBoard {
    pub id: Option<String>,
    pub pgn: String,
    pub fen: Option<String>,
    pub arrows: Vec<Value>,
    pub circles: Vec<Value>,
    pub notes_md: String,
    pub other: BTreeMap<String, Value>,
}

/// Every field a bundle of this build is allowed to consume.
const KNOWN_TOP: &[&str] = &[
    "schema",
    "id",
    "title",
    "description_md",
    "tags",
    "lineage",
    "chapters",
    "lua",
];
const KNOWN_CHAPTER: &[&str] = &[
    "id",
    "ordinal",
    "title",
    "primary_board_id",
    "boards",
    "notes_md",
];
const KNOWN_BOARD: &[&str] = &["id", "pgn", "fen", "arrows", "circles", "notes_md"];

/// Export a study as a bundle.
pub fn export(db: &Db, study_id: &str) -> Result<Value> {
    let study = db
        .study(study_id)?
        .with_context(|| format!("study {study_id} not found"))?;
    let tags: Vec<String> = serde_json::from_str(&study.tags_json).unwrap_or_default();
    let lineage = serde_json::from_str::<Value>(&study.metadata_json)
        .ok()
        .and_then(|metadata| metadata.get("branched_from").cloned())
        .and_then(|from| {
            Some(Lineage {
                from: from.get("study_id")?.as_str()?.to_string(),
                version: from.get("version")?.as_i64()?,
            })
        });

    let mut chapters = Vec::new();
    for chapter in db.chapters(study_id)? {
        let mut boards = Vec::new();
        for board in db.boards("study_chapter", &chapter.id)? {
            let content: Value =
                serde_json::from_str(&board.content_json).unwrap_or_else(|_| json!({}));
            boards.push(json!({
                "id": board.id,
                "pgn": board_pgn(&chapter.title, &board),
                "fen": board.start_fen,
                "arrows": content.get("arrows").cloned().unwrap_or_else(|| json!([])),
                "circles": content.get("circles").cloned().unwrap_or_else(|| json!([])),
                "notes_md": content
                    .get("notes_md")
                    .and_then(|n| n.as_str())
                    .unwrap_or_default(),
            }));
        }
        chapters.push(json!({
            "id": chapter.id,
            "ordinal": chapter.ordinal,
            "title": chapter.title,
            "primary_board_id": chapter.primary_board_id,
            "boards": boards,
            "notes_md": chapter.notes_md,
        }));
    }

    Ok(json!({
        "schema": SCHEMA,
        "id": study.id,
        "title": study.title,
        "description_md": study.description_md,
        "tags": tags,
        "lineage": lineage.map(|lineage| json!({"from": lineage.from, "version": lineage.version})),
        "chapters": chapters,
        "lua": null,
    }))
}

/// A board as one PGN game, with the study and chapter in its headers.
pub fn board_pgn(chapter_title: &str, board: &rbc_db::Board) -> String {
    let content: Value = serde_json::from_str(&board.content_json).unwrap_or_else(|_| json!({}));
    let moves: Vec<String> = content
        .get("moves")
        .and_then(|m| m.as_array())
        .map(|moves| {
            moves
                .iter()
                .filter_map(|m| m.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let mut pgn = format!(
        "[Event \"{}\"]\n[Site \"RustyBrickChess study\"]\n[Round \"{}\"]\n[White \"{}\"]\n[Black \"\"]\n[Result \"*\"]\n",
        escape_header(&board.title),
        escape_header(chapter_title),
        escape_header(&board.title),
    );
    if board.start_fen.trim() != "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" {
        pgn.push_str(&format!(
            "[SetUp \"1\"]\n[FEN \"{}\"]\n",
            escape_header(&board.start_fen)
        ));
    }
    pgn.push('\n');
    let mut line = String::new();
    for (index, mv) in moves.iter().enumerate() {
        if index % 2 == 0 {
            line.push_str(&format!("{}. ", index / 2 + 1));
        }
        line.push_str(mv);
        line.push(' ');
    }
    pgn.push_str(line.trim_end());
    if !line.is_empty() {
        pgn.push(' ');
    }
    pgn.push_str("*\n");
    pgn
}

/// Quotes inside a PGN header are escaped by doubling them.
fn escape_header(text: &str) -> String {
    text.replace('"', "\"\"")
}

/// Parse a bundle, keeping what this build does not understand.
pub fn parse(json: &Value) -> Result<Bundle> {
    let object = json.as_object().context("a bundle must be a JSON object")?;
    let schema = object
        .get("schema")
        .and_then(|s| s.as_str())
        .context("a bundle needs a schema")?;
    if !schema.starts_with("rbc-study/") {
        anyhow::bail!("not a study bundle: schema {schema}");
    }

    let mut chapters = Vec::new();
    for entry in object
        .get("chapters")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default()
    {
        let chapter = entry.as_object().context("a chapter must be an object")?;
        let mut boards = Vec::new();
        for board_entry in chapter
            .get("boards")
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default()
        {
            let board = board_entry
                .as_object()
                .context("a board must be an object")?;
            boards.push(BundleBoard {
                id: board.get("id").and_then(|v| v.as_str()).map(str::to_string),
                pgn: board
                    .get("pgn")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                fen: board
                    .get("fen")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                arrows: board
                    .get("arrows")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default(),
                circles: board
                    .get("circles")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default(),
                notes_md: board
                    .get("notes_md")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                other: leftovers(board, KNOWN_BOARD),
            });
        }
        chapters.push(BundleChapter {
            id: chapter
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            ordinal: chapter.get("ordinal").and_then(|v| v.as_i64()).unwrap_or(0),
            title: chapter
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Chapter")
                .to_string(),
            primary_board_id: chapter
                .get("primary_board_id")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            boards,
            notes_md: chapter
                .get("notes_md")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            other: leftovers(chapter, KNOWN_CHAPTER),
        });
    }

    let lineage = object.get("lineage").and_then(|lineage| {
        Some(Lineage {
            from: lineage.get("from")?.as_str()?.to_string(),
            version: lineage.get("version")?.as_i64()?,
        })
    });

    Ok(Bundle {
        schema: schema.to_string(),
        id: object
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        title: object
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Study")
            .to_string(),
        description_md: object
            .get("description_md")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        tags: object
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|tags| {
                tags.iter()
                    .filter_map(|tag| tag.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        lineage,
        chapters,
        other: leftovers(object, KNOWN_TOP),
    })
}

/// The fields in `object` that are not in `known`.
fn leftovers(object: &Map<String, Value>, known: &[&str]) -> BTreeMap<String, Value> {
    object
        .iter()
        .filter(|(key, _)| !known.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

/// Write a bundle back to JSON, restoring the fields it carried but we do not use.
pub fn to_json(bundle: &Bundle) -> Value {
    let mut object = Map::new();
    object.insert("schema".into(), json!(bundle.schema));
    if let Some(id) = &bundle.id {
        object.insert("id".into(), json!(id));
    }
    object.insert("title".into(), json!(bundle.title));
    object.insert("description_md".into(), json!(bundle.description_md));
    object.insert("tags".into(), json!(bundle.tags));
    object.insert(
        "lineage".into(),
        match &bundle.lineage {
            Some(lineage) => json!({"from": lineage.from, "version": lineage.version}),
            None => Value::Null,
        },
    );
    let chapters: Vec<Value> = bundle
        .chapters
        .iter()
        .map(|chapter| {
            let mut value = json!({
                "ordinal": chapter.ordinal,
                "title": chapter.title,
                "notes_md": chapter.notes_md,
                "boards": chapter.boards.iter().map(|board| {
                    let mut board_value = json!({
                        "pgn": board.pgn,
                        "arrows": board.arrows,
                        "circles": board.circles,
                        "notes_md": board.notes_md,
                    });
                    if let (Some(id), Some(object)) = (&board.id, board_value.as_object_mut()) {
                        object.insert("id".into(), json!(id));
                    }
                    if let (Some(fen), Some(object)) = (&board.fen, board_value.as_object_mut()) {
                        object.insert("fen".into(), json!(fen));
                    }
                    for (key, extra) in &board.other {
                        if let Some(object) = board_value.as_object_mut() {
                            object.insert(key.clone(), extra.clone());
                        }
                    }
                    board_value
                }).collect::<Vec<_>>(),
            });
            if let (Some(id), Some(object)) = (&chapter.id, value.as_object_mut()) {
                object.insert("id".into(), json!(id));
            }
            if let (Some(primary), Some(object)) =
                (&chapter.primary_board_id, value.as_object_mut())
            {
                object.insert("primary_board_id".into(), json!(primary));
            }
            for (key, extra) in &chapter.other {
                if let Some(object) = value.as_object_mut() {
                    object.insert(key.clone(), extra.clone());
                }
            }
            value
        })
        .collect();
    object.insert("chapters".into(), json!(chapters));
    for (key, extra) in &bundle.other {
        object.insert(key.clone(), extra.clone());
    }
    Value::Object(object)
}
