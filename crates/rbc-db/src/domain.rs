//! Multi-board workspaces, studies and the v4 repository API.
//!
//! Every write that changes board content is revision-checked. Locked or
//! published content returns `copy_required`; the caller must copy it.

use anyhow::{Context, Result};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fmt;
use uuid::Uuid;

use crate::Db;

const NOW: &str = "CAST(strftime('%s','now') AS INTEGER)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardWriteError {
    NotFound(String),
    Conflict { current: i64 },
    CopyRequired,
    Invalid(String),
    Db(String),
}

impl fmt::Display for BoardWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "board {id} not found"),
            Self::Conflict { current } => {
                write!(f, "board revision conflict (current {current})")
            }
            Self::CopyRequired => write!(f, "board is locked or published; copy it to edit"),
            Self::Invalid(message) => write!(f, "{message}"),
            Self::Db(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for BoardWriteError {}

impl From<rusqlite::Error> for BoardWriteError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Db(value.to_string())
    }
}

impl From<anyhow::Error> for BoardWriteError {
    fn from(value: anyhow::Error) -> Self {
        Self::Db(value.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub position: i64,
    pub active_board_id: Option<String>,
    pub created: i64,
    pub updated: i64,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Board {
    pub id: String,
    pub parent_kind: String,
    pub parent_id: String,
    pub kind: String,
    pub title: String,
    pub variant: String,
    pub start_fen: String,
    pub revision: i64,
    pub mutability: String,
    pub lifecycle: String,
    pub content_json: String,
    pub metadata_json: String,
    pub created: i64,
    pub updated: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Study {
    pub id: String,
    pub title: String,
    pub description_md: String,
    pub lifecycle: String,
    pub current_version: i64,
    pub tags_json: String,
    pub metadata_json: String,
    pub created: i64,
    pub updated: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: String,
    pub study_id: String,
    pub ordinal: i64,
    pub title: String,
    pub primary_board_id: String,
    pub board_ids_json: String,
    pub notes_md: String,
    pub metadata_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudyVersion {
    pub id: String,
    pub study_id: String,
    pub version: i64,
    pub bundle_json: String,
    pub provenance: Option<String>,
    pub created: i64,
    pub published: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudyProgress {
    pub study_id: String,
    pub chapter_id: String,
    pub board_id: String,
    pub user: String,
    pub completed: bool,
    pub attempts: i64,
    pub score: Option<f64>,
    pub hints: i64,
    pub mastery: Option<f64>,
    pub last_seen: i64,
}

fn workspace_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: row.get(0)?,
        title: row.get(1)?,
        kind: row.get(2)?,
        position: row.get(3)?,
        active_board_id: row.get(4)?,
        created: row.get(5)?,
        updated: row.get(6)?,
        archived: row.get::<_, i64>(7)? != 0,
    })
}

fn board_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Board> {
    Ok(Board {
        id: row.get(0)?,
        parent_kind: row.get(1)?,
        parent_id: row.get(2)?,
        kind: row.get(3)?,
        title: row.get(4)?,
        variant: row.get(5)?,
        start_fen: row.get(6)?,
        revision: row.get(7)?,
        mutability: row.get(8)?,
        lifecycle: row.get(9)?,
        content_json: row.get(10)?,
        metadata_json: row.get(11)?,
        created: row.get(12)?,
        updated: row.get(13)?,
    })
}

fn study_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Study> {
    Ok(Study {
        id: row.get(0)?,
        title: row.get(1)?,
        description_md: row.get(2)?,
        lifecycle: row.get(3)?,
        current_version: row.get(4)?,
        tags_json: row.get(5)?,
        metadata_json: row.get(6)?,
        created: row.get(7)?,
        updated: row.get(8)?,
    })
}

fn chapter_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Chapter> {
    Ok(Chapter {
        id: row.get(0)?,
        study_id: row.get(1)?,
        ordinal: row.get(2)?,
        title: row.get(3)?,
        primary_board_id: row.get(4)?,
        board_ids_json: row.get(5)?,
        notes_md: row.get(6)?,
        metadata_json: row.get(7)?,
    })
}

const BOARD_COLUMNS: &str = "id, parent_kind, parent_id, kind, title, variant, start_fen, \
    revision, mutability, lifecycle, content_json, metadata_json, created, updated";

const STUDY_COLUMNS: &str = "id, title, description_md, lifecycle, current_version, \
    tags_json, metadata_json, created, updated";

const CHAPTER_COLUMNS: &str = "id, study_id, ordinal, title, primary_board_id, \
    board_ids_json, notes_md, metadata_json";

/// The three JSON blobs an AI session is created with.
///
/// They are separate columns but always travel together, and passing them as
/// three `&str` arguments made the call site a guessing game about which was
/// which.
#[derive(Debug, Clone, Copy, Default)]
pub struct SessionProfile<'a> {
    pub model_json: &'a str,
    pub context_spec_json: &'a str,
    pub permissions_json: &'a str,
}

impl Db {
    // ------------------------------------------------------------------
    // Workspaces
    // ------------------------------------------------------------------

    pub fn create_workspace(&self, title: &str, kind: &str) -> Result<Workspace> {
        let id = Uuid::new_v4().to_string();
        {
            let conn = self.conn.lock().expect("db lock");
            let position: i64 = conn.query_row(
                "SELECT COALESCE(MAX(position), -1) + 1 FROM workspaces",
                [],
                |row| row.get(0),
            )?;
            conn.execute(
                &format!(
                    "INSERT INTO workspaces (id, title, kind, position, active_board_id,
                     created, updated, archived)
                     VALUES (?1, ?2, ?3, ?4, NULL, {NOW}, {NOW}, 0)"
                ),
                params![id, title, kind, position],
            )?;
        }
        self.workspace(&id)?.context("workspace just inserted")
    }

    pub fn workspace(&self, id: &str) -> Result<Option<Workspace>> {
        let conn = self.conn.lock().expect("db lock");
        Ok(conn
            .query_row(
                "SELECT id, title, kind, position, active_board_id, created, updated, archived
                 FROM workspaces WHERE id = ?1",
                params![id],
                workspace_row,
            )
            .optional()?)
    }

    pub fn workspaces(&self, include_archived: bool) -> Result<Vec<Workspace>> {
        let conn = self.conn.lock().expect("db lock");
        let sql = if include_archived {
            "SELECT id, title, kind, position, active_board_id, created, updated, archived
             FROM workspaces ORDER BY position, created"
        } else {
            "SELECT id, title, kind, position, active_board_id, created, updated, archived
             FROM workspaces WHERE archived = 0 ORDER BY position, created"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt
            .query_map([], workspace_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn update_workspace(
        &self,
        id: &str,
        title: Option<&str>,
        active_board_id: Option<Option<&str>>,
    ) -> Result<Workspace> {
        {
            let conn = self.conn.lock().expect("db lock");
            if let Some(title) = title {
                conn.execute(
                    "UPDATE workspaces SET title = ?1, updated = CAST(strftime('%s','now') AS INTEGER)
                     WHERE id = ?2",
                    params![title, id],
                )?;
            }
            if let Some(active) = active_board_id {
                conn.execute(
                    "UPDATE workspaces SET active_board_id = ?1,
                     updated = CAST(strftime('%s','now') AS INTEGER) WHERE id = ?2",
                    params![active, id],
                )?;
            }
        }
        self.workspace(id)?
            .with_context(|| format!("workspace {id} not found"))
    }

    pub fn archive_workspace(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "UPDATE workspaces SET archived = 1,
             updated = CAST(strftime('%s','now') AS INTEGER) WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Boards
    // ------------------------------------------------------------------

    pub fn create_board(
        &self,
        parent_kind: &str,
        parent_id: &str,
        kind: &str,
        title: &str,
        start_fen: &str,
        content_json: &str,
    ) -> Result<Board> {
        let id = Uuid::new_v4().to_string();
        {
            let conn = self.conn.lock().expect("db lock");
            conn.execute(
                &format!(
                    "INSERT INTO boards
                     (id, parent_kind, parent_id, kind, title, variant, start_fen,
                      revision, mutability, lifecycle, content_json, metadata_json,
                      created, updated)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'standard', ?6, 1, 'mutable', 'draft',
                             ?7, '{{}}', {NOW}, {NOW})"
                ),
                params![
                    id,
                    parent_kind,
                    parent_id,
                    kind,
                    title,
                    start_fen,
                    content_json
                ],
            )?;
        }
        self.board(&id)?.context("board just inserted")
    }

    pub fn board(&self, id: &str) -> Result<Option<Board>> {
        let conn = self.conn.lock().expect("db lock");
        Ok(conn
            .query_row(
                &format!("SELECT {BOARD_COLUMNS} FROM boards WHERE id = ?1"),
                params![id],
                board_row,
            )
            .optional()?)
    }

    pub fn boards(&self, parent_kind: &str, parent_id: &str) -> Result<Vec<Board>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare(&format!(
            "SELECT {BOARD_COLUMNS} FROM boards
             WHERE parent_kind = ?1 AND parent_id = ?2 ORDER BY created, rowid"
        ))?;
        let rows = stmt
            .query_map(params![parent_kind, parent_id], board_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn update_board_content(
        &self,
        id: &str,
        expected_revision: i64,
        content_json: &str,
        actor: Option<&str>,
    ) -> std::result::Result<Board, BoardWriteError> {
        let next;
        {
            let conn = self.conn.lock().expect("db lock");
            let board = conn
                .query_row(
                    &format!("SELECT {BOARD_COLUMNS} FROM boards WHERE id = ?1"),
                    params![id],
                    board_row,
                )
                .optional()?
                .ok_or_else(|| BoardWriteError::NotFound(id.to_owned()))?;
            if board.mutability == "locked" || board.lifecycle == "published" {
                return Err(BoardWriteError::CopyRequired);
            }
            if board.revision != expected_revision {
                return Err(BoardWriteError::Conflict {
                    current: board.revision,
                });
            }
            next = board.revision + 1;
            conn.execute(
                "UPDATE boards SET content_json = ?1, revision = ?2,
                 updated = CAST(strftime('%s','now') AS INTEGER) WHERE id = ?3",
                params![content_json, next, id],
            )?;
            conn.execute(
                "INSERT INTO board_revisions (board_id, revision, content_json, actor, created)
                 VALUES (?1, ?2, ?3, ?4, CAST(strftime('%s','now') AS INTEGER))",
                params![id, next, content_json, actor],
            )?;
        }
        self.board(id)
            .map_err(BoardWriteError::from)?
            .ok_or_else(|| BoardWriteError::NotFound(id.to_owned()))
    }

    pub fn copy_board(
        &self,
        id: &str,
        parent_kind: Option<&str>,
        parent_id: Option<&str>,
        title: Option<&str>,
    ) -> Result<Board> {
        let board = self
            .board(id)?
            .with_context(|| format!("board {id} not found"))?;
        let new_id = Uuid::new_v4().to_string();
        let mut metadata: serde_json::Value =
            serde_json::from_str(&board.metadata_json).unwrap_or_else(|_| json!({}));
        if let Some(object) = metadata.as_object_mut() {
            object.insert("copied_from".into(), json!(board.id));
            object.insert("copied_revision".into(), json!(board.revision));
        }
        let parent_kind = parent_kind.unwrap_or(&board.parent_kind);
        let parent_id = parent_id.unwrap_or(&board.parent_id);
        let title = title.unwrap_or(&board.title);
        {
            let conn = self.conn.lock().expect("db lock");
            conn.execute(
                &format!(
                    "INSERT INTO boards
                     (id, parent_kind, parent_id, kind, title, variant, start_fen,
                      revision, mutability, lifecycle, content_json, metadata_json,
                      created, updated)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, 'mutable', 'draft', ?8, ?9,
                             {NOW}, {NOW})"
                ),
                params![
                    new_id,
                    parent_kind,
                    parent_id,
                    board.kind,
                    title,
                    board.variant,
                    board.start_fen,
                    board.content_json,
                    metadata.to_string()
                ],
            )?;
        }
        self.board(&new_id)?.context("copied board just inserted")
    }

    pub fn set_board_mutability(&self, id: &str, mutability: &str) -> Result<Board> {
        if !matches!(mutability, "mutable" | "locked") {
            anyhow::bail!("mutability must be mutable or locked");
        }
        {
            let conn = self.conn.lock().expect("db lock");
            conn.execute(
                "UPDATE boards SET mutability = ?1,
                 updated = CAST(strftime('%s','now') AS INTEGER) WHERE id = ?2",
                params![mutability, id],
            )?;
        }
        self.board(id)?
            .with_context(|| format!("board {id} not found"))
    }

    pub fn set_board_lifecycle(&self, id: &str, lifecycle: &str) -> Result<Board> {
        if !matches!(lifecycle, "draft" | "published" | "archived") {
            anyhow::bail!("lifecycle must be draft, published or archived");
        }
        {
            let conn = self.conn.lock().expect("db lock");
            conn.execute(
                "UPDATE boards SET lifecycle = ?1,
                 updated = CAST(strftime('%s','now') AS INTEGER) WHERE id = ?2",
                params![lifecycle, id],
            )?;
        }
        self.board(id)?
            .with_context(|| format!("board {id} not found"))
    }

    pub fn archive_board(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "UPDATE boards SET lifecycle = 'archived',
             updated = CAST(strftime('%s','now') AS INTEGER) WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Delete a board and its revisions. A chapter's primary board is refused;
    /// the chapter must be removed or repointed first.
    pub fn delete_board(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        let primary: i64 = conn.query_row(
            "SELECT COUNT(*) FROM study_chapters WHERE primary_board_id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        if primary > 0 {
            anyhow::bail!("board {id} is a chapter primary; remove the chapter first");
        }
        conn.execute(
            "DELETE FROM board_revisions WHERE board_id = ?1",
            params![id],
        )?;
        conn.execute("DELETE FROM boards WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn reparent_board(
        &self,
        id: &str,
        parent_kind: &str,
        parent_id: &str,
        expected_revision: i64,
    ) -> std::result::Result<Board, BoardWriteError> {
        {
            let conn = self.conn.lock().expect("db lock");
            let board = conn
                .query_row(
                    &format!("SELECT {BOARD_COLUMNS} FROM boards WHERE id = ?1"),
                    params![id],
                    board_row,
                )
                .optional()?
                .ok_or_else(|| BoardWriteError::NotFound(id.to_owned()))?;
            if board.mutability == "locked" || board.lifecycle == "published" {
                return Err(BoardWriteError::CopyRequired);
            }
            if board.revision != expected_revision {
                return Err(BoardWriteError::Conflict {
                    current: board.revision,
                });
            }
            conn.execute(
                "UPDATE boards SET parent_kind = ?1, parent_id = ?2, revision = ?3,
                 updated = CAST(strftime('%s','now') AS INTEGER) WHERE id = ?4",
                params![parent_kind, parent_id, board.revision + 1, id],
            )?;
        }
        self.board(id)
            .map_err(BoardWriteError::from)?
            .ok_or_else(|| BoardWriteError::NotFound(id.to_owned()))
    }

    // ------------------------------------------------------------------
    // Studies
    // ------------------------------------------------------------------

    pub fn create_study(&self, title: &str, description_md: &str) -> Result<Study> {
        let id = Uuid::new_v4().to_string();
        {
            let conn = self.conn.lock().expect("db lock");
            conn.execute(
                &format!(
                    "INSERT INTO studies
                     (id, title, description_md, lifecycle, current_version, tags_json,
                      metadata_json, created, updated)
                     VALUES (?1, ?2, ?3, 'draft', 1, '[]', '{{}}', {NOW}, {NOW})"
                ),
                params![id, title, description_md],
            )?;
        }
        self.study(&id)?.context("study just inserted")
    }

    pub fn study(&self, id: &str) -> Result<Option<Study>> {
        let conn = self.conn.lock().expect("db lock");
        Ok(conn
            .query_row(
                &format!("SELECT {STUDY_COLUMNS} FROM studies WHERE id = ?1"),
                params![id],
                study_row,
            )
            .optional()?)
    }

    pub fn studies(&self) -> Result<Vec<Study>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare(&format!(
            "SELECT {STUDY_COLUMNS} FROM studies ORDER BY updated DESC, created DESC"
        ))?;
        Ok(stmt
            .query_map([], study_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn chapters(&self, study_id: &str) -> Result<Vec<Chapter>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare(&format!(
            "SELECT {CHAPTER_COLUMNS} FROM study_chapters
             WHERE study_id = ?1 ORDER BY ordinal, rowid"
        ))?;
        Ok(stmt
            .query_map(params![study_id], chapter_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn add_chapter(
        &self,
        study_id: &str,
        title: &str,
        start_fen: &str,
        notes_md: &str,
    ) -> Result<(Chapter, Board)> {
        self.study(study_id)?
            .with_context(|| format!("study {study_id} not found"))?;
        let chapter_id = Uuid::new_v4().to_string();
        let board = self.create_board(
            "study_chapter",
            &chapter_id,
            "study_primary",
            title,
            start_fen,
            "{}",
        )?;
        let board_ids = json!([board.id]).to_string();
        {
            let conn = self.conn.lock().expect("db lock");
            let ordinal: i64 = conn.query_row(
                "SELECT COALESCE(MAX(ordinal), 0) + 1 FROM study_chapters WHERE study_id = ?1",
                params![study_id],
                |row| row.get(0),
            )?;
            conn.execute(
                "INSERT INTO study_chapters
                 (id, study_id, ordinal, title, primary_board_id, board_ids_json,
                  notes_md, metadata_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, '{}')",
                params![
                    chapter_id, study_id, ordinal, title, board.id, board_ids, notes_md
                ],
            )?;
            conn.execute(
                "UPDATE studies SET updated = CAST(strftime('%s','now') AS INTEGER)
                 WHERE id = ?1",
                params![study_id],
            )?;
        }
        let chapter = self
            .chapters(study_id)?
            .into_iter()
            .find(|chapter| chapter.id == chapter_id)
            .context("chapter just inserted")?;
        Ok((chapter, board))
    }

    pub fn add_board_to_chapter(
        &self,
        study_id: &str,
        chapter_id: &str,
        title: &str,
        start_fen: &str,
        content_json: &str,
    ) -> Result<Board> {
        let chapter = self
            .chapters(study_id)?
            .into_iter()
            .find(|chapter| chapter.id == chapter_id)
            .with_context(|| format!("chapter {chapter_id} not found in study {study_id}"))?;
        let board = self.create_board(
            "study_chapter",
            chapter_id,
            "study_additional",
            title,
            start_fen,
            content_json,
        )?;
        let mut ids: Vec<String> =
            serde_json::from_str(&chapter.board_ids_json).unwrap_or_default();
        ids.push(board.id.clone());
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "UPDATE study_chapters SET board_ids_json = ?1 WHERE id = ?2",
            params![json!(ids).to_string(), chapter_id],
        )?;
        Ok(board)
    }

    pub fn reorder_chapters(&self, study_id: &str, chapter_ids: &[String]) -> Result<()> {
        let mut conn = self.conn.lock().expect("db lock");
        let tx = conn.transaction()?;
        for (index, id) in chapter_ids.iter().enumerate() {
            let changed = tx.execute(
                "UPDATE study_chapters SET ordinal = ?1 WHERE id = ?2 AND study_id = ?3",
                params![(index + 1) as i64, id, study_id],
            )?;
            if changed != 1 {
                anyhow::bail!("chapter {id} is not in study {study_id}");
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Replace a study's description, or one chapter's markdown notes.
    ///
    /// The two live in different columns and mean different things: a study's
    /// description introduces the whole thing, a chapter's notes belong to that
    /// chapter. Passing a chapter id picks the second.
    pub fn write_study_text(
        &self,
        study_id: &str,
        chapter_id: Option<&str>,
        markdown: &str,
    ) -> Result<()> {
        self.study(study_id)?
            .with_context(|| format!("study {study_id} not found"))?;
        let conn = self.conn.lock().expect("db lock");
        match chapter_id {
            Some(chapter_id) => {
                let changed = conn.execute(
                    "UPDATE study_chapters SET notes_md = ?1 WHERE id = ?2 AND study_id = ?3",
                    params![markdown, chapter_id, study_id],
                )?;
                if changed == 0 {
                    anyhow::bail!("no chapter {chapter_id} in study {study_id}");
                }
            }
            None => {
                conn.execute(
                    "UPDATE studies SET description_md = ?1,
                     updated = CAST(strftime('%s','now') AS INTEGER) WHERE id = ?2",
                    params![markdown, study_id],
                )?;
            }
        }
        Ok(())
    }

    pub fn publish_study(&self, study_id: &str, expected_version: i64) -> Result<StudyVersion> {
        let study = self
            .study(study_id)?
            .with_context(|| format!("study {study_id} not found"))?;
        if study.current_version != expected_version {
            anyhow::bail!("study version conflict (current {})", study.current_version);
        }
        let chapters = self.chapters(study_id)?;
        let mut chapter_values = Vec::new();
        for chapter in &chapters {
            let boards = self.boards("study_chapter", &chapter.id)?;
            chapter_values.push(json!({"chapter": chapter, "boards": boards}));
        }
        let bundle = json!({
            "schema": "rbc-study/1",
            "study": study,
            "chapters": chapter_values,
        });
        let version = study.current_version + 1;
        let id = Uuid::new_v4().to_string();
        {
            let conn = self.conn.lock().expect("db lock");
            conn.execute(
                &format!(
                    "INSERT INTO study_versions
                     (id, study_id, version, bundle_json, provenance, created, published)
                     VALUES (?1, ?2, ?3, ?4, NULL, {NOW}, {NOW})"
                ),
                params![id, study_id, version, bundle.to_string()],
            )?;
            conn.execute(
                "UPDATE studies SET lifecycle = 'published', current_version = ?1,
                 updated = CAST(strftime('%s','now') AS INTEGER) WHERE id = ?2",
                params![version, study_id],
            )?;
        }
        let conn = self.conn.lock().expect("db lock");
        Ok(conn.query_row(
            "SELECT id, study_id, version, bundle_json, provenance, created, published
             FROM study_versions WHERE id = ?1",
            params![id],
            |row| {
                Ok(StudyVersion {
                    id: row.get(0)?,
                    study_id: row.get(1)?,
                    version: row.get(2)?,
                    bundle_json: row.get(3)?,
                    provenance: row.get(4)?,
                    created: row.get(5)?,
                    published: row.get(6)?,
                })
            },
        )?)
    }

    pub fn study_versions(&self, study_id: &str) -> Result<Vec<StudyVersion>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare(
            "SELECT id, study_id, version, bundle_json, provenance, created, published
             FROM study_versions WHERE study_id = ?1 ORDER BY version",
        )?;
        Ok(stmt
            .query_map(params![study_id], |row| {
                Ok(StudyVersion {
                    id: row.get(0)?,
                    study_id: row.get(1)?,
                    version: row.get(2)?,
                    bundle_json: row.get(3)?,
                    provenance: row.get(4)?,
                    created: row.get(5)?,
                    published: row.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn copy_study(&self, study_id: &str) -> Result<Study> {
        let study = self
            .study(study_id)?
            .with_context(|| format!("study {study_id} not found"))?;
        let copy = self.create_study(&format!("{} (copy)", study.title), &study.description_md)?;
        for chapter in self.chapters(study_id)? {
            let old_primary = self.board(&chapter.primary_board_id)?;
            let start_fen = old_primary
                .as_ref()
                .map(|board| board.start_fen.clone())
                .unwrap_or_else(|| {
                    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".into()
                });
            let (new_chapter, new_primary) =
                self.add_chapter(&copy.id, &chapter.title, &start_fen, &chapter.notes_md)?;
            if let Some(old) = old_primary {
                let _ = self.update_board_content(
                    &new_primary.id,
                    1,
                    &old.content_json,
                    Some("copy_study"),
                );
            }
            let boards = self.boards("study_chapter", &chapter.id)?;
            for board in boards {
                if board.id == chapter.primary_board_id {
                    continue;
                }
                self.add_board_to_chapter(
                    &copy.id,
                    &new_chapter.id,
                    &board.title,
                    &board.start_fen,
                    &board.content_json,
                )?;
            }
        }
        let mut metadata: serde_json::Value =
            serde_json::from_str(&study.metadata_json).unwrap_or_else(|_| json!({}));
        if let Some(object) = metadata.as_object_mut() {
            object.insert("copied_from".into(), json!(study.id));
            object.insert("copied_version".into(), json!(study.current_version));
        }
        {
            let conn = self.conn.lock().expect("db lock");
            conn.execute(
                "UPDATE studies SET metadata_json = ?1 WHERE id = ?2",
                params![metadata.to_string(), copy.id],
            )?;
        }
        self.study(&copy.id)?.context("copied study just inserted")
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_study_progress(
        &self,
        study_id: &str,
        chapter_id: &str,
        board_id: &str,
        user: &str,
        completed: bool,
        attempts: i64,
        score: Option<f64>,
        hints: i64,
        mastery: Option<f64>,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            &format!(
                "INSERT INTO study_progress
                 (study_id, chapter_id, board_id, user, completed, attempts, score, hints,
                  mastery, last_seen)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, {NOW})
                 ON CONFLICT(study_id, chapter_id, board_id, user) DO UPDATE SET
                   completed = excluded.completed,
                   attempts = excluded.attempts,
                   score = excluded.score,
                   hints = excluded.hints,
                   mastery = excluded.mastery,
                   last_seen = excluded.last_seen"
            ),
            params![
                study_id,
                chapter_id,
                board_id,
                user,
                a_i64(completed),
                attempts,
                score,
                hints,
                mastery
            ],
        )?;
        Ok(())
    }

    pub fn study_progress(&self, study_id: &str, user: &str) -> Result<Vec<StudyProgress>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare(
            "SELECT study_id, chapter_id, board_id, user, completed, attempts, score, hints,
                    mastery, last_seen
             FROM study_progress WHERE study_id = ?1 AND user = ?2
             ORDER BY chapter_id, board_id",
        )?;
        Ok(stmt
            .query_map(params![study_id, user], |row| {
                Ok(StudyProgress {
                    study_id: row.get(0)?,
                    chapter_id: row.get(1)?,
                    board_id: row.get(2)?,
                    user: row.get(3)?,
                    completed: row.get::<_, i64>(4)? != 0,
                    attempts: row.get(5)?,
                    score: row.get(6)?,
                    hints: row.get(7)?,
                    mastery: row.get(8)?,
                    last_seen: row.get(9)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSession {
    /// Absent means this session has no ceiling of its own.
    #[serde(default)]
    pub token_budget: Option<i64>,
    /// What this session has spent so far.
    #[serde(default)]
    pub tokens_used: i64,
    pub id: String,
    pub title: String,
    pub mode: String,
    pub home_board_id: Option<String>,
    pub target_board_id: Option<String>,
    pub model_json: String,
    pub context_spec_json: String,
    pub permissions_json: String,
    pub created: i64,
    pub updated: i64,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiTurn {
    pub id: String,
    pub session_id: String,
    pub seq: i64,
    pub role: String,
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls_json: String,
    pub tool_results_json: String,
    pub context_json: String,
    pub context_sha256: String,
    pub spec_version: String,
    pub template_version: String,
    pub tool_schema_version: String,
    pub engine_fingerprint: Option<String>,
    pub model_json: String,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub latency_ms: i64,
    pub error_json: Option<String>,
    pub created: i64,
}

#[derive(Debug, Clone, Default)]
pub struct AiTurnInput {
    pub role: String,
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls_json: String,
    pub tool_results_json: String,
    pub context_json: String,
    pub context_sha256: String,
    pub spec_version: String,
    pub template_version: String,
    pub tool_schema_version: String,
    pub engine_fingerprint: Option<String>,
    pub model_json: String,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub latency_ms: i64,
    pub error_json: Option<String>,
}

fn ai_session_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AiSession> {
    Ok(AiSession {
        token_budget: row.get(11)?,
        tokens_used: row.get(12)?,
        id: row.get(0)?,
        title: row.get(1)?,
        mode: row.get(2)?,
        home_board_id: row.get(3)?,
        target_board_id: row.get(4)?,
        model_json: row.get(5)?,
        context_spec_json: row.get(6)?,
        permissions_json: row.get(7)?,
        created: row.get(8)?,
        updated: row.get(9)?,
        archived: row.get::<_, i64>(10)? != 0,
    })
}

fn ai_turn_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AiTurn> {
    Ok(AiTurn {
        id: row.get(0)?,
        session_id: row.get(1)?,
        seq: row.get(2)?,
        role: row.get(3)?,
        content: row.get(4)?,
        reasoning: row.get(5)?,
        tool_calls_json: row.get(6)?,
        tool_results_json: row.get(7)?,
        context_json: row.get(8)?,
        context_sha256: row.get(9)?,
        spec_version: row.get(10)?,
        template_version: row.get(11)?,
        tool_schema_version: row.get(12)?,
        engine_fingerprint: row.get(13)?,
        model_json: row.get(14)?,
        tokens_in: row.get(15)?,
        tokens_out: row.get(16)?,
        latency_ms: row.get(17)?,
        error_json: row.get(18)?,
        created: row.get(19)?,
    })
}

/// One stored, replayable stream event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiEventRow {
    pub stream_kind: String,
    pub stream_id: String,
    pub event_id: i64,
    pub payload_json: String,
    pub created: i64,
}

fn ai_event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AiEventRow> {
    Ok(AiEventRow {
        stream_kind: row.get(0)?,
        stream_id: row.get(1)?,
        event_id: row.get(2)?,
        payload_json: row.get(3)?,
        created: row.get(4)?,
    })
}

/// One approval request, as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiApproval {
    pub id: String,
    pub origin: String,
    pub session_id: Option<String>,
    pub target_kind: String,
    pub target_id: String,
    pub action: String,
    pub changeset_json: String,
    pub status: String,
    pub tool: String,
    pub summary: String,
    pub created: i64,
    pub resolved: Option<i64>,
    pub expires: Option<i64>,
}

/// The fields a caller supplies when it queues an approval.
#[derive(Debug, Clone, Default)]
pub struct AiApprovalInput {
    pub origin: String,
    pub session_id: Option<String>,
    pub target_kind: String,
    pub target_id: String,
    pub action: String,
    pub tool: String,
    pub summary: String,
    pub changeset_json: String,
    pub created: i64,
    pub expires: Option<i64>,
}

fn ai_approval_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AiApproval> {
    Ok(AiApproval {
        id: row.get(0)?,
        origin: row.get(1)?,
        session_id: row.get(2)?,
        target_kind: row.get(3)?,
        target_id: row.get(4)?,
        action: row.get(5)?,
        changeset_json: row.get(6)?,
        status: row.get(7)?,
        created: row.get(8)?,
        resolved: row.get(9)?,
        tool: row.get(10)?,
        summary: row.get(11)?,
        expires: row.get(12)?,
    })
}

const AI_APPROVAL_COLUMNS: &str = "id, origin, session_id, target_kind, target_id, action,     changeset_json, status, created, resolved, tool, summary, expires";

const AI_EVENT_COLUMNS: &str = "stream_kind, stream_id, event_id, payload_json, created";

const AI_SESSION_COLUMNS: &str = "id, title, mode, home_board_id, target_board_id, model_json,     context_spec_json, permissions_json, created, updated, archived, token_budget, tokens_used";

const AI_TURN_COLUMNS: &str = "id, session_id, seq, role, content, reasoning, tool_calls_json,     tool_results_json, context_json, context_sha256, spec_version, template_version,     tool_schema_version, engine_fingerprint, model_json, tokens_in, tokens_out, latency_ms,     error_json, created";

impl Db {
    pub fn create_ai_session(
        &self,
        title: &str,
        mode: &str,
        home_board_id: Option<&str>,
        target_board_id: Option<&str>,
        profile: SessionProfile<'_>,
    ) -> Result<AiSession> {
        let SessionProfile {
            model_json,
            context_spec_json,
            permissions_json,
        } = profile;
        let id = Uuid::new_v4().to_string();
        {
            let conn = self.conn.lock().expect("db lock");
            conn.execute(
                &format!(
                    "INSERT INTO ai_sessions
                     (id, title, mode, home_board_id, target_board_id, model_json,
                      context_spec_json, permissions_json, created, updated, archived)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, {NOW}, {NOW}, 0)"
                ),
                params![
                    id,
                    title,
                    mode,
                    home_board_id,
                    target_board_id,
                    model_json,
                    context_spec_json,
                    permissions_json
                ],
            )?;
        }
        self.ai_session(&id)?.context("AI session just inserted")
    }

    pub fn ai_session(&self, id: &str) -> Result<Option<AiSession>> {
        let conn = self.conn.lock().expect("db lock");
        Ok(conn
            .query_row(
                &format!("SELECT {AI_SESSION_COLUMNS} FROM ai_sessions WHERE id = ?1"),
                params![id],
                ai_session_row,
            )
            .optional()?)
    }

    pub fn ai_sessions(&self) -> Result<Vec<AiSession>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare(&format!(
            "SELECT {AI_SESSION_COLUMNS} FROM ai_sessions WHERE archived = 0
             ORDER BY updated DESC, created DESC"
        ))?;
        Ok(stmt
            .query_map([], ai_session_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn rename_ai_session(&self, id: &str, title: &str) -> Result<AiSession> {
        {
            let conn = self.conn.lock().expect("db lock");
            conn.execute(
                "UPDATE ai_sessions SET title = ?1,
                 updated = CAST(strftime('%s','now') AS INTEGER) WHERE id = ?2",
                params![title, id],
            )?;
        }
        self.ai_session(id)?
            .with_context(|| format!("AI session {id} not found"))
    }

    pub fn archive_ai_session(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "UPDATE ai_sessions SET archived = 1,
             updated = CAST(strftime('%s','now') AS INTEGER) WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Set a session's own spend ceiling, or clear it.
    pub fn set_ai_session_budget(&self, id: &str, tokens: Option<i64>) -> Result<AiSession> {
        {
            let conn = self.conn.lock().expect("db lock");
            conn.execute(
                "UPDATE ai_sessions SET token_budget = ?1,
                 updated = CAST(strftime('%s','now') AS INTEGER) WHERE id = ?2",
                params![tokens, id],
            )?;
        }
        self.ai_session(id)?
            .with_context(|| format!("AI session {id} not found"))
    }

    /// Add what a turn spent to the session's running total.
    pub fn add_ai_session_tokens(&self, id: &str, tokens: i64) -> Result<i64> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "UPDATE ai_sessions SET tokens_used = tokens_used + ?1,
             updated = CAST(strftime('%s','now') AS INTEGER) WHERE id = ?2",
            params![tokens, id],
        )?;
        let used: i64 = conn.query_row(
            "SELECT tokens_used FROM ai_sessions WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        Ok(used)
    }

    /// Whether a session may take another turn, and how much is left.
    ///
    /// A session with no ceiling always may; the answer is `(true, None)`.
    pub fn ai_session_has_budget(&self, id: &str) -> Result<bool> {
        let session = self
            .ai_session(id)?
            .with_context(|| format!("AI session {id} not found"))?;
        Ok(match session.token_budget {
            Some(budget) => session.tokens_used < budget,
            None => true,
        })
    }

    /// Replace a session's permission profile.
    ///
    /// The profile decides what the policy auto-approves, so it is a write a
    /// person makes deliberately, not something a turn can change.
    pub fn set_ai_session_permissions(
        &self,
        id: &str,
        permissions_json: &str,
    ) -> Result<AiSession> {
        {
            let conn = self.conn.lock().expect("db lock");
            conn.execute(
                "UPDATE ai_sessions SET permissions_json = ?1,
                 updated = CAST(strftime('%s','now') AS INTEGER) WHERE id = ?2",
                params![permissions_json, id],
            )?;
        }
        self.ai_session(id)?
            .with_context(|| format!("AI session {id} not found"))
    }

    pub fn set_ai_session_target(&self, id: &str, board_id: Option<&str>) -> Result<AiSession> {
        {
            let conn = self.conn.lock().expect("db lock");
            conn.execute(
                "UPDATE ai_sessions SET target_board_id = ?1,
                 updated = CAST(strftime('%s','now') AS INTEGER) WHERE id = ?2",
                params![board_id, id],
            )?;
        }
        self.ai_session(id)?
            .with_context(|| format!("AI session {id} not found"))
    }

    pub fn append_ai_turn(&self, session_id: &str, input: &AiTurnInput) -> Result<AiTurn> {
        let id = Uuid::new_v4().to_string();
        {
            let conn = self.conn.lock().expect("db lock");
            let seq: i64 = conn.query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM ai_turns WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )?;
            conn.execute(
                &format!(
                    "INSERT INTO ai_turns
                     (id, session_id, seq, role, content, reasoning, tool_calls_json,
                      tool_results_json, context_json, context_sha256, spec_version,
                      template_version, tool_schema_version, engine_fingerprint, model_json,
                      tokens_in, tokens_out, latency_ms, error_json, created)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                             ?14, ?15, ?16, ?17, ?18, ?19, {NOW})"
                ),
                params![
                    id,
                    session_id,
                    seq,
                    input.role,
                    input.content,
                    input.reasoning,
                    input.tool_calls_json,
                    input.tool_results_json,
                    input.context_json,
                    input.context_sha256,
                    input.spec_version,
                    input.template_version,
                    input.tool_schema_version,
                    input.engine_fingerprint,
                    input.model_json,
                    input.tokens_in,
                    input.tokens_out,
                    input.latency_ms,
                    input.error_json
                ],
            )?;
            conn.execute(
                "UPDATE ai_sessions SET updated = CAST(strftime('%s','now') AS INTEGER)
                 WHERE id = ?1",
                params![session_id],
            )?;
        }
        let conn = self.conn.lock().expect("db lock");
        Ok(conn.query_row(
            &format!("SELECT {AI_TURN_COLUMNS} FROM ai_turns WHERE id = ?1"),
            params![id],
            ai_turn_row,
        )?)
    }

    pub fn ai_turns(&self, session_id: &str) -> Result<Vec<AiTurn>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare(&format!(
            "SELECT {AI_TURN_COLUMNS} FROM ai_turns WHERE session_id = ?1 ORDER BY seq"
        ))?;
        Ok(stmt
            .query_map(params![session_id], ai_turn_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // --- approval queue ----------------------------------------------------

    /// Record a pending approval request.
    pub fn create_ai_approval(&self, input: &AiApprovalInput) -> Result<AiApproval> {
        let id = Uuid::new_v4().to_string();
        {
            let conn = self.conn.lock().expect("db lock");
            conn.execute(
                "INSERT INTO ai_approvals
                 (id, origin, session_id, target_kind, target_id, action,
                  changeset_json, status, created, resolved, tool, summary, expires)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8, NULL, ?9, ?10, ?11)",
                params![
                    id,
                    input.origin,
                    input.session_id,
                    input.target_kind,
                    input.target_id,
                    input.action,
                    input.changeset_json,
                    input.created,
                    input.tool,
                    input.summary,
                    input.expires
                ],
            )?;
        }
        self.ai_approval(&id)?.context("approval just inserted")
    }

    pub fn ai_approval(&self, id: &str) -> Result<Option<AiApproval>> {
        let conn = self.conn.lock().expect("db lock");
        Ok(conn
            .query_row(
                &format!("SELECT {AI_APPROVAL_COLUMNS} FROM ai_approvals WHERE id = ?1"),
                params![id],
                ai_approval_row,
            )
            .optional()?)
    }

    /// Approvals in one status, oldest first.
    pub fn ai_approvals(&self, status: &str) -> Result<Vec<AiApproval>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare(&format!(
            "SELECT {AI_APPROVAL_COLUMNS} FROM ai_approvals
             WHERE status = ?1 ORDER BY created, rowid"
        ))?;
        Ok(stmt
            .query_map(params![status], ai_approval_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Move a pending approval to a resolved status.
    ///
    /// Returns the row it actually moved, so a caller cannot believe it
    /// resolved something another writer had already settled.
    pub fn resolve_ai_approval(
        &self,
        id: &str,
        status: &str,
        resolved: i64,
    ) -> Result<Option<AiApproval>> {
        {
            let conn = self.conn.lock().expect("db lock");
            let changed = conn.execute(
                "UPDATE ai_approvals SET status = ?1, resolved = ?2
                 WHERE id = ?3 AND status = 'pending'",
                params![status, resolved, id],
            )?;
            if changed == 0 {
                return Ok(None);
            }
        }
        self.ai_approval(id)
    }

    /// Expire every pending approval whose deadline has passed.
    ///
    /// Returns how many were expired, so a scheduler can report it.
    pub fn expire_ai_approvals(&self, now: i64) -> Result<usize> {
        let conn = self.conn.lock().expect("db lock");
        let expired = conn.execute(
            "UPDATE ai_approvals SET status = 'expired', resolved = ?1
             WHERE status = 'pending' AND expires IS NOT NULL AND expires <= ?1",
            params![now],
        )?;
        Ok(expired)
    }
    /// Append one event to a stream's replay log.
    ///
    /// The event id is assigned here, so two writers on the same stream cannot
    /// hand a client two different events with the same id.
    pub fn append_ai_event(
        &self,
        stream_kind: &str,
        stream_id: &str,
        payload_json: &str,
    ) -> Result<AiEventRow> {
        let conn = self.conn.lock().expect("db lock");
        let event_id: i64 = conn.query_row(
            "SELECT COALESCE(MAX(event_id), 0) + 1 FROM ai_events
             WHERE stream_kind = ?1 AND stream_id = ?2",
            params![stream_kind, stream_id],
            |row| row.get(0),
        )?;
        conn.execute(
            &format!(
                "INSERT INTO ai_events (stream_kind, stream_id, event_id, payload_json, created)
                 VALUES (?1, ?2, ?3, ?4, {NOW})"
            ),
            params![stream_kind, stream_id, event_id, payload_json],
        )?;
        Ok(conn.query_row(
            &format!(
                "SELECT {AI_EVENT_COLUMNS} FROM ai_events
                 WHERE stream_kind = ?1 AND stream_id = ?2 AND event_id = ?3"
            ),
            params![stream_kind, stream_id, event_id],
            ai_event_row,
        )?)
    }

    /// Every event on a stream after `after`, in order.
    ///
    /// `after = 0` replays the stream from the beginning, which is what a
    /// client does when it has no cursor.
    pub fn ai_events(
        &self,
        stream_kind: &str,
        stream_id: &str,
        after: i64,
    ) -> Result<Vec<AiEventRow>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare(&format!(
            "SELECT {AI_EVENT_COLUMNS} FROM ai_events
             WHERE stream_kind = ?1 AND stream_id = ?2 AND event_id > ?3
             ORDER BY event_id"
        ))?;
        Ok(stmt
            .query_map(params![stream_kind, stream_id, after], ai_event_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The highest event id on a stream, or 0 when nothing was written.
    pub fn latest_ai_event_id(&self, stream_kind: &str, stream_id: &str) -> Result<i64> {
        let conn = self.conn.lock().expect("db lock");
        Ok(conn.query_row(
            "SELECT COALESCE(MAX(event_id), 0) FROM ai_events
             WHERE stream_kind = ?1 AND stream_id = ?2",
            params![stream_kind, stream_id],
            |row| row.get(0),
        )?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiMatch {
    pub id: String,
    pub session_id: Option<String>,
    pub white_json: String,
    pub black_json: String,
    pub config_json: String,
    pub state_json: String,
    pub pgn: String,
    pub result_json: Option<String>,
    pub termination: Option<String>,
    pub rated: bool,
    pub created: i64,
    pub ended: Option<i64>,
}

fn ai_match_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AiMatch> {
    Ok(AiMatch {
        id: row.get(0)?,
        session_id: row.get(1)?,
        white_json: row.get(2)?,
        black_json: row.get(3)?,
        config_json: row.get(4)?,
        state_json: row.get(5)?,
        pgn: row.get(6)?,
        result_json: row.get(7)?,
        termination: row.get(8)?,
        rated: row.get::<_, i64>(9)? != 0,
        created: row.get(10)?,
        ended: row.get(11)?,
    })
}

const AI_MATCH_COLUMNS: &str = "id, session_id, white_json, black_json, config_json,     state_json, pgn, result_json, termination, rated, created, ended";

impl Db {
    pub fn create_ai_match(
        &self,
        session_id: Option<&str>,
        white_json: &str,
        black_json: &str,
        config_json: &str,
    ) -> Result<AiMatch> {
        let id = Uuid::new_v4().to_string();
        {
            let conn = self.conn.lock().expect("db lock");
            conn.execute(
                &format!(
                    "INSERT INTO ai_matches
                     (id, session_id, white_json, black_json, config_json, state_json, pgn,
                      result_json, termination, rated, created, ended)
                     VALUES (?1, ?2, ?3, ?4, ?5, '{{}}', '', NULL, NULL, 0, {NOW}, NULL)"
                ),
                params![id, session_id, white_json, black_json, config_json],
            )?;
        }
        self.ai_match(&id)?.context("AI match just inserted")
    }

    /// Recent matches, newest first. A match list is what the Play tab shows,
    /// so it comes from the database rather than from whatever this browser
    /// happens to remember.
    pub fn ai_matches(&self, limit: i64) -> Result<Vec<AiMatch>> {
        let conn = self.conn.lock().expect("db lock");
        let mut statement = conn.prepare(&format!(
            "SELECT {AI_MATCH_COLUMNS} FROM ai_matches ORDER BY created DESC LIMIT ?1"
        ))?;
        let rows = statement
            .query_map(params![limit.clamp(1, 200)], ai_match_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn ai_match(&self, id: &str) -> Result<Option<AiMatch>> {
        let conn = self.conn.lock().expect("db lock");
        Ok(conn
            .query_row(
                &format!("SELECT {AI_MATCH_COLUMNS} FROM ai_matches WHERE id = ?1"),
                params![id],
                ai_match_row,
            )
            .optional()?)
    }

    pub fn save_ai_match(
        &self,
        id: &str,
        state_json: &str,
        pgn: &str,
        result_json: Option<&str>,
        termination: Option<&str>,
        ended: bool,
    ) -> Result<AiMatch> {
        {
            let conn = self.conn.lock().expect("db lock");
            conn.execute(
                &format!(
                    "UPDATE ai_matches SET state_json = ?1, pgn = ?2, result_json = ?3,
                     termination = ?4,
                     ended = CASE WHEN ?5 THEN {NOW} ELSE ended END
                     WHERE id = ?6"
                ),
                params![state_json, pgn, result_json, termination, ended as i64, id],
            )?;
        }
        self.ai_match(id)?
            .with_context(|| format!("AI match {id} not found"))
    }
}

fn a_i64(value: bool) -> i64 {
    if value { 1 } else { 0 }
}
