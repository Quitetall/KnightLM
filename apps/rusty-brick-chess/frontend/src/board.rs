use brick_chess::pieces::{piece_markup, PieceSet};
use brick_chess::scene::{index_to_name, name_to_index, Color, Kind};
use brick_chess::{Command, Orientation, Shape, Widget};

use crate::sound::{self, SoundEvent};
use gloo_timers::callback::Timeout;
use leptos::html::Div;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

#[derive(Clone, Copy)]
struct Drag {
    from: usize,
    pointer_id: i32,
    size: f64,
    start_x: f64,
    start_y: f64,
}

fn set_ghost_style(ghost: &web_sys::HtmlElement, size: f64, x: f64, y: f64, transition: Option<u32>) {
    let style = web_sys::HtmlElement::style(ghost);
    let _ = style.set_property("display", "block");
    let _ = style.set_property("width", &format!("{size}px"));
    let _ = style.set_property("height", &format!("{size}px"));
    let _ = style.set_property(
        "transition",
        &transition
            .map(|ms| format!("transform {ms}ms ease-out"))
            .unwrap_or_else(|| "none".to_owned()),
    );
    let _ = style.set_property(
        "transform",
        &format!(
            "translate3d({}px,{}px,0)",
            x - size / 2.0,
            y - size / 2.0
        ),
    );
}

fn ghost_element(node: &NodeRef<Div>) -> Option<web_sys::HtmlElement> {
    node.get()
        .and_then(|ghost| ghost.dyn_into::<web_sys::HtmlElement>().ok())
}

fn show_ghost(node: &NodeRef<Div>, html: &str, size: f64, x: f64, y: f64) {
    if let Some(ghost) = ghost_element(node) {
        ghost.set_inner_html(html);
        set_ghost_style(&ghost, size, x, y, None);
    }
}

fn slide_ghost(node: &NodeRef<Div>, size: f64, x: f64, y: f64) {
    if let Some(ghost) = ghost_element(node) {
        set_ghost_style(&ghost, size, x, y, Some(110));
    }
}

fn hide_ghost(node: &NodeRef<Div>) {
    if let Some(ghost) = ghost_element(node) {
        let _ = web_sys::HtmlElement::style(&ghost).set_property("display", "none");
    }
}

fn sound_for(events: &[brick_chess::Event]) -> Option<SoundEvent> {
    let mut sound = None;
    for event in events {
        match event {
            brick_chess::Event::Move { san, .. } => {
                sound = Some(if san.ends_with('#') {
                    SoundEvent::Checkmate
                } else if san.contains('+') {
                    SoundEvent::Check
                } else if san.contains('x') {
                    SoundEvent::Capture
                } else {
                    SoundEvent::Move
                });
            }
            brick_chess::Event::GameOver { result, .. } => {
                sound = Some(if result == "1/2-1/2" {
                    SoundEvent::Draw
                } else {
                    SoundEvent::Victory
                });
            }
            brick_chess::Event::Error { .. } => sound = Some(SoundEvent::Error),
            _ => {}
        }
    }
    sound
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

fn apply_play(w: &RwSignal<Widget>, uci: &str) -> Vec<brick_chess::Event> {
    let mut widget = w.get();
    let events = widget.apply(Command::Play {
        san: None,
        uci: Some(uci.to_owned()),
    });
    w.set(widget);
    events
}

fn shape_color(name: &str) -> &'static str {
    match name.to_ascii_lowercase().as_str() {
        "blue" => "#4a7ad4",
        "red" => "#d44a4a",
        "yellow" => "#d4b84a",
        _ => "#4a9e4a",
    }
}

