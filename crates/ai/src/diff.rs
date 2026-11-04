//! The structural diff: two studies, compared by what they contain rather than
//! by what their bytes look like.
//!
//! Three rules decide everything here:
//!
//! - **Compare never mutates.** This module takes `&Db` and returns a report; it
//!   has no write path at all, which is cheaper to trust than a promise.
//! - **Alignment is explicit.** A chapter matches by id, then ordinal, then
//!   title; a board by id, then title, then normalized FEN. Two studies that have
//!   drifted apart still line up chapter-for-chapter when the chapters are the
//!   same chapters with new content, and a chapter that was renamed keeps its
//!   place if its id survived.
//! - **A change says where it is.** Every entry carries the chapter and board it
//!   belongs to, so a caller can focus the right board and ply without guessing.

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;

use rbc_db::Db;

/// Which of the two sides an entry belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
}

/// What happened to a thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Change {
    Added,
    Removed,
    Changed,
    Unchanged,
}

impl Change {
    /// Whether this entry is worth showing.
    pub fn is_a_change(self) -> bool {
        !matches!(self, Change::Unchanged)
    }
}

/// One field that differs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldChange {
    pub field: String,
    pub left: String,
    pub right: String,
}

/// A chapter and what became of it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChapterDiff {
    pub change: Change,
    pub left_id: Option<String>,
    pub right_id: Option<String>,
    pub title: String,
    /// Boards that only the left has, presented as titles.
    pub boards_removed: Vec<String>,
    pub boards_added: Vec<String>,
    pub boards_changed: Vec<BoardDiff>,
    pub fields: Vec<FieldChange>,
    pub notes: Option<TextDiff>,
    /// Completion on each side, when a user was given.
    pub progress: Option<ProgressDiff>,
}

/// A board inside a chapter.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BoardDiff {
    pub change: Change,
    pub title: String,
    pub left_id: Option<String>,
    pub right_id: Option<String>,
    /// The ply at which the move trees first differ, if they do.
    pub first_divergence: Option<usize>,
    /// Moves present only on the left, at `first_divergence`.
    pub moves_removed: Vec<String>,
    pub moves_added: Vec<String>,
    /// Arrows and circles: `(shape, side)` pairs that are not on both sides.
    pub shapes_left_only: Vec<String>,
    pub shapes_right_only: Vec<String>,
    /// Annotations that differ, as `ply: comment` lines.
    pub annotations_left_only: Vec<String>,
    pub annotations_right_only: Vec<String>,
    pub fields: Vec<FieldChange>,
    pub notes: Option<TextDiff>,
}

/// A markdown or text difference, as lines.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TextDiff {
    pub removed: Vec<String>,
    pub added: Vec<String>,
}

impl TextDiff {
    pub fn is_empty(&self) -> bool {
        self.removed.is_empty() && self.added.is_empty()
    }
}

/// One chapter's completion, on each side.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProgressDiff {
    pub user: String,
    pub left_completed: bool,
    pub right_completed: bool,
    pub left_score: Option<f64>,
    pub right_score: Option<f64>,
}

/// The whole report.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiffReport {
    pub left: StudySummary,
    pub right: StudySummary,
    pub chapters: Vec<ChapterDiff>,
    pub fields: Vec<FieldChange>,
    /// True when the two sides have nothing worth reporting.
    pub identical: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StudySummary {
    pub id: String,
    pub title: String,
    pub lifecycle: String,
    pub version: i64,
    pub chapters: usize,
}

