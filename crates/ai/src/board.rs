//! Board and study tools bound to the repository.
//!
//! This is the [`ToolHost`] the server gives the agent loop. Every write is
//! revision-checked and mutability-checked by `rbc-db`, and the chess rules
//! live here: a move is validated against the position the board's own move
//! list produces, never against whatever the model believes.
//!
//! The content shape is the one the repository already stores
//! (`{"moves":["e4"]}` and friends). Fields this build does not model are
//! carried through untouched, so a newer client's data never disappears
//! because an older tool rewrote the object.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use shakmaty::{
    CastlingMode, Chess, EnPassantMode, Move, Position, fen::Fen, san::SanPlus, uci::UciMove,
};

use crate::agent::ToolHost;
use crate::protocol::{ErrorInfo, ToolCall, ToolResult, result_code};
use crate::result::{applied, internal, ok, refused, refused_with};
use rbc_db::{Board, BoardWriteError, Db};

/// One arrow drawn on the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arrow {
    pub from: String,
    pub to: String,
    #[serde(default = "default_color")]
    pub color: String,
}

/// One highlighted square.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Circle {
    pub square: String,
    #[serde(default = "default_color")]
    pub color: String,
}

/// One annotation attached to a ply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlyAnnotation {
    pub ply: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

fn default_color() -> String {
    "green".to_string()
}

/// The stored shape of a board's content.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BoardContent {
    /// Moves from `start_fen`, in SAN order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moves: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<PlyAnnotation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arrows: Vec<Arrow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub circles: Vec<Circle>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes_md: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    /// Anything this build does not model, preserved verbatim.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

impl BoardContent {
    /// Read stored content, tolerating the empty string the server writes for a
    /// fresh board.
    pub fn parse(json: &str) -> Result<Self, ErrorInfo> {
        if json.trim().is_empty() {
            return Ok(Self::default());
        }
        serde_json::from_str(json).map_err(|error| ErrorInfo {
            code: result_code::INTERNAL.to_string(),
            message: format!("the board's stored content could not be read: {error}"),
            retryable: false,
            provider_code: None,
        })
    }

    /// The JSON to store.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// The position after this content's moves.
    pub fn position(&self, start_fen: &str) -> Result<Chess, String> {
        let (mut position, mode) = parse_position(start_fen)?;
        for (index, text) in self.moves.iter().enumerate() {
            let mv = resolve_move(&position, mode, text)
                .ok_or_else(|| format!("move {} ({text}) is not legal on this board", index + 1))?;
            position = position
                .play(mv)
                .map_err(|error| format!("move {} ({text}): {error}", index + 1))?;
        }
        Ok(position)
    }

    /// The content after playing `text`, without storing anything.
    ///
    /// This is what an approval shows: the board as it would be. It validates
    /// the move exactly as the real write does, so a preview cannot promise
    /// something `play_move` would refuse.
    pub fn with_move(&self, start_fen: &str, text: &str) -> Result<Self, String> {
        let position = self.position(start_fen)?;
        let mode = parse_position(start_fen)?.1;
        let mv = resolve_move(&position, mode, text)
            .ok_or_else(|| format!("{text} is not a legal move in this position"))?;
        let san = SanPlus::from_move(position, mv).to_string();
        let mut next = self.clone();
        next.moves.push(san);
        Ok(next)
    }

    /// The current FEN.
    pub fn fen(&self, start_fen: &str) -> Result<String, String> {
        Ok(fen_of(&self.position(start_fen)?))
    }
}

/// The board tools, over one database.
pub struct BoardWriteHost<'a> {
    db: &'a Db,
    /// Who the writes are attributed to in the revision log.
    actor: &'a str,
}

impl<'a> BoardWriteHost<'a> {
    /// Bind the tools to a database and an actor name.
    pub fn new(db: &'a Db, actor: &'a str) -> Self {
        Self { db, actor }
    }
}