fn display_xy(square: &str, orientation: Orientation) -> Option<(f64, f64)> {
    let idx = name_to_index(square)?;
    let file = (idx % 8) as f64;
    let rank = (idx / 8) as f64;
    Some(match orientation {
        Orientation::White => (file, 7.0 - rank),
        Orientation::Black => (7.0 - file, rank),
    })
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
pub fn Board(
    #[prop(into)] pgn: Signal<String>,
    #[prop(optional)] title: Option<Signal<String>>,
    #[prop(optional)] on_position: Option<Callback<String>>,
    #[prop(optional)] on_ply: Option<Callback<usize>>,
    #[prop(optional)] goto: Option<Signal<Option<(u32, usize)>>>,
    #[prop(optional)] play: Option<Signal<Option<(u32, String)>>>,
    #[prop(optional)] load_fen: Option<Signal<Option<(u32, String, String)>>>,
    #[prop(optional)] on_move: Option<Callback<(String, String, String)>>,
    #[prop(optional)] on_play: Option<Callback<(String, String, String)>>,
    #[prop(optional)] animate_last: Option<Signal<bool>>,
    #[prop(optional)] animation_ms: Option<Signal<u32>>,
    #[prop(optional)] shapes: Option<Signal<Vec<Shape>>>,
    #[prop(optional)] on_shape: Option<Callback<Shape>>,
    #[prop(optional, into)] theme: Option<Signal<String>>,
    #[prop(optional, into)] pieces: Option<Signal<String>>,
    #[prop(optional, into)] orientation: Option<Signal<String>>,
    #[prop(optional)] on_theme: Option<Callback<String>>,
    #[prop(optional)] on_pieces: Option<Callback<String>>,
) -> impl IntoView {
    let widget = RwSignal::new(Widget::new());
    let move_cb = on_move;

    // Load a PGN pushed by the parent (e.g. a game picked in the database).
    Effect::new(move |_| {
        let text = pgn.get();
        if !text.trim().is_empty() {
            widget.update(|w| {
                w.apply(Command::LoadPgn { pgn: text });
            });
        }
    });

    // Report the current FEN upward for the engine panel.
    if let Some(cb) = on_position {
        Effect::new(move |_| {
            let fen = widget.with(|w| w.fen());
            cb.run(fen);
        });
    }

    // Report the current ply (history index) upward for the analysis tree.
    if let Some(cb) = on_ply {
        Effect::new(move |_| {
            let ply = widget.with(|w| w.history_index());
            cb.run(ply);
        });
    }

    if let Some(load_fen) = load_fen {
        Effect::new(move |_| {
            if let Some((_, fen, variant)) = load_fen.get() {
                widget.update(|w| {
                    w.apply(Command::SetVariant { variant });
                    w.apply(Command::LoadFen { fen });
                });
            }
        });
    }

    // Saved appearance (Settings) and forced orientation (puzzles).
    if let Some(theme) = theme {
        Effect::new(move |_| {
            let value = theme.get();
            widget.update(move |w| {
                w.apply(Command::SetTheme { theme: value });
            });
        });
    }
    if let Some(pieces) = pieces {
        Effect::new(move |_| {
            let value = pieces.get();
            widget.update(move |w| {
                w.apply(Command::SetPieces { pieces: value });
            });
        });
    }
    if let Some(orientation) = orientation {
        Effect::new(move |_| {
            let value = orientation.get();
            widget.update(move |w| {
                w.apply(Command::Orient { color: value });
            });
        });
    }

    let drag: RwSignal<Option<Drag>> = RwSignal::new(None);
    let drag_from: RwSignal<Option<usize>> = RwSignal::new(None);
    let dragged = RwSignal::new(false);
    let shape_from: RwSignal<Option<usize>> = RwSignal::new(None);
    let drop_role: RwSignal<Option<char>> = RwSignal::new(None);
    let board_pct: RwSignal<u32> = RwSignal::new(100);
    let promo: RwSignal<Option<(usize, usize)>> = RwSignal::new(None);
    let status: RwSignal<String> = RwSignal::new(String::new());
    let fen_input: RwSignal<String> = RwSignal::new(String::new());
    let pgn_input: RwSignal<String> = RwSignal::new(String::new());
    let show_pgn: RwSignal<bool> = RwSignal::new(false);
    let moves_ref = NodeRef::<leptos::html::Div>::new();
    let board_ref = NodeRef::<Div>::new();
    let ghost_ref = NodeRef::<Div>::new();
    let anim_ms = move || animation_ms.map(|s| s.get()).unwrap_or(0).min(1000);

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
        sound::play(SoundEvent::Select);
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
        let events = apply_play(&widget, &mv);
        if let Some(sound) = sound_for(&events) {
            sound::play(sound);
        }
        status.set(describe(&events));
        if let Some(cb) = move_cb {
            for event in &events {
                if let brick_chess::Event::Move { uci, san, fen, .. } = event {
                    cb.run((uci.clone(), san.clone(), fen.clone()));
                }
            }
        }
    };

    let square_at = move |ev: &leptos::ev::PointerEvent| -> Option<usize> {
        let board = board_ref.get()?;
        let rect = board.get_bounding_client_rect();
        let width = rect.width();
        let height = rect.height();
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        let x = ev.client_x() as f64 - rect.left();
        let y = ev.client_y() as f64 - rect.top();
        if x < 0.0 || y < 0.0 || x >= width || y >= height {
            return None;
        }
        let col = ((x / width) * 8.0).floor().clamp(0.0, 7.0) as usize;
        let row = ((y / height) * 8.0).floor().clamp(0.0, 7.0) as usize;
        Some(widget.with(|w| w.scene().square_at(row * 8 + col)))
    };

    let square_center = move |idx: usize| -> Option<(f64, f64, f64)> {
        let board = board_ref.get()?;
        let rect = board.get_bounding_client_rect();
        let size = rect.width() / 8.0;
        if size <= 0.0 {
            return None;
        }
        let sc = widget.with_untracked(|w| w.scene());
        let file = (idx % 8) as f64;
        let rank = (idx / 8) as f64;
        let (col, row) = match sc.orientation {
            Orientation::White => (file, 7.0 - rank),
            Orientation::Black => (7.0 - file, rank),
        };
        Some((
            rect.left() + (col + 0.5) * size,
            rect.top() + (row + 0.5) * size,
            size,
        ))
    };

    let animate_back = move |from: usize| {
        if anim_ms() == 0 {
            hide_ghost(&ghost_ref);
            drag_from.set(None);
            return;
        }
        if let Some((x, y, size)) = square_center(from) {
            slide_ghost(&ghost_ref, size, x, y);
            Timeout::new(120, move || {
                hide_ghost(&ghost_ref);
                drag_from.set(None);
            })
            .forget();
        } else {
            hide_ghost(&ghost_ref);
            drag_from.set(None);
        }
    };

    let animate_move = move |from: usize, to: usize| {
        if anim_ms() == 0 {
            hide_ghost(&ghost_ref);
            drag_from.set(None);
            attempt(from, to);
            return;
        }
        let cands = widget.with(|w| candidates(w, from, to));
        if cands.is_empty() {
            hide_ghost(&ghost_ref);
            drag_from.set(None);
            return;
        }
        if is_promotion(&cands) {
            hide_ghost(&ghost_ref);
            drag_from.set(None);
            promo.set(Some((from, to)));
            return;
        }
        let Some((fx, fy, size)) = square_center(from) else {
            attempt(from, to);
            return;
        };
        let Some((tx, ty, _)) = square_center(to) else {
            attempt(from, to);
            return;
        };
        let piece = widget.with(|w| w.scene().board[from]);
        if let Some(p) = piece {
            let html = piece_svg(widget.with(|w| w.pieces()), p.kind, p.color);
            show_ghost(&ghost_ref, &html, size, fx, fy);
            drag_from.set(Some(from));
            let ms = anim_ms();
            Timeout::new(10, move || slide_ghost(&ghost_ref, size, tx, ty)).forget();
            Timeout::new(ms + 15, move || {
                attempt(from, to);
                hide_ghost(&ghost_ref);
                drag_from.set(None);
            })
            .forget();
        } else {
            attempt(from, to);
        }
    };

    let on_board_down = move |ev: leptos::ev::PointerEvent| {
        let Some(idx) = square_at(&ev) else { return };
        if ev.button() == 2 {
            shape_from.set(Some(idx));
            return;
        }
        if ev.button() != 0 {
            return;
        }
        let sc = widget.with(|w| w.scene());
        let turn = widget.with(|w| turn_color(w));
        if let Some(p) = sc.board[idx] {
            if p.color == turn {
                let size = square_center(idx).map(|(_, _, s)| s).unwrap_or(40.0);
                let x = ev.client_x() as f64;
                let y = ev.client_y() as f64;
                let html = piece_svg(widget.with(|w| w.pieces()), p.kind, p.color);
                show_ghost(&ghost_ref, &html, size, x, y);
                select_square(idx);
                drag_from.set(Some(idx));
                dragged.set(false);
                if let Some(target) = ev.current_target() {
                    if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                        let _ = el.set_pointer_capture(ev.pointer_id());
                    }
                }
                drag.set(Some(Drag {
                    from: idx,
                    pointer_id: ev.pointer_id(),
                    size,
                    start_x: x,
                    start_y: y,
                }));
            }
        }
    };

    let on_board_move = move |ev: leptos::ev::PointerEvent| {
        let Some(d) = drag.get() else { return };
        let dx = ev.client_x() as f64 - d.start_x;
        let dy = ev.client_y() as f64 - d.start_y;
        if !dragged.get() && (dx * dx + dy * dy).sqrt() > 3.0 {
            dragged.set(true);
        }
        if dragged.get() {
            if let Some(ghost) = ghost_element(&ghost_ref) {
                set_ghost_style(
                    &ghost,
                    d.size,
                    ev.client_x() as f64,
                    ev.client_y() as f64,
                    Some(0),
                );
            }
        }
    };

    let on_board_up = move |ev: leptos::ev::PointerEvent| {
        let idx = square_at(&ev);
        if ev.button() == 2 {
            if let (Some(from), Some(to)) = (shape_from.get(), idx) {
                let color = if ev.shift_key() {
                    "red"
                } else if ev.alt_key() {
                    "blue"
                } else if ev.ctrl_key() {
                    "yellow"
                } else {
                    "green"
                };
                let shape = if from == to {
                    Shape::Circle {
                        square: index_to_name(to),
                        color: color.into(),
                    }
                } else {
                    Shape::Arrow {
                        from: index_to_name(from),
                        to: index_to_name(to),
                        color: color.into(),
                    }
                };
                if let Some(cb) = on_shape {
                    cb.run(shape);
                }
            }
            shape_from.set(None);
            return;
        }
        if ev.button() != 0 {
            return;
        }
        if let Some(role) = drop_role.get() {
            if let Some(to) = idx {
                let empty = widget.with(|w| w.scene().board[to].is_none());
                if empty {
                    let uci = format!("{}@{}", role, index_to_name(to));
                    let events = apply_play(&widget, &uci);
                    if let Some(sound) = sound_for(&events) {
                        sound::play(sound);
                    }
                    status.set(describe(&events));
                    if let Some(cb) = move_cb {
                        for event in &events {
                            if let brick_chess::Event::Move { uci, san, fen, .. } = event {
                                cb.run((uci.clone(), san.clone(), fen.clone()));
                            }
                        }
                    }
                }
            }
            drop_role.set(None);
            return;
        }
        if let Some(d) = drag.get() {
            if let Some(board) = board_ref.get() {
                let _ = board.release_pointer_capture(d.pointer_id);
            }
            let moved = dragged.get_untracked();
            dragged.set(false);
            drag.set(None);
            if moved {
                match idx {
                    Some(to) if to != d.from => {
                        if !widget.with(|w| candidates(w, d.from, to)).is_empty() {
                            animate_move(d.from, to);
                        } else {
                            animate_back(d.from);
                        }
                    }
                    _ => animate_back(d.from),
                }
            } else {
                hide_ghost(&ghost_ref);
                drag_from.set(None);
            }
            return;
        }
        let Some(to) = idx else { return };
        if let Some(from) = widget.with(|w| w.selection().and_then(name_to_index)) {
            if !widget.with(|w| candidates(w, from, to)).is_empty() {
                animate_move(from, to);
            } else if widget.with(|w| w.scene().board[to].is_none()) {
                widget.update(|w| {
                    w.apply(Command::Select {
                        square: String::new(),
                    });
                });
            }
        }
    };

    let on_pointer_cancel = move |_| {
        if let Some(d) = drag.get() {
            if let Some(board) = board_ref.get() {
                let _ = board.release_pointer_capture(d.pointer_id);
            }
        }
        drag.set(None);
        dragged.set(false);
        hide_ghost(&ghost_ref);
        drag_from.set(None);
    };

    if let Some(play) = play {
        Effect::new(move |_| {
            let Some((_, uci)) = play.get() else {
                return;
            };
            let squares = if uci.len() >= 4 && !uci.contains('@') {
                name_to_index(&uci[0..2]).zip(name_to_index(&uci[2..4]))
            } else {
                None
            };
            if anim_ms() > 0 {
            if let Some((from, to)) = squares {
                if let (Some((fx, fy, size)), Some((tx, ty, _)), Some(piece)) = (
                    square_center(from),
                    square_center(to),
                    widget.with_untracked(|w| w.scene().board[from]),
                ) {
                    let html =
                        piece_svg(widget.with_untracked(|w| w.pieces()), piece.kind, piece.color);
                    show_ghost(&ghost_ref, &html, size, fx, fy);
                    drag_from.set(Some(from));
                    let ms = anim_ms();
                    Timeout::new(10, move || slide_ghost(&ghost_ref, size, tx, ty)).forget();
                    Timeout::new(ms + 15, move || {
                        let mut next = widget.get_untracked();
                        let events = next.apply(Command::Play {
                            san: None,
                            uci: Some(uci),
                        });
                        widget.set(next);
                        if let Some(sound) = sound_for(&events) {
                            sound::play(sound);
                        }
                        hide_ghost(&ghost_ref);
                        drag_from.set(None);
                        if let Some(cb) = on_play {
                            for event in &events {
                                if let brick_chess::Event::Move { uci, san, fen, .. } = event {
                                    cb.run((uci.clone(), san.clone(), fen.clone()));
                                }
                            }
                        }
                    })
                    .forget();
                    return;
                }
            }
            }
            let mut next = widget.get_untracked();
            let events = next.apply(Command::Play {
                san: None,
                uci: Some(uci),
            });
            widget.set(next);
            if let Some(sound) = sound_for(&events) {
                sound::play(sound);
            }
            if let Some(cb) = on_play {
                for event in &events {
                    if let brick_chess::Event::Move { uci, san, fen, .. } = event {
                        cb.run((uci.clone(), san.clone(), fen.clone()));
                    }
                }
            }
        });
    }

    let load_fen = move |_| {
        let fen = fen_input.get();
        if !fen.trim().is_empty() {
            let mut next = widget.get();
            let events = next.apply(Command::LoadFen { fen: fen.clone() });
            widget.set(next);
            promo.set(None);
            status.set(describe(&events));
        }
    };
    let load_pgn = move |_| {
        let pgn = pgn_input.get();
        if !pgn.trim().is_empty() {
            let mut next = widget.get();
            let events = next.apply(Command::LoadPgn { pgn: pgn.clone() });
            widget.set(next);
            promo.set(None);
            status.set(describe(&events));
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
        let applied = v.clone();
        widget.update(move |w| {
            w.apply(Command::SetTheme { theme: applied });
        });
        if let Some(cb) = on_theme {
            cb.run(v);
        }
    };
    let set_pieces = move |ev: leptos::ev::Event| {
        let v = event_target_value(&ev);
        let applied = v.clone();
        widget.update(move |w| {
            w.apply(Command::SetPieces { pieces: applied });
        });
        if let Some(cb) = on_pieces {
            cb.run(v);
        }
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

    if let Some(goto) = goto {
        Effect::new(move |_| {
            if let Some((_, index)) = goto.get() {
                let current = widget.with_untracked(|w| w.history_index());
                let ms = anim_ms();
                if ms > 0 && index == current + 1 {
                    if let Some(uci) = widget.with_untracked(|w| w.move_uci_at(index)) {
                        if uci.len() >= 4 && !uci.contains('@') {
                            if let (Some(from), Some(to)) = (
                                name_to_index(&uci[0..2]),
                                name_to_index(&uci[2..4]),
                            ) {
                                if let (
                                    Some((fx, fy, size)),
                                    Some((tx, ty, _)),
                                    Some(piece),
                                ) = (
                                    square_center(from),
                                    square_center(to),
                                    widget.with_untracked(|w| w.scene().board[from]),
                                ) {
                                    let html = piece_svg(
                                        widget.with_untracked(|w| w.pieces()),
                                        piece.kind,
                                        piece.color,
                                    );
                                    show_ghost(&ghost_ref, &html, size, fx, fy);
                                    drag_from.set(Some(from));
                                    Timeout::new(10, move || {
                                        slide_ghost(&ghost_ref, size, tx, ty)
                                    })
                                    .forget();
                                    Timeout::new(ms + 15, move || {
                                        widget.update(|w| {
                                            w.apply(Command::Goto { index });
                                        });
                                        hide_ghost(&ghost_ref);
                                        drag_from.set(None);
                                    })
                                    .forget();
                                    return;
                                }
                            }
                        }
                    }
                }
                widget.update(|w| {
                    w.apply(Command::Goto { index });
                });
            }
        });
    }

    let cells = move || {
        let sc = widget.with(|w| w.scene());
        let dragging = drag_from.get();
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
                if dragging == Some(idx) {
                    class.push_str(" dragging-src");
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
                        data-square=index_to_name(idx)
                        aria-label=index_to_name(idx)
                        inner_html=inner
                    ></button>
                }
            })
            .collect_view()
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
                                                let events = apply_play(&widget, &uci);
                                                if let Some(sound) = sound_for(&events) {
                                                    sound::play(sound);
                                                }
                                                status.set(describe(&events));
                                                if let Some(cb) = move_cb {
                                                    for event in &events {
                                                        if let brick_chess::Event::Move { uci, san, fen, .. } = event {
                                                            cb.run((uci.clone(), san.clone(), fen.clone()));
                                                        }
                                                    }
                                                }
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

    let pocket_row = move |color: Color| {
        let roles = [
            (Kind::Queen, 'Q'),
            (Kind::Rook, 'R'),
            (Kind::Bishop, 'B'),
            (Kind::Knight, 'N'),
            (Kind::Pawn, 'P'),
        ];
        let sc = widget.with(|w| w.scene());
        let turn = widget.with(|w| turn_color(w));
        let active = turn == color;
        let set = sc.pieces;
        let items = roles
            .into_iter()
            .filter_map(|(kind, letter)| {
                let count = sc.pocket(color, kind);
                (count > 0).then(|| {
                    let html = piece_svg(set, kind, color);
                    view! {
                        <button
                            class=if active { "pocket-piece active" } else { "pocket-piece" }
                            title=format!("Drop {letter}")
                            on:click=move |_| {
                                if active {
                                    drop_role.set(Some(letter));
                                }
                            }
                            inner_html=format!("{html}<span class=\"pocket-count\">{count}</span>")
                        ></button>
                    }
                })
            })
            .collect_view();
        items
    };

    let shapes_layer = move || {
        let list = shapes.map(|s| s.get()).unwrap_or_default();
        if list.is_empty() {
            return None;
        }
        let orientation = widget.with(|w| w.orientation());
        let items = list
            .into_iter()
            .filter_map(|shape| match shape {
                Shape::Arrow { from, to, color } => {
                    let (x1, y1) = display_xy(&from, orientation)?;
                    let (x2, y2) = display_xy(&to, orientation)?;
                    let stroke = shape_color(&color);
                    Some(
                        view! {
                            <line
                                x1=x1 + 0.5
                                y1=y1 + 0.5
                                x2=x2 + 0.5
                                y2=y2 + 0.5
                                stroke=stroke
                                stroke-width="0.09"
                                stroke-linecap="round"
                                opacity="0.85"
                                marker-end="url(#rbc-arrow)"
                            ></line>
                        }
                        .into_any(),
                    )
                }
                Shape::Circle { square, color } => {
                    let (cx, cy) = display_xy(&square, orientation)?;
                    let stroke = shape_color(&color);
                    Some(
                        view! {
                            <circle
                                cx=cx + 0.5
                                cy=cy + 0.5
                                r="0.42"
                                fill="none"
                                stroke=stroke
                                stroke-width="0.07"
                                opacity="0.85"
                            ></circle>
                        }
                        .into_any(),
                    )
                }
                Shape::Highlight { squares, color } => {
                    let stroke = shape_color(&color);
                    let cells = squares
                        .into_iter()
                        .filter_map(|sq| {
                            let (x, y) = display_xy(&sq, orientation)?;
                            Some(
                                view! {
                                    <rect x=x y=y width="1" height="1" fill=stroke opacity="0.22"></rect>
                                }
                                .into_any(),
                            )
                        })
                        .collect_view();
                    Some(view! { <g>{cells}</g> }.into_any())
                }
            })
            .collect_view();
        Some(view! {
            <svg class="shapes" viewBox="0 0 8 8" preserveAspectRatio="none">
                <defs>
                    <marker
                        id="rbc-arrow"
                        markerWidth="3"
                        markerHeight="3"
                        refX="2.2"
                        refY="1.5"
                        orient="auto"
                    >
                        <path d="M0,0 L3,1.5 L0,3 Z" fill="context-stroke"></path>
                    </marker>
                </defs>
                {items}
            </svg>
        })
    };

    view! {
        <div class="layout">
            <div class="board-col">
                <div
                    class="board-wrap"
                    style=move || {
                        format!(
                            "width: min({}%, calc(100vh - 240px)); margin-inline: auto;",
                            board_pct.get()
                        )
                    }
                    tabindex="0"
                    on:keydown=on_key
                    on:contextmenu=move |ev| ev.prevent_default()
                >
                    <div class="pockets pockets-black">{move || pocket_row(Color::Black)}</div>
                    <div
                        class=move || {
                            let t = match widget.with(|w| w.theme()) {
                                brick_chess::Theme::Green => "green",
                                brick_chess::Theme::Brown => "brown",
                                brick_chess::Theme::Blue => "blue",
                            };
                            let animated = animate_last.map(|s| s.get()).unwrap_or(false);
                            format!(
                                "board {t}{}",
                                if animated { " animate-last" } else { "" }
                            )
                        }
                        node_ref=board_ref
                        on:pointerdown=on_board_down
                        on:pointermove=on_board_move
                        on:pointerup=on_board_up
                        on:pointercancel=on_pointer_cancel
                    >
                        {cells}
                    </div>
                    <div class="pockets pockets-white">{move || pocket_row(Color::White)}</div>
                    {shapes_layer}
                    <div class="drag-ghost" node_ref=ghost_ref></div>
                    {promo_modal}
                </div>
            </div>

            <aside class="panel">
                <div class="panel-title">
                    {move || {
                        title
                            .as_ref()
                            .map(|t| t.get())
                            .unwrap_or_else(|| "Board".to_owned())
                    }}
                </div>
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
                    <label class="size-control" title="Board size">
                        "Size"
                        <input
                            type="range"
                            min="45"
                            max="100"
                            step="5"
                            prop:value=move || board_pct.get().to_string()
                            on:input=move |ev| {
                                board_pct.set(event_target_value(&ev).parse().unwrap_or(100))
                            }
                        />
                    </label>
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