/// Compare two studies.
///
/// `for_user`, when given, adds the progress overlay to each chapter.
pub fn diff_studies(
    db: &Db,
    left: &str,
    right: &str,
    for_user: Option<&str>,
) -> Result<DiffReport> {
    let left_study = db
        .study(left)?
        .with_context(|| format!("study {left} not found"))?;
    let right_study = db
        .study(right)?
        .with_context(|| format!("study {right} not found"))?;

    let left_chapters = db.chapters(left)?;
    let right_chapters = db.chapters(right)?;
    let left_progress = for_user
        .map(|user| db.study_progress(left, user))
        .transpose()?
        .unwrap_or_default();
    let right_progress = for_user
        .map(|user| db.study_progress(right, user))
        .transpose()?
        .unwrap_or_default();

    let pairs = align_chapters(&left_chapters, &right_chapters);
    let mut chapters = Vec::new();
    for pair in pairs {
        let left_boards = match &pair.left {
            Some(chapter) => db.boards("study_chapter", &chapter.id)?,
            None => Vec::new(),
        };
        let right_boards = match &pair.right {
            Some(chapter) => db.boards("study_chapter", &chapter.id)?,
            None => Vec::new(),
        };
        chapters.push(diff_chapter(
            pair,
            &left_boards,
            &right_boards,
            for_user,
            &left_progress,
            &right_progress,
        ));
    }

    let fields = study_fields(&left_study, &right_study);
    let identical = fields.is_empty()
        && chapters.iter().all(|chapter| {
            chapter.change == Change::Unchanged
                && chapter
                    .boards_changed
                    .iter()
                    .all(|board| !board.change.is_a_change())
                && chapter.boards_added.is_empty()
                && chapter.boards_removed.is_empty()
                && chapter
                    .notes
                    .as_ref()
                    .map(TextDiff::is_empty)
                    .unwrap_or(true)
                && chapter
                    .progress
                    .as_ref()
                    .map(|p| p.left_completed == p.right_completed)
                    != Some(false)
        });

    Ok(DiffReport {
        left: summary(&left_study, left_chapters.len()),
        right: summary(&right_study, right_chapters.len()),
        chapters,
        fields,
        identical,
    })
}

fn summary(study: &rbc_db::Study, chapters: usize) -> StudySummary {
    StudySummary {
        id: study.id.clone(),
        title: study.title.clone(),
        lifecycle: study.lifecycle.clone(),
        version: study.current_version,
        chapters,
    }
}

fn study_fields(left: &rbc_db::Study, right: &rbc_db::Study) -> Vec<FieldChange> {
    let mut changes = Vec::new();
    let mut note = |field: &str, a: String, b: String| {
        if a != b {
            changes.push(FieldChange {
                field: field.to_string(),
                left: a,
                right: b,
            });
        }
    };
    note("title", left.title.clone(), right.title.clone());
    note(
        "description",
        left.description_md.clone(),
        right.description_md.clone(),
    );
    note("lifecycle", left.lifecycle.clone(), right.lifecycle.clone());
    note(
        "tags",
        tag_list(&left.tags_json),
        tag_list(&right.tags_json),
    );
    changes
}

fn tag_list(json: &str) -> String {
    serde_json::from_str::<Vec<String>>(json)
        .map(|tags| tags.join(", "))
        .unwrap_or_default()
}

/// One aligned pair of chapters.
struct ChapterPair<'a> {
    left: Option<&'a rbc_db::Chapter>,
    right: Option<&'a rbc_db::Chapter>,
}

/// Align chapters by id, then ordinal, then title.
fn align_chapters<'a>(
    left: &'a [rbc_db::Chapter],
    right: &'a [rbc_db::Chapter],
) -> Vec<ChapterPair<'a>> {
    let mut used_right = vec![false; right.len()];
    let mut pairs = Vec::new();

    // Pass one: the same id is the same chapter, whatever else changed.
    for chapter in left {
        if let Some((index, _)) = right
            .iter()
            .enumerate()
            .find(|(index, other)| !used_right[*index] && other.id == chapter.id)
        {
            used_right[index] = true;
            pairs.push(ChapterPair {
                left: Some(chapter),
                right: Some(&right[index]),
            });
        }
    }
    let left_matched: Vec<&str> = pairs
        .iter()
        .filter_map(|pair| pair.left.map(|chapter| chapter.id.as_str()))
        .collect();

    // Pass two: the same ordinal, for studies that were copied and re-saved.
    for chapter in left {
        if left_matched.contains(&chapter.id.as_str()) {
            continue;
        }
        if let Some((index, _)) = right
            .iter()
            .enumerate()
            .find(|(index, other)| !used_right[*index] && other.ordinal == chapter.ordinal)
        {
            used_right[index] = true;
            pairs.push(ChapterPair {
                left: Some(chapter),
                right: Some(&right[index]),
            });
        }
    }

    // Pass three: the same title, for a chapter that moved.
    for chapter in left {
        if pairs
            .iter()
            .any(|pair| pair.left.map(|c| c.id.as_str()) == Some(chapter.id.as_str()))
        {
            continue;
        }
        if let Some((index, _)) = right
            .iter()
            .enumerate()
            .find(|(index, other)| !used_right[*index] && other.title == chapter.title)
        {
            used_right[index] = true;
            pairs.push(ChapterPair {
                left: Some(chapter),
                right: Some(&right[index]),
            });
        }
    }

    // Whatever is left only exists on one side.
    for chapter in left {
        if !pairs
            .iter()
            .any(|pair| pair.left.map(|c| c.id.as_str()) == Some(chapter.id.as_str()))
        {
            pairs.push(ChapterPair {
                left: Some(chapter),
                right: None,
            });
        }
    }
    for (index, chapter) in right.iter().enumerate() {
        if !used_right[index] {
            pairs.push(ChapterPair {
                left: None,
                right: Some(chapter),
            });
        }
    }

    // Present the report in the left study's order, with additions last.
    pairs.sort_by_key(|pair| {
        pair.left
            .map(|chapter| chapter.ordinal)
            .unwrap_or(i64::MAX / 2)
    });
    pairs
}

