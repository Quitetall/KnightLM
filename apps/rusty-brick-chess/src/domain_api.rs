//! Workspace, board and study HTTP routes (schema v4 domain).

use axum::{
    Json,
    extract::{Path, Query, State},
    response::sse::{Event, KeepAlive, Sse},
};
use rbc_db::{Board, BoardWriteError, Chapter, Study, StudyProgress, StudyVersion, Workspace};
use serde::Deserialize;

use crate::api::{ApiError, AppState};

fn write_error(error: BoardWriteError) -> ApiError {
    match error {
        BoardWriteError::NotFound(id) => ApiError::BadRequest(format!("board {id} not found")),
        BoardWriteError::Conflict { current } => {
            ApiError::BadRequest(format!("board revision conflict (current {current})"))
        }
        BoardWriteError::CopyRequired => {
            ApiError::BadRequest("board is locked or published; copy it to edit".into())
        }
        BoardWriteError::Invalid(message) | BoardWriteError::Db(message) => {
            ApiError::Internal(anyhow::anyhow!(message))
        }
    }
}

#[derive(Deserialize)]
pub struct CreateWorkspace {
    pub title: String,
    #[serde(default = "default_workspace_kind")]
    pub kind: String,
}

fn default_workspace_kind() -> String {
    "analysis".into()
}

#[derive(Deserialize)]
pub struct UpdateWorkspace {
    pub title: Option<String>,
    pub active_board_id: Option<String>,
}

pub async fn list_workspaces(
    State(state): State<AppState>,
) -> Result<Json<Vec<Workspace>>, ApiError> {
    Ok(Json(state.db.workspaces(true)?))
}

pub async fn create_workspace(
    State(state): State<AppState>,
    Json(body): Json<CreateWorkspace>,
) -> Result<Json<Workspace>, ApiError> {
    Ok(Json(state.db.create_workspace(&body.title, &body.kind)?))
}

pub async fn get_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Workspace>, ApiError> {
    state
        .db
        .workspace(&id)?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

pub async fn update_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateWorkspace>,
) -> Result<Json<Workspace>, ApiError> {
    let active = body
        .active_board_id
        .as_deref()
        .map(|value| if value.is_empty() { None } else { Some(value) });
    Ok(Json(state.db.update_workspace(
        &id,
        body.title.as_deref(),
        active,
    )?))
}

pub async fn delete_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.db.archive_workspace(&id)?;
    Ok(Json(serde_json::json!({ "archived": true })))
}

#[derive(Deserialize)]
pub struct CreateBoard {
    pub parent_kind: String,
    pub parent_id: String,
    #[serde(default = "default_board_kind")]
    pub kind: String,
    pub title: String,
    pub start_fen: String,
    #[serde(default = "default_content")]
    pub content_json: String,
}

fn default_board_kind() -> String {
    "analysis".into()
}

fn default_content() -> String {
    "{}".into()
}

