use brick_chess::pieces::{piece_markup, PieceSet};
use brick_chess::scene::{index_to_name, name_to_index, Color, Kind};
use brick_chess::{Command, Widget};
use leptos::prelude::*;

#[derive(Clone, Copy)]
struct Drag {
    from: usize,
    kind: Kind,
    color: Color,
    x: f64,
    y: f64,
    moved: bool,
}

fn piece_svg(set: PieceSet, kind: Kind, color: Color) -> String {
    let vb = brick_chess::pieces::view_box(set);
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"{vb}\" width=\"100%\" height=\"100%\" aria-hidden=\"true\">{}</svg>",
        piece_markup(set, kind, color)
    )
}

fn candidates(w: &Widget, from: usize, to: usize) -> Vec<String> {
    let f = index_to_name(from);
    let t = index_to_name(to);
    w.current()
        .legal_moves()
        .into_iter()
        .filter(|m| m.len() >= 4 && &m[0..2] == f && &m[2..4] == t)
        .collect()
}

fn is_promotion(cands: &[String]) -> bool {
    cands.iter().any(|m| m.len() > 4)
}

fn turn_color(w: &Widget) -> Color {
    if w.fen().split_whitespace().nth(1) == Some("b") {
        Color::Black
    } else {
        Color::White
    }
}

fn describe(events: &[brick_chess::Event]) -> String {
    for e in events {
        match e {
            brick_chess::Event::Move { san, .. } => return format!("played {san}"),
            brick_chess::Event::GameOver { result, reason } => return format!("{result} — {reason}"),
            brick_chess::Event::Error { code, detail } => return format!("refused: {code} ({detail})"),
            _ => {}
        }
    }
    String::new()
}

fn play_uci(w: &RwSignal<Widget>, uci: &str) -> String {
    let mut widget = w.get();
    let events = widget.apply(Command::Play {
        san: None,
        uci: Some(uci.to_owned()),
    });
    w.set(widget);
    describe(&events)
}

// ---------------------------------------------------------------------------
// Captured pieces and material
// ---------------------------------------------------------------------------

fn kind_index(k: Kind) -> usize {
    match k {
        Kind::King => 0,
        Kind::Queen => 1,
        Kind::Rook => 2,
        Kind::Bishop => 3,
        Kind::Knight => 4,
        Kind::Pawn => 5,
    }
}

fn value(k: Kind) -> i32 {
    match k {
        Kind::Pawn => 1,
        Kind::Knight | Kind::Bishop => 3,
        Kind::Rook => 5,
        Kind::Queen => 9,
        Kind::King => 0,
    }
}

fn count_fen(fen: &str) -> [[u32; 6]; 2] {
    let mut counts = [[0u32; 6]; 2];
    for c in fen.split_whitespace().next().unwrap_or("").chars() {
        let (side, lower) = if c.is_ascii_uppercase() { (0, c.to_ascii_lowercase()) } else { (1, c) };
        let kind = match lower {
            'k' => Some(Kind::King),
            'q' => Some(Kind::Queen),
            'r' => Some(Kind::Rook),
            'b' => Some(Kind::Bishop),
            'n' => Some(Kind::Knight),
            'p' => Some(Kind::Pawn),
            _ => None,
        };
        if let Some(k) = kind {
            counts[side][kind_index(k)] += 1;
        }
    }
    counts
}

fn count_board(w: &Widget) -> [[u32; 6]; 2] {
    let mut counts = [[0u32; 6]; 2];
    for p in w.scene().board.iter().flatten() {
        let side = if p.color == Color::White { 0 } else { 1 };
        counts[side][kind_index(p.kind)] += 1;
    }
    counts
}

fn captured_list(w: &Widget, side: usize) -> Vec<Kind> {
    let initial = count_fen(&w.initial_fen());
    let current = count_board(w);
    let mut out = Vec::new();
    for i in 0..6 {
        let missing = initial[side][i].saturating_sub(current[side][i]);
        for _ in 0..missing {
            out.push(kind_from_index(i));
        }
    }
    out
}

fn kind_from_index(i: usize) -> Kind {
    match i {
        0 => Kind::King,
        1 => Kind::Queen,
        2 => Kind::Rook,
        3 => Kind::Bishop,
        4 => Kind::Knight,
        _ => Kind::Pawn,
    }
}

fn material(w: &Widget) -> i32 {
    let mut diff = 0;
    for p in w.scene().board.iter().flatten() {
        let v = value(p.kind);
        diff += if p.color == Color::White { v } else { -v };
    }
    diff
}

// ---------------------------------------------------------------------------