fn diff_chapter(
    pair: ChapterPair<'_>,
    left_boards: &[rbc_db::Board],
    right_boards: &[rbc_db::Board],
    for_user: Option<&str>,
    left_progress: &[rbc_db::StudyProgress],
    right_progress: &[rbc_db::StudyProgress],
) -> ChapterDiff {
    let change = match (pair.left, pair.right) {
        (Some(_), None) => Change::Removed,
        (None, Some(_)) => Change::Added,
        (Some(left), Some(right)) => {
            if left.title != right.title
                || left.notes_md != right.notes_md
                || left.ordinal != right.ordinal
                || left.primary_board_id != right.primary_board_id
            {
                Change::Changed
            } else {
                Change::Unchanged
            }
        }
        (None, None) => Change::Unchanged,
    };

    let mut fields = Vec::new();
    if let (Some(left), Some(right)) = (pair.left, pair.right) {
        if left.title != right.title {
            fields.push(FieldChange {
                field: "title".into(),
                left: left.title.clone(),
                right: right.title.clone(),
            });
        }
        if left.ordinal != right.ordinal {
            fields.push(FieldChange {
                field: "ordinal".into(),
                left: left.ordinal.to_string(),
                right: right.ordinal.to_string(),
            });
        }
        if left.primary_board_id != right.primary_board_id {
            fields.push(FieldChange {
                field: "primary board".into(),
                left: left.primary_board_id.clone(),
                right: right.primary_board_id.clone(),
            });
        }
    }

    let board_pairs = align_boards(left_boards, right_boards);
    let mut boards_changed = Vec::new();
    let mut boards_removed = Vec::new();
    let mut boards_added = Vec::new();
    for board_pair in board_pairs {
        let diff = diff_board(board_pair);
        match diff.change {
            Change::Removed => boards_removed.push(diff.title.clone()),
            Change::Added => boards_added.push(diff.title.clone()),
            _ => boards_changed.push(diff),
        }
    }

    let notes = match (pair.left, pair.right) {
        (Some(left), Some(right)) => {
            let diff = line_diff(&left.notes_md, &right.notes_md);
            (!diff.is_empty()).then_some(diff)
        }
        (None, Some(right)) => Some(TextDiff {
            removed: Vec::new(),
            added: right.notes_md.lines().map(str::to_string).collect(),
        }),
        (Some(left), None) => Some(TextDiff {
            removed: left.notes_md.lines().map(str::to_string).collect(),
            added: Vec::new(),
        }),
        (None, None) => None,
    };

    let progress = for_user.map(|user| {
        let chapter_id_left = pair.left.map(|c| c.id.as_str());
        let chapter_id_right = pair.right.map(|c| c.id.as_str());
        let find = |rows: &[rbc_db::StudyProgress], id: Option<&str>| {
            id.and_then(|id| rows.iter().find(|row| row.chapter_id == id).cloned())
        };
        let left_row = find(left_progress, chapter_id_left);
        let right_row = find(right_progress, chapter_id_right);
        ProgressDiff {
            user: user.to_string(),
            left_completed: left_row.as_ref().map(|row| row.completed).unwrap_or(false),
            right_completed: right_row.as_ref().map(|row| row.completed).unwrap_or(false),
            left_score: left_row.and_then(|row| row.score),
            right_score: right_row.and_then(|row| row.score),
        }
    });

    ChapterDiff {
        change,
        left_id: pair.left.map(|chapter| chapter.id.clone()),
        right_id: pair.right.map(|chapter| chapter.id.clone()),
        title: pair
            .left
            .map(|chapter| chapter.title.clone())
            .or_else(|| pair.right.map(|chapter| chapter.title.clone()))
            .unwrap_or_default(),
        boards_removed,
        boards_added,
        boards_changed,
        fields,
        notes,
        progress,
    }
}

