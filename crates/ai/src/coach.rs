//! Coach-mode tool executors (docs/model/modes-and-tools.md section 3).
//!
//! A [`CoachSession`] holds the real game position and a separate scratch branch.
//! The tools never touch the real game: `scratch_set` and `scratch_step` move the
//! scratch board only, `show_game_move` is a pointer, and the mark tools are UI
//! effects returned as structured data ([`UiEffect`]) for the app to draw. Every call
//! is validated against the board and every failure is a typed [`CoachError`].
//!
//! The model's tool set is `tools::model_tool_specs(ModelMode::Coach)`, separate from
//! the app registry; this module only runs calls that set already accepted, and
//! refuses any other name itself ([`CoachError::NotCoachTool`]).

use std::fmt;

use knightlm_core::state::{GameRecord, GameState};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use shakmaty::Square;

use crate::tools::{ModelCall, ModelCallError, TOOL_CALL_CLOSE, TOOL_CALL_OPEN, parse_model_call};

/// Coach tool calls one turn may spend, bad requests included.
pub const COACH_CALL_BUDGET: usize = 32;
/// Arrows and highlights the model may have on the board at once.
pub const MAX_MARKS: usize = 16;
/// Plies `scratch_step` takes in one call.
pub const MAX_STEP_MOVES: usize = 8;

/// Mark colours, the `color` enum of the tool schemas. Green when omitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarkColor {
    Green,
    Red,
    Blue,
    Yellow,
}

impl MarkColor {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "green" => Some(Self::Green),
            "red" => Some(Self::Red),
            "blue" => Some(Self::Blue),
            "yellow" => Some(Self::Yellow),
            _ => None,
        }
    }
}

/// One thing the model drew on the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Mark {
    Arrow {
        from: String,
        to: String,
        color: MarkColor,
    },
    Square {
        square: String,
        color: MarkColor,
    },
}

/// What the app should do on screen. Structured data, never rendered here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub enum UiEffect {
    /// Point at a ply of the real game (0 is the start, n is after n moves).
    ShowPly { ply: usize, fen: String },
    /// A mark was added (or replaced an identical one).
    AddMark { mark: Mark },
    /// Every mark the model drew was removed.
    ClearMarks,
}

/// Why a coach call was refused. Each variant has a stable [`CoachError::code`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoachError {
    /// The name is not a coach tool (a play tool, an app tool, or nothing at all).
    NotCoachTool(String),
    /// The turn's [`COACH_CALL_BUDGET`] is spent.
    CallBudget,
    /// An argument is missing or has the wrong shape.
    BadArguments(String),
    /// `scratch_set` was given text that is not a valid position.
    BadFen(String),
    /// A `scratch_step` move is not playable on the scratch board.
    IllegalMove {
        index: usize,
        mv: String,
    },
    /// `scratch_step` takes 1 to [`MAX_STEP_MOVES`] moves.
    MoveCount(usize),
    /// `show_game_move` named a ply the game has not reached.
    BadPly {
        ply: u64,
        last: usize,
    },
    BadSquare(String),
    BadColor(String),
    /// An arrow from a square to itself.
    SameSquare(String),
    /// [`MAX_MARKS`] marks are already on the board.
    TooManyMarks,
}

impl CoachError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotCoachTool(_) => "not_coach_tool",
            Self::CallBudget => "call_budget",
            Self::BadArguments(_) => "bad_arguments",
            Self::BadFen(_) => "bad_fen",
            Self::IllegalMove { .. } => "illegal_move",
            Self::MoveCount(_) => "move_count",
            Self::BadPly { .. } => "bad_ply",
            Self::BadSquare(_) => "bad_square",
            Self::BadColor(_) => "bad_color",
            Self::SameSquare(_) => "same_square",
            Self::TooManyMarks => "too_many_marks",
        }
    }
}

fn shown(text: &str) -> String {
    text.chars().take(16).collect()
}

