//! The Assistant tab: session rail, board, transcript + composer.
//!
//! This is the first functional slice of the assistant workspace described in
//! `30-assistant-ux.md`. It talks to the `/api/ai/*` routes and keeps its own
//! view state; the shared `AppCtx` only supplies the current language and the
//! position the analysis tab is showing.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

use crate::ai_api;
use crate::board::Board;
use crate::i18n;
use crate::model::line_pgn;
use crate::views::AppCtx;

const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// The only shared state the Assistant needs from the app.
#[derive(Clone)]
pub struct AssistantCtx {
    pub assistant_draft: RwSignal<String>,
    pub language: RwSignal<String>,
    pub fen: RwSignal<String>,
    pub selected_pgn: RwSignal<String>,
    /// The conversation this tab is pinned to, when it is a pinned tab.
    pub pinned: Option<String>,
    /// Open a conversation in a tab of its own.
    pub pin_session: UnsyncCallback<String, ()>,
    /// Published back to the shell so the board strip can mark the target.
    pub ai_session_id: RwSignal<Option<String>>,
    pub ai_target_board: RwSignal<Option<String>>,
}

impl AssistantCtx {
    fn from_app(ctx: AppCtx) -> Self {
        let pinned = ctx
            .workspaces
            .get_untracked()
            .into_iter()
            .find(|ws| ws.key == ctx.active_ws.get_untracked())
            .and_then(|ws| ws.session_id);
        Self {
            assistant_draft: ctx.assistant_draft,
            language: ctx.language,
            fen: ctx.fen,
            selected_pgn: ctx.selected_pgn,
            pinned,
            pin_session: ctx.pin_session,
            ai_session_id: ctx.ai_session_id,
            ai_target_board: ctx.ai_target_board,
        }
    }
}

/// One tool call as a card: what ran, and what it answered.
#[derive(Clone, Debug)]
struct ToolCard {
    tool: String,
    ok: bool,
    code: String,
    summary: String,
}