#[component]
pub fn Board() -> impl IntoView {
    let widget = RwSignal::new(Widget::new());
    let drag: RwSignal<Option<Drag>> = RwSignal::new(None);
    let hover: RwSignal<Option<usize>> = RwSignal::new(None);
    let promo: RwSignal<Option<(usize, usize)>> = RwSignal::new(None);
    let status: RwSignal<String> = RwSignal::new(String::new());
    let fen_input: RwSignal<String> = RwSignal::new(String::new());
    let pgn_input: RwSignal<String> = RwSignal::new(String::new());
    let show_pgn: RwSignal<bool> = RwSignal::new(false);
    let moves_ref = NodeRef::<leptos::html::Div>::new();

    // Keep the current move visible as the game advances.
    Effect::new(move |_| {
        let _ = widget.with(|w| w.history_index());
        if let Some(el) = moves_ref.get() {
            if let Ok(Some(cur)) = el.query_selector(".move.current") {
                cur.scroll_into_view_with_bool(false);
            }
        }
    });

    let select_square = move |idx: usize| {
        widget.update(|w| {
            w.apply(Command::Select {
                square: index_to_name(idx),
            });
        });
    };

    let attempt = move |from: usize, to: usize| {
        let cands = widget.with(|w| candidates(w, from, to));
        if cands.is_empty() {
            return;
        }
        if is_promotion(&cands) {
            promo.set(Some((from, to)));
            return;
        }
        let mv = cands
            .iter()
            .max_by_key(|m| if m.ends_with('q') { 1 } else { 0 })
            .cloned()
            .unwrap();
        status.set(play_uci(&widget, &mv));
    };

    let on_down = move |idx: usize| {
        let sc = widget.with(|w| w.scene());
        if let Some(p) = sc.board[idx] {
            if p.color == widget.with(|w| turn_color(w)) {
                drag.set(Some(Drag {
                    from: idx,
                    kind: p.kind,
                    color: p.color,
                    x: 0.0,
                    y: 0.0,
                    moved: false,
                }));
            }
        }
    };

    let on_move = move |ev: leptos::ev::PointerEvent| {
        if let Some(mut d) = drag.get() {
            d.x = ev.client_x() as f64;
            d.y = ev.client_y() as f64;
            d.moved = true;
            drag.set(Some(d));
        }
    };

    let on_up = move |idx: usize| {
        let d = drag.get();
        let moved = d.map(|d| d.moved).unwrap_or(false);
        if moved {
            if let Some(d) = d {
                if d.from != idx {
                    attempt(d.from, idx);
                }
            }
        } else {
            if let Some(sel_idx) = widget.with(|w| w.selection().and_then(name_to_index)) {
                let cands = widget.with(|w| candidates(w, sel_idx, idx));
                if !cands.is_empty() {
                    attempt(sel_idx, idx);
                    drag.set(None);
                    hover.set(None);
                    return;
                }
            }
            select_square(idx);
        }
        drag.set(None);
        hover.set(None);
    };

    let cancel = move |_| {
        drag.set(None);
        hover.set(None);
    };

    let load_fen = move |_| {
        let fen = fen_input.get();
        if !fen.trim().is_empty() {
            widget.update(|w| {
                w.apply(Command::LoadFen { fen: fen.clone() });
            });
            promo.set(None);
            status.set(String::new());
        }
    };
    let load_pgn = move |_| {
        let pgn = pgn_input.get();
        if !pgn.trim().is_empty() {
            widget.update(|w| {
                w.apply(Command::LoadPgn { pgn: pgn.clone() });
            });
            promo.set(None);
            status.set(String::new());
            show_pgn.set(false);
        }
    };
    let copy_fen = move |_| {
        let fen = widget.with(|w| w.fen());
        if let Some(win) = web_sys::window() {
            let _ = win.navigator().clipboard().write_text(&fen);
        }
        status.set("FEN copied".into());
    };
    let flip = move |_| {
        widget.update(|w| {
            w.apply(Command::Flip);
        });
    };
    let reset = move |_| {
        widget.update(|w| {
            w.apply(Command::Reset { fen: None });
        });
        promo.set(None);
        status.set(String::new());
    };
    let set_theme = move |ev: leptos::ev::Event| {
        let v = event_target_value(&ev);
        widget.update(move |w| {
            w.apply(Command::SetTheme { theme: v });
        });
    };
    let set_pieces = move |ev: leptos::ev::Event| {
        let v = event_target_value(&ev);
        widget.update(move |w| {
            w.apply(Command::SetPieces { pieces: v });
        });
    };
    let go = move |index: usize| {
        widget.update(|w| {
            w.apply(Command::Goto { index });
        });
        status.set(String::new());
    };
    let do_prev = move || {
        widget.update(|w| {
            w.apply(Command::Undo);
        });
        status.set(String::new());
    };
    let do_next = move || {
        let (idx, len) = widget.with(|w| (w.history_index(), w.history_len()));
        if idx + 1 < len {
            go(idx + 1);
        }
    };
    let on_key = move |ev: leptos::ev::KeyboardEvent| match ev.key().as_str() {
        "ArrowLeft" => do_prev(),
        "ArrowRight" => do_next(),
        _ => {}
    };

    let cells = move || {
        let sc = widget.with(|w| w.scene());
        let dragging = drag.get().filter(|d| d.moved).map(|d| d.from);
        let hov = hover.get();
        (0..64)
            .map(|display| {
                let idx = sc.square_at(display);
                let piece = sc.board[idx];
                let is_light = sc.is_light(display);
                let selected = sc.selected == Some(idx);
                let target = sc.targets.contains(&idx);
                let last = sc.last_move.map_or(false, |(f, t)| f == idx || t == idx);
                let dest = sc.last_move.map_or(false, |(_, t)| t == idx);
                let check = sc.check == Some(idx);
                let hovered = hov == Some(idx) && dragging.is_some();
                let mut class = String::from(if is_light { "sq light" } else { "sq dark" });
                if selected {
                    class.push_str(" selected");
                }
                if target {
                    class.push_str(if piece.is_some() { " capture" } else { " target" });
                }
                if last {
                    class.push_str(" last");
                }
                if dest {
                    class.push_str(" dest");
                }
                if check {
                    class.push_str(" check");
                }
                if hovered {
                    class.push_str(" hovered");
                }
                // coordinates: rank on the left edge, file on the bottom edge
                let rank_label = if display % 8 == 0 {
                    let rank = 8 - display / 8;
                    format!("<span class=\"coord rank\">{rank}</span>")
                } else {
                    String::new()
                };
                let file_label = if display / 8 == 7 {
                    let file = (b'a' + (display % 8) as u8) as char;
                    format!("<span class=\"coord file\">{file}</span>")
                } else {
                    String::new()
                };
                let inner = format!(
                    "{}{}{}",
                    piece.map(|p| piece_svg(sc.pieces, p.kind, p.color)).unwrap_or_default(),
                    rank_label,
                    file_label
                );
                view! {
                    <button
                        class=class
                        on:pointerdown=move |_| on_down(idx)
                        on:pointerup=move |_| on_up(idx)
                        on:pointerenter=move |_| {
                            if drag.get().map(|d| d.moved).unwrap_or(false) {
                                hover.set(Some(idx));
                            }
                        }
                        inner_html=inner
                    ></button>
                }
            })
            .collect_view()
    };

    let ghost = move || {
        drag.get().filter(|d| d.moved).map(|d| {
            let set = widget.with(|w| w.pieces());
            let html = piece_svg(set, d.kind, d.color);
            view! {
                <div
                    class="ghost"
                    style=format!("left:{}px; top:{}px;", d.x - 30.0, d.y - 30.0)
                    inner_html=html
                ></div>
            }
        })
    };

    let promo_modal = move || {
        promo.get().map(|(from, to)| {
            let from_name = index_to_name(from);
            let to_name = index_to_name(to);
            let color = widget.with(|w| turn_color(w));
            let set = widget.with(|w| w.pieces());
            let opts = [(Kind::Queen, 'q'), (Kind::Rook, 'r'), (Kind::Bishop, 'b'), (Kind::Knight, 'n')];
            view! {
                <div class="promo-overlay">
                    <div class="promo-card">
                        <div class="promo-title">"Promote to"</div>
                        <div class="promo-row">
                            {opts
                                .into_iter()
                                .map(|(kind, letter)| {
                                    let html = piece_svg(set, kind, color);
                                    let from_name = from_name.clone();
                                    let to_name = to_name.clone();
                                    view! {
                                        <button
                                            class="promo-btn"
                                            inner_html=html
                                            on:click=move |_| {
                                                let uci = format!("{from_name}{to_name}{letter}");
                                                status.set(play_uci(&widget, &uci));
                                                promo.set(None);
                                            }
                                        ></button>
                                    }
                                })
                                .collect_view()}
                        </div>
                    </div>
                </div>
            }
        })
    };

    let captured = move || {
        let w = widget.get();
        let set = w.pieces();
        let white_lost = captured_list(&w, 0); // white pieces captured (missing)
        let black_lost = captured_list(&w, 1);
        let diff = material(&w);
        let white_html: String = white_lost
            .iter()
            .map(|k| format!("<span class=\"cap\">{}</span>", piece_svg(set, *k, Color::White)))
            .collect();
        let black_html: String = black_lost
            .iter()
            .map(|k| format!("<span class=\"cap\">{}</span>", piece_svg(set, *k, Color::Black)))
            .collect();
        let adv = if diff == 0 {
            String::new()
        } else if diff > 0 {
            format!("+{diff}")
        } else {
            format!("{diff}")
        };
        view! {
            <div class="captured">
                <div class="cap-row"><span class="cap-side">"White lost"</span><span inner_html=white_html></span></div>
                <div class="cap-row"><span class="cap-side">"Black lost"</span><span inner_html=black_html></span></div>
                <div class="cap-adv">"Material: " <b>{adv}</b></div>
            </div>
        }
    };

    let move_rows = move || {
        let sans = widget.with(|w| w.san_moves());
        let current = widget.with(|w| w.history_index());
        let mut rows = Vec::new();
        let mut i = 0;
        while i < sans.len() {
            let n = i / 2 + 1;
            let white = sans[i].clone();
            let black = sans.get(i + 1).cloned();
            let w_idx = i + 1;
            let w_cls = if current == w_idx { "move current" } else { "move" };
            let black_view = black.map(|b| {
                let b_idx = i + 2;
                let b_cls = if current == b_idx { "move current" } else { "move" };
                view! {
                    <button class=b_cls on:click=move |_| go(b_idx)>{b}</button>
                }
            });
            rows.push(view! {
                <div class="move-row">
                    <span class="move-no">{format!("{n}.")}</span>
                    <button class=w_cls on:click=move |_| go(w_idx)>{white}</button>
                    {black_view}
                </div>
            });
            i += 2;
        }
        view! { <div class="moves" node_ref=moves_ref>{rows}</div> }
    };

    view! {
        <div class="layout">
            <div class="board-col">
                <div class="board-wrap" tabindex="0" on:keydown=on_key>
                    <div
                        class=move || {
                            let t = match widget.with(|w| w.theme()) {
                                brick_chess::Theme::Green => "green",
                                brick_chess::Theme::Brown => "brown",
                                brick_chess::Theme::Blue => "blue",
                            };
                            format!("board {t}")
                        }
                        on:pointermove=on_move
                        on:pointerup=cancel
                    >
                        {cells}
                    </div>
                    {ghost}
                    {promo_modal}
                </div>
            </div>

            <aside class="panel">
                <div class="panel-title">"KnightLM"</div>
                {captured}
                <div class="movelist">
                    <div class="panel-sub">"Moves"</div>
                    {move_rows}
                </div>
                <div class="controls">
                    <button on:click=move |_| do_prev() title="Previous">"◀"</button>
                    <button on:click=move |_| do_next() title="Next">"▶"</button>
                    <button on:click=flip>"Flip"</button>
                    <button on:click=reset>"Reset"</button>
                </div>
                <div class="selects">
                    <label>
                        "Theme"
                        <select
                            prop:value=move || match widget.with(|w| w.theme()) {
                                brick_chess::Theme::Green => "green",
                                brick_chess::Theme::Brown => "brown",
                                brick_chess::Theme::Blue => "blue",
                            }
                            on:change=set_theme
                        >
                            <option value="green">"Green"</option>
                            <option value="brown">"Brown"</option>
                            <option value="blue">"Blue"</option>
                        </select>
                    </label>
                    <label>
                        "Pieces"
                        <select
                            prop:value=move || widget.with(|w| w.pieces()).slug()
                            on:change=set_pieces
                        >
                            {PieceSet::ALL
                                .into_iter()
                                .map(|s| view! { <option value=s.slug()>{s.label()}</option> })
                                .collect_view()}
                        </select>
                    </label>
                </div>
                <div class="load">
                    <input
                        type="text"
                        placeholder="Load FEN…"
                        prop:value=move || fen_input.get()
                        on:input=move |ev| fen_input.set(event_target_value(&ev))
                    />
                    <button on:click=load_fen>"Load"</button>
                </div>
                <button class="link" on:click=move |_| show_pgn.update(|v| *v = !*v)>
                    {move || if show_pgn.get() { "Hide PGN" } else { "Paste PGN…" }}
                </button>
                {move || show_pgn.get().then(|| view! {
                    <div class="pgn-box">
                        <textarea
                            placeholder="Paste a PGN game…"
                            prop:value=move || pgn_input.get()
                            on:input=move |ev| pgn_input.set(event_target_value(&ev))
                        ></textarea>
                        <button on:click=load_pgn>"Load PGN"</button>
                    </div>
                })}
                <div class="fen-line">
                    <code>{move || widget.with(|w| w.fen())}</code>
                    <button class="link" on:click=copy_fen>"Copy"</button>
                </div>
                <div class="status">{move || status.get()}</div>
            </aside>
        </div>
    }
}