struct BoardPair<'a> {
    left: Option<&'a rbc_db::Board>,
    right: Option<&'a rbc_db::Board>,
}

/// Align boards by id, then title, then normalized FEN.
fn align_boards<'a>(left: &'a [rbc_db::Board], right: &'a [rbc_db::Board]) -> Vec<BoardPair<'a>> {
    let mut used = vec![false; right.len()];
    let mut pairs = Vec::new();

    let take = |pairs: &mut Vec<BoardPair<'a>>,
                used: &mut Vec<bool>,
                chapter: &'a rbc_db::Board,
                predicate: &dyn Fn(&rbc_db::Board) -> bool| {
        if let Some((index, other)) = right
            .iter()
            .enumerate()
            .find(|(index, other)| !used[*index] && predicate(other))
        {
            used[index] = true;
            pairs.push(BoardPair {
                left: Some(chapter),
                right: Some(other),
            });
            true
        } else {
            false
        }
    };

    for board in left {
        if take(&mut pairs, &mut used, board, &|other| other.id == board.id) {
            continue;
        }
        if take(&mut pairs, &mut used, board, &|other| {
            other.title == board.title
        }) {
            continue;
        }
        let _ = take(&mut pairs, &mut used, board, &|other| {
            normalize_fen(&other.start_fen) == normalize_fen(&board.start_fen)
        });
    }
    for board in left {
        if !pairs
            .iter()
            .any(|pair| pair.left.map(|b| b.id.as_str()) == Some(board.id.as_str()))
        {
            pairs.push(BoardPair {
                left: Some(board),
                right: None,
            });
        }
    }
    for (index, board) in right.iter().enumerate() {
        if !used[index] {
            pairs.push(BoardPair {
                left: None,
                right: Some(board),
            });
        }
    }
    pairs
}

/// A FEN without its halfmove and fullmove counters.
///
/// Two positions are the same position even if someone played them out three
/// moves later in another study.
pub fn normalize_fen(fen: &str) -> String {
    let parts: Vec<&str> = fen.split_whitespace().take(4).collect();
    parts.join(" ")
}