pub async fn list_boards(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Board>>, ApiError> {
    Ok(Json(state.db.boards("workspace", &id)?))
}

pub async fn create_board(
    State(state): State<AppState>,
    Json(body): Json<CreateBoard>,
) -> Result<Json<Board>, ApiError> {
    Ok(Json(state.db.create_board(
        &body.parent_kind,
        &body.parent_id,
        &body.kind,
        &body.title,
        &body.start_fen,
        &body.content_json,
    )?))
}

pub async fn get_board(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Board>, ApiError> {
    state.db.board(&id)?.map(Json).ok_or(ApiError::NotFound)
}

#[derive(Deserialize)]
pub struct UpdateBoard {
    pub expected_revision: i64,
    pub content_json: String,
    pub actor: Option<String>,
}

pub async fn update_board(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateBoard>,
) -> Result<Json<Board>, ApiError> {
    state
        .db
        .update_board_content(&id, body.expected_revision, &body.content_json, body.actor.as_deref())
        .map(Json)
        .map_err(write_error)
}

#[derive(Deserialize, Default)]
pub struct CopyBoard {
    pub parent_kind: Option<String>,
    pub parent_id: Option<String>,
    pub title: Option<String>,
}

pub async fn copy_board(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CopyBoard>,
) -> Result<Json<Board>, ApiError> {
    Ok(Json(state.db.copy_board(
        &id,
        body.parent_kind.as_deref(),
        body.parent_id.as_deref(),
        body.title.as_deref(),
    )?))
}

#[derive(Deserialize)]
pub struct ReparentBoard {
    pub parent_kind: String,
    pub parent_id: String,
    /// The revision the caller saw; a board that moved on is a conflict.
    pub expected_revision: i64,
}

/// Move a board to another parent.
///
/// One parent only: a board is in exactly one workspace or study chapter, and
/// moving it is a revision-checked write rather than a copy.
pub async fn reparent_board(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ReparentBoard>,
) -> Result<Json<Board>, ApiError> {
    let board = state
        .db
        .reparent_board(
            &id,
            &body.parent_kind,
            &body.parent_id,
            body.expected_revision,
        )
        .map_err(write_error)?;
    Ok(Json(board))
}

pub async fn lock_board(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Board>, ApiError> {
    Ok(Json(state.db.set_board_mutability(&id, "locked")?))
}

pub async fn unlock_board(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Board>, ApiError> {
    Ok(Json(state.db.set_board_mutability(&id, "mutable")?))
}

#[derive(Deserialize)]
pub struct CreateStudy {
    pub title: String,
    #[serde(default)]
    pub description_md: String,
}

pub async fn list_studies(State(state): State<AppState>) -> Result<Json<Vec<Study>>, ApiError> {
    Ok(Json(state.db.studies()?))
}

pub async fn create_study(
    State(state): State<AppState>,
    Json(body): Json<CreateStudy>,
) -> Result<Json<Study>, ApiError> {
    Ok(Json(state.db.create_study(&body.title, &body.description_md)?))
}

pub async fn get_study(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Study>, ApiError> {
    state.db.study(&id)?.map(Json).ok_or(ApiError::NotFound)
}

pub async fn list_chapters(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Chapter>>, ApiError> {
    Ok(Json(state.db.chapters(&id)?))
}

#[derive(Deserialize)]
pub struct CreateChapter {
    pub title: String,
    pub start_fen: String,
    #[serde(default)]
    pub notes_md: String,
}

pub async fn create_chapter(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CreateChapter>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (chapter, board) = state
        .db
        .add_chapter(&id, &body.title, &body.start_fen, &body.notes_md)?;
    Ok(Json(serde_json::json!({
        "chapter": chapter,
        "board": board,
    })))
}

#[derive(Deserialize)]
pub struct PublishStudy {
    pub expected_version: i64,
}

pub async fn publish_study(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PublishStudy>,
) -> Result<Json<StudyVersion>, ApiError> {
    Ok(Json(state.db.publish_study(&id, body.expected_version)?))
}

pub async fn copy_study(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Study>, ApiError> {
    Ok(Json(state.db.copy_study(&id)?))
}

#[derive(Deserialize)]
pub struct ProgressQuery {
    #[serde(default = "default_user")]
    pub user: String,
}

fn default_user() -> String {
    "local".into()
}

pub async fn get_progress(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ProgressQuery>,
) -> Result<Json<Vec<StudyProgress>>, ApiError> {
    Ok(Json(state.db.study_progress(&id, &query.user)?))
}

#[derive(Deserialize)]
pub struct ProgressBody {
    pub chapter_id: String,
    pub board_id: String,
    #[serde(default = "default_user")]
    pub user: String,
    #[serde(default)]
    pub completed: bool,
    #[serde(default)]
    pub attempts: i64,
    pub score: Option<f64>,
    #[serde(default)]
    pub hints: i64,
    pub mastery: Option<f64>,
}

pub async fn put_progress(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ProgressBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.db.set_study_progress(
        &id,
        &body.chapter_id,
        &body.board_id,
        &body.user,
        body.completed,
        body.attempts,
        body.score,
        body.hints,
        body.mastery,
    )?;
    Ok(Json(serde_json::json!({ "saved": true })))
}

#[derive(Deserialize)]
pub struct CreateAiSession {
    pub title: String,
    #[serde(default = "default_chat_mode")]
    pub mode: String,
    pub home_board_id: Option<String>,
    pub target_board_id: Option<String>,
    #[serde(default = "default_json_object")]
    pub model_json: String,
    #[serde(default = "default_json_object")]
    pub context_spec_json: String,
    #[serde(default = "default_json_object")]
    pub permissions_json: String,
}

fn default_chat_mode() -> String {
    "chat".into()
}

fn default_json_object() -> String {
    "{}".into()
}

pub async fn list_ai_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<rbc_db::AiSession>>, ApiError> {
    Ok(Json(state.db.ai_sessions()?))
}

pub async fn create_ai_session(
    State(state): State<AppState>,
    Json(body): Json<CreateAiSession>,
) -> Result<Json<rbc_db::AiSession>, ApiError> {
    Ok(Json(state.db.create_ai_session(
        &body.title,
        &body.mode,
        body.home_board_id.as_deref(),
        body.target_board_id.as_deref(),
        &body.model_json,
        &body.context_spec_json,
        &body.permissions_json,
    )?))
}

pub async fn get_ai_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<rbc_db::AiSession>, ApiError> {
    state.db.ai_session(&id)?.map(Json).ok_or(ApiError::NotFound)
}

#[derive(Deserialize)]
pub struct RenameAiSession {
    pub title: String,
}

pub async fn rename_ai_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<RenameAiSession>,
) -> Result<Json<rbc_db::AiSession>, ApiError> {
    Ok(Json(state.db.rename_ai_session(&id, &body.title)?))
}

#[derive(Deserialize)]
pub struct SetAiTarget {
    pub board_id: Option<String>,
}

pub async fn set_ai_target(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SetAiTarget>,
) -> Result<Json<rbc_db::AiSession>, ApiError> {
    Ok(Json(state.db.set_ai_session_target(&id, body.board_id.as_deref())?))
}

pub async fn archive_ai_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.db.archive_ai_session(&id)?;
    Ok(Json(serde_json::json!({"archived": true})))
}

pub async fn list_ai_turns(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<rbc_db::AiTurn>>, ApiError> {
    Ok(Json(state.db.ai_turns(&id)?))
}

#[derive(Deserialize)]
pub struct AppendAiTurn {
    pub role: String,
    #[serde(default)]
    pub content: String,
    pub reasoning: Option<String>,
    #[serde(default = "default_json_array")]
    pub tool_calls_json: String,
    #[serde(default = "default_json_array")]
    pub tool_results_json: String,
    #[serde(default = "default_json_object")]
    pub context_json: String,
    #[serde(default)]
    pub context_sha256: String,
    #[serde(default)]
    pub spec_version: String,
    #[serde(default)]
    pub template_version: String,
    #[serde(default)]
    pub tool_schema_version: String,
    pub engine_fingerprint: Option<String>,
    #[serde(default = "default_json_object")]
    pub model_json: String,
    #[serde(default)]
    pub tokens_in: i64,
    #[serde(default)]
    pub tokens_out: i64,
    #[serde(default)]
    pub latency_ms: i64,
    pub error_json: Option<String>,
}

fn default_json_array() -> String {
    "[]".into()
}

pub async fn append_ai_turn(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AppendAiTurn>,
) -> Result<Json<rbc_db::AiTurn>, ApiError> {
    let input = rbc_db::AiTurnInput {
        role: body.role,
        content: body.content,
        reasoning: body.reasoning,
        tool_calls_json: body.tool_calls_json,
        tool_results_json: body.tool_results_json,
        context_json: body.context_json,
        context_sha256: body.context_sha256,
        spec_version: body.spec_version,
        template_version: body.template_version,
        tool_schema_version: body.tool_schema_version,
        engine_fingerprint: body.engine_fingerprint,
        model_json: body.model_json,
        tokens_in: body.tokens_in,
        tokens_out: body.tokens_out,
        latency_ms: body.latency_ms,
        error_json: body.error_json,
    };
    Ok(Json(state.db.append_ai_turn(&id, &input)?))
}

#[derive(Deserialize)]
pub struct CompileContextBody {
    pub fen: Option<String>,
    #[serde(default)]
    pub history: Vec<String>,
    pub mode: Option<String>,
    pub side: Option<String>,
    pub variant: Option<String>,
    pub preset: Option<String>,
    pub token_budget: Option<u32>,
}

pub async fn compile_context(
    Json(body): Json<CompileContextBody>,
) -> Result<Json<rbc_context::CompiledContext>, ApiError> {
    let mut spec = match body.preset.as_deref().unwrap_or("chat-v1") {
        "play-v1" => rbc_context::ContextSpec::play_v1(),
        "rated-standard-v1" => rbc_context::ContextSpec::rated_standard_v1(),
        "training-v1" => rbc_context::ContextSpec::training_v1(),
        "chat-v1" => rbc_context::ContextSpec::chat_v1(),
        other => return Err(ApiError::BadRequest(format!("unknown preset {other}"))),
    };
    if let Some(budget) = body.token_budget {
        spec.token_budget = budget;
    }
    let mode = match body.mode.as_deref().unwrap_or("chat") {
        "chat" => rbc_context::Mode::Chat,
        "play" => rbc_context::Mode::Play,
        "watch" => rbc_context::Mode::Watch,
        other => return Err(ApiError::BadRequest(format!("unknown mode {other}"))),
    };
    let side = match body.side.as_deref() {
        Some("white") => Some(rbc_context::Side::White),
        Some("black") => Some(rbc_context::Side::Black),
        Some(other) => return Err(ApiError::BadRequest(format!("unknown side {other}"))),
        None => None,
    };
    let input = rbc_context::ContextInput {
        fen: body.fen,
        history: body.history,
        mode,
        side,
        variant: body.variant,
        ..Default::default()
    };
    Ok(Json(rbc_context::compile(&spec, &input)?))
}

#[derive(Deserialize)]
pub struct SendAiTurn {
    pub message: String,
    pub fen: Option<String>,
    pub depth: Option<u32>,
}

pub async fn send_ai_turn(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SendAiTurn>,
) -> Result<Json<serde_json::Value>, ApiError> {
    use rbc_ai::agent::{AgentLoop, TurnContext, TurnPlan};
    use rbc_ai::policy::PermissionProfile;
    use rbc_ai::protocol::{ModelRef, SessionMode};

    let session = state.db.ai_session(&id)?.ok_or(ApiError::NotFound)?;
    let settings = crate::api::effective(&state);
    let url = settings
        .model_url
        .clone()
        .ok_or(ApiError::Unavailable("model URL is not set"))?;
    let fen = match body.fen.filter(|value| !value.trim().is_empty()) {
        Some(fen) => fen,
        None => {
            let board_id = session
                .target_board_id
                .as_deref()
                .or(session.home_board_id.as_deref());
            match board_id {
                Some(board_id) => state
                    .db
                    .board(board_id)?
                    .map(|board| board.start_fen)
                    .unwrap_or_else(|| {
                        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".into()
                    }),
                None => "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".into(),
            }
        }
    };
    let spec = rbc_context::ContextSpec::chat_v1();
    let input = rbc_context::ContextInput {
        fen: Some(fen.clone()),
        ..Default::default()
    };
    let compiled = rbc_context::compile(&spec, &input)?;

    let user_turn = state.db.append_ai_turn(
        &id,
        &rbc_db::AiTurnInput {
            role: "user".into(),
            content: body.message.clone(),
            context_json: serde_json::to_string(&compiled.data).unwrap_or_default(),
            context_sha256: compiled.sha256.clone(),
            spec_version: format!("{}@{}", compiled.spec_id, compiled.spec_version),
            template_version: "rbc-prompt-v1".into(),
            tool_schema_version: "1".into(),
            model_json: session.model_json.clone(),
            ..Default::default()
        },
    )?;

    let model: ModelRef = serde_json::from_str(&session.model_json)
        .unwrap_or_else(|_| ModelRef::new("openai_compatible", "knightlm"));
    let permissions: PermissionProfile =
        serde_json::from_str(&session.permissions_json).unwrap_or_default();

    let mut plan = TurnPlan::chat(
        id.clone(),
        format!("turn-{}", user_turn.seq),
        model.clone(),
        TurnContext {
            system: compiled.prompt.clone(),
            context_json: serde_json::to_string(&compiled.data).unwrap_or_default(),
            context_sha256: compiled.sha256.clone(),
            spec_version: format!("{}@{}", compiled.spec_id, compiled.spec_version),
            template_version: "rbc-prompt-v1".into(),
            tool_schema_version: "1".into(),
            engine_fingerprint: None,
            tokens_estimate: compiled.tokens,
        },
        body.message.clone(),
    );
    plan.seq = user_turn.seq as u64;
    plan.mode = match session.mode.as_str() {
        "play" => SessionMode::Play,
        "watch" => SessionMode::Watch,
        _ => SessionMode::Chat,
    };
    plan.permissions = permissions;
    plan.history = history_messages(&state.db, &id, user_turn.seq);

    let db_for_turn = state.db.clone();
    let engine_path = settings.engine_path.clone();
    let plan_for_turn = plan.clone();
    // One token per running turn; `POST /cancel` finds it here.
    let cancel = rbc_ai::provider::Cancellation::new();
    let cancel_for_turn = cancel.clone();
    let session_key = id.clone();
    state
        .turn_cancels
        .lock()
        .expect("turn registry")
        .insert(session_key.clone(), cancel.clone());
    let outcome = tokio::task::spawn_blocking(
        move || -> Result<rbc_ai::agent::AgentOutcome, String> {
            let config = rbc_ai::provider::ProviderConfig::openai_compatible(
                &url,
                "openai_compatible",
                plan_for_turn.model.model.clone(),
            );
            let provider =
                rbc_ai::provider::OpenAiProvider::new(config).map_err(|error| error.message)?;
            let engine = engine_path
                .and_then(|path| crate::ai_host::LocalEngine::spawn(&path, Default::default()).ok());
            let engine_ref: Option<&dyn rbc_ai::reads::Engine> =
                engine.as_ref().map(|engine| engine as &dyn rbc_ai::reads::Engine);
            let mut host = crate::ai_host::SessionHost::new(
                &db_for_turn,
                "assistant",
                engine_ref,
                plan_for_turn.session_id.clone(),
                rbc_ai::policy::Origin::InApp,
            );
            let registry = rbc_ai::tools::ToolRegistry::builtin();
            let cancel = cancel_for_turn;
            // Each event is stored as it happens, so a client watching the
            // stream sees a turn as it runs rather than all at once when it
            // ends. Recording is best effort: a failing write must not lose
            // the answer itself.
            let log = rbc_ai::events::EventLog::new(&db_for_turn);
            let stream = rbc_ai::events::StreamKey::session(plan_for_turn.session_id.clone());
            let outcome = AgentLoop::new(&provider, &registry, &mut host).run(
                &plan_for_turn,
                &cancel,
                &mut |event| {
                    let _ = log.append(&stream, &event);
                },
            );
            Ok(outcome)
        },
    )
    .await
    .map_err(|error| ApiError::Internal(anyhow::anyhow!(error)))?;
    // The turn is over, whatever it returned: nothing can cancel it now.
    state
        .turn_cancels
        .lock()
        .expect("turn registry")
        .remove(&session_key);

    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(message) => {
            let error_json = serde_json::json!({
                "code": "provider_error",
                "message": message,
                "retryable": true,
            })
            .to_string();
            let assistant_turn = state.db.append_ai_turn(
                &id,
                &rbc_db::AiTurnInput {
                    role: "assistant".into(),
                    context_json: serde_json::to_string(&compiled.data).unwrap_or_default(),
                    context_sha256: compiled.sha256.clone(),
                    spec_version: format!("{}@{}", compiled.spec_id, compiled.spec_version),
                    template_version: "rbc-prompt-v1".into(),
                    tool_schema_version: "1".into(),
                    engine_fingerprint: None,
                    model_json: session.model_json.clone(),
                    error_json: Some(error_json),
                    ..Default::default()
                },
            )?;
            return Ok(Json(serde_json::json!({
                "user_turn": user_turn,
                "assistant_turn": assistant_turn,
                "error": message,
            })));
        }
    };

    // One assistant turn records the whole loop: final text, every tool call
    // and result, the summed usage and any terminal error.
    let calls: Vec<rbc_ai::protocol::ToolCall> = outcome
        .drafts
        .iter()
        .flat_map(|draft| draft.tool_calls.clone())
        .collect();
    let reply = outcome.reply();
    let assistant_turn = state.db.append_ai_turn(
        &id,
        &rbc_db::AiTurnInput {
            role: "assistant".into(),
            content: reply.clone(),
            reasoning: outcome
                .drafts
                .iter()
                .rev()
                .find_map(|draft| draft.reasoning.clone()),
            tool_calls_json: serde_json::to_string(&calls).unwrap_or_else(|_| "[]".into()),
            tool_results_json: serde_json::to_string(&outcome.tool_results)
                .unwrap_or_else(|_| "[]".into()),
            context_json: serde_json::to_string(&compiled.data).unwrap_or_default(),
            context_sha256: compiled.sha256.clone(),
            spec_version: format!("{}@{}", compiled.spec_id, compiled.spec_version),
            template_version: "rbc-prompt-v1".into(),
            tool_schema_version: "1".into(),
            engine_fingerprint: None,
            model_json: session.model_json.clone(),
            tokens_in: outcome.usage.tokens_in as i64,
            tokens_out: outcome.usage.tokens_out as i64,
            latency_ms: outcome.usage.latency_ms as i64,
            error_json: outcome
                .error
                .as_ref()
                .map(|error| serde_json::to_string(error).unwrap_or_default()),
        },
    )?;

    if reply.trim().is_empty() {
        if let Some(error) = &outcome.error {
            return Ok(Json(serde_json::json!({
                "user_turn": user_turn,
                "assistant_turn": assistant_turn,
                "error": error.message,
            })));
        }
    }

    Ok(Json(serde_json::json!({
        "reply": reply,
        "actions": [],
        "tools": outcome
            .tool_results
            .iter()
            .map(|result| result.tool.clone())
            .collect::<Vec<_>>(),
        "user_turn": user_turn,
        "assistant_turn": assistant_turn,
        "context": {
            "sha256": compiled.sha256,
            "spec_id": compiled.spec_id,
            "spec_version": compiled.spec_version,
            "tokens": compiled.tokens,
            "sections": compiled.sections,
        },
    })))
}

/// The conversation before a turn, as provider messages.
///
/// Only text is replayed: tool calls and their results live on the stored turn
/// for the transcript, and replaying them here would mean re-sending arguments
/// the model no longer has context for. The window is bounded so a long
/// session cannot grow the prompt without limit.
fn history_messages(
    db: &rbc_db::Db,
    session_id: &str,
    before_seq: i64,
) -> Vec<rbc_ai::provider::ChatMessage> {
    use rbc_ai::provider::ChatMessage;
    const HISTORY_TURNS: usize = 20;
    let Ok(turns) = db.ai_turns(session_id) else {
        return Vec::new();
    };
    let mut messages: Vec<ChatMessage> = turns
        .into_iter()
        .filter(|turn| turn.seq < before_seq && !turn.content.trim().is_empty())
        .filter_map(|turn| match turn.role.as_str() {
            "user" => Some(ChatMessage::user(turn.content)),
            "assistant" => Some(ChatMessage::assistant(turn.content)),
            _ => None,
        })
        .collect();
    if messages.len() > HISTORY_TURNS {
        messages.drain(..messages.len() - HISTORY_TURNS);
    }
    messages
}

pub async fn replay_ai_turn(
    State(state): State<AppState>,
    Path((id, seq)): Path<(String, i64)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = state.db.ai_session(&id)?.ok_or(ApiError::NotFound)?;
    let turn = state
        .db
        .ai_turns(&id)?
        .into_iter()
        .find(|turn| turn.seq == seq)
        .ok_or(ApiError::NotFound)?;
    let board_id = session
        .target_board_id
        .as_deref()
        .or(session.home_board_id.as_deref());
    let fen = match board_id {
        Some(board_id) => state
            .db
            .board(board_id)?
            .map(|board| board.start_fen)
            .unwrap_or_else(|| {
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".into()
            }),
        None => "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".into(),
    };
    let compiled = rbc_context::compile(
        &rbc_context::ContextSpec::chat_v1(),
        &rbc_context::ContextInput {
            fen: Some(fen),
            ..Default::default()
        },
    )?;
    let stored_context: serde_json::Value =
        serde_json::from_str(&turn.context_json).unwrap_or(serde_json::Value::Null);
    let diff = rbc_ai::context_diff::diff_contexts(&stored_context, &compiled.data);
    Ok(Json(serde_json::json!({
        "seq": seq,
        "stored_sha256": turn.context_sha256,
        "stored_spec": turn.spec_version,
        "diff": diff,
        "rebuilt_sha256": compiled.sha256,
        "rebuilt_spec": format!("{}@{}", compiled.spec_id, compiled.spec_version),
        "match": turn.context_sha256 == compiled.sha256,
        "tokens": compiled.tokens,
        "sections": compiled.sections,
    })))
}

use rbc_ai::game::{Match, MatchConfig, MatchSnapshot, Player};

#[derive(Deserialize)]
pub struct CreateMatch {
    pub config: MatchConfig,
    pub session_id: Option<String>,
}

fn match_payload(id: &str, game: &Match) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "state": game.state(),
        "fen": game.fen(),
        "turn": if game.turn() == shakmaty::Color::White { "white" } else { "black" },
        "moves": game.moves(),
        "legal_moves": game.legal_moves(),
        "result": game.result(),
        "clock": game.clock(),
        "pgn": game.pgn(),
    })
}

pub async fn create_match(
    State(state): State<AppState>,
    Json(body): Json<CreateMatch>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut game = Match::new(body.config.clone())?;
    // A timed match starts its clock when it is created, not on the first move:
    // otherwise the side to move would think for free.
    if game.clock().is_some() {
        game.tick(now_ms());
    }
    let config_json = serde_json::to_string(&body.config)?;
    let white_json = serde_json::to_string(&body.config.white)?;
    let black_json = serde_json::to_string(&body.config.black)?;
    let row = state.db.create_ai_match(
        body.session_id.as_deref(),
        &white_json,
        &black_json,
        &config_json,
    )?;
    let snapshot = game.snapshot();
    state.db.save_ai_match(
        &row.id,
        &serde_json::to_string(&snapshot)?,
        &game.pgn(),
        None,
        None,
        false,
    )?;
    Ok(Json(match_payload(&row.id, &game)))
}

pub async fn get_match(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let row = state.db.ai_match(&id)?.ok_or(ApiError::NotFound)?;
    let snapshot: MatchSnapshot = serde_json::from_str(&row.state_json)?;
    let game = Match::from_snapshot(snapshot)?;
    Ok(Json(match_payload(&row.id, &game)))
}

#[derive(Deserialize)]
pub struct MatchMoveBody {
    pub mv: String,
}

pub async fn play_match_move(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<MatchMoveBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let row = state.db.ai_match(&id)?.ok_or(ApiError::NotFound)?;
    let snapshot: MatchSnapshot = serde_json::from_str(&row.state_json)?;
    let mut game = Match::from_snapshot(snapshot)?;
    // A timed match is charged against the wall clock; an untimed one ignores
    // the timestamp entirely.
    if game.clock().is_some() {
        game.apply_at(&body.mv, now_ms())
            .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    } else {
        game.apply(&body.mv)
            .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    }
    let finished = game.result().is_some();
    let snapshot = game.snapshot();
    let result_json = game
        .result()
        .map(serde_json::to_string)
        .transpose()?;
    let termination = game.result().map(|result| result.termination.clone());
    state.db.save_ai_match(
        &id,
        &serde_json::to_string(&snapshot)?,
        &game.pgn(),
        result_json.as_deref(),
        termination.as_deref(),
        finished,
    )?;
    Ok(Json(match_payload(&id, &game)))
}

#[derive(Deserialize)]
pub struct ResignBody {
    pub color: String,
}

pub async fn resign_match(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ResignBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let row = state.db.ai_match(&id)?.ok_or(ApiError::NotFound)?;
    let snapshot: MatchSnapshot = serde_json::from_str(&row.state_json)?;
    let mut game = Match::from_snapshot(snapshot)?;
    let color = match body.color.as_str() {
        "white" => shakmaty::Color::White,
        "black" => shakmaty::Color::Black,
        other => return Err(ApiError::BadRequest(format!("unknown colour {other}"))),
    };
    // Charging the clock before resigning keeps the recorded time honest.
    if game.clock().is_some() {
        game.tick(now_ms());
    }
    game.resign(color);
    let snapshot = game.snapshot();
    let result_json = game
        .result()
        .map(serde_json::to_string)
        .transpose()?;
    state.db.save_ai_match(
        &id,
        &serde_json::to_string(&snapshot)?,
        &game.pgn(),
        result_json.as_deref(),
        Some("resignation"),
        true,
    )?;
    Ok(Json(match_payload(&id, &game)))
}

pub async fn match_pgn(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, ApiError> {
    let row = state.db.ai_match(&id)?.ok_or(ApiError::NotFound)?;
    Ok(row.pgn)
}

#[allow(dead_code)]
fn _players_are_serde(_p: &Player) {}

#[derive(Deserialize)]
pub struct StreamQuery {
    /// The last event id the client saw; 0 or absent replays from the start.
    #[serde(default)]
    pub after: Option<u64>,
}

/// `GET /api/ai/sessions/{id}/stream?after=<event id>`
///
/// Server-sent events carrying the normalized turn stream. The `id:` of each
/// event is its cursor, so a browser `EventSource` reconnects with
/// `Last-Event-ID` and only receives what it missed. Events are read from the
/// stored log rather than pushed from the running turn, which means a client
/// that connects late, or reconnects after a drop, sees exactly the same stream.
pub async fn stream_ai_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<StreamQuery>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>>, ApiError>
{
    if state.db.ai_session(&id)?.is_none() {
        return Err(ApiError::NotFound);
    }
    let db = state.db.clone();
    let (sender, receiver) = tokio::sync::mpsc::channel(64);
    let mut cursor = query.after.unwrap_or(0);

    tokio::spawn(async move {
        use rbc_ai::events::{EventLog, StreamKey};
        let stream = StreamKey::session(id);
        // Polling the log keeps this stateless: the turn being streamed is one
        // request on another connection, and a finished turn needs no live
        // channel to exist any more.
        let mut quiet_polls = 0u32;
        loop {
            match EventLog::new(&db).replay(&stream, cursor) {
                Ok(events) if !events.is_empty() => {
                    quiet_polls = 0;
                    for event in events {
                        cursor = event.event_id;
                        let data = match serde_json::to_string(&event) {
                            Ok(data) => data,
                            Err(_) => continue,
                        };
                        let item = Ok(Event::default()
                            .event("ai")
                            .id(cursor.to_string())
                            .data(data));
                        if sender.send(item).await.is_err() {
                            return; // the client went away
                        }
                    }
                }
                Ok(_) => {
                    quiet_polls += 1;
                    if quiet_polls % 20 == 0 {
                        let keep = Ok(Event::default().comment("keep-alive"));
                        if sender.send(keep).await.is_err() {
                            return;
                        }
                    }
                    // About two minutes of silence closes an abandoned tab.
                    if quiet_polls > 480 {
                        return;
                    }
                }
                Err(_) => return,
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    });

    Ok(Sse::new(tokio_stream::wrappers::ReceiverStream::new(receiver))
        .keep_alive(KeepAlive::default()))
}


/// The wall clock, in Unix milliseconds.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

// --- approval queue --------------------------------------------------------

/// `GET /api/ai/approvals` — everything still actionable, oldest first.
pub async fn list_approvals(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let store = rbc_ai::approval::ApprovalStore::new(&state.db);
    let pending = store
        .pending(crate::ai_host::now_ms())
        .map_err(|error| ApiError::Internal(anyhow::anyhow!(error)))?;
    Ok(Json(serde_json::json!({"pending": pending})))
}

/// `GET /api/ai/approvals/{id}` — the status an MCP client polls.
pub async fn get_approval(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let store = rbc_ai::approval::ApprovalStore::new(&state.db);
    match store
        .get(&id)
        .map_err(|error| ApiError::Internal(anyhow::anyhow!(error)))?
    {
        Some(request) => Ok(Json(serde_json::json!(request))),
        None => Err(ApiError::NotFound),
    }
}

/// `POST /api/ai/approvals/{id}/approve`
pub async fn approve_approval(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    resolve_approval(&state, &id, true)
}

/// `POST /api/ai/approvals/{id}/deny`
pub async fn deny_approval(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    resolve_approval(&state, &id, false)
}

/// Answer a pending request, reporting a lost race by name.
fn resolve_approval(
    state: &AppState,
    id: &str,
    approve: bool,
) -> Result<Json<serde_json::Value>, ApiError> {
    use rbc_ai::approval::{ApprovalError, ApprovalStore};
    let store = ApprovalStore::new(&state.db);
    let now = crate::ai_host::now_ms();
    let resolved = if approve {
        store.approve(id, now)
    } else {
        store.deny(id, now)
    };
    match resolved {
        Ok(request) => {
            // Approving means "do it": the reviewed call is replayed here, with
            // the arguments the person saw. A refused write leaves the approval
            // approved (the decision stands) and reports why nothing was
            // written, so the answer is never a silent no-op.
            let result = if approve {
                execute_approved(&state.db, &request)
            } else {
                None
            };
            Ok(Json(serde_json::json!({
                "approval": request,
                "result": result,
            })))
        }
        Err(ApprovalError::NotFound(_)) => Err(ApiError::NotFound),
        Err(error @ ApprovalError::AlreadyResolved(_)) | Err(error @ ApprovalError::Expired) => {
            Err(ApiError::BadRequest(error.to_string()))
        }
    }
}

/// Replay the call an approval was granted for.
///
/// The tool and its arguments come from the stored changeset, not from the
/// request, so what runs is exactly what was reviewed. Board and study writes
/// go through the same host the agent loop uses, which means the revision
/// checks still apply: an approval for a board that moved since is a conflict,
/// not a blind overwrite.
fn execute_approved(
    db: &rbc_db::Db,
    request: &rbc_ai::protocol::ApprovalRequest,
) -> Option<rbc_ai::protocol::ToolResult> {
    let changeset = rbc_ai::approval::changeset_of(request)?;
    let arguments = changeset.arguments?;
    let call = rbc_ai::protocol::ToolCall::new(request.id.clone(), request.tool.clone(), arguments);
    let mut host = rbc_ai::board::BoardWriteHost::new(db, "approval");
    Some(rbc_ai::agent::ToolHost::call(&mut host, &call))
}


/// `POST /api/ai/sessions/{id}/cancel`
///
/// Stops the turn running for this session. The provider checks the token
/// before every round and before every tool call, so a cancelled turn stops
/// before its next write; the answer says whether a turn was actually running,
/// because cancelling nothing is not an error, it is a no-op.
pub async fn cancel_ai_turn(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cancel = state
        .turn_cancels
        .lock()
        .expect("turn registry")
        .get(&id)
        .cloned();
    let cancelled = match cancel {
        Some(cancel) => {
            cancel.cancel();
            true
        }
        None => false,
    };
    Ok(Json(serde_json::json!({
        "session_id": id,
        "cancelled": cancelled,
    })))
}