impl ToolHost for BoardWriteHost<'_> {
    fn call(&mut self, call: &ToolCall) -> ToolResult {
        match call.name.as_str() {
            "play_move" => self.play_move(call),
            "draw_arrow" => self.draw_arrow(call),
            "draw_circle" => self.draw_circle(call),
            "clear_shapes" => self.clear_shapes(call),
            "annotate" => self.annotate(call),
            "write_note" => self.write_note(call),
            "create_board" => self.create_board(call),
            "copy_board" => self.copy_board(call),
            "lock_board" => self.set_mutability(call, "locked"),
            "unlock_board" => self.set_mutability(call, "mutable"),
            "get_board" => self.get_board(call),
            "list_boards" => self.list_boards(call),
            "list_studies" => self.list_studies(call),
            "get_study" => self.get_study(call),
            "create_study" => self.create_study(call),
            "add_chapter" => self.add_chapter(call),
            "add_board_to_chapter" => self.add_board_to_chapter(call),
            "reorder_chapters" => self.reorder_chapters(call),
            "write_study_text" => self.write_study_text(call),
            "publish_study" => self.publish_study(call),
            "get_chapter" => self.get_chapter(call),
            other => refused(
                call,
                result_code::NOT_FOUND,
                format!("no tool called {other}"),
            ),
        }
    }
}

