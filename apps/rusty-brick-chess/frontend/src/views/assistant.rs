//! The Assistant tab: session rail, board, transcript + composer.
//!
//! This is the first functional slice of the assistant workspace described in
//! `30-assistant-ux.md`. It talks to the `/api/ai/*` routes and keeps its own
//! view state; the shared `AppCtx` only supplies the current language and the
//! position the analysis tab is showing.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::ai_api;
use crate::board::Board;
use crate::i18n;
use crate::model::line_pgn;
use crate::views::AppCtx;

const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// The only shared state the Assistant needs from the app.
#[derive(Clone, Copy)]
pub struct AssistantCtx {
    pub language: RwSignal<String>,
    pub fen: RwSignal<String>,
    pub selected_pgn: RwSignal<String>,
}

impl AssistantCtx {
    fn from_app(ctx: AppCtx) -> Self {
        Self {
            language: ctx.language,
            fen: ctx.fen,
            selected_pgn: ctx.selected_pgn,
        }
    }
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
        language,
        fen,
        selected_pgn,
    } = AssistantCtx::from_app(ctx);

    // Sessions and the selected conversation.
    let sessions = RwSignal::new(Vec::<ai_api::AiSessionInfo>::new());
    let selected = RwSignal::new(None::<String>);
    let turns = RwSignal::new(Vec::<ai_api::AiTurnInfo>::new());
    let draft = RwSignal::new(String::new());
    let sending = RwSignal::new(false);
    let error = RwSignal::new(String::new());
    // `(sha256, tokens)` from the last successful send.
    let last_ctx = RwSignal::new(None::<(String, u32)>);
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
    Effect::new(move |_| {
        spawn_local(async move {
            match ai_api::list_ai_sessions().await {
                Ok(list) => {
                    let first = list.first().map(|session| session.id.clone());
                    sessions.set(list);
                    if selected.get_untracked().is_none() {
                        selected.set(first);
                    }
                }
                Err(e) => error.set(e),
            }
        });
    });

    // Reload the transcript whenever the selected session changes.
    Effect::new(move |_| {
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
        let message = draft.get_untracked().trim().to_owned();
        if message.is_empty() || sending.get_untracked() {
            return;
        }
        let fen = fen.get_untracked();
        let fen = (!fen.trim().is_empty()).then_some(fen);
        draft.set(String::new());
        error.set(String::new());
        sending.set(true);
        spawn_local(async move {
            let result = ai_api::send_ai_message(&id, &message, fen.as_deref()).await;
            sending.set(false);
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
                        last_ctx.set(Some((context.sha256, context.tokens)));
                    }
                    if let Some(message) = response.error {
                        error.set(message);
                    }
                }
                Err(e) => error.set(e),
            }
        });
    };

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
                <div style="display:flex;flex-direction:column;gap:.3rem;overflow-y:auto;min-height:0;flex:1 1 auto;">
                    {move || {
                        sessions
                            .get()
                            .into_iter()
                            .map(|session| {
                                let id_class = session.id.clone();
                                let id_click = session.id.clone();
                                let title = session.title.clone();
                                let mode = session.mode.clone();
                                view! {
                                    <button
                                        class=move || {
                                            if selected.get().as_deref() == Some(id_class.as_str()) {
                                                "dock-tab active"
                                            } else {
                                                "dock-tab"
                                            }
                                        }
                                        style="width:100%;display:flex;justify-content:space-between;align-items:center;gap:.4rem;text-align:left;"
                                        title=title.clone()
                                        on:click=move |_| selected.set(Some(id_click.clone()))
                                    >
                                        <span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">
                                            {title.clone()}
                                        </span>
                                        <span class="muted">{mode}</span>
                                    </button>
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
                                let failure = turn
                                    .error_json
                                    .as_deref()
                                    .and_then(error_text);
                                let context = (!turn.context_sha256.is_empty()).then(|| {
                                    format!("ctx {}", sha8(&turn.context_sha256))
                                });
                                view! {
                                    <div class=class>
                                        <div>{turn.content.clone()}</div>
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
                                        {failure.map(|message| view! {
                                            <div class="chat-msg error">{message}</div>
                                        })}
                                    </div>
                                }
                            })
                            .collect_view()
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
                            <div class="muted">
                                {i18n::t(&language.get(), "assistant.sending")}
                            </div>
                        })
                    }}
                </div>
                {move || {
                    let message = error.get();
                    (!message.is_empty()).then(|| view! {
                        <div class="chat-msg error">{message}</div>
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
                <div class="chat-input">
                    <textarea
                        placeholder=move || i18n::t(&language.get(), "assistant.placeholder")
                        prop:value=move || draft.get()
                        on:input=move |ev| draft.set(event_target_value(&ev))
                        on:keydown=move |ev| {
                            if ev.key() == "Enter" && !ev.shift_key() {
                                ev.prevent_default();
                                send();
                            }
                        }
                    ></textarea>
                    <button
                        disabled=move || sending.get() || selected.get().is_none()
                        on:click=move |_| send()
                    >
                        {move || if sending.get() {
                            i18n::t(&language.get(), "assistant.sending")
                        } else {
                            i18n::t(&language.get(), "assistant.send")
                        }}
                    </button>
                </div>
                <div class="muted">
                    {move || i18n::t(&language.get(), "assistant.composer_hint")}
                </div>
            </aside>
        </section>
    }
}