/// Read a turn's tool results for the transcript cards.
fn tool_cards(json: &str) -> Vec<ToolCard> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(ToolCard {
                        tool: item.get("tool")?.as_str()?.to_string(),
                        ok: item.get("ok").and_then(|ok| ok.as_bool()).unwrap_or(false),
                        code: item
                            .get("code")
                            .and_then(|code| code.as_str())
                            .unwrap_or("")
                            .to_string(),
                        summary: item
                            .get("summary")
                            .and_then(|summary| summary.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Turn a raw failure into something a person can act on.
///
/// The two failures worth naming are the ones with a specific remedy: a prompt
/// past the model's window, and a tool that refused because the board is locked.
/// Everything else is passed through as the server said it.
fn explain(language: &str, raw: &str) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("context") || lower.contains("too long") || lower.contains("maximum") {
        return i18n::t(language, "assistant.context_too_large").to_string();
    }
    if lower.contains("locked") || lower.contains("published") {
        return i18n::t(language, "assistant.locked_board").to_string();
    }
    raw.to_string()
}

/// Extract the human-readable message from a stored `error_json` blob.
fn error_text(json: &str) -> Option<String> {
    if json.trim().is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    value
        .get("message")
        .and_then(|m| m.as_str())
        .map(str::to_owned)
        .or_else(|| Some(json.to_owned()))
}

/// Tool names from a turn's `tool_calls_json` array. Accepts the plain string
/// form the server stores today and the `{function:{name}}` form as a fallback.
fn tool_names(json: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.as_str()
                        .map(str::to_owned)
                        .or_else(|| {
                            item.get("function")
                                .and_then(|f| f.get("name"))
                                .and_then(|n| n.as_str())
                                .map(str::to_owned)
                        })
                        .or_else(|| item.get("name").and_then(|n| n.as_str()).map(str::to_owned))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn sha8(sha: &str) -> String {
    sha.chars().take(8).collect()
}

pub fn assistant(ctx: AppCtx) -> impl IntoView {
    let AssistantCtx {
        assistant_draft,
        language,
        fen,
        pin_session,
        ai_session_id,
        ai_target_board,
        pinned,
        selected_pgn,
    } = AssistantCtx::from_app(ctx);

    // Sessions and the selected conversation.
    let sessions = RwSignal::new(Vec::<ai_api::AiSessionInfo>::new());
    let selected = RwSignal::new(None::<String>);
    let turns = RwSignal::new(Vec::<ai_api::AiTurnInfo>::new());
    let draft = assistant_draft;
    let sending = RwSignal::new(false);
    let error = RwSignal::new(String::new());
    // --- composer state -------------------------------------------------
    // Attachments are references the model should see in the turn: a board, a
    // study, a game, a player. Nothing is uploaded; each is a label and a value
    // the message carries.
    let attachments = RwSignal::new(Vec::<(String, String, String)>::new());
    let palette = RwSignal::new(None::<&'static str>);
    let palette_index = RwSignal::new(0usize);
    let palette_items = RwSignal::new(Vec::<(String, String)>::new());
    let temperature = RwSignal::new(0.2f32);
    // Empty means "whatever the session is configured with".
    let model_override = RwSignal::new(String::new());
    let reference_source = RwSignal::new(Vec::<(String, String)>::new());
    // Whether anything is configured to answer: a provider registry row, or the
    // MODEL_URL this build was started with. Without one, the composer still
    // works and every send fails, so say so before it does.
    let has_provider = RwSignal::new(true);

    // --- composer helpers -----------------------------------------------
    // Slash commands are prompts the app ships, not model features: picking one
    // inserts text a person could have typed.
    let commands: Vec<(&'static str, &'static str)> = vec![
        ("/analyse", "assistant.cmd_analyse"),
        ("/plan", "assistant.cmd_plan"),
        ("/blunder", "assistant.cmd_blunder"),
        ("/explain", "assistant.cmd_explain"),
        ("/drills", "assistant.cmd_drills"),
    ];

    spawn_local(async move {
        let configured = api::list_providers()
            .await
            .map(|list| !list.is_empty())
            .unwrap_or(false);
        has_provider.set(configured);
    });

    // The reference list behind `@`: boards and studies, loaded once.
    spawn_local(async move {
        let mut refs = Vec::new();
        if let Ok(studies) = api::list_studies().await {
            for study in studies {
                refs.push((format!("study:{}", study.id), study.title));
            }
        }
        if let Ok(workspaces) = api::list_workspaces().await {
            for workspace in workspaces {
                if let Ok(boards) = api::list_boards(&workspace.id).await {
                    for board in boards {
                        refs.push((format!("board:{}", board.id), board.title));
                    }
                }
            }
        }
        reference_source.set(refs);
    });

    // Recompute the palette from what has just been typed.
    let refresh_palette = move |text: &str| {
        let trimmed = text.trim_start();
        if let Some(rest) = trimmed.strip_prefix('/').filter(|rest| !rest.contains(' ')) {
            let needle = rest.to_lowercase();
            let items: Vec<(String, String)> = commands
                .iter()
                .filter(|(command, _)| command.trim_start_matches('/').starts_with(&needle))
                .map(|(command, description)| {
                    (
                        command.to_string(),
                        format!(
                            "{command} — {}",
                            i18n::t(&language.get_untracked(), description)
                        ),
                    )
                })
                .collect();
            if items.is_empty() {
                palette.set(None);
            } else {
                palette_items.set(items);
                palette_index.set(0);
                palette.set(Some("slash"));
            }
            return;
        }
        if let Some(at) = trimmed.rfind('@') {
            let needle = trimmed[at + 1..].to_lowercase();
            if !needle.contains(' ') {
                let items: Vec<(String, String)> = reference_source
                    .get_untracked()
                    .into_iter()
                    .filter(|(_, label)| label.to_lowercase().contains(&needle))
                    .take(8)
                    .collect();
                if !items.is_empty() {
                    palette_items.set(items);
                    palette_index.set(0);
                    palette.set(Some("mention"));
                    return;
                }
            }
        }
        palette.set(None);
    };

    // Apply the highlighted entry: a command replaces the draft, a mention
    // becomes a chip and is removed from the text.
    let pick_palette = move |key: &str| {
        match palette.get_untracked() {
            Some("slash") => {
                let addition = match key {
                    "/analyse" => {
                        "Analyse this position: material, pawn structure, the three most forcing moves."
                    }
                    "/plan" => "What is each side's plan here, and what should I do next?",
                    "/blunder" => "Find the mistakes in this game and the better move at each.",
                    "/explain" => "Explain this position to a beginner.",
                    "/drills" => "Suggest practice that would help me in this kind of position.",
                    _ => "",
                };
                draft.set(addition.to_string());
            }
            Some("mention") => {
                let label = palette_items
                    .get_untracked()
                    .into_iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, label)| label)
                    .unwrap_or_else(|| key.to_string());
                let (kind, id) = key.split_once(':').unwrap_or(("ref", key));
                attachments.update(|list| list.push((kind.to_string(), label, id.to_string())));
                // Drop the half-typed `@query` from the draft.
                let text = draft.get_untracked();
                if let Some(at) = text.rfind('@') {
                    draft.set(text[..at].trim_end().to_string());
                }
            }
            _ => {}
        }
        palette.set(None);
    };

    // `(sha256, tokens)` from the last successful send.
    let last_ctx = RwSignal::new(None::<(String, u32)>);
    // Rail controls: a search filter and an inline rename.
    let query = RwSignal::new(String::new());
    let renaming = RwSignal::new(None::<String>);
    let rename_text = RwSignal::new(String::new());
    // The turn being streamed right now: its text, its reasoning and the tools
    // it has called so far.
    let live_text = RwSignal::new(String::new());
    let live_reasoning = RwSignal::new(String::new());
    let live_tools = RwSignal::new(Vec::<String>::new());
    let show_reasoning = RwSignal::new(false);
    // Bumped when a streamed turn finishes, to reload the stored transcript.
    let refresh = RwSignal::new(0u32);
    // The context inspector: the request that was compiled, and the result of
    // rebuilding it against today's position.
    let inspector_open = RwSignal::new(false);
    let inspector = RwSignal::new(None::<ai_api::AiCompiledContext>);
    let replay = RwSignal::new(None::<serde_json::Value>);
    // Pinned shows the exact context that was sent; live rebuilds it for the
    // position on the board right now, which is how drift becomes visible.
    let context_live = RwSignal::new(false);
    let live_context = RwSignal::new(None::<ai_api::AiCompiledContext>);
    // The approval queue: writes waiting for a person.
    let approvals_open = RwSignal::new(false);
    let approvals = RwSignal::new(Vec::<ai_api::ApprovalInfo>::new());
    let approvals_msg = RwSignal::new(String::new());
    let approvals_load_error = RwSignal::new(None::<String>);
    let refresh_approvals = RwSignal::new(0u64);
    // Scopes this session may take without asking, per origin.
    let in_app_grants = RwSignal::new(Vec::<String>::new());
    let source = StoredValue::new(None::<web_sys::EventSource>);
    let transcript = NodeRef::<leptos::html::Div>::new();

    // Keep the newest message in view as the transcript grows.
    Effect::new(move |_| {
        let _ = turns.get();
        let _ = error.get();
        if let Some(element) = transcript.get() {
            element.set_scroll_top(element.scroll_height());
        }
    });

    // Load the session rail once on mount. Restoring the first session keeps the
    // tab useful after a reload; an empty list shows the new-chat empty state.
    let pinned_for_load = pinned.clone();
    Effect::new(move |_| {
        // A pinned tab starts on its conversation; every other tab returns to
        // the one that was last open.
        let pinned = pinned_for_load.clone();
        spawn_local(async move {
            match ai_api::list_ai_sessions().await {
                Ok(list) => {
                    // Return to the conversation that was open, when it still
                    // exists; otherwise start from the newest one.
                    let remembered = pinned.or_else(crate::model::read_active_ai_session);
                    let restored = remembered
                        .filter(|id| list.iter().any(|session| &session.id == id))
                        .or_else(|| list.first().map(|session| session.id.clone()));
                    sessions.set(list);
                    if selected.get_untracked().is_none() {
                        selected.set(restored);
                    }
                }
                Err(e) => error.set(e),
            }
        });
    });

    // Remember the open conversation so a reload comes back to it.
    Effect::new(move |_| {
        crate::model::write_active_ai_session(selected.get().as_deref());
    });

    // Reload the transcript whenever the selected session changes, or when a
    // streamed turn tells us it has finished.
    Effect::new(move |_| {
        let _ = refresh.get();
        let Some(id) = selected.get() else {
            turns.set(Vec::new());
            return;
        };
        turns.set(Vec::new());
        error.set(String::new());
        last_ctx.set(None);
        spawn_local(async move {
            match ai_api::list_ai_turns(&id).await {
                // Ignore a late response for a session the user already left.
                Ok(list) if selected.get_untracked().as_deref() == Some(id.as_str()) => {
                    turns.set(list)
                }
                Ok(_) => {}
                Err(e) => error.set(e),
            }
        });
    });

    let new_chat = move || {
        if sending.get_untracked() {
            return;
        }
        let title = i18n::t(&language.get_untracked(), "assistant.new_chat").to_string();
        error.set(String::new());
        spawn_local(async move {
            match ai_api::create_ai_session(&title, "chat", None, None).await {
                Ok(session) => {
                    let id = session.id.clone();
                    sessions.update(|list| list.insert(0, session));
                    selected.set(Some(id));
                }
                Err(e) => error.set(e),
            }
        });
    };

    let send = move || {
        let Some(id) = selected.get_untracked() else {
            error.set(i18n::t(&language.get_untracked(), "assistant.no_session").to_string());
            return;
        };
        let mut message = draft.get_untracked().trim().to_owned();
        if message.is_empty() || sending.get_untracked() {
            return;
        }
        // Attachments travel in the message text, in a block the model can read
        // and a person can see in the transcript afterwards.
        let attached = attachments.get_untracked();
        if !attached.is_empty() {
            let refs = attached
                .iter()
                .map(|(kind, label, value)| format!("- {kind}: {label} ({value})"))
                .collect::<Vec<_>>()
                .join("\n");
            message = format!("{message}\n\nContext:\n{refs}");
        }
        let fen = fen.get_untracked();
        let fen = (!fen.trim().is_empty()).then_some(fen);
        draft.set(String::new());
        error.set(String::new());
        sending.set(true);
        live_text.set(String::new());
        live_reasoning.set(String::new());
        live_tools.set(Vec::new());

        // Watch the session's event stream while the turn runs. Events for
        // turns that are already stored are replayed too; the highest stored
        // sequence tells us which ones to ignore, so history is not rendered
        // twice.
        let known_seq = turns
            .get_untracked()
            .iter()
            .map(|turn| turn.seq as u64)
            .max()
            .unwrap_or(0);
        let mut live = false;
        if let Ok(stream) = web_sys::EventSource::new(&ai_api::stream_url(&id)) {
            let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
                move |event: web_sys::MessageEvent| {
                    let Some(data) = event.data().as_string() else {
                        return;
                    };
                    let Ok(value) = serde_json::from_str::<serde_json::Value>(&data) else {
                        return;
                    };
                    match value.get("type").and_then(|kind| kind.as_str()) {
                        Some("turn_started") => {
                            let seq = value.get("seq").and_then(|seq| seq.as_u64()).unwrap_or(0);
                            live = seq > known_seq;
                            if live {
                                live_text.set(String::new());
                                live_reasoning.set(String::new());
                                live_tools.set(Vec::new());
                            }
                        }
                        Some("content_delta") if live => {
                            if let Some(text) = value.get("text").and_then(|t| t.as_str()) {
                                live_text.update(|current| current.push_str(text));
                            }
                        }
                        Some("reasoning_delta") if live => {
                            if let Some(text) = value.get("text").and_then(|t| t.as_str()) {
                                live_reasoning.update(|current| current.push_str(text));
                            }
                        }
                        Some("tool_call") if live => {
                            if let Some(name) = value.get("name").and_then(|n| n.as_str()) {
                                live_tools.update(|list| list.push(name.to_string()));
                            }
                        }
                        Some("turn_done") if live => {
                            live = false;
                            source.with_value(|held| {
                                if let Some(stream) = held {
                                    stream.close();
                                }
                            });
                            refresh.update(|count| *count += 1);
                        }
                        _ => {}
                    }
                },
            );
            // The server names its events (`event: ai`), and a named event does
            // not reach `onmessage` — only a listener for that name does.
            let _ =
                stream.add_event_listener_with_callback("ai", on_message.as_ref().unchecked_ref());
            // The listener lives as long as the page; the stream is closed when
            // the turn finishes or the session changes.
            on_message.forget();
            source.set_value(Some(stream));
        }

        spawn_local(async move {
            let result = ai_api::send_ai_message(
                &id,
                &message,
                fen.as_deref(),
                Some(temperature.get_untracked()),
                {
                    let model = model_override.get_untracked();
                    if model.is_empty() { None } else { Some(model) }
                }
                .as_deref(),
            )
            .await;
            sending.set(false);
            // The turn may have queued a write while the panel stayed open.
            refresh_approvals.update(|revision| *revision += 1);
            source.with_value(|held| {
                if let Some(stream) = held {
                    stream.close();
                }
            });
            live_text.set(String::new());
            live_reasoning.set(String::new());
            live_tools.set(Vec::new());
            if selected.get_untracked().as_deref() != Some(id.as_str()) {
                return;
            }
            match result {
                Ok(response) => {
                    if let Some(turn) = response.user_turn {
                        turns.update(|list| list.push(turn));
                    }
                    if let Some(turn) = response.assistant_turn {
                        turns.update(|list| list.push(turn));
                    }
                    if let Some(context) = response.context {
                        last_ctx.set(Some((context.sha256.clone(), context.tokens)));
                        inspector.set(Some(ai_api::AiCompiledContext {
                            schema: String::new(),
                            spec_id: context.spec_id,
                            spec_version: context.spec_version,
                            tokenizer: String::new(),
                            sections: context.sections,
                            prompt: String::new(),
                            data: serde_json::Value::Null,
                            tokens: context.tokens,
                            sha256: context.sha256,
                        }));
                        replay.set(None);
                    }
                    if let Some(message) = response.error {
                        error.set(message);
                    }
                }
                Err(e) => error.set(e),
            }
        });
    };

    // Publish the session and its target so the board strip can mark the board
    // the assistant is writing to, even when it is not the one on screen.
    {
        Effect::new(move |_| {
            let id = selected.get();
            ai_session_id.set(id.clone());
            let Some(id) = id else {
                ai_target_board.set(None);
                return;
            };
            spawn_local(async move {
                if let Ok(session) = ai_api::get_ai_session(&id).await {
                    ai_target_board.set(session.target_board_id);
                }
            });
        });
    }

    // Read the session's auto-approve scopes when it changes.
    Effect::new(move |_| {
        let Some(id) = selected.get() else {
            in_app_grants.set(Vec::new());
            return;
        };
        spawn_local(async move {
            if let Ok(session) = ai_api::get_ai_session(&id).await {
                let profile: serde_json::Value =
                    serde_json::from_str(&session.permissions_json).unwrap_or_default();
                let scopes = profile
                    .get("in_app")
                    .and_then(|list| list.as_array())
                    .map(|list| {
                        list.iter()
                            .filter_map(|scope| scope.as_str().map(str::to_string))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                in_app_grants.set(scopes);
            }
        });
    });

    // Load on open and after requests that can change the pending queue.
    Effect::new(move |_| {
        let revision = refresh_approvals.get();
        if !approvals_open.get_untracked() {
            return;
        }
        spawn_local(async move {
            let result = ai_api::list_approvals().await;
            if refresh_approvals.try_get_untracked() != Some(revision) {
                return;
            }
            match result {
                Ok(list) => {
                    approvals.set(list);
                    approvals_load_error.set(None);
                }
                Err(error) => approvals_load_error.set(Some(error)),
            }
        });
    });

    // While the inspector is live, rebuild the context for the current board so
    // it can be compared with what was actually sent.
    Effect::new(move |_| {
        if !context_live.get() || !inspector_open.get() {
            return;
        }
        let fen = fen.get();
        let fen = (!fen.trim().is_empty()).then_some(fen);
        spawn_local(async move {
            match ai_api::compile_context(fen.as_deref(), "chat", None, "chat-v1").await {
                Ok(compiled) => live_context.set(Some(compiled)),
                // The message signal, not the error binding.
                Err(message) => error.set(message),
            }
        });
    });

    // The board shows the current position the rest of the app is looking at.
    // With no FEN yet (fresh restore straight into Assistant), fall back to the
    // selected analysis PGN and finally the start position.
    let board_pgn = Signal::derive(move || {
        let current = fen.get();
        if !current.trim().is_empty() {
            return line_pgn(&current, &[], "standard");
        }
        let pgn = selected_pgn.get();
        if !pgn.trim().is_empty() {
            return pgn;
        }
        line_pgn(START_FEN, &[], "standard")
    });
    let selected_title = move || {
        let current = selected.get();
        sessions
            .get()
            .into_iter()
            .find(|session| Some(session.id.as_str()) == current.as_deref())
            .map(|session| session.title)
            .unwrap_or_default()
    };

    view! {
        <section
            class="analysis"
            style="grid-template-columns:220px minmax(0,1fr) 360px;gap:1rem;height:100%;min-height:0;"
        >
            <aside class="engine-pane" style="min-height:0;overflow:hidden;">
                <div class="panel-head">
                    <h2>{move || i18n::t(&language.get(), "assistant.sessions")}</h2>
                </div>
                <button
                    style="padding:.45rem .7rem;background:#2f4a35;color:#eaf5ec;border:1px solid #3f5f45;border-radius:7px;cursor:pointer;"
                    on:click=move |_| new_chat()
                >
                    {move || i18n::t(&language.get(), "assistant.new_chat")}
                </button>
                <input
                    type="search"
                    placeholder=move || i18n::t(&language.get(), "assistant.search")
                    aria-label=move || i18n::t(&language.get(), "assistant.search")
                    prop:value=move || query.get()
                    on:input=move |event| query.set(event_target_value(&event))
                    style="padding:.35rem .5rem;border-radius:7px;border:1px solid #3f5f45;background:#1b2620;color:#eaf5ec;"
                />
                {move || match renaming.get() {
                    Some(id) => {
                        let id_save = id.clone();
                        let sessions = sessions;
                        view! {
                            <div style="display:flex;gap:.3rem;">
                                <input
                                    type="text"
                                    aria-label=move || i18n::t(&language.get(), "assistant.rename")
                                    prop:value=move || rename_text.get()
                                    on:input=move |event| rename_text.set(event_target_value(&event))
                                    style="flex:1 1 auto;min-width:0;padding:.3rem .4rem;border-radius:7px;border:1px solid #3f5f45;background:#1b2620;color:#eaf5ec;"
                                />
                                <button
                                    on:click=move |_| {
                                        let id = id_save.clone();
                                        let title = rename_text.get_untracked();
                                        if title.trim().is_empty() {
                                            return;
                                        }
                                        spawn_local(async move {
                                            if ai_api::rename_ai_session(&id, title.trim()).await.is_ok() {
                                                sessions.update(|list| {
                                                    if let Some(session) = list.iter_mut().find(|s| s.id == id) {
                                                        session.title = title.trim().to_string();
                                                    }
                                                });
                                            }
                                        });
                                        renaming.set(None);
                                    }
                                >
                                    {move || i18n::t(&language.get(), "assistant.save")}
                                </button>
                                <button on:click=move |_| renaming.set(None)>
                                    {move || i18n::t(&language.get(), "assistant.cancel")}
                                </button>
                            </div>
                        }.into_any()
                    }
                    None => ().into_any(),
                }}
                <div style="display:flex;flex-direction:column;gap:.3rem;overflow-y:auto;min-height:0;flex:1 1 auto;">
                    {move || {
                        let needle = query.get().trim().to_lowercase();
                        sessions
                            .get()
                            .into_iter()
                            .filter(|session| {
                                needle.is_empty()
                                    || session.title.to_lowercase().contains(&needle)
                                    || session.mode.to_lowercase().contains(&needle)
                            })
                            .map(|session| {
                                let id_class = session.id.clone();
                                let id_click = session.id.clone();
                                let title = session.title.clone();
                                let mode = session.mode.clone();
                                let id_rename = session.id.clone();
                                let id_archive = session.id.clone();
                                let id_pin = session.id.clone();
                                let title_for_rename = title.clone();
                                let sessions_for_archive = sessions;
                                let selected_for_archive = selected;
                                view! {
                                    <div style="display:flex;gap:.2rem;align-items:center;">
                                        <button
                                            class=move || {
                                                if selected.get().as_deref() == Some(id_class.as_str()) {
                                                    "dock-tab active"
                                                } else {
                                                    "dock-tab"
                                                }
                                            }
                                            style="flex:1 1 auto;min-width:0;display:flex;justify-content:space-between;align-items:center;gap:.4rem;text-align:left;"
                                            title=title.clone()
                                            on:click=move |_| selected.set(Some(id_click.clone()))
                                        >
                                            <span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">
                                                {title.clone()}
                                            </span>
                                            <span class="muted">{mode}</span>
                                        </button>
                                        <button
                                            title=move || i18n::t(&language.get(), "assistant.rename")
                                            aria-label=move || i18n::t(&language.get(), "assistant.rename")
                                            on:click=move |_| {
                                                rename_text.set(title_for_rename.clone());
                                                renaming.set(Some(id_rename.clone()));
                                            }
                                        >
                                            "✎"
                                        </button>
                                        <button
                                            title=move || i18n::t(&language.get(), "assistant.pin")
                                            aria-label=move || i18n::t(&language.get(), "assistant.pin")
                                            on:click=move |_| pin_session.run(id_pin.clone())
                                        >
                                            "⧉"
                                        </button>
                                        <button
                                            title=move || i18n::t(&language.get(), "assistant.archive")
                                            aria-label=move || i18n::t(&language.get(), "assistant.archive")
                                            on:click=move |_| {
                                                let id = id_archive.clone();
                                                spawn_local(async move {
                                                    if ai_api::archive_ai_session(&id).await.is_ok() {
                                                        sessions_for_archive.update(|list| list.retain(|s| s.id != id));
                                                        if selected_for_archive.get_untracked().as_deref() == Some(id.as_str()) {
                                                            let next = sessions_for_archive
                                                                .get_untracked()
                                                                .first()
                                                                .map(|s| s.id.clone());
                                                            selected_for_archive.set(next);
                                                        }
                                                    }
                                                });
                                            }
                                        >
                                            "×"
                                        </button>
                                    </div>
                                }
                            })
                            .collect_view()
                    }}
                    {move || {
                        sessions
                            .get()
                            .is_empty()
                            .then(|| view! {
                                <div class="muted">
                                    {i18n::t(&language.get(), "assistant.no_sessions")}
                                </div>
                            })
                    }}
                </div>
            </aside>

            <div
                class="board-pane"
                style="min-width:0;min-height:0;overflow-y:auto;display:flex;flex-direction:column;gap:.6rem;"
            >
                <div class="panel-head">
                    <h2>{move || i18n::t(&language.get(), "assistant.board")}</h2>
                    <span class="muted">{move || selected_title()}</span>
                </div>
                <Board
                    pgn=board_pgn
                    read_only=true
                    lang=Signal::derive(move || language.get())
                />
            </div>

            <aside class="engine-pane" style="min-height:0;overflow:hidden;">
                <div class="panel-head">
                    <h2>{move || i18n::t(&language.get(), "assistant.chat")}</h2>
                    <span class="muted">{move || selected_title()}</span>
                </div>
                <div class="chat-messages" aria-live="polite" style="min-height:0;" node_ref=transcript>
                    {move || {
                        turns
                            .get()
                            .into_iter()
                            .map(|turn| {
                                let class = if turn.role == "user" {
                                    "chat-msg user"
                                } else {
                                    "chat-msg assistant"
                                };
                                let tools = tool_names(&turn.tool_calls_json);
                                let cards = tool_cards(&turn.tool_results_json);
                                let failure = turn
                                    .error_json
                                    .as_deref()
                                    .and_then(error_text);
                                let context = (!turn.context_sha256.is_empty()).then(|| {
                                    format!("ctx {}", sha8(&turn.context_sha256))
                                });
                                let reasoning = turn
                                    .reasoning
                                    .clone()
                                    .filter(|text| !text.trim().is_empty());
                                view! {
                                    <div class=class>
                                        {reasoning.map(|text| view! {
                                            <details class="chat-reasoning">
                                                <summary>
                                                    {move || i18n::t(&language.get(), "assistant.reasoning")}
                                                </summary>
                                                <div class="muted">{text}</div>
                                            </details>
                                        })}
                                        <div>{turn.content.clone()}</div>
                                        {(!cards.is_empty()).then(|| view! {
                                            <div class="chat-tools">
                                                {cards
                                                    .into_iter()
                                                    .map(|card| {
                                                        let class = if card.ok {
                                                            "tool-card"
                                                        } else {
                                                            "tool-card failed"
                                                        };
                                                        view! {
                                                            <div class=class>
                                                                <span class="tool-chip">
                                                                    {format!("{} · {}", card.tool, card.code)}
                                                                </span>
                                                                <span class="muted">{card.summary}</span>
                                                            </div>
                                                        }
                                                    })
                                                    .collect_view()}
                                            </div>
                                        })}
                                        {(!tools.is_empty()).then(|| view! {
                                            <div class="chat-tools">
                                                {tools
                                                    .into_iter()
                                                    .map(|tool| view! {
                                                        <span class="tool-chip">{format!("tool: {tool}")}</span>
                                                    })
                                                    .collect_view()}
                                            </div>
                                        })}
                                        {context.map(|label| view! { <div class="muted">{label}</div> })}
                                        {(turn.tokens_out > 0).then(|| view! {
                                            <div class="muted">
                                                {format!("{} in · {} out", turn.tokens_in, turn.tokens_out)}
                                            </div>
                                        })}
                                        {failure.map(|message| view! {
                                            <div class="chat-msg error">{message}</div>
                                        })}
                                    </div>
                                }
                            })
                            .collect_view()
                    }}
                    {move || {
                        let text = live_text.get();
                        let reasoning = live_reasoning.get();
                        let tools = live_tools.get();
                        (!text.is_empty() || !tools.is_empty()).then(|| view! {
                            <div class="chat-msg assistant streaming">
                                {(!reasoning.is_empty()).then(|| view! {
                                    <details class="chat-reasoning" open=show_reasoning.get()>
                                        <summary>
                                            {move || i18n::t(&language.get(), "assistant.reasoning")}
                                        </summary>
                                        <div class="muted">{reasoning.clone()}</div>
                                    </details>
                                })}
                                <div>{text.clone()}</div>
                                {(!tools.is_empty()).then(|| view! {
                                    <div class="chat-tools">
                                        {tools
                                            .into_iter()
                                            .map(|tool| view! {
                                                <span class="tool-chip">{format!("tool: {tool}")}</span>
                                            })
                                            .collect_view()}
                                    </div>
                                })}
                            </div>
                        })
                    }}
                    {move || {
                        (turns.get().is_empty() && error.get().is_empty() && !sending.get())
                            .then(|| view! {
                                <div class="muted">
                                    {i18n::t(&language.get(), "assistant.no_turns")}
                                </div>
                            })
                    }}
                    {move || {
                        sending.get().then(|| view! {
                            <div class="muted" role="status" aria-live="polite">
                                {i18n::t(&language.get(), "assistant.sending")}
                            </div>
                        })
                    }}
                </div>
                {move || {
                    let message = error.get();
                    (!message.is_empty()).then(|| {
                        let shown = explain(&language.get_untracked(), &message);
                        view! {
                            <div class="chat-msg error" role="alert" aria-live="assertive">{shown}</div>
                        }
                    })
                }}
                <div class="muted">
                    {move || {
                        last_ctx
                            .get()
                            .map(|(sha, tokens)| format!("ctx {} · {tokens} tokens", sha8(&sha)))
                            .unwrap_or_else(|| {
                                i18n::t(&language.get(), "assistant.no_context").to_string()
                            })
                    }}
                </div>
                <label class="muted" style="display:flex;gap:.35rem;align-items:center;font-size:.8rem;">
                    <input
                        type="checkbox"
                        prop:checked=move || show_reasoning.get()
                        on:change=move |ev| show_reasoning.set(event_target_checked(&ev))
                    />
                    {move || i18n::t(&language.get(), "assistant.expand_reasoning")}
                </label>
                <div style="display:flex;gap:.4rem;align-items:center;">
                    <button
                        aria-expanded=move || if approvals_open.get() { "true" } else { "false" }
                        on:click=move |_| {
                            approvals_open.update(|open| *open = !*open);
                            if approvals_open.get_untracked() {
                                approvals_msg.set(String::new());
                                approvals_load_error.set(None);
                            }
                            refresh_approvals.update(|revision| *revision += 1);
                        }
                    >
                        {move || {
                            // The count is only known once the queue has been
                            // read; the label stays useful before that.
                            format!("{} ({})", i18n::t(&language.get(), "approvals.title"), approvals.get().len())
                        }}
                    </button>
                    <button on:click=move |_| inspector_open.update(|open| *open = !*open)>
                        {move || i18n::t(&language.get(), "assistant.context")}
                    </button>
                    {move || {
                        // Tracked: the button appears once the transcript has
                        // loaded a turn with a stored context.
                        let seq = turns
                            .get()
                            .iter()
                            .rev()
                            .find(|turn| turn.role == "assistant" && !turn.context_sha256.is_empty())
                            .map(|turn| turn.seq);
                        let Some(seq) = seq else { return ().into_any() };
                        let id = selected.get_untracked();
                        view! {
                            <button on:click=move |_| {
                                let Some(id) = id.clone() else { return };
                                let replay = replay;
                                spawn_local(async move {
                                    match ai_api::replay_ai_turn(&id, seq).await {
                                        Ok(value) => replay.set(Some(value)),
                                        Err(error) => replay.set(Some(serde_json::json!({"error": error}))),
                                    }
                                });
                            }>
                                {move || i18n::t(&language.get(), "assistant.replay")}
                            </button>
                        }.into_any()
                    }}
                </div>
                {move || approvals_open.get().then(|| view! {
                    <div class="approval-queue" style="max-height:200px;overflow-y:auto;font-size:.8rem;">
                        {move || {
                            let list = approvals.get();
                            if list.is_empty() {
                                return view! {
                                    <div class="muted">{move || i18n::t(&language.get(), "approvals.empty")}</div>
                                }.into_any();
                            }
                            list.into_iter().map(|approval| {
                                let id_approve = approval.id.clone();
                                let id_deny = approval.id.clone();
                                let tool = approval.tool.clone();
                                let summary = approval.changeset_summary.clone();
                                let notes = approval
                                    .changeset
                                    .as_ref()
                                    .and_then(|changeset| changeset.get("notes"))
                                    .and_then(|notes| notes.as_array())
                                    .map(|notes| {
                                        notes
                                            .iter()
                                            .filter_map(|note| note.as_str().map(str::to_string))
                                            .collect::<Vec<_>>()
                                    })
                                    .unwrap_or_default();
                                let approve = approvals;
                                let deny = approvals;
                                let msg = approvals_msg;
                                view! {
                                    <div class="approval-row">
                                        <div class="tool-chip">{tool}</div>
                                        <div>{summary}</div>
                                        {(!notes.is_empty()).then(|| view! {
                                            <div class="muted">{notes.join(" · ")}</div>
                                        })}
                                        <div style="display:flex;gap:.3rem;">
                                            <button on:click=move |_| {
                                                let id = id_approve.clone();
                                                let msg = msg;
                                                spawn_local(async move {
                                                    match ai_api::resolve_approval(&id, true).await {
                                                        Ok(report) => {
                                                            approve.update(|list| list.retain(|a| a.id != id));
                                                            refresh_approvals.update(|revision| *revision += 1);
                                                            let applied = report
                                                                .get("result")
                                                                .filter(|result| !result.is_null())
                                                                .map(|result| {
                                                                    let ok = result.get("ok").and_then(|ok| ok.as_bool()).unwrap_or(false);
                                                                    let summary = result.get("summary").and_then(|s| s.as_str()).unwrap_or("");
                                                                    if ok { format!("applied · {summary}") } else { format!("refused · {summary}") }
                                                                })
                                                                .unwrap_or_else(|| "approved".to_string());
                                                            msg.set(applied);
                                                        }
                                                        Err(error) => msg.set(error),
                                                    }
                                                });
                                            }>
                                                {move || i18n::t(&language.get(), "approvals.approve")}
                                            </button>
                                            <button on:click=move |_| {
                                                let id = id_deny.clone();
                                                spawn_local(async move {
                                                    match ai_api::resolve_approval(&id, false).await {
                                                        Ok(_) => {
                                                            deny.update(|list| list.retain(|a| a.id != id));
                                                            refresh_approvals.update(|revision| *revision += 1);
                                                        }
                                                        Err(error) => msg.set(error),
                                                    }
                                                });
                                            }>
                                                {move || i18n::t(&language.get(), "approvals.deny")}
                                            </button>
                                        </div>
                                    </div>
                                }
                            }).collect_view().into_any()
                        }}
                        {move || {
                            let list = approvals.get();
                            (!list.is_empty()).then(|| view! {
                                <button on:click=move |_| {
                                    let ids: Vec<String> = approvals.get_untracked().iter().map(|a| a.id.clone()).collect();
                                    let msg = approvals_msg;
                                    spawn_local(async move {
                                        let mut failed = 0;
                                        for id in ids {
                                            if ai_api::resolve_approval(&id, true).await.is_err() {
                                                failed += 1;
                                            }
                                        }
                                        refresh_approvals.update(|revision| *revision += 1);
                                        if failed > 0 {
                                            msg.set(format!("{failed} approval(s) could not be answered"));
                                        }
                                    });
                                }>
                                    {move || i18n::t(&language.get(), "approvals.approve_all")}
                                </button>
                            })
                        }}
                        <div class="auto-approve" style="display:flex;gap:.5rem;align-items:center;flex-wrap:wrap;">
                            <span class="muted">
                                {move || i18n::t(&language.get(), "approvals.auto")}
                            </span>
                            {["board-write", "study-content", "destructive"]
                                .into_iter()
                                .map(|scope| {
                                    let name = scope.to_string();
                                    let name_for_check = name.clone();
                                    let name_for_change = name.clone();
                                    view! {
                                        <label class="muted" style="display:flex;gap:.25rem;align-items:center;">
                                            <input
                                                type="checkbox"
                                                prop:checked=move || in_app_grants.get().iter().any(|s| s == &name_for_check)
                                                on:change=move |event| {
                                                    let granted = event_target_checked(&event);
                                                    let name = name_for_change.clone();
                                                    let Some(id) = selected.get_untracked() else { return };
                                                    let mut scopes = in_app_grants.get_untracked();
                                                    scopes.retain(|scope| scope != &name);
                                                    if granted {
                                                        scopes.push(name.clone());
                                                    }
                                                    in_app_grants.set(scopes.clone());
                                                    let msg = approvals_msg;
                                                    spawn_local(async move {
                                                        if let Err(error) = ai_api::set_ai_permissions(&id, &scopes, &[]).await {
                                                            msg.set(error);
                                                        }
                                                    });
                                                }
                                            />
                                            {name}
                                        </label>
                                    }
                                })
                                .collect_view()}
                        </div>
                        <div class="muted" role="status">
                            {move || approvals_load_error.get().unwrap_or_else(|| approvals_msg.get())}
                        </div>
                    </div>
                })}
                {move || inspector_open.get().then(|| {
                    view! {
                        <div class="context-inspector" style="max-height:220px;overflow-y:auto;font-size:.8rem;">
                            <div style="display:flex;gap:.35rem;align-items:center;">
                                <button on:click=move |_| context_live.set(false)>
                                    {move || i18n::t(&language.get(), "assistant.sent")}
                                </button>
                                <button on:click=move |_| context_live.set(true)>
                                    {move || i18n::t(&language.get(), "assistant.live")}
                                </button>
                                <span class="muted">
                                    {move || {
                                        if context_live.get() {
                                            i18n::t(&language.get(), "assistant.live_hint").to_string()
                                        } else {
                                            i18n::t(&language.get(), "assistant.sent_hint").to_string()
                                        }
                                    }}
                                </span>
                            </div>
                            {move || {
                                // Drift is only meaningful with both sides known.
                                let sent = inspector
                                    .get()
                                    .map(|context| context.sha256)
                                    .filter(|sha| !sha.is_empty());
                                let live = live_context
                                    .get()
                                    .map(|context| context.sha256)
                                    .filter(|sha| !sha.is_empty());
                                match (sent, live) {
                                    (Some(sent), Some(live)) if sent != live => Some(view! {
                                        <div class="context-drift">
                                            {move || i18n::t(&language.get(), "assistant.board_changed")}
                                        </div>
                                    }),
                                    _ => None,
                                }
                            }}
                            {move || {
                                let context = if context_live.get() {
                                    live_context.get()
                                } else {
                                    inspector.get()
                                };
                                context.map(|context| {
                                let sha = sha8(&context.sha256);
                                let tokens = context.tokens;
                                let canonical = context
                                    .sections
                                    .iter()
                                    .map(|section| format!("[{}]\n{}", section.id, section.content))
                                    .collect::<Vec<_>>()
                                    .join("\n\n");
                                view! {
                                    <div class="muted">{format!("{sha} · {tokens} tokens · {} sections", context.sections.len())}</div>
                                    <div class="context-sections">
                                        {context.sections.iter().map(|section| {
                                            let id = section.id.clone();
                                            let chars = section.content.chars().count();
                                            let truncated = section.truncated;
                                            let required = section.required;
                                            let included = section.included;
                                            view! {
                                                <div class=if included { "context-section" } else { "context-section skipped" }>
                                                    <span class="tool-chip">{id}</span>
                                                    <span class="muted">
                                                        {if truncated {
                                                            format!("{chars} chars · truncated")
                                                        } else {
                                                            format!("{chars} chars")
                                                        }}
                                                    </span>
                                                    {required.then(|| view! { <span class="muted">"required"</span> })}
                                                </div>
                                            }
                                        }).collect_view()}
                                    </div>
                                    <details>
                                        <summary>{move || i18n::t(&language.get(), "assistant.canonical")}</summary>
                                        <pre style="white-space:pre-wrap;">{canonical}</pre>
                                    </details>
                                }.into_any()
                                }).unwrap_or_else(|| view! {
                                    <div class="muted">{move || i18n::t(&language.get(), "assistant.no_context")}</div>
                                }.into_any())
                            }}
                            {move || replay.get().map(|value| {
                                let identical = value.get("diff")
                                    .and_then(|diff| diff.get("identical"))
                                    .and_then(|flag| flag.as_bool())
                                    .unwrap_or(false);
                                if identical {
                                    view! { <div class="muted">"replay · identical"</div> }.into_any()
                                } else {
                                    let changed = value.get("diff").cloned().unwrap_or(serde_json::Value::Null);
                                    view! {
                                        <div class="muted">"replay · differs"</div>
                                        <pre style="white-space:pre-wrap;">{changed.to_string()}</pre>
                                    }.into_any()
                                }
                            })}
                        </div>
                    }
                })}
                // Attachments ride in the turn as references, so the model sees
                // what "this board" means without an upload step.
                <div class="composer-attach">
                    <button
                        class="attach-fen"
                        on:click=move |_| {
                            // A FEN typed or pasted into the composer is the one the
                            // person means; the position on screen is the fallback.
                            let typed = draft.get_untracked();
                            let from_draft = typed
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .windows(4)
                                .find(|window| {
                                    window[0].contains('/')
                                        && matches!(window[1], "w" | "b")
                                        && window[2].chars().all(|c| "KQkq-".contains(c))
                                })
                                .map(|window| window.join(" "));
                            let position = from_draft.filter(|fen| !fen.trim().is_empty()).or_else(|| {
                                let current = fen.get_untracked();
                                (!current.trim().is_empty()).then_some(current)
                            });
                            match position {
                                Some(position) => {
                                    attachments.update(|list| {
                                        list.push(("fen".into(), "position".into(), position));
                                    });
                                    draft.set(String::new());
                                }
                                None => error.set(
                                    i18n::t(&language.get_untracked(), "assistant.no_position")
                                        .to_string(),
                                ),
                            }
                        }
                    >
                        {move || i18n::t(&language.get(), "assistant.attach_fen")}
                    </button>
                    <button
                        class="attach-pgn"
                        on:click=move |_| {
                            // The whole draft is taken as the game and removed from
                            // the question, so a pasted PGN does not become prose.
                            let text = draft.get_untracked();
                            if text.contains("[Event") || text.contains("1.") {
                                attachments.update(|list| {
                                    list.push(("pgn".into(), "pasted game".into(), text.clone()));
                                });
                                draft.set(String::new());
                            }
                        }
                    >
                        {move || i18n::t(&language.get(), "assistant.attach_pgn")}
                    </button>
                    <input
                        class="attach-image"
                        type="file"
                        accept="image/*"
                        on:change=move |ev| {
                            let Some(files) = ev.target().and_then(|t| {
                                wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlInputElement>(t).ok()
                            }).and_then(|input| input.files()) else {
                                return;
                            };
                            let Some(file) = files.get(0) else {
                                return;
                            };
                            let reader = web_sys::FileReader::new().ok();
                            let Some(reader) = reader else { return };
                            let reader_for_load = reader.clone();
                            let onload = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || {
                                if let Ok(result) = reader_for_load.result()
                                    && let Some(data) = result.as_string() {
                                        spawn_local(async move {
                                            match api::recognize_image(&data).await {
                                                Ok(vision) => {
                                                    attachments.update(|list| {
                                                        list.push((
                                                            "position".into(),
                                                            format!("from image ({:.0}%)", vision.confidence * 100.0),
                                                            vision.placement,
                                                        ));
                                                    });
                                                }
                                                Err(failure) => error.set(failure),
                                            }
                                        });
                                    }
                            });
                            reader.set_onload(Some(onload.as_ref().unchecked_ref()));
                            onload.forget();
                            let _ = reader.read_as_data_url(&file);
                        }
                    />
                </div>
                <div class="composer-chips">
                    {move || attachments.get().into_iter().enumerate().map(|(index, (kind, label, _))| {
                        view! {
                            <span class="context-chip" data-kind=kind.clone()>
                                {format!("{kind}: {label}")}
                                <button
                                    class="chip-remove"
                                    aria-label="remove attachment"
                                    on:click=move |_| attachments.update(|list| {
                                        if index < list.len() { list.remove(index); }
                                    })
                                >"×"</button>
                            </span>
                        }
                    }).collect_view()}
                </div>
                <div class="composer-palette-anchor">
                    {move || palette.get().map(|kind| {
                        let items = palette_items.get();
                        let index = palette_index.get();
                        view! {
                            <ul class=format!("composer-palette {kind}") role="listbox" id="composer-palette">
                                {items.into_iter().enumerate().map(|(i, (key, label))| {
                                    let selected_class = if i == index { "item selected" } else { "item" };
                                    view! {
                                        <li class=selected_class role="option" aria-selected=if i == index { "true" } else { "false" }
                                            data-key=key.clone()
                                            on:click={
                                                let key = key.clone();
                                                move |_| pick_palette(&key)
                                            }
                                        >{label}</li>
                                    }
                                }).collect_view()}
                            </ul>
                        }.into_any()
                    })}
                </div>
                {move || (!has_provider.get() && model_override.get() != super::play::KNIGHTLM_MODEL).then(|| view! {
                    <div class="provider-missing" role="status">
                        {move || i18n::t(&language.get(), "assistant.no_provider")}
                    </div>
                })}
                <div class="chat-input">
                    <textarea
                        class="composer"
                        role="combobox"
                        aria-expanded=move || palette.get().is_some().to_string()
                        aria-controls="composer-palette"
                        aria-autocomplete="list"
                        aria-label=move || i18n::t(&language.get(), "assistant.composer")
                        placeholder=move || i18n::t(&language.get(), "assistant.placeholder")
                        prop:value=move || draft.get()
                        on:input=move |ev| {
                            let text = event_target_value(&ev);
                            draft.set(text.clone());
                            refresh_palette(&text);
                        }
                        on:keydown=move |ev| {
                            match ev.key().as_str() {
                                "Enter" if !ev.shift_key() => {
                                    if palette.get_untracked().is_some() {
                                        // Enter accepts the highlighted command
                                        // rather than sending a half-typed one.
                                        ev.prevent_default();
                                        if let Some((key, _)) = palette_items
                                            .get_untracked()
                                            .get(palette_index.get_untracked())
                                            .cloned()
                                        {
                                            pick_palette(&key);
                                        }
                                    } else {
                                        ev.prevent_default();
                                        send();
                                    }
                                }
                                "Escape" => {
                                    if palette.get_untracked().is_some() {
                                        ev.prevent_default();
                                        palette.set(None);
                                    }
                                }
                                "ArrowDown" if palette.get_untracked().is_some() => {
                                    ev.prevent_default();
                                    let len = palette_items.get_untracked().len();
                                    if len > 0 {
                                        palette_index.update(|i| *i = (*i + 1) % len);
                                    }
                                }
                                "ArrowUp" if palette.get_untracked().is_some() => {
                                    ev.prevent_default();
                                    let len = palette_items.get_untracked().len();
                                    if len > 0 {
                                        palette_index.update(|i| *i = (*i + len - 1) % len);
                                    }
                                }
                                _ => {}
                            }
                        }
                    ></textarea>
                    <button
                        class="composer-send"
                        disabled=move || selected.get().is_none()
                        on:click=move |_| {
                            if sending.get_untracked() {
                                if let Some(id) = selected.get_untracked() {
                                    // Ask the server to stop the turn; the
                                    // stream closes when the turn reports done.
                                    spawn_local(async move {
                                        let _ = ai_api::cancel_ai_turn(&id).await;
                                    });
                                }
                            } else {
                                send();
                            }
                        }
                    >
                        {move || if sending.get() {
                            i18n::t(&language.get(), "assistant.stop")
                        } else {
                            i18n::t(&language.get(), "assistant.send")
                        }}
                    </button>
                </div>
                <div class="composer-controls">
                    <label class="composer-model">
                        {move || i18n::t(&language.get(), "assistant.model")}
                        <input
                            class="model-override"
                            placeholder=move || i18n::t(&language.get(), "assistant.model_auto")
                            prop:value=move || model_override.get()
                            on:input=move |ev| model_override.set(event_target_value(&ev))
                        />
                    </label>
                    <button type="button" class="composer-knightlm"
                        aria-pressed=move || if model_override.get() == super::play::KNIGHTLM_MODEL { "true" } else { "false" }
                        disabled=move || sending.get()
                        on:click=move |_| model_override.set(super::play::KNIGHTLM_MODEL.into())>
                        "KnightLM (LAMU)"
                    </button>
                    <button type="button" class="composer-play"
                        on:click=move |_| ctx.switch_tab.run(crate::model::Tab::Play)>
                        {move || i18n::t(&language.get(), "play.from_assistant")}
                    </button>
                    <label class="composer-temperature">
                        {move || i18n::t(&language.get(), "assistant.temperature")}
                        <input
                            class="temperature"
                            type="range"
                            min="0" max="2" step="0.1"
                            prop:value=move || temperature.get().to_string()
                            on:input=move |ev| {
                                if let Ok(value) = event_target_value(&ev).parse::<f32>() {
                                    temperature.set(value);
                                }
                            }
                        />
                        <output class="temperature-value">{move || format!("{:.1}", temperature.get())}</output>
                    </label>
                    <button
                        class="composer-regenerate"
                        disabled=move || sending.get() || selected.get().is_none()
                        on:click=move |_| {
                            // Regenerate asks again with the last thing a person
                            // said; the previous answer stays in the transcript.
                            let last_user = turns
                                .get_untracked()
                                .into_iter().rfind(|turn| turn.role == "user")
                                .map(|turn| turn.content);
                            if let Some(content) = last_user {
                                draft.set(content);
                                send();
                            }
                        }
                    >
                        {move || i18n::t(&language.get(), "assistant.regenerate")}
                    </button>
                </div>
                <div class="muted">
                    {move || i18n::t(&language.get(), "assistant.composer_hint")}
                </div>
            </aside>
        </section>
    }
}
