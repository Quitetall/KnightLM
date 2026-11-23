//! The Play AI tab: one screen to get a chess opponent that talks to you.
//!
//! Pick an opponent (the always-available test bot, or the KnightLM model, which this screen
//! can download and start for you), pick a colour, press Start, play on the board and chat.
//! Everything goes through the same match routes as the Play tab (`/api/ai/matches/...`) and
//! the managed runtime routes (`/api/knightlm/runtime/...`). Nothing here shows raw JSON:
//! every failure becomes a sentence, and the test bot is always offered as a way out.
//!
//! This file is self-contained on purpose: it is hooked into the app by one tab entry.

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::{Value, json};

use crate::ai_api;
use crate::board::Board;
use crate::i18n;
use crate::views::AppCtx;

const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const BOT_PROVIDER: &str = "local:rbc-test-bot";
const BOT_MODEL: &str = "rbc-test-bot";

const STYLE: &str = r#"
.playai { display: grid; gap: 12px; max-width: 760px; }
.playai h2 { margin: 0; }
.playai-setup, .playai-model { display: grid; gap: 8px; padding: 12px; border: 1px solid var(--border, #8884); border-radius: 8px; }
.playai-choices { display: flex; flex-wrap: wrap; gap: 8px; }
.playai-choice { padding: 8px 12px; border: 1px solid var(--border, #8886); border-radius: 8px; background: transparent; color: inherit; cursor: pointer; text-align: left; }
.playai-choice.on { border-color: #3b82f6; box-shadow: 0 0 0 2px #3b82f655; }
.playai-badge { display: inline-block; padding: 1px 8px; border-radius: 99px; background: #f59e0b33; border: 1px solid #f59e0b; font-size: 0.8em; margin-left: 6px; }
.playai-progress { width: 100%; height: 14px; }
.playai-error { color: #dc2626; }
.playai-log { list-style: none; margin: 0; padding: 0; max-height: 220px; overflow: auto; display: grid; gap: 4px; }
.playai-line.user { justify-self: end; background: #3b82f633; border-radius: 8px; padding: 4px 8px; }
.playai-line.assistant { justify-self: start; background: #8882; border-radius: 8px; padding: 4px 8px; }
.playai-compose { display: flex; gap: 8px; }
.playai-compose input { flex: 1; }
"#;

/// A failure as a sentence: a body that is JSON (or empty) never reaches the screen.
fn plain(error: &str) -> String {
    let text = error.trim();
    if text.is_empty() || text.starts_with('{') || text.starts_with('[') || text.starts_with('<') {
        "Something went wrong. Please try again.".to_string()
    } else {
        text.to_string()
    }
}

async fn runtime_call(method: &str, path: &str) -> Result<Value, String> {
    let request = if method == "POST" {
        gloo_net::http::Request::post(path)
    } else {
        gloo_net::http::Request::get(path)
    };
    let response = request.send().await.map_err(|e| e.to_string())?;
    let ok = response.ok();
    let text = response.text().await.map_err(|e| e.to_string())?;
    let value = serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
    if ok {
        Ok(value)
    } else {
        Err(value["error"].as_str().unwrap_or("").to_string())
    }
}

fn human() -> Value {
    json!({"kind": "human", "name": "You"})
}

fn state_of(status: &Value) -> &str {
    status["phase"]["state"].as_str().unwrap_or("")
}

fn mb(bytes: u64) -> String {
    format!("{}", bytes / 1_000_000)
}

pub fn play_ai(ctx: AppCtx) -> impl IntoView {
    let language = ctx.language;
    let status = RwSignal::new(Value::Null);
    // "bot" or "model"
    let opponent = RwSignal::new("bot");
    let color = RwSignal::new("white");
    let busy = RwSignal::new(false);
    let message = RwSignal::new(String::new());
    // After a model failure: offer the test bot as the way out.
    let offer_bot = RwSignal::new(false);
    let detail = RwSignal::new(None::<Value>);
    let chat_log = RwSignal::new(Vec::<(String, String)>::new());
    let chat_ctx = RwSignal::new((None::<String>, Vec::<Value>::new()));
    let chat_input = RwSignal::new(String::new());
    let marks = RwSignal::new(Vec::<brick_chess::Shape>::new());
    let polling = RwSignal::new(false);

    // Poll the runtime while something is in flight.
    let poll = move || {
        if polling.get_untracked() {
            return;
        }
        polling.set(true);
        spawn_local(async move {
            loop {
                match runtime_call("GET", "/api/knightlm/runtime").await {
                    Ok(value) => {
                        let active = matches!(
                            state_of(&value),
                            "downloading" | "verifying" | "setting_up" | "starting"
                        );
                        status.set(value);
                        if !active {
                            break;
                        }
                    }
                    Err(_) => break,
                }
                gloo_timers::future::TimeoutFuture::new(800).await;
            }
            polling.set(false);
        });
    };
    poll();

    let get_model = move || {
        message.set(String::new());
        spawn_local(async move {
            match runtime_call("POST", "/api/knightlm/runtime/install").await {
                Ok(value) => status.set(value),
                Err(error) => message.set(plain(&error)),
            }
            poll();
        });
    };

    let apply_turn = move |reply: &Value| {
        let tools = &reply["tools"];
        marks.set(crate::board::shapes_from_coach_marks(&tools["marks"]));
        if let Some(text) = tools["reply"].as_str().filter(|t| !t.trim().is_empty()) {
            chat_log.update(|log| log.push(("assistant".into(), text.to_string())));
        }
        let chat = &reply["chat"];
        if !chat.is_null() {
            chat_ctx.set((
                chat["summary"].as_str().map(str::to_owned),
                chat["recent"].as_array().cloned().unwrap_or_default(),
            ));
        }
        if let Some(next) = reply.get("match").filter(|m| !m.is_null()) {
            detail.set(Some(next.clone()));
        }
    };

    // The opponent's move (and remark), when it is the opponent's turn.
    let model_turn = move |id: String| {
        busy.set(true);
        spawn_local(async move {
            let (summary, history) = chat_ctx.get_untracked();
            let body = json!({"chat": true, "summary": summary, "history": history});
            match ai_api::match_action(&id, "model-move", body).await {
                Ok(reply) => apply_turn(&reply),
                Err(error) => {
                    message.set(plain(&error));
                    offer_bot.set(true);
                }
            }
            busy.set(false);
        });
    };

    let start = move || {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        message.set(String::new());
        offer_bot.set(false);
        chat_log.set(Vec::new());
        chat_ctx.set((None, Vec::new()));
        marks.set(Vec::new());
        detail.set(None);
        let choice = opponent.get_untracked();
        let mine = color.get_untracked();
        spawn_local(async move {
            let seat = if choice == "bot" {
                json!({"kind": "model", "provider": BOT_PROVIDER, "model": BOT_MODEL})
            } else {
                match runtime_call("POST", "/api/knightlm/runtime/start").await {
                    Ok(value) => {
                        let provider = value["provider_id"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        status.set(value);
                        json!({"kind": "model", "provider": format!("provider:{provider}"), "model": "knightlm"})
                    }
                    Err(error) => {
                        message.set(plain(&error));
                        offer_bot.set(true);
                        busy.set(false);
                        poll();
                        return;
                    }
                }
            };
            let (white, black) = if mine == "white" {
                (human(), seat)
            } else {
                (seat, human())
            };
            let config = json!({
                "start_fen": START_FEN, "white": white, "black": black,
                "max_plies": 300, "rated": false
            });
            let id = match ai_api::create_ai_match(config).await {
                Ok(value) => value["id"].as_str().unwrap_or_default().to_string(),
                Err(error) => {
                    message.set(plain(&error));
                    busy.set(false);
                    return;
                }
            };
            match ai_api::get_ai_match(&id).await {
                Ok(payload) => {
                    let opponent_first = mine == "black";
                    detail.set(Some(payload));
                    busy.set(false);
                    if opponent_first {
                        model_turn(id);
                    }
                }
                Err(error) => {
                    message.set(plain(&error));
                    busy.set(false);
                }
            }
        });
    };

    let play_move = move |uci: String| {
        let Some(id) = detail
            .get_untracked()
            .and_then(|d| d["id"].as_str().map(str::to_owned))
        else {
            return;
        };
        busy.set(true);
        message.set(String::new());
        spawn_local(async move {
            match ai_api::match_action(&id, "move", json!({"mv": uci})).await {
                Ok(value) => {
                    let next = value.get("match").cloned().unwrap_or(value);
                    let their_turn = next["result"].is_null()
                        && next["turn"]
                            .as_str()
                            .is_some_and(|turn| next[turn]["kind"] == "model");
                    detail.set(Some(next));
                    busy.set(false);
                    if their_turn {
                        model_turn(id);
                    }
                }
                Err(error) => {
                    message.set(plain(&error));
                    if let Ok(fresh) = ai_api::get_ai_match(&id).await {
                        detail.set(Some(fresh));
                    }
                    busy.set(false);
                }
            }
        });
    };

    let send_chat = move || {
        let text = chat_input.get_untracked().trim().to_string();
        let Some(id) = detail
            .get_untracked()
            .and_then(|d| d["id"].as_str().map(str::to_owned))
        else {
            return;
        };
        if text.is_empty() || busy.get_untracked() {
            return;
        }
        chat_input.set(String::new());
        chat_log.update(|log| log.push(("user".into(), text.clone())));
        busy.set(true);
        let (summary, history) = chat_ctx.get_untracked();
        spawn_local(async move {
            let body = json!({"message": text, "summary": summary, "history": history});
            match ai_api::match_action(&id, "chat", body).await {
                Ok(value) => {
                    if let Some(reply) = value["reply"].as_str() {
                        chat_log.update(|log| log.push(("assistant".into(), reply.to_string())));
                    }
                    let chat = &value["chat"];
                    if !chat.is_null() {
                        chat_ctx.set((
                            chat["summary"].as_str().map(str::to_owned),
                            chat["recent"].as_array().cloned().unwrap_or_default(),
                        ));
                    }
                }
                Err(error) => message.set(plain(&error)),
            }
            busy.set(false);
        });
    };

    let resign = move || {
        let Some(id) = detail
            .get_untracked()
            .and_then(|d| d["id"].as_str().map(str::to_owned))
        else {
            return;
        };
        let mine = color.get_untracked();
        spawn_local(async move {
            if let Ok(value) = ai_api::match_action(&id, "resign", json!({"color": mine})).await {
                let next = value.get("match").cloned().unwrap_or(value);
                if next.get("id").is_some() {
                    detail.set(Some(next));
                }
            }
        });
    };

    // The model panel: what the runtime is doing, in words.
    let model_panel = move || {
        let lang = language.get();
        let s = status.get();
        let phase = &s["phase"];
        let mut lines: Vec<String> = Vec::new();
        let mut progress: Option<(u64, u64)> = None;
        let mut can_get = false;
        match state_of(&s) {
            "not_installed" => {
                lines.push(i18n::t(&lang, "playai.get_size").to_string());
                can_get = true;
                if s["python"]["found"] == false && s["mode"] == "real" {
                    lines.push(i18n::t(&lang, "playai.no_python").to_string());
                }
            }
            "downloading" => {
                let done = phase["bytes"].as_u64().unwrap_or(0);
                let total = phase["total"].as_u64().unwrap_or(1).max(1);
                progress = Some((done, total));
                lines.push(i18n::tf(
                    &lang,
                    "playai.downloading",
                    &[("done", mb(done)), ("total", mb(total))],
                ));
            }
            "verifying" => lines.push(i18n::t(&lang, "playai.verifying").to_string()),
            "setting_up" => lines.push(i18n::t(&lang, "playai.setting_up").to_string()),
            "ready" => lines.push(i18n::t(&lang, "playai.ready").to_string()),
            "starting" => lines.push(i18n::t(&lang, "playai.starting").to_string()),
            "running" => lines.push(i18n::t(&lang, "playai.running").to_string()),
            "error" => {
                lines.push(plain(phase["message"].as_str().unwrap_or("")));
                can_get = true;
            }
            _ => lines.push(i18n::t(&lang, "playai.checking").to_string()),
        }
        if let Some(note) = s["device_note"].as_str().filter(|_| s["installed"] == true) {
            lines.push(note.to_string());
        }
        view! {
            <div class="playai-model" role="status">
                {lines.into_iter().map(|line| view! { <p>{line}</p> }).collect_view()}
                {progress.map(|(done, total)| view! {
                    <progress class="playai-progress" max=total.to_string() value=done.to_string()></progress>
                })}
                {can_get.then(|| view! {
                    <button class="playai-get" on:click=move |_| get_model()>
                        {move || i18n::t(&language.get(), "playai.get_model")}
                    </button>
                })}
            </div>
        }
    };

    view! {
        <section class="playai">
            <style>{STYLE}</style>
            <h2>{move || i18n::t(&language.get(), "playai.title")}</h2>
            <p class="muted">{move || i18n::t(&language.get(), "playai.intro")}</p>

            <div class="playai-setup">
                <strong>{move || i18n::t(&language.get(), "playai.opponent")}</strong>
                <div class="playai-choices">
                    <button
                        class=move || if opponent.get() == "bot" { "playai-choice playai-bot on" } else { "playai-choice playai-bot" }
                        on:click=move |_| opponent.set("bot")
                    >
                        <strong>{move || i18n::t(&language.get(), "playai.bot")}</strong>
                        <br />
                        <small>{move || i18n::t(&language.get(), "playai.bot_note")}</small>
                    </button>
                    <button
                        class=move || if opponent.get() == "model" { "playai-choice playai-knightlm on" } else { "playai-choice playai-knightlm" }
                        on:click=move |_| { opponent.set("model"); poll(); }
                    >
                        <strong>{move || i18n::t(&language.get(), "playai.model")}</strong>
                        <br />
                        <small>{move || i18n::t(&language.get(), "playai.model_note")}</small>
                    </button>
                </div>
                {move || (opponent.get() == "model").then(model_panel)}
                {move || (status.get()["mode"] == "fake").then(|| view! {
                    <p class="muted">{i18n::t(&language.get(), "playai.fake_note")}</p>
                })}
                <strong>{move || i18n::t(&language.get(), "playai.color")}</strong>
                <div class="playai-choices">
                    <button class=move || if color.get() == "white" { "playai-choice playai-white on" } else { "playai-choice playai-white" }
                        on:click=move |_| color.set("white")>
                        {move || i18n::t(&language.get(), "playai.white")}
                    </button>
                    <button class=move || if color.get() == "black" { "playai-choice playai-black on" } else { "playai-choice playai-black" }
                        on:click=move |_| color.set("black")>
                        {move || i18n::t(&language.get(), "playai.black")}
                    </button>
                </div>
                <div class="playai-choices">
                    <button
                        class="playai-start"
                        disabled=move || busy.get() || (opponent.get() == "model" && matches!(state_of(&status.get()), "not_installed" | "downloading" | "verifying" | "setting_up"))
                        on:click=move |_| start()
                    >
                        {move || i18n::t(&language.get(), if detail.get().is_some() { "playai.new_game" } else { "playai.start" })}
                    </button>
                    {move || offer_bot.get().then(|| view! {
                        <button class="playai-use-bot" on:click=move |_| {
                            opponent.set("bot");
                            offer_bot.set(false);
                            message.set(String::new());
                            start();
                        }>
                            {move || i18n::t(&language.get(), "playai.use_bot")}
                        </button>
                    })}
                </div>
                <p class="playai-error" role="alert">{move || message.get()}</p>
            </div>

            {move || {
                let Some(payload) = detail.get() else {
                    return ().into_any();
                };
                let lang = language.get();
                let turn = payload["turn"].as_str().unwrap_or("").to_string();
                let result = payload.get("result").filter(|r| !r.is_null()).cloned();
                let finished = result.is_some();
                let human_turn = payload[turn.as_str()]["kind"] == "human";
                let mine = ["white", "black"]
                    .into_iter()
                    .find(|c| payload[*c]["kind"] == "human")
                    .unwrap_or("white")
                    .to_string();
                let their = if mine == "white" { "black" } else { "white" };
                let is_bot = payload[their]["provider"] == BOT_PROVIDER;
                let fen = payload["fen"].as_str().unwrap_or(START_FEN).to_owned();
                let orientation = mine.clone();
                let status_line = if let Some(result) = &result {
                    format!(
                        "{} {}",
                        i18n::t(&lang, "playai.finished"),
                        result["score"].as_str().unwrap_or("")
                    )
                } else if human_turn {
                    i18n::t(&lang, "playai.your_turn").to_string()
                } else {
                    i18n::t(&lang, "playai.thinking").to_string()
                };
                let moves = payload["moves"]
                    .as_array()
                    .map(|moves| {
                        moves
                            .iter()
                            .filter_map(|m| m["san"].as_str())
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_default();
                view! {
                    <div class="playai-game">
                        <p class="playai-versus">
                            <strong>
                                {i18n::t(&lang, "playai.you")}
                                " vs "
                                {if is_bot { i18n::t(&lang, "playai.bot") } else { i18n::t(&lang, "playai.model") }}
                            </strong>
                            {is_bot.then(|| view! {
                                <span class="playai-badge">{i18n::t(&lang, "playai.badge_bot")}</span>
                            })}
                        </p>
                        <div class="playai-status" role="status">{status_line}</div>
                        <Board
                            pgn=Signal::derive(String::new)
                            load_fen=Signal::derive(move || Some((0, fen.clone(), "standard".into())))
                            orientation=Signal::derive(move || orientation.clone())
                            board_only=true
                            read_only=finished || !human_turn
                            interaction_disabled=busy.into()
                            shapes=Signal::derive(move || marks.get())
                            lang=language.into()
                            on_move=Callback::new(move |(uci, _, _): (String, String, String)| play_move(uci))
                        />
                        <p class="playai-moves">{moves}</p>
                        <h4>{i18n::t(&lang, "playai.chat")}</h4>
                        <ul class="playai-log" aria-live="polite">
                            {move || chat_log.get().into_iter().map(|(role, text)| view! {
                                <li class=format!("playai-line {role}")>{text}</li>
                            }).collect_view()}
                        </ul>
                        <div class="playai-compose">
                            <input class="playai-input" type="text"
                                placeholder=move || i18n::t(&language.get(), "playai.chat_placeholder")
                                prop:value=move || chat_input.get()
                                on:input=move |event| chat_input.set(event_target_value(&event))
                                on:keydown=move |event| { if event.key() == "Enter" { send_chat(); } } />
                            <button class="playai-send" disabled=move || busy.get() on:click=move |_| send_chat()>
                                {move || i18n::t(&language.get(), "playai.send")}
                            </button>
                            {(!finished).then(|| view! {
                                <button class="playai-resign" disabled=move || busy.get() on:click=move |_| resign()>
                                    {move || i18n::t(&language.get(), "playai.resign")}
                                </button>
                            })}
                        </div>
                    </div>
                }.into_any()
            }}
        </section>
    }
}