impl BoardWriteHost<'_> {
    fn play_move(&mut self, call: &ToolCall) -> ToolResult {
        let board = match self.load(call) {
            Ok(board) => board,
            Err(result) => return result,
        };
        let text = string_arg(call, "move").unwrap_or_default();
        let mut content = match BoardContent::parse(&board.content_json) {
            Ok(content) => content,
            Err(error) => return internal(call, error),
        };

        // Validate against the move list, not the model's belief.
        let position = match content.position(&board.start_fen) {
            Ok(position) => position,
            Err(message) => return refused(call, result_code::ILLEGAL, message),
        };
        let mode = match parse_position(&board.start_fen) {
            Ok((_, mode)) => mode,
            Err(message) => return refused(call, result_code::INTERNAL, message),
        };
        let Some(mv) = resolve_move(&position, mode, &text) else {
            return refused(
                call,
                result_code::ILLEGAL,
                format!("{text} is not a legal move in this position"),
            );
        };
        let san = SanPlus::from_move(position.clone(), mv).to_string();
        let played = match position.play(mv) {
            Ok(played) => played,
            Err(error) => {
                return refused(call, result_code::ILLEGAL, format!("{text}: {error}"));
            }
        };

        content.moves.push(san.clone());
        let fen = fen_of(&played);
        let mut result = self.commit(call, &board, &content, format!("played {san}"));
        if result.ok
            && let Some(data) = result.data.as_mut().and_then(Value::as_object_mut)
        {
            data.insert("san".into(), Value::String(san));
            data.insert("fen".into(), Value::String(fen));
            data.insert("plies".into(), Value::from(content.moves.len()));
        }
        result
    }

    fn draw_arrow(&mut self, call: &ToolCall) -> ToolResult {
        let from = string_arg(call, "from").unwrap_or_default();
        let to = string_arg(call, "to").unwrap_or_default();
        if !square_ok(&from) || !square_ok(&to) {
            return refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "from and to must be squares like e2 and e4".to_string(),
            );
        }
        let color = string_arg(call, "color").unwrap_or_else(|| "green".to_string());
        self.edit(call, |content| {
            content
                .arrows
                .retain(|arrow| arrow.from != from || arrow.to != to);
            content.arrows.push(Arrow {
                from: from.clone(),
                to: to.clone(),
                color: color.clone(),
            });
            format!("drew {from}-{to}")
        })
    }

    fn draw_circle(&mut self, call: &ToolCall) -> ToolResult {
        let square = string_arg(call, "square").unwrap_or_default();
        if !square_ok(&square) {
            return refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "square must be a square like e4".to_string(),
            );
        }
        let color = string_arg(call, "color").unwrap_or_else(|| "green".to_string());
        self.edit(call, |content| {
            content.circles.retain(|circle| circle.square != square);
            content.circles.push(Circle {
                square: square.clone(),
                color: color.clone(),
            });
            format!("circled {square}")
        })
    }

    fn clear_shapes(&mut self, call: &ToolCall) -> ToolResult {
        self.edit(call, |content| {
            let cleared = content.arrows.len() + content.circles.len();
            content.arrows.clear();
            content.circles.clear();
            format!("cleared {cleared} shape(s)")
        })
    }

    fn annotate(&mut self, call: &ToolCall) -> ToolResult {
        let Some(ply) = call.arguments.get("ply").and_then(Value::as_u64) else {
            return refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "ply must be a non-negative integer".to_string(),
            );
        };
        let nag = string_arg(call, "nag");
        let comment = string_arg(call, "comment");
        if nag.is_none() && comment.is_none() {
            return refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "give a nag or a comment".to_string(),
            );
        }
        self.edit(call, |content| {
            let ply = ply as u32;
            content
                .annotations
                .retain(|annotation| annotation.ply != ply);
            content.annotations.push(PlyAnnotation {
                ply,
                nag: nag.clone(),
                comment: comment.clone(),
            });
            content.annotations.sort_by_key(|annotation| annotation.ply);
            format!("annotated ply {ply}")
        })
    }

    fn write_note(&mut self, call: &ToolCall) -> ToolResult {
        let markdown = string_arg(call, "markdown").unwrap_or_default();
        self.edit(call, |content| {
            content.notes_md = markdown.clone();
            "updated the notes".to_string()
        })
    }

    fn create_board(&mut self, call: &ToolCall) -> ToolResult {
        let Some(workspace_id) = string_arg(call, "workspace_id") else {
            return refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "workspace_id is required".to_string(),
            );
        };
        let fen = string_arg(call, "fen").unwrap_or_else(default_fen);
        if let Err(message) = parse_position(&fen) {
            return refused(call, result_code::INVALID_ARGUMENTS, message);
        }
        let title = string_arg(call, "title").unwrap_or_else(|| "Board".to_string());
        let kind = string_arg(call, "kind").unwrap_or_else(|| "analysis".to_string());
        match self
            .db
            .create_board("workspace", &workspace_id, &kind, &title, &fen, "")
        {
            Ok(board) => created(call, &board),
            Err(error) => refused(
                call,
                result_code::INTERNAL,
                format!("could not create the board: {error}"),
            ),
        }
    }

    fn copy_board(&mut self, call: &ToolCall) -> ToolResult {
        let Some(board_id) = string_arg(call, "board_id") else {
            return refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "board_id is required".to_string(),
            );
        };
        let workspace_id = string_arg(call, "workspace_id");
        let title = string_arg(call, "title");
        let parent = workspace_id.as_deref().map(|id| ("workspace", id));
        let copied = self.db.copy_board(
            &board_id,
            parent.map(|(kind, _)| kind),
            parent.map(|(_, id)| id),
            title.as_deref(),
        );
        match copied {
            Ok(board) => {
                let mut result = created(call, &board);
                if let Some(data) = result.data.as_mut().and_then(Value::as_object_mut) {
                    data.insert("copied_from".into(), Value::String(board_id));
                }
                result
            }
            Err(error) => refused(
                call,
                result_code::NOT_FOUND,
                format!("could not copy the board: {error}"),
            ),
        }
    }

    fn set_mutability(&mut self, call: &ToolCall, mutability: &str) -> ToolResult {
        let Some(board_id) = string_arg(call, "board_id") else {
            return refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "board_id is required".to_string(),
            );
        };
        match self.db.set_board_mutability(&board_id, mutability) {
            Ok(board) => {
                let mut result = ok(
                    call,
                    format!("board is now {mutability}"),
                    board_data(&board),
                );
                result.revision = Some(board.revision as u64);
                result
            }
            Err(error) => refused(
                call,
                result_code::NOT_FOUND,
                format!("could not change mutability: {error}"),
            ),
        }
    }

    fn get_board(&mut self, call: &ToolCall) -> ToolResult {
        match self.load(call) {
            Ok(board) => {
                let mut result = ok(call, format!("read {}", board.title), board_data(&board));
                result.revision = Some(board.revision as u64);
                result
            }
            Err(result) => result,
        }
    }

    fn list_boards(&mut self, call: &ToolCall) -> ToolResult {
        let boards = match string_arg(call, "workspace_id") {
            Some(workspace_id) => self.db.boards("workspace", &workspace_id),
            None => {
                let mut all = Vec::new();
                let Ok(workspaces) = self.db.workspaces(false) else {
                    return refused(
                        call,
                        result_code::INTERNAL,
                        "could not list workspaces".to_string(),
                    );
                };
                for workspace in workspaces {
                    match self.db.boards("workspace", &workspace.id) {
                        Ok(mut boards) => all.append(&mut boards),
                        Err(error) => {
                            return refused(
                                call,
                                result_code::INTERNAL,
                                format!("could not list boards: {error}"),
                            );
                        }
                    }
                }
                Ok(all)
            }
        };
        match boards {
            Ok(boards) => {
                let rows: Vec<Value> = boards.iter().map(board_data).collect();
                ok(call, format!("{} board(s)", rows.len()), Value::Array(rows))
            }
            Err(error) => refused(
                call,
                result_code::INTERNAL,
                format!("could not list boards: {error}"),
            ),
        }
    }

    fn list_studies(&mut self, call: &ToolCall) -> ToolResult {
        match self.db.studies() {
            Ok(studies) => {
                let rows: Vec<Value> = studies
                    .iter()
                    .map(|study| {
                        serde_json::json!({
                            "id": study.id,
                            "title": study.title,
                            "lifecycle": study.lifecycle,
                        })
                    })
                    .collect();
                ok(
                    call,
                    format!("{} study/studies", rows.len()),
                    Value::Array(rows),
                )
            }
            Err(error) => refused(
                call,
                result_code::INTERNAL,
                format!("could not list studies: {error}"),
            ),
        }
    }

    fn get_study(&mut self, call: &ToolCall) -> ToolResult {
        let Some(study_id) = string_arg(call, "study_id") else {
            return refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "study_id is required".to_string(),
            );
        };
        match self.db.study(&study_id) {
            Ok(Some(study)) => {
                let chapters = self
                    .db
                    .chapters(&study.id)
                    .map(|chapters| {
                        chapters
                            .into_iter()
                            .map(|chapter| {
                                serde_json::json!({
                                    "id": chapter.id,
                                    "title": chapter.title,
                                    "ordinal": chapter.ordinal,
                                    "primary_board_id": chapter.primary_board_id,
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                ok(
                    call,
                    format!("read study {}", study.title),
                    serde_json::json!({
                        "id": study.id,
                        "title": study.title,
                        "description_md": study.description_md,
                        "lifecycle": study.lifecycle,
                        "chapters": chapters,
                    }),
                )
            }
            Ok(None) => refused(
                call,
                result_code::NOT_FOUND,
                format!("no study with id {study_id}"),
            ),
            Err(error) => refused(
                call,
                result_code::INTERNAL,
                format!("could not read the study: {error}"),
            ),
        }
    }

    /// Refuse a write to a study that has been published.
    ///
    /// A published study is a snapshot people cite; changing it underneath them
    /// is the same mistake as editing a locked board. The way forward is a copy,
    /// which `create_study` plus `add_chapter` can build, so the refusal says so.
    // The Err half is a full ToolResult, which is what a refusal is here; boxing
    // it would only move the cost into every call site.
    #[allow(clippy::result_large_err)]
    fn study_is_writable(&self, call: &ToolCall, study_id: &str) -> Result<(), ToolResult> {
        match self.db.study(study_id) {
            // The lifecycle is the signal, not the version counter: adding a
            // chapter bumps the version, and a draft is still a draft.
            Ok(Some(study)) if study.lifecycle == "published" => Err(refused(
                call,
                result_code::CONFLICT,
                format!(
                    "study {study_id} is published at version {}; make a copy to change it",
                    study.current_version
                ),
            )),
            Ok(Some(_)) => Ok(()),
            Ok(None) => Err(refused(
                call,
                result_code::NOT_FOUND,
                format!("no study {study_id}"),
            )),
            Err(error) => Err(refused(
                call,
                result_code::INTERNAL,
                format!("could not read the study: {error}"),
            )),
        }
    }

    fn add_chapter(&mut self, call: &ToolCall) -> ToolResult {
        let Some(study_id) = string_arg(call, "study_id") else {
            return refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "study_id is required".to_string(),
            );
        };
        if let Err(refusal) = self.study_is_writable(call, &study_id) {
            return refusal;
        }
        let Some(title) = string_arg(call, "title") else {
            return refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "title is required for a chapter".to_string(),
            );
        };
        let start_fen = string_arg(call, "start_fen")
            .unwrap_or_else(|| knightlm_core::state::START_FEN.to_string());
        let notes = string_arg(call, "notes").unwrap_or_default();
        match self.db.add_chapter(&study_id, &title, &start_fen, &notes) {
            Ok((chapter, board)) => ok(
                call,
                format!("added chapter {title}"),
                serde_json::json!({
                    "study_id": study_id,
                    "chapter_id": chapter.id,
                    "board_id": board.id,
                    "revision": board.revision,
                }),
            ),
            Err(error) => refused(
                call,
                result_code::INTERNAL,
                format!("could not add the chapter: {error}"),
            ),
        }
    }

    fn add_board_to_chapter(&mut self, call: &ToolCall) -> ToolResult {
        let Some(chapter_id) = string_arg(call, "chapter_id") else {
            return refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "chapter_id is required".to_string(),
            );
        };
        let Some(study_id) = string_arg(call, "study_id") else {
            return refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "study_id is required".to_string(),
            );
        };
        if let Err(refusal) = self.study_is_writable(call, &study_id) {
            return refusal;
        }
        let title = string_arg(call, "title").unwrap_or_else(|| "Board".to_string());
        let fen =
            string_arg(call, "fen").unwrap_or_else(|| knightlm_core::state::START_FEN.to_string());
        match self
            .db
            .add_board_to_chapter(&study_id, &chapter_id, &title, &fen, "{}")
        {
            Ok(board) => ok(
                call,
                format!("added board {title} to the chapter"),
                serde_json::json!({
                    "study_id": study_id,
                    "chapter_id": chapter_id,
                    "board_id": board.id,
                    "revision": board.revision,
                }),
            ),
            Err(error) => refused(
                call,
                result_code::INTERNAL,
                format!("could not add the board: {error}"),
            ),
        }
    }

    fn reorder_chapters(&mut self, call: &ToolCall) -> ToolResult {
        let Some(study_id) = string_arg(call, "study_id") else {
            return refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "study_id is required".to_string(),
            );
        };
        if let Err(refusal) = self.study_is_writable(call, &study_id) {
            return refusal;
        }
        let Some(order) = call.arguments.get("chapter_ids").and_then(|v| v.as_array()) else {
            return refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "chapter_ids is required".to_string(),
            );
        };
        let ids: Vec<String> = order
            .iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect();
        match self.db.reorder_chapters(&study_id, &ids) {
            Ok(()) => ok(
                call,
                format!("reordered {} chapters", ids.len()),
                serde_json::json!({"study_id": study_id, "chapter_ids": ids}),
            ),
            Err(error) => refused(
                call,
                result_code::INTERNAL,
                format!("could not reorder the chapters: {error}"),
            ),
        }
    }

    fn write_study_text(&mut self, call: &ToolCall) -> ToolResult {
        let Some(study_id) = string_arg(call, "study_id") else {
            return refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "study_id is required".to_string(),
            );
        };
        if let Err(refusal) = self.study_is_writable(call, &study_id) {
            return refusal;
        }
        let Some(text) = string_arg(call, "markdown") else {
            return refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "markdown is required".to_string(),
            );
        };
        let chapter_id = string_arg(call, "chapter_id");
        match self
            .db
            .write_study_text(&study_id, chapter_id.as_deref(), &text)
        {
            Ok(()) => ok(
                call,
                "wrote the study text",
                serde_json::json!({"study_id": study_id, "chapter_id": chapter_id}),
            ),
            Err(error) => refused(
                call,
                result_code::INTERNAL,
                format!("could not write the text: {error}"),
            ),
        }
    }

    fn publish_study(&mut self, call: &ToolCall) -> ToolResult {
        let Some(study_id) = string_arg(call, "study_id") else {
            return refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "study_id is required".to_string(),
            );
        };
        if let Err(refusal) = self.study_is_writable(call, &study_id) {
            return refusal;
        }
        // Publishing twice without a new version is a conflict, which the
        // repository reports; asking for the current version makes that explicit.
        let expected = match self
            .db
            .study(&study_id)
            .ok()
            .flatten()
            .map(|study| study.current_version)
        {
            Some(current) => current,
            None => {
                return refused(call, result_code::NOT_FOUND, format!("no study {study_id}"));
            }
        };
        match self.db.publish_study(&study_id, expected) {
            Ok(version) => ok(
                call,
                format!("published version {}", version.version),
                serde_json::json!({
                    "study_id": study_id,
                    "version": version.version,
                    "version_id": version.id,
                }),
            ),
            Err(error) => refused(
                call,
                result_code::CONFLICT,
                format!("could not publish: {error}"),
            ),
        }
    }

    fn get_chapter(&mut self, call: &ToolCall) -> ToolResult {
        let Some(study_id) = string_arg(call, "study_id") else {
            return refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "study_id is required".to_string(),
            );
        };
        let chapter_id = string_arg(call, "chapter_id").unwrap_or_default();
        match self.db.chapters(&study_id) {
            Ok(chapters) => match chapters.into_iter().find(|c| c.id == chapter_id) {
                Some(chapter) => ok(
                    call,
                    format!("chapter {}", chapter.title),
                    serde_json::to_value(&chapter).unwrap_or(serde_json::Value::Null),
                ),
                None => refused(
                    call,
                    result_code::NOT_FOUND,
                    format!("no chapter {chapter_id} in study {study_id}"),
                ),
            },
            Err(error) => refused(
                call,
                result_code::INTERNAL,
                format!("could not read the study: {error}"),
            ),
        }
    }

    fn create_study(&mut self, call: &ToolCall) -> ToolResult {
        let Some(title) = string_arg(call, "title") else {
            return refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "title is required".to_string(),
            );
        };
        let description = string_arg(call, "description").unwrap_or_default();
        match self.db.create_study(&title, &description) {
            Ok(study) => ok(
                call,
                format!("created study {title}"),
                serde_json::json!({"study_id": study.id, "title": study.title}),
            ),
            Err(error) => refused(
                call,
                result_code::INTERNAL,
                format!("could not create the study: {error}"),
            ),
        }
    }

    /// Load the board a call targets.
    ///
    /// The error is the tool result itself — it is handed back to the caller as
    /// the whole answer, so boxing it would move an allocation without making
    /// anything smaller.
    #[allow(clippy::result_large_err)]
    fn load(&self, call: &ToolCall) -> Result<Board, ToolResult> {
        let Some(board_id) = string_arg(call, "board_id") else {
            return Err(refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "board_id is required".to_string(),
            ));
        };
        match self.db.board(&board_id) {
            Ok(Some(board)) => Ok(board),
            Ok(None) => Err(refused(
                call,
                result_code::NOT_FOUND,
                format!("no board with id {board_id}"),
            )),
            Err(error) => Err(refused(
                call,
                result_code::INTERNAL,
                format!("could not read the board: {error}"),
            )),
        }
    }

    /// Read, modify and store a board's content in one step.
    fn edit<F>(&mut self, call: &ToolCall, edit: F) -> ToolResult
    where
        F: FnOnce(&mut BoardContent) -> String,
    {
        let board = match self.load(call) {
            Ok(board) => board,
            Err(result) => return result,
        };
        let mut content = match BoardContent::parse(&board.content_json) {
            Ok(content) => content,
            Err(error) => return internal(call, error),
        };
        let summary = edit(&mut content);
        self.commit(call, &board, &content, summary)
    }

    /// Store new content, reporting a conflict instead of overwriting.
    fn commit(
        &mut self,
        call: &ToolCall,
        board: &Board,
        content: &BoardContent,
        summary: String,
    ) -> ToolResult {
        let expected = call
            .arguments
            .get("expected_revision")
            .and_then(Value::as_i64)
            .unwrap_or(board.revision);
        match self.db.update_board_content(
            &board.id,
            expected,
            &content.to_json(),
            Some(self.actor),
        ) {
            Ok(updated) => {
                let mut data = board_data(&updated);
                if let Ok(fen) = content.fen(&updated.start_fen)
                    && let Some(object) = data.as_object_mut()
                {
                    object.insert("fen".into(), Value::String(fen));
                }
                let mut result = ok(call, summary, data);
                result.revision = Some(updated.revision as u64);
                result
            }
            Err(BoardWriteError::Conflict { current }) => refused_with(
                call,
                result_code::CONFLICT,
                format!("the board moved to revision {current}; re-read it and retry"),
                serde_json::json!({"current_revision": current}),
            ),
            Err(BoardWriteError::CopyRequired) => refused(
                call,
                result_code::COPY_REQUIRED,
                "the board is locked or published; copy it and edit the copy".to_string(),
            ),
            Err(BoardWriteError::NotFound(id)) => refused(
                call,
                result_code::NOT_FOUND,
                format!("no board with id {id}"),
            ),
            Err(BoardWriteError::Invalid(message) | BoardWriteError::Db(message)) => {
                refused(call, result_code::INTERNAL, message)
            }
        }
    }
}