impl fmt::Display for CoachError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotCoachTool(name) => {
                write!(f, "`{}` is not a coach tool", shown(name))
            }
            Self::CallBudget => write!(f, "coach tool budget of {COACH_CALL_BUDGET} calls spent"),
            Self::BadArguments(why) => write!(f, "bad arguments: {why}"),
            Self::BadFen(why) => write!(f, "not a valid position: {why}"),
            Self::IllegalMove { index, mv } => write!(
                f,
                "move {} `{}` is not playable on the scratch board; {index} played, none kept",
                index + 1,
                shown(mv)
            ),
            Self::MoveCount(n) => {
                write!(f, "scratch_step takes 1 to {MAX_STEP_MOVES} moves, got {n}")
            }
            Self::BadPly { ply, last } => {
                write!(f, "ply {ply} is not in the game (0 to {last})")
            }
            Self::BadSquare(square) => write!(f, "`{}` is not a square", shown(square)),
            Self::BadColor(color) => write!(f, "`{}` is not a mark colour", shown(color)),
            Self::SameSquare(square) => write!(f, "an arrow needs two squares, got {square} twice"),
            Self::TooManyMarks => write!(f, "{MAX_MARKS} marks are already on the board"),
        }
    }
}

impl std::error::Error for CoachError {}

/// A successful coach call: rules facts for the model, and an optional UI effect.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoachReply {
    pub call: usize,
    pub result: Value,
    pub ui: Option<UiEffect>,
}

/// The real game plus the model's scratch branch and marks.
#[derive(Debug, Clone)]
pub struct CoachSession {
    record: GameRecord,
    real: GameState,
    scratch: GameState,
    marks: Vec<Mark>,
    shown_ply: Option<usize>,
    calls: usize,
}

impl CoachSession {
    /// Start from the real game. The scratch board starts at its current position.
    pub fn new(record: GameRecord) -> anyhow::Result<Self> {
        let real = GameState::replay(&record)?;
        Ok(Self {
            scratch: real.clone(),
            real,
            record,
            marks: Vec::new(),
            shown_ply: None,
            calls: 0,
        })
    }

    pub fn calls(&self) -> usize {
        self.calls
    }

    /// The real game position. No coach call changes it.
    pub fn real(&self) -> &GameState {
        &self.real
    }

    pub fn scratch(&self) -> &GameState {
        &self.scratch
    }

    /// Marks currently on the board, oldest first.
    pub fn marks(&self) -> &[Mark] {
        &self.marks
    }

    /// The ply the model last pointed at, if any.
    pub fn shown_ply(&self) -> Option<usize> {
        self.shown_ply
    }

    /// Run one call. The call is charged against the budget first, so a bad
    /// request spends it like a good one.
    pub fn execute(&mut self, call: &ModelCall) -> Result<CoachReply, CoachError> {
        if !matches!(
            call.name.as_str(),
            "scratch_set"
                | "scratch_step"
                | "show_game_move"
                | "mark_arrow"
                | "mark_square"
                | "clear_marks"
        ) {
            return Err(CoachError::NotCoachTool(call.name.clone()));
        }
        if self.calls >= COACH_CALL_BUDGET {
            return Err(CoachError::CallBudget);
        }
        self.calls += 1;
        let number = self.calls;
        let args = &call.arguments;
        let (result, ui) = match call.name.as_str() {
            "scratch_set" => (self.scratch_set(text(args, "fen")?)?, None),
            "scratch_step" => (self.scratch_step(&texts(args, "moves")?)?, None),
            "show_game_move" => self.show_game_move(args)?,
            "mark_arrow" => self.mark_arrow(args)?,
            "mark_square" => self.mark_square(args)?,
            _ => {
                self.marks.clear();
                (json!({"marks": 0}), Some(UiEffect::ClearMarks))
            }
        };
        Ok(CoachReply {
            call: number,
            result,
            ui,
        })
    }

    fn scratch_set(&mut self, fen: &str) -> Result<Value, CoachError> {
        let record = GameRecord {
            initial_fen: fen.to_owned(),
            moves: Vec::new(),
            history_complete: false,
            game_id: "coach-scratch".into(),
            source: "coach".into(),
            ..self.record.clone()
        };
        let state = GameState::replay(&record)
            .map_err(|err| CoachError::BadFen(err.root_cause().to_string()))?;
        self.scratch = state;
        Ok(describe(&self.scratch, 0))
    }

    fn scratch_step(&mut self, moves: &[String]) -> Result<Value, CoachError> {
        if moves.is_empty() || moves.len() > MAX_STEP_MOVES {
            return Err(CoachError::MoveCount(moves.len()));
        }
        // All or nothing: the scratch board only moves when every move is playable.
        let mut next = self.scratch.clone();
        for (index, mv) in moves.iter().enumerate() {
            next.play(mv).map_err(|_| CoachError::IllegalMove {
                index,
                mv: mv.clone(),
            })?;
        }
        self.scratch = next;
        Ok(describe(&self.scratch, moves.len()))
    }