fn diff_board(pair: BoardPair<'_>) -> BoardDiff {
    let (change, title) = match (pair.left, pair.right) {
        (Some(_), None) => (
            Change::Removed,
            pair.left.map(|b| b.title.clone()).unwrap_or_default(),
        ),
        (None, Some(_)) => (
            Change::Added,
            pair.right.map(|b| b.title.clone()).unwrap_or_default(),
        ),
        (Some(_), Some(_)) => (
            Change::Unchanged,
            pair.left.map(|b| b.title.clone()).unwrap_or_default(),
        ),
        (None, None) => (Change::Unchanged, String::new()),
    };
    let mut diff = BoardDiff {
        change,
        title,
        left_id: pair.left.map(|board| board.id.clone()),
        right_id: pair.right.map(|board| board.id.clone()),
        first_divergence: None,
        moves_removed: Vec::new(),
        moves_added: Vec::new(),
        shapes_left_only: Vec::new(),
        shapes_right_only: Vec::new(),
        annotations_left_only: Vec::new(),
        annotations_right_only: Vec::new(),
        fields: Vec::new(),
        notes: None,
    };

    let left_content = pair.left.map(|board| content_of(&board.content_json));
    let right_content = pair.right.map(|board| content_of(&board.content_json));

    if let (Some(left), Some(right)) = (&left_content, &right_content) {
        // Move tree: the common prefix is the same game, the rest is the change.
        let left_moves = string_list(left, "moves");
        let right_moves = string_list(right, "moves");
        let common = left_moves
            .iter()
            .zip(right_moves.iter())
            .take_while(|(a, b)| a == b)
            .count();
        if common < left_moves.len() || common < right_moves.len() {
            diff.first_divergence = Some(common);
            diff.moves_removed = left_moves.iter().skip(common).cloned().collect();
            diff.moves_added = right_moves.iter().skip(common).cloned().collect();
            diff.change = Change::Changed;
        }

        let left_shapes = shapes_of(left);
        let right_shapes = shapes_of(right);
        for shape in &left_shapes {
            if !right_shapes.contains(shape) {
                diff.shapes_left_only.push(shape.clone());
            }
        }
        for shape in &right_shapes {
            if !left_shapes.contains(shape) {
                diff.shapes_right_only.push(shape.clone());
            }
        }

        let left_annotations = annotations_of(left);
        let right_annotations = annotations_of(right);
        for annotation in &left_annotations {
            if !right_annotations.contains(annotation) {
                diff.annotations_left_only.push(annotation.clone());
            }
        }
        for annotation in &right_annotations {
            if !left_annotations.contains(annotation) {
                diff.annotations_right_only.push(annotation.clone());
            }
        }

        let left_notes = left
            .get("notes_md")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let right_notes = right
            .get("notes_md")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let notes = line_diff(left_notes, right_notes);
        if !notes.is_empty() {
            diff.notes = Some(notes);
        }

        let left_title = pair
            .left
            .map(|board| board.title.clone())
            .unwrap_or_default();
        let right_title = pair
            .right
            .map(|board| board.title.clone())
            .unwrap_or_default();
        if left_title != right_title {
            diff.fields.push(FieldChange {
                field: "title".into(),
                left: left_title,
                right: right_title,
            });
        }
        let left_fen = pair
            .left
            .map(|board| board.start_fen.clone())
            .unwrap_or_default();
        let right_fen = pair
            .right
            .map(|board| board.start_fen.clone())
            .unwrap_or_default();
        if normalize_fen(&left_fen) != normalize_fen(&right_fen) {
            diff.fields.push(FieldChange {
                field: "start position".into(),
                left: left_fen,
                right: right_fen,
            });
        }

        let changed = !diff.moves_added.is_empty()
            || !diff.moves_removed.is_empty()
            || !diff.shapes_left_only.is_empty()
            || !diff.shapes_right_only.is_empty()
            || !diff.annotations_left_only.is_empty()
            || !diff.annotations_right_only.is_empty()
            || diff.notes.is_some()
            || !diff.fields.is_empty();
        if changed {
            diff.change = Change::Changed;
        }
    }

    diff
}

fn content_of(json: &str) -> Value {
    serde_json::from_str(json).unwrap_or(Value::Null)
}

fn string_list(content: &Value, key: &str) -> Vec<String> {
    content
        .get(key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Arrows and circles as comparable strings, so set membership is a `contains`.
fn shapes_of(content: &Value) -> Vec<String> {
    let mut shapes = Vec::new();
    if let Some(arrows) = content.get("arrows").and_then(|a| a.as_array()) {
        for arrow in arrows {
            if let (Some(from), Some(to)) = (
                arrow.get("from").and_then(|v| v.as_str()),
                arrow.get("to").and_then(|v| v.as_str()),
            ) {
                shapes.push(format!("arrow {from}-{to}"));
            }
        }
    }
    if let Some(circles) = content.get("circles").and_then(|c| c.as_array()) {
        for circle in circles {
            if let Some(square) = circle.get("square").and_then(|v| v.as_str()) {
                shapes.push(format!("circle {square}"));
            }
        }
    }
    shapes
}

fn annotations_of(content: &Value) -> Vec<String> {
    content
        .get("annotations")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let ply = item.get("ply").and_then(|v| v.as_i64())?;
                    let comment = item.get("comment").and_then(|v| v.as_str()).unwrap_or("");
                    let nag = item
                        .get("nag")
                        .and_then(|v| v.as_str())
                        .map(|nag| format!(" {nag}"))
                        .unwrap_or_default();
                    Some(format!("{ply}: {comment}{nag}"))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A line diff: the lines only on one side.
///
/// This is deliberately not a minimal edit script. A study's notes are prose, and
/// "these lines were removed, these were added" is what a reader needs; an
/// optimal shortest-edit-script would be harder to explain and no more useful.
pub fn line_diff(left: &str, right: &str) -> TextDiff {
    let left_lines: Vec<&str> = left.lines().collect();
    let right_lines: Vec<&str> = right.lines().collect();
    TextDiff {
        removed: left_lines
            .iter()
            .filter(|line| !right_lines.contains(line))
            .map(|line| line.to_string())
            .collect(),
        added: right_lines
            .iter()
            .filter(|line| !left_lines.contains(line))
            .map(|line| line.to_string())
            .collect(),
    }
}