// --- helpers ---------------------------------------------------------------

fn parse_position(fen: &str) -> Result<(Chess, CastlingMode), String> {
    knightlm_core::state::parse_chess(fen).map_err(|error| format!("invalid FEN: {error}"))
}

fn resolve_move(position: &Chess, mode: CastlingMode, text: &str) -> Option<Move> {
    let wanted = text.trim().trim_end_matches(['!', '?', '+', '#']);
    for mv in position.legal_moves() {
        let uci = UciMove::from_move(mv, mode).to_string();
        if uci == wanted {
            return Some(mv);
        }
        let san = SanPlus::from_move(position.clone(), mv).to_string();
        if san == wanted || san.trim_end_matches(['!', '?', '+', '#']) == wanted {
            return Some(mv);
        }
    }
    None
}

fn fen_of(position: &Chess) -> String {
    Fen::from_position(position, EnPassantMode::Legal).to_string()
}

fn default_fen() -> String {
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".to_string()
}

fn square_ok(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() == 2 && (b'a'..=b'h').contains(&bytes[0]) && (b'1'..=b'8').contains(&bytes[1])
}

fn string_arg(call: &ToolCall, name: &str) -> Option<String> {
    call.arguments
        .get(name)
        .and_then(Value::as_str)
        .map(|value| value.to_string())
}

fn board_data(board: &Board) -> Value {
    serde_json::json!({
        "board_id": board.id,
        "title": board.title,
        "parent_kind": board.parent_kind,
        "parent_id": board.parent_id,
        "kind": board.kind,
        "start_fen": board.start_fen,
        "revision": board.revision,
        "mutability": board.mutability,
        "lifecycle": board.lifecycle,
    })
}

/// A newly created or copied board, reported as an applied write.
fn created(call: &ToolCall, board: &Board) -> ToolResult {
    applied(
        call,
        board.revision as u64,
        format!("created board {}", board.title),
        board_data(board),
    )
}