    fn show_game_move(&mut self, args: &Value) -> Result<(Value, Option<UiEffect>), CoachError> {
        let last = self.record.moves.len();
        let ply = args.get("ply").and_then(Value::as_u64).ok_or_else(|| {
            CoachError::BadArguments("`ply` must be a non-negative integer".into())
        })?;
        if ply as usize > last {
            return Err(CoachError::BadPly { ply, last });
        }
        let ply = ply as usize;
        let mut prefix = self.record.clone();
        prefix.moves.truncate(ply);
        let state = GameState::replay(&prefix)
            .map_err(|err| CoachError::BadArguments(err.root_cause().to_string()))?;
        let fen = state.fen();
        self.shown_ply = Some(ply);
        let played = (ply > 0).then(|| self.record.moves[ply - 1].clone());
        Ok((
            json!({"ply": ply, "fen": fen, "move": played}),
            Some(UiEffect::ShowPly { ply, fen }),
        ))
    }

    fn add_mark(&mut self, mark: Mark) -> Result<(Value, Option<UiEffect>), CoachError> {
        // An identical mark replaces itself, so repeating a call cannot fill the board.
        self.marks.retain(|m| m != &mark);
        if self.marks.len() >= MAX_MARKS {
            return Err(CoachError::TooManyMarks);
        }
        self.marks.push(mark.clone());
        Ok((
            json!({"marks": self.marks.len()}),
            Some(UiEffect::AddMark { mark }),
        ))
    }

    fn mark_arrow(&mut self, args: &Value) -> Result<(Value, Option<UiEffect>), CoachError> {
        let from = square(text(args, "from")?)?;
        let to = square(text(args, "to")?)?;
        if from == to {
            return Err(CoachError::SameSquare(from));
        }
        let color = color(args)?;
        self.add_mark(Mark::Arrow { from, to, color })
    }

    fn mark_square(&mut self, args: &Value) -> Result<(Value, Option<UiEffect>), CoachError> {
        let square = square(text(args, "square")?)?;
        let color = color(args)?;
        self.add_mark(Mark::Square { square, color })
    }
}

fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str, CoachError> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| CoachError::BadArguments(format!("`{key}` must be a string")))
}

fn texts(args: &Value, key: &str) -> Result<Vec<String>, CoachError> {
    let items = args
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| CoachError::BadArguments(format!("`{key}` must be an array")))?;
    items
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| CoachError::BadArguments(format!("`{key}` must hold strings")))
        })
        .collect()
}

fn square(name: &str) -> Result<String, CoachError> {
    match name.parse::<Square>() {
        Ok(sq) if name.len() == 2 && name == sq.to_string() => Ok(sq.to_string()),
        _ => Err(CoachError::BadSquare(name.to_owned())),
    }
}

fn color(args: &Value) -> Result<MarkColor, CoachError> {
    match args.get("color") {
        None | Some(Value::Null) => Ok(MarkColor::Green),
        Some(Value::String(name)) => {
            MarkColor::parse(name).ok_or_else(|| CoachError::BadColor(name.clone()))
        }
        Some(_) => Err(CoachError::BadArguments("`color` must be a string".into())),
    }
}

/// Rules facts about a scratch position. No score, no evaluation.
fn describe(state: &GameState, played: usize) -> Value {
    let facts = state.facts();
    json!({
        "fen": state.fen(),
        "played": played,
        "in_check": facts.in_check,
        "legal_move_count": state.legal_moves().len(),
        "terminal_reason": facts.terminal_reason,
    })
}

/// A call result as the model sees it.
pub fn format_coach_result(outcome: &Result<CoachReply, CoachError>) -> String {
    let body = match outcome {
        Ok(reply) => json!({"call": reply.call, "ok": true, "result": reply.result}),
        Err(err) => json!({"ok": false, "error": {"code": err.code(), "message": err.to_string()}}),
    };
    format!("<tool_result>{body}</tool_result>")
}

/// Every `<tool_call>` block of coach text, in order, each parsed on its own.
/// A coach turn may hold several (unlike a play completion); an unclosed block
/// at the end is [`ModelCallError::NotOneCall`].
pub fn extract_tool_calls(text: &str) -> Vec<Result<ModelCall, ModelCallError>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(TOOL_CALL_OPEN) {
        let after = &rest[start..];
        match after.find(TOOL_CALL_CLOSE) {
            Some(end) => {
                let end = end + TOOL_CALL_CLOSE.len();
                out.push(parse_model_call(&after[..end]));
                rest = &after[end..];
            }
            None => {
                out.push(Err(ModelCallError::NotOneCall));
                break;
            }
        }
    }
    out
}

/// Coach text with the `<tool_call>` blocks removed.
pub fn strip_tool_calls(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find(TOOL_CALL_OPEN) {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        match after.find(TOOL_CALL_CLOSE) {
            Some(end) => rest = &after[end + TOOL_CALL_CLOSE.len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use knightlm_core::SCHEMA_VERSION;
    use knightlm_core::state::START_FEN;

    fn record(moves: &[&str]) -> GameRecord {
        GameRecord {
            schema_version: SCHEMA_VERSION,
            game_id: "g".into(),
            source: "test".into(),
            initial_fen: START_FEN.into(),
            history_complete: true,
            moves: moves.iter().map(|m| (*m).to_owned()).collect(),
            variant: "standard".into(),
        }
    }

    fn call(name: &str, arguments: Value) -> ModelCall {
        ModelCall {
            name: name.into(),
            arguments,
        }
    }

    fn session() -> CoachSession {
        CoachSession::new(record(&["e2e4", "e7e5"])).unwrap()
    }

    #[test]
    fn the_scratch_branch_never_changes_the_real_game() {
        let mut s = session();
        let real_fen = s.real().fen();
        let step = s
            .execute(&call("scratch_step", json!({"moves": ["g1f3", "b8c6"]})))
            .unwrap();
        assert_eq!(step.result["played"], 2);
        assert_ne!(s.scratch().fen(), real_fen);
        s.execute(&call(
            "scratch_set",
            json!({"fen": "8/8/8/8/8/8/8/k1K5 w - - 0 1"}),
        ))
        .unwrap();
        s.execute(&call("show_game_move", json!({"ply": 1})))
            .unwrap();
        assert_eq!(s.real().fen(), real_fen);
        assert_eq!(s.real().record().moves, vec!["e2e4", "e7e5"]);
    }

    #[test]
    fn scratch_step_is_all_or_nothing_and_typed() {
        let mut s = session();
        let before = s.scratch().fen();
        let err = s
            .execute(&call("scratch_step", json!({"moves": ["g1f3", "e2e4"]})))
            .unwrap_err();
        assert_eq!(
            err,
            CoachError::IllegalMove {
                index: 1,
                mv: "e2e4".into()
            }
        );
        assert_eq!(err.code(), "illegal_move");
        assert_eq!(s.scratch().fen(), before);
        let too_many = vec!["g1f3"; 9];
        assert_eq!(
            s.execute(&call("scratch_step", json!({"moves": too_many})))
                .unwrap_err(),
            CoachError::MoveCount(9)
        );
        assert_eq!(
            s.execute(&call("scratch_step", json!({"moves": []})))
                .unwrap_err(),
            CoachError::MoveCount(0)
        );
    }

    #[test]
    fn scratch_set_validates_the_fen() {
        let mut s = session();
        let ok = s
            .execute(&call("scratch_set", json!({"fen": START_FEN})))
            .unwrap();
        assert_eq!(ok.result["legal_move_count"], 20);
        let before = s.scratch().fen();
        for bad in ["", "nonsense", "8/8/8/8/8/8/8/8 w - - 0 1"] {
            let err = s
                .execute(&call("scratch_set", json!({"fen": bad})))
                .unwrap_err();
            assert_eq!(err.code(), "bad_fen", "{bad}");
        }
        assert_eq!(s.scratch().fen(), before);
    }

    #[test]
    fn show_game_move_points_at_a_real_ply_only() {
        let mut s = session();
        let r = s
            .execute(&call("show_game_move", json!({"ply": 1})))
            .unwrap();
        assert_eq!(r.result["move"], "e2e4");
        assert_eq!(
            r.ui,
            Some(UiEffect::ShowPly {
                ply: 1,
                fen: r.result["fen"].as_str().unwrap().into()
            })
        );
        assert_eq!(s.shown_ply(), Some(1));
        let r0 = s
            .execute(&call("show_game_move", json!({"ply": 0})))
            .unwrap();
        assert!(r0.result["move"].is_null());
        assert_eq!(
            s.execute(&call("show_game_move", json!({"ply": 3})))
                .unwrap_err(),
            CoachError::BadPly { ply: 3, last: 2 }
        );
        assert_eq!(
            s.execute(&call("show_game_move", json!({"ply": -1})))
                .unwrap_err()
                .code(),
            "bad_arguments"
        );
    }

    #[test]
    fn marks_are_structured_ui_effects() {
        let mut s = session();
        let a = s
            .execute(&call(
                "mark_arrow",
                json!({"from": "g1", "to": "f3", "color": "red"}),
            ))
            .unwrap();
        assert_eq!(
            a.ui,
            Some(UiEffect::AddMark {
                mark: Mark::Arrow {
                    from: "g1".into(),
                    to: "f3".into(),
                    color: MarkColor::Red
                }
            })
        );
        let q = s
            .execute(&call("mark_square", json!({"square": "e4"})))
            .unwrap();
        assert_eq!(
            serde_json::to_value(&q.ui).unwrap(),
            json!({"effect": "add_mark", "mark": {"kind": "square", "square": "e4", "color": "green"}})
        );
        assert_eq!(s.marks().len(), 2);
        // A repeat replaces itself.
        s.execute(&call("mark_square", json!({"square": "e4"})))
            .unwrap();
        assert_eq!(s.marks().len(), 2);
        let cleared = s.execute(&call("clear_marks", json!({}))).unwrap();
        assert_eq!(cleared.ui, Some(UiEffect::ClearMarks));
        assert!(s.marks().is_empty());
    }

    #[test]
    fn mark_arguments_are_checked_against_the_board() {
        let mut s = session();
        let bad = |s: &mut CoachSession, name: &str, args: Value| {
            s.execute(&call(name, args)).unwrap_err().code()
        };
        assert_eq!(
            bad(&mut s, "mark_square", json!({"square": "i9"})),
            "bad_square"
        );
        assert_eq!(
            bad(&mut s, "mark_square", json!({"square": "E4"})),
            "bad_square"
        );
        assert_eq!(
            bad(&mut s, "mark_square", json!({"square": "e44"})),
            "bad_square"
        );
        assert_eq!(
            bad(
                &mut s,
                "mark_square",
                json!({"square": "e4", "color": "pink"})
            ),
            "bad_color"
        );
        assert_eq!(
            bad(&mut s, "mark_arrow", json!({"from": "e4", "to": "e4"})),
            "same_square"
        );
        assert_eq!(
            bad(&mut s, "mark_arrow", json!({"from": "e4"})),
            "bad_arguments"
        );
        assert!(s.marks().is_empty());
        for file in 'a'..='h' {
            for rank in ['1', '2'] {
                let sq = format!("{file}{rank}");
                s.execute(&call("mark_square", json!({"square": sq})))
                    .unwrap();
            }
        }
        assert_eq!(s.marks().len(), MAX_MARKS);
        assert_eq!(
            bad(&mut s, "mark_square", json!({"square": "a3"})),
            "too_many_marks"
        );
    }

    #[test]
    fn other_tools_are_refused_and_the_budget_is_shared() {
        let mut s = session();
        for name in [
            "submit_move",
            "list_legal_moves",
            "analyze",
            "evaluate",
            "play_move",
            "nope",
        ] {
            assert_eq!(
                s.execute(&call(name, json!({"move": "e2e4"})))
                    .unwrap_err()
                    .code(),
                "not_coach_tool",
                "{name}"
            );
        }
        // Refused names do not spend the budget; bad requests do.
        assert_eq!(s.calls(), 0);
        for _ in 0..COACH_CALL_BUDGET {
            let _ = s.execute(&call("mark_square", json!({"square": "zz"})));
        }
        assert_eq!(
            s.execute(&call("clear_marks", json!({}))).unwrap_err(),
            CoachError::CallBudget
        );
    }

    #[test]
    fn extracts_every_block_and_strips_them() {
        let text = "Provisional: e2e4\nLook.<tool_call>{\"name\":\"mark_square\",\"arguments\":{\"square\":\"e4\"}}</tool_call> and <tool_call>{nope</tool_call>tail<tool_call>{";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].as_ref().unwrap().name, "mark_square");
        assert!(matches!(calls[1], Err(ModelCallError::Malformed(_))));
        assert_eq!(calls[2], Err(ModelCallError::NotOneCall));
        assert_eq!(strip_tool_calls(text), "Provisional: e2e4\nLook. and tail");
    }

    #[test]
    fn results_format_for_the_model() {
        let mut s = session();
        let ok = s.execute(&call("clear_marks", json!({})));
        assert_eq!(
            format_coach_result(&ok),
            "<tool_result>{\"call\":1,\"ok\":true,\"result\":{\"marks\":0}}</tool_result>"
        );
        let err = s.execute(&call("mark_square", json!({"square": "zz"})));
        assert!(format_coach_result(&err).contains("\"code\":\"bad_square\""));
    }
}
