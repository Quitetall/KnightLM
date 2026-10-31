use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use brick_chess::scene::index_to_name;
use crate::board::{Board};
use crate::api;
use crate::i18n;
use crate::model::*;
use crate::views::AppCtx;

pub fn analysis(ctx: AppCtx) -> impl IntoView {
    let AppCtx {
        add_collection,
        ai_shapes,
        analysis,
        analysis_rep,
        analyzing,
        animate_last_move,
        ann_nag,
        ann_text,
        annotating,
        chat_messages,
        chat_text,
        chat_tools,
        clock,
        clock_active,
        clock_base_input,
        clock_epoch,
        clock_inc,
        clock_inc_input,
        cloud,
        collections,
        console_busy,
        console_cmd,
        console_log,
        console_name,
        console_path,
        console_running,
        dock,
        edit_moves_open,
        editor_castling,
        editor_ep,
        editor_open,
        editor_palette,
        editor_pieces,
        editor_turn,
        editor_variant,
        engine_results,
        engines,
        eval_hover,
        evals,
        fen,
        game_detail,
        goto_sig,
        keybinds,
        language,
        lines,
        load_fen_sig,
        moves_text,
        piece_animation_ms,
        play_as,
        play_uci,
        record_moves,
        report,
        reporting,
        reps,
        selected_game_id,
        selected_pgn,
        selected_title,
        settings_coords,
        settings_dark,
        settings_light,
        settings_pieces,
        settings_theme,
        shapes,
        show_tags,
        tag_black,
        tag_black_elo,
        tag_date,
        tag_event,
        tag_result,
        tag_round,
        tag_site,
        tag_time_control,
        tag_white,
        tag_white_elo,
        tb_result,
        tb_status,
        thinking,
        tree_ply,
        variant_only,
        vision_msg,
        vs_engine,
        ..
    } = ctx;
    let add_game_to_collection = move |a| ctx.add_game_to_collection.run(a);
    let add_game_to_repertoire = move |a| ctx.add_game_to_repertoire.run(a);
    let add_line_to_repertoire = move |a| ctx.add_line_to_repertoire.run(a);
    let annotate_game = move |a| ctx.annotate_game.run(a);
    let apply_editor = move |a| ctx.apply_editor.run(a);
    let apply_moves = move |a| ctx.apply_moves.run(a);
    let choose_side = move |a| ctx.choose_side.run(a);
    let classify = move |a| ctx.classify.run(a);
    let console_send = move || ctx.console_send.run(());
    let console_start = move |a| ctx.console_start.run(a);
    let console_stop = move |a| ctx.console_stop.run(a);
    let delete_current = move |a| ctx.delete_current.run(a);
    let delete_tree_from_here = move |a| ctx.delete_tree_from_here.run(a);
    let export_report = move |a| ctx.export_report.run(a);
    let fetch_cloud = move |a| ctx.fetch_cloud.run(a);
    let fetch_engine_lines = move |a| ctx.fetch_engine_lines.run(a);
    let fetch_lines = move |a| ctx.fetch_lines.run(a);
    let on_board_move = move |args| ctx.on_board_move.run(args);
    let on_engine_play = move |args| ctx.on_engine_play.run(args);
    let open_editor = move |a| ctx.open_editor.run(a);
    let persist_settings = move || ctx.persist_settings.run(());
    let probe_tablebase = move |a| ctx.probe_tablebase.run(a);
    let promote_selected = move |a| ctx.promote_selected.run(a);
    let run_analysis = move |a| ctx.run_analysis.run(a);
    let save_annotation = move |a| ctx.save_annotation.run(a);
    let save_tags = move |a| ctx.save_tags.run(a);
    let scan_image = move |a| ctx.scan_image.run(a);
    let select_node = move |a| ctx.select_node.run(a);
    let send_chat = move || ctx.send_chat.run(());
    let toggle_vs_engine = move |a| ctx.toggle_vs_engine.run(a);
    let tree_nodes = move || ctx.tree_nodes.run(());
    let truncate_moves = move |a| ctx.truncate_moves.run(a);
    let score = move || {
        analysis
            .get()
            .map(|a| {
                if let Some(m) = a.mate {
                    format!("M{m}")
                } else if let Some(c) = a.score_cp {
                    format!("{:+.2}", c as f64 / 100.0)
                } else {
                    "—".to_owned()
                }
            })
            .unwrap_or_else(|| "—".to_owned())
    };
    view! {
        <section class="analysis">
            <div class="board-pane">
                {move || {
                    clock
                        .get()
                        .map(|(white, black)| {
                            let black_turn = fen.get().split_whitespace().nth(1) == Some("b");
                            let running = clock_active.get();
                            let mut white_cls = String::from("clock-chip");
                            let mut black_cls = String::from("clock-chip");
                            if running && !black_turn {
                                white_cls.push_str(" active");
                            }
                            if running && black_turn {
                                black_cls.push_str(" active");
                            }
                            if white < 30 {
                                white_cls.push_str(" low");
                            }
                            if black < 30 {
                                black_cls.push_str(" low");
                            }
                            view! {
                                <div class="clocks">
                                    <div class=white_cls>
                                        {format!(
                                            "{} {}",
                                            i18n::t(&language.get(), "common.white"),
                                            format_clock(white),
                                        )}
                                    </div>
                                    <div class=black_cls>
                                        {format!(
                                            "{} {}",
                                            i18n::t(&language.get(), "common.black"),
                                            format_clock(black),
                                        )}
                                    </div>
                                </div>
                                <div class="clock-controls">
                                    <button
                                        on:click=move |_| {
                                            if clock_active.get_untracked() {
                                                clock_active.set(false);
                                            } else if clock.get_untracked().is_some() {
                                                clock_epoch.update(|e| *e += 1);
                                                clock_active.set(true);
                                            }
                                        }
                                    >
                                        {move || if clock_active.get() {
                                        i18n::t(&language.get(), "common.pause")
                                    } else {
                                        i18n::t(&language.get(), "common.start")
                                    }}
                                    </button>
                                    <button on:click=move |_| {
                                        let base = clock_base_input.get_untracked() as i64;
                                        let inc = clock_inc_input.get_untracked() as i64;
                                        clock.set(Some((base, base)));
                                        clock_inc.set(inc);
                                        clock_active.set(false);
                                        clock_epoch.update(|e| *e += 1);
                                    }>{move || i18n::t(&language.get(), "common.reset")}</button>
                                    <input
                                        type="number"
                                        min="0"
                                        max="86400"
                                        title=move || i18n::t(&language.get(), "clock.base")
                                        prop:value=move || clock_base_input.get().to_string()
                                        on:input=move |ev| {
                                            clock_base_input.set(event_target_value(&ev).parse().unwrap_or(180))
                                        }
                                    />
                                    <input
                                        type="number"
                                        min="0"
                                        max="300"
                                        title=move || i18n::t(&language.get(), "clock.increment")
                                        prop:value=move || clock_inc_input.get().to_string()
                                        on:input=move |ev| {
                                            clock_inc_input.set(event_target_value(&ev).parse().unwrap_or(0))
                                        }
                                    />
                                </div>
                            }
                        })
                }}
                <Board
                    pgn=selected_pgn
                    title=Signal::derive(move || selected_title.get())
                    on_position=Callback::new(move |f: String| fen.set(f))
                    on_ply=Callback::new(move |p: usize| tree_ply.set(p))
                    goto=Signal::derive(move || goto_sig.get())
                    play=Signal::derive(move || play_uci.get())
                    load_fen=Signal::derive(move || load_fen_sig.get())
                    on_move=Callback::new(on_board_move)
                    on_play=Callback::new(on_engine_play)
                    animate_last=Signal::derive(move || animate_last_move.get())
                    animation_ms=Signal::derive(move || piece_animation_ms.get())
                    keybinds=Signal::derive(move || keybinds.get())
                    orientation=Signal::derive(move || {
                        if vs_engine.get() && play_as.get() == "black" {
                            "black".to_string()
                        } else {
                            "white".to_string()
                        }
                    })
                    shapes=Signal::derive(move || {
                        let mut list = shapes.get();
                        list.extend(ai_shapes.get());
                        list
                    })
                    on_shape=Callback::new(move |s: brick_chess::Shape| {
                        shapes.update(|v| v.push(s));
                    })
                    theme=Signal::derive(move || settings_theme.get())
                    pieces=Signal::derive(move || settings_pieces.get())
                                    light_color=Signal::derive(move || settings_light.get())
                                    dark_color=Signal::derive(move || settings_dark.get())
                                    coords=Signal::derive(move || settings_coords.get())
                                    lang=Signal::derive(move || language.get())
                    on_theme=Callback::new(move |v: String| {
                        settings_theme.set(v);
                        persist_settings();
                    })
                    on_pieces=Callback::new(move |v: String| {
                        settings_pieces.set(v);
                        persist_settings();
                    })
                />
                <div class="eval-graph">
                    {move || {
                        let rows = evals.get();
                        if rows.is_empty() {
                            return view! {
                                <div class="muted eval-empty">
                                    {move || i18n::t(&language.get(), "analysis.classify_hint")}
                                </div>
                            }
                            .into_any();
                        }
                        let n = rows.len().max(2) as f64;
                        let pts: Vec<(f64, f64)> = rows.iter().map(eval_point).collect();
                        let poly = pts
                            .iter()
                            .map(|(ply, y)| {
                                format!("{:.2},{:.2}", ply * 100.0 / (n - 1.0), y)
                            })
                            .collect::<Vec<_>>()
                            .join(" ");
                        let hover = eval_hover.get().filter(|h| *h < rows.len());
                        let marker = hover.map(|h| {
                            (h as f64 * 100.0 / (n - 1.0), pts[h].1)
                        });
                        let readout = hover.map(|h| {
                            let san = report
                                .get()
                                .and_then(|r| {
                                    r.moves.iter().find(|m| m.ply == h).map(|m| m.san.clone())
                                })
                                .unwrap_or_default();
                            format!("ply {} {} · {}", h, san, eval_text(&rows[h]))
                        });
                        view! {
                            <div class="eval-readout muted">
                                {readout.unwrap_or_else(|| "hover the graph; click to jump".into())}
                            </div>
                            <svg
                                class="eval-svg"
                                viewBox="0 0 100 100"
                                preserveAspectRatio="none"
                                on:mousemove=move |ev| {
                                    let Some(target) = ev.current_target() else { return };
                                    let Ok(el) = target.dyn_into::<web_sys::Element>() else {
                                        return;
                                    };
                                    let width = el.client_width().max(1) as f64;
                                    let x = ev.offset_x() as f64;
                                    let idx = ((x / width) * (n - 1.0))
                                        .round()
                                        .clamp(0.0, n - 1.0) as usize;
                                    eval_hover.set(Some(idx));
                                }
                                on:mouseleave=move |_| eval_hover.set(None)
                                on:click=move |_| {
                                    let Some(ply) = eval_hover.get() else { return };
                                    if let Some(detail) = game_detail.get() {
                                        let items = detail
                                            .movetext
                                            .as_deref()
                                            .map(|json| tree_items(&api::parse_movetext(json)))
                                            .unwrap_or_default();
                                        if let Some(item) = items
                                            .into_iter()
                                            .find(|i| i.depth == 0 && i.ply == ply)
                                        {
                                            select_node(item);
                                        }
                                    }
                                }
                            >
                                <line x1="0" y1="50" x2="100" y2="50" stroke="#31313c" stroke-width="0.5"></line>
                                <polyline points=poly fill="none" stroke="#7fd18a" stroke-width="1.2"></polyline>
                                {marker
                                    .map(|(x, y)| {
                                        view! {
                                            <line x1=x y1="0" x2=x y2="100" stroke="#9a9aa8" stroke-width="0.4" stroke-dasharray="2,2"></line>
                                            <circle cx=x cy=y r="1.6" fill="#7fd18a"></circle>
                                        }
                                    })}
                            </svg>
                        }
                        .into_any()
                    }}
                </div>
            </div>
            <aside class="engine-pane">
                <div class="panel-head"><h2>{move || i18n::t(&language.get(), "settings.engine")}</h2></div>
                <div class="row-btns">
                    <label class="checkline">
                        <input
                            type="checkbox"
                            prop:checked=move || vs_engine.get()
                            on:change=toggle_vs_engine
                        />
                        {move || i18n::t(&language.get(), "analysis.play_engine")}
                    </label>
                    <select
                        prop:value=move || play_as.get()
                        on:change=choose_side
                    >
                        <option value="white">
                            {move || i18n::t(&language.get(), "analysis.you_white")}
                        </option>
                        <option value="black">
                            {move || i18n::t(&language.get(), "analysis.you_black")}
                        </option>
                    </select>
                    {move || thinking.get().then(|| view! {
                        <span class="muted">
                            {i18n::t(&language.get(), "analysis.engine_thinking")}
                        </span>
                    })}
                </div>
                <div class="eval-card">
                    {move || {
                        analysis
                            .get()
                            .map(|a| {
                                view! {
                                    <div class="eval-score">{score}</div>
                                    <div class="muted">{format!("depth {}", a.depth)}</div>
                                }
                                .into_any()
                            })
                            .unwrap_or_else(|| {
                                view! { <div class="muted">{i18n::t(&language.get(), "analysis.no_analysis")}</div> }.into_any()
                            })
                    }}
                </div>
                {move || {
                    cloud
                        .get()
                        .map(|c| {
                            if !c.available {
                                return view! {
                                    <div class="muted">
                                        {i18n::t(&language.get(), "analysis.cloud_missing")}
                                    </div>
                                }
                                    .into_any();
                            }
                            let score = c
                                .mate
                                .map(|m| format!("M{m}"))
                                .or_else(|| c.cp.map(|cp| format!("{:+.2}", cp as f64 / 100.0)))
                                .unwrap_or_else(|| "—".to_owned());
                            view! {
                                <div class="engine-body">
                                    <div class="best">{format!("Cloud {score}")}</div>
                                    <div class="muted">
                                        {format!(
                                            "depth {} · {} knodes",
                                            c.depth.unwrap_or(0),
                                            c.knodes.unwrap_or(0),
                                        )}
                                    </div>
                                    <div class="pv">{c.pv.join(" ")}</div>
                                </div>
                            }
                                .into_any()
                        })
                }}
                <div class="row-btns">
                    <button on:click=run_analysis disabled=move || analyzing.get() || variant_only.get()>
                        {move || {
                            if analyzing.get() {
                                i18n::t(&language.get(), "analysis.analyzing")
                            } else {
                                i18n::t(&language.get(), "analysis.analyze")
                            }
                        }}
                    </button>
                    <button on:click=fetch_lines disabled=move || variant_only.get()>
                        {move || i18n::t(&language.get(), "analysis.lines")}
                    </button>
                    <button on:click=fetch_engine_lines disabled=move || variant_only.get()>
                        {move || i18n::t(&language.get(), "analysis.all_engines")}
                    </button>
                    <button on:click=fetch_cloud>
                        {move || i18n::t(&language.get(), "analysis.cloud_eval")}
                    </button>
                </div>
                {move || {
                    let list = engine_results.get();
                    if list.is_empty() {
                        return None;
                    }
                    Some(view! {
                        <div class="lines">
                            {list
                                .into_iter()
                                .map(|engine| {
                                    view! {
                                        <div class="engine-group">
                                            <div class="panel-sub">{engine.name.clone()}</div>
                                            {match engine.error {
                                                Some(error) => view! {
                                                    <div class="muted">{error}</div>
                                                }.into_any(),
                                                None => view! {
                                                    {engine
                                                        .lines
                                                        .into_iter()
                                                        .map(|l| {
                                                            let score = if let Some(m) = l.mate {
                                                                format!("M{m}")
                                                            } else if let Some(c) = l.score_cp {
                                                                format!("{:+.2}", c as f64 / 100.0)
                                                            } else {
                                                                "—".to_owned()
                                                            };
                                                            view! {
                                                                <div class="line">
                                                                    <span class="line-score">{score}</span>
                                                                    <span class="pv">{l.pv.join(" ")}</span>
                                                                </div>
                                                            }
                                                        })
                                                        .collect_view()}
                                                }.into_any(),
                                            }}
                                        </div>
                                    }
                                })
                                .collect_view()}
                        </div>
                    })
                }}
                {move || {
                    let ls = lines.get();
                    if ls.is_empty() {
                        return None;
                    }
                    Some(view! {
                        <div class="lines">
                            {ls
                                .into_iter()
                                .map(|l| {
                                    let score = if let Some(m) = l.mate {
                                        format!("M{m}")
                                    } else if let Some(c) = l.score_cp {
                                        format!("{:+.2}", c as f64 / 100.0)
                                    } else {
                                        "—".to_owned()
                                    };
                                    view! {
                                        <div class="line">
                                            <span class="line-score">{score}</span>
                                            <span class="pv">{l.pv.join(" ")}</span>
                                        </div>
                                    }
                                })
                                .collect_view()}
                        </div>
                    })
                }}
                {move || {
                    analysis
                        .get()
                        .map(|a| {
                            view! {
                                <div class="engine-body">
                                    <div class="best">{format!("Best: {}", a.best_move)}</div>
                                    <div class="pv">{a.pv.join(" ")}</div>
                                    <div class="muted">
                                        {a.nodes.map(|n| format!("{n} nodes")).unwrap_or_default()}
                                    </div>
                                </div>
                            }
                        })
                }}
                <div class="panel-sub">{move || i18n::t(&language.get(), "analysis.tablebase")}</div>
                <div class="row-btns">
                    <button on:click=probe_tablebase>{move || i18n::t(&language.get(), "tb.probe")}</button>
                    <span class="muted">{move || tb_status.get()}</span>
                </div>
                {move || {
                    tb_result
                        .get()
                        .map(|t| {
                            let wdl = match t.wdl.as_str() {
                                "win" => i18n::t(&language.get(), "tb.win"),
                                "cursed-win" => i18n::t(&language.get(), "tb.cursed"),
                                "draw" => i18n::t(&language.get(), "db.draw"),
                                "blessed-loss" => i18n::t(&language.get(), "tb.blessed"),
                                "loss" => i18n::t(&language.get(), "tb.loss"),
                                other => other,
                            };
                            view! {
                                <div class="engine-body">
                                    <div class="best">{format!("{wdl} · {} pieces", t.pieces)}</div>
                                    <div class="muted">
                                        {t.best_move.clone().map(|m| format!("best: {m}")).unwrap_or_default()}
                                    </div>
                                    <div class="muted">{format!("tables: {}", t.path)}</div>
                                </div>
                            }
                        })
                }}
                <div class="panel-sub">{move || i18n::t(&language.get(), "analysis.move_classification")}</div>
                <div class="row-btns">
                    <button on:click=export_report disabled=move || report.get().is_none()>
                        {move || i18n::t(&language.get(), "analysis.export_report")}
                    </button>
                    <button on:click=annotate_game disabled=move || annotating.get() || report.get().is_none()>
                        {move || i18n::t(&language.get(), "analysis.annotate")}
                    </button>
                    <button on:click=classify disabled=move || reporting.get() || variant_only.get()>
                        {move || {
                            if reporting.get() {
                                i18n::t(&language.get(), "analysis.classifying")
                            } else {
                                i18n::t(&language.get(), "analysis.classify")
                            }
                        }}
                    </button>
                    <a
                        class="link"
                        href=move || {
                            selected_game_id.get().map(api::game_pgn_url).unwrap_or_default()
                        }
                        download=move || {
                            let title = selected_title.get();
                            let name: String = title
                                .chars()
                                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                                .collect();
                            format!("{name}.pgn")
                        }
                        target="_blank"
                        rel="noopener"
                    >
                        {move || i18n::t(&language.get(), "db.save_pgn")}
                    </a>
                    <button on:click=delete_current>{move || i18n::t(&language.get(), "common.delete")}</button>
                </div>
                <div class="row-btns">
                    <select
                        prop:value=move || add_collection.get().to_string()
                        on:change=move |ev| {
                            add_collection.set(event_target_value(&ev).parse().unwrap_or(0))
                        }
                    >
                        <option value="0">{move || i18n::t(&language.get(), "db.add_to_collection")}</option>
                        {move || {
                            collections
                                .get()
                                .into_iter()
                                .map(|c| {
                                    view! {
                                        <option value=c.id.to_string()>{c.name.clone()}</option>
                                    }
                                })
                                .collect_view()
                        }}
                    </select>
                    <button on:click=add_game_to_collection>{move || i18n::t(&language.get(), "common.add")}</button>
                </div>
                {move || {
                    report
                        .get()
                        .map(|r| {
                            let n = r.moves.len().max(2) as f64;
                            let points = r
                                .moves
                                .iter()
                                .enumerate()
                                .map(|(i, m)| {
                                    let x = i as f64 * (400.0 / (n - 1.0));
                                    let y = 100.0 - m.win_after;
                                    format!("{x:.1},{y:.1}")
                                })
                                .collect::<Vec<_>>()
                                .join(" ");
                            view! {
                                <div class="report">
                                    <div class="acc">
                                        {format!(
                                            "White {:.1}% · Black {:.1}%",
                                            r.white_accuracy.unwrap_or(0.0),
                                            r.black_accuracy.unwrap_or(0.0),
                                        )}
                                    </div>
                                    <div class="muted">
                                        {format!(
                                            "Lichess — inacc {} · mistakes {} · blunders {}",
                                            r.lichess_counts.inaccuracy,
                                            r.lichess_counts.mistake,
                                            r.lichess_counts.blunder,
                                        )}
                                    </div>
                                    <div class="muted">
                                        {format!(
                                            "Expected points — inacc {} · mistakes {} · blunders {}",
                                            r.expected_points_counts.inaccuracy,
                                            r.expected_points_counts.mistake,
                                            r.expected_points_counts.blunder,
                                        )}
                                    </div>
                                    <svg class="spark" viewBox="0 0 400 100" preserveAspectRatio="none">
                                        <polyline points=points fill="none" stroke="#7fd18a" stroke-width="2"></polyline>
                                    </svg>
                                    <div class="mv-list">
                                        {r
                                            .moves
                                            .iter()
                                            .map(|m| {
                                                view! {
                                                    <span class=format!("mv {}", m.lichess)>{m.san.clone()}</span>
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                </div>
                            }
                        })
                }}
                <div class="panel-sub">{move || i18n::t(&language.get(), "analysis.tree")}</div>
                {tree_nodes}
                <div class="row-btns">
                    <button on:click=delete_tree_from_here>
                        {move || i18n::t(&language.get(), "analysis.delete_from")}
                    </button>
                    <button on:click=promote_selected>
                        {move || i18n::t(&language.get(), "analysis.promote")}
                    </button>
                    <label class="checkline">
                        <input
                            type="checkbox"
                            prop:checked=move || record_moves.get()
                            on:change=move |ev| record_moves.set(event_target_checked(&ev))
                        />
                        {move || i18n::t(&language.get(), "analysis.record_moves")}
                    </label>
                </div>
                <div class="panel-sub">{move || i18n::t(&language.get(), "analysis.assistant")}</div>
                <div class="row-btns">
                    <button on:click=move |_| dock.set("ai".into())>{move || i18n::t(&language.get(), "analysis.ai_chat")}</button>
                    <button on:click=move |_| dock.set("console".into())>{move || i18n::t(&language.get(), "analysis.engine_console")}</button>
                </div>
                <div class="muted">
                    {move || i18n::t(&language.get(), "analysis.assistant_hint")}
                </div>
                <div class="panel-sub">{move || i18n::t(&language.get(), "analysis.game_tags")}</div>
                <div class="row-btns">
                    <button on:click=move |_| show_tags.update(|v| *v = !*v)>
                        {move || {
                            if show_tags.get() {
                                i18n::t(&language.get(), "db.hide_tags")
                            } else {
                                i18n::t(&language.get(), "db.edit_tags")
                            }
                        }}
                    </button>
                    <span class="muted">{move || game_detail.get().map(|d| d.event.unwrap_or_default()).unwrap_or_default()}</span>
                </div>
                {move || {
                    show_tags
                        .get()
                        .then(|| {
                            view! {
                                <div class="tag-form">
                                    <label>{move || i18n::t(&language.get(), "db.event")}<input type="text" prop:value=move || tag_event.get() on:input=move |ev| tag_event.set(event_target_value(&ev))/></label>
                                    <label>"Site"<input type="text" prop:value=move || tag_site.get() on:input=move |ev| tag_site.set(event_target_value(&ev))/></label>
                                    <label>{move || i18n::t(&language.get(), "db.date")}<input type="text" placeholder="YYYY.MM.DD" prop:value=move || tag_date.get() on:input=move |ev| tag_date.set(event_target_value(&ev))/></label>
                                    <label>"Round"<input type="text" prop:value=move || tag_round.get() on:input=move |ev| tag_round.set(event_target_value(&ev))/></label>
                                    <label>"White"<input type="text" prop:value=move || tag_white.get() on:input=move |ev| tag_white.set(event_target_value(&ev))/></label>
                                    <label>"Black"<input type="text" prop:value=move || tag_black.get() on:input=move |ev| tag_black.set(event_target_value(&ev))/></label>
                                    <label>"Result"<input type="text" prop:value=move || tag_result.get() on:input=move |ev| tag_result.set(event_target_value(&ev))/></label>
                                    <label>"WhiteElo"<input type="text" prop:value=move || tag_white_elo.get() on:input=move |ev| tag_white_elo.set(event_target_value(&ev))/></label>
                                    <label>"BlackElo"<input type="text" prop:value=move || tag_black_elo.get() on:input=move |ev| tag_black_elo.set(event_target_value(&ev))/></label>
                                    <label>"TimeControl"<input type="text" prop:value=move || tag_time_control.get() on:input=move |ev| tag_time_control.set(event_target_value(&ev))/></label>
                                    <button on:click=save_tags>{move || i18n::t(&language.get(), "db.save_tags")}</button>
                                </div>
                            }
                        })
                }}
                <div class="panel-sub">{move || i18n::t(&language.get(), "analysis.board_editor")}</div>
                <div class="row-btns">
                    <button on:click=open_editor>
                        {move || if editor_open.get() {
                            i18n::t(&language.get(), "editor.close")
                        } else {
                            i18n::t(&language.get(), "analysis.edit_position")
                        }}
                    </button>
                </div>
                {move || {
                    editor_open
                        .get()
                        .then(|| {
                            let palette = editor_palette.get();
                            let options = ['K', 'Q', 'R', 'B', 'N', 'P', 'k', 'q', 'r', 'b', 'n', 'p'];
                            view! {
                                <div class="editor">
                                    <div class="palette">
                                        {options
                                            .into_iter()
                                            .map(|ch| {
                                                let cls = if palette == Some(ch) { "palette-btn active" } else { "palette-btn" };
                                                view! {
                                                    <button
                                                        class=cls
                                                        on:click=move |_| editor_palette.set(Some(ch))
                                                    >
                                                        {piece_glyph(ch)}
                                                    </button>
                                                }
                                            })
                                            .collect_view()}
                                        <button
                                            class=if palette.is_none() { "palette-btn active" } else { "palette-btn" }
                                            on:click=move |_| editor_palette.set(None)
                                        >
                                            "⌫"
                                        </button>
                                    </div>
                                    <div class="editor-grid">
                                        {move || {
                                            let pieces = editor_pieces.get();
                                            (0..64)
                                                .map(|display| {
                                                    let rank = 7 - display / 8;
                                                    let file = display % 8;
                                                    let idx = rank * 8 + file;
                                                    let ch = pieces[idx];
                                                    let cls = format!(
                                                        "edit-sq {} {}",
                                                        if (display / 8 + display % 8) % 2 == 0 { "light" } else { "dark" },
                                                        ch.map(|c| if c.is_ascii_uppercase() { "white" } else { "black" }).unwrap_or(""),
                                                    );
                                                    view! {
                                                        <button
                                                            class=cls
                                                            data-edit-square=index_to_name(idx)
                                                            on:click=move |_| {
                                                                let piece = editor_palette.get_untracked();
                                                                editor_pieces.update(|v| v[idx] = piece);
                                                            }
                                                        >
                                                            {ch.map(piece_glyph).unwrap_or("·")}
                                                        </button>
                                                    }
                                                })
                                                .collect_view()
                                        }}
                                    </div>
                                    <div class="editor-fields">
                                        <label>"Turn"<input type="text" prop:value=move || editor_turn.get() on:input=move |ev| editor_turn.set(event_target_value(&ev))/></label>
                                        <label>{move || i18n::t(&language.get(), "editor.castling")}<input type="text" prop:value=move || editor_castling.get() on:input=move |ev| editor_castling.set(event_target_value(&ev))/></label>
                                        <label>{move || i18n::t(&language.get(), "editor.en_passant")}<input type="text" prop:value=move || editor_ep.get() on:input=move |ev| editor_ep.set(event_target_value(&ev))/></label>
                                        <label class="checkline">
                                            {move || i18n::t(&language.get(), "settings.scan_image")}
                                            <input
                                                type="file"
                                                accept="image/*"
                                                on:change=scan_image
                                            />
                                        </label>
                                        <label>
                                            {move || i18n::t(&language.get(), "db.variant")}
                                            <select
                                                prop:value=move || editor_variant.get()
                                                on:change=move |ev| editor_variant.set(event_target_value(&ev))
                                            >
                                                <option value="standard">"Standard"</option>
                                                <option value="chess960">"Chess960"</option>
                                                <option value="atomic">"Atomic"</option>
                                                <option value="antichess">"Antichess"</option>
                                                <option value="kingofthehill">"King of the Hill"</option>
                                                <option value="3check">"Three-check"</option>
                                                <option value="crazyhouse">"Crazyhouse"</option>
                                                <option value="racingkings">"Racing Kings"</option>
                                                <option value="horde">"Horde"</option>
                                            </select>
                                        </label>
                                    </div>
                                    <div class="row-btns">
                                        <button on:click=move |_| {
                                            editor_pieces.update(|pieces| {
                                                for rank in 0..4 {
                                                    for file in 0..8 {
                                                        pieces.swap(rank * 8 + file, (7 - rank) * 8 + file);
                                                    }
                                                }
                                            });
                                        }>{move || i18n::t(&language.get(), "editor.flip_ranks")}</button>
                                        <button on:click=move |_| {
                                            editor_pieces.update(|pieces| {
                                                for rank in 0..8 {
                                                    for file in 0..4 {
                                                        pieces.swap(rank * 8 + file, rank * 8 + (7 - file));
                                                    }
                                                }
                                            });
                                        }>{move || i18n::t(&language.get(), "editor.flip_files")}</button>
                                        <button on:click=move |_| {
                                            editor_pieces.update(|pieces| {
                                                pieces.reverse();
                                            });
                                        }>{move || i18n::t(&language.get(), "editor.rotate")}</button>
                                    </div>
                                    <button on:click=apply_editor>{move || i18n::t(&language.get(), "editor.use_position")}</button>
                                    <div class="muted">{move || vision_msg.get()}</div>
                                </div>
                            }
                        })
                }}
                <div class="panel-sub">{move || i18n::t(&language.get(), "analysis.edit_moves")}</div>
                <div class="row-btns">
                    <button
                        on:click=move |_| {
                            if !edit_moves_open.get_untracked() {
                                if let Some(id) = selected_game_id.get_untracked() {
                                    spawn_local(async move {
                                        if let Ok(text) = api::game_pgn_text(id).await {
                                            moves_text.set(text);
                                        }
                                    });
                                }
                            }
                            edit_moves_open.update(|v| *v = !*v);
                        }
                    >
                        {move || if edit_moves_open.get() {
                            i18n::t(&language.get(), "editor.close")
                        } else {
                            i18n::t(&language.get(), "analysis.edit_moves")
                        }}
                    </button>
                    <button on:click=truncate_moves>{move || i18n::t(&language.get(), "analysis.keep_up")}</button>
                </div>
                {move || {
                    edit_moves_open
                        .get()
                        .then(|| {
                            view! {
                                <div class="pgn-box">
                                    <textarea
                                        placeholder="1. e4 e5 2. Nf3 (2. Bc4 Nf6) …"
                                        prop:value=move || moves_text.get()
                                        on:input=move |ev| moves_text.set(event_target_value(&ev))
                                    ></textarea>
                                    <button on:click=apply_moves>{move || i18n::t(&language.get(), "analysis.save_moves")}</button>
                                </div>
                            }
                        })
                }}
                <div class="panel-sub">{move || i18n::t(&language.get(), "openings.repertoire")}</div>
                <div class="row-btns">
                    <select
                        prop:value=move || analysis_rep.get().to_string()
                        on:change=move |ev| {
                            analysis_rep.set(event_target_value(&ev).parse().unwrap_or(0))
                        }
                    >
                        <option value="0">{move || i18n::t(&language.get(), "analysis.add_line_to")}</option>
                        {move || {
                            reps
                                .get()
                                .into_iter()
                                .map(|r| {
                                    view! {
                                        <option value=r.id.to_string()>
                                            {format!("{} ({})", r.name, r.color)}
                                        </option>
                                    }
                                })
                                .collect_view()
                        }}
                    </select>
                    <button on:click=add_line_to_repertoire>
                        {move || i18n::t(&language.get(), "analysis.add_line")}
                    </button>
                    <button on:click=add_game_to_repertoire>
                        {move || i18n::t(&language.get(), "analysis.add_game")}
                    </button>
                </div>
                <div class="panel-sub">{move || i18n::t(&language.get(), "analysis.annotation")}</div>
                <div class="ann-editor">
                    <input
                        type="text"
                        placeholder=move || i18n::t(&language.get(), "analysis.comment_ph")
                        prop:value=move || ann_text.get()
                        on:input=move |ev| ann_text.set(event_target_value(&ev))
                    />
                    <select
                        prop:value=move || ann_nag.get().to_string()
                        on:change=move |ev| ann_nag.set(event_target_value(&ev).parse().unwrap_or(0))
                    >
                        <option value="0">"—"</option>
                        <option value="1">"!"</option>
                        <option value="2">"?"</option>
                        <option value="3">"!!"</option>
                        <option value="4">"??"</option>
                        <option value="5">"!?"</option>
                        <option value="6">"?!"</option>
                    </select>
                    <button on:click=save_annotation>{move || i18n::t(&language.get(), "common.save")}</button>
                </div>
            </aside>
            {move || {
                let kind = dock.get();
                if kind.is_empty() {
                    return None;
                }
                let ai = view! {
                    <div class="dock-body">
                        <div class="chat-messages" aria-live="polite">
                            {move || {
                                chat_messages
                                    .get()
                                    .into_iter()
                                    .map(|(role, text)| {
                                        view! {
                                            <div class=format!("chat-msg {role}")>{text}</div>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </div>
                        {move || {
                            let tools = chat_tools.get();
                            (!tools.is_empty()).then(|| view! {
                                <div class="chat-tools">
                                    {tools
                                        .into_iter()
                                        .map(|tool| view! { <span class="tool-chip">{tool}</span> })
                                        .collect_view()}
                                </div>
                            })
                        }}
                        <div class="row-btns">
                            <button on:click=move |_| {
                                chat_text.set(i18n::t(&language.get(), "analysis.prompt_analyse").into());
                                send_chat();
                            }>{move || i18n::t(&language.get(), "analysis.analyse")}</button>
                            <button on:click=move |_| {
                                chat_text.set(i18n::t(&language.get(), "analysis.prompt_plan").into());
                                send_chat();
                            }>{move || i18n::t(&language.get(), "analysis.explain")}</button>
                            <button on:click=move |_| ai_shapes.set(Vec::new())>{move || i18n::t(&language.get(), "analysis.clear_arrows")}</button>
                        </div>
                        <div class="chat-input">
                            <textarea
                                placeholder=move || i18n::t(&language.get(), "analysis.ask_hint")
                                prop:value=move || chat_text.get()
                                on:input=move |ev| chat_text.set(event_target_value(&ev))
                            ></textarea>
                            <button on:click=move |_| send_chat()>{move || i18n::t(&language.get(), "analysis.send")}</button>
                        </div>
                    </div>
                }
                .into_any();
                let console = view! {
                    <div class="dock-body">
                        <div class="engine-fields">
                            <label>
                                {move || i18n::t(&language.get(), "settings.engine")}
                                <select
                                    prop:value=move || console_path.get()
                                    on:change=move |ev| console_path.set(event_target_value(&ev))
                                >
                                    {move || {
                                        engines
                                            .get()
                                            .into_iter()
                                            .map(|engine| {
                                                view! {
                                                    <option value=engine.path.clone()>
                                                        {format!("{} — {}", engine.name, engine.path)}
                                                    </option>
                                                }
                                            })
                                            .collect_view()
                                    }}
                                </select>
                            </label>
                            <div class="row-btns">
                                <button on:click=console_start disabled=move || console_running.get()>
                                    {move || i18n::t(&language.get(), "common.start")}
                                </button>
                                <button on:click=console_stop disabled=move || !console_running.get()>
                                    {move || i18n::t(&language.get(), "practice.stop")}
                                </button>
                                <span class="status">
                                    {move || {
                                        if console_running.get() {
                                            console_name.get()
                                        } else {
                                            "stopped".to_owned()
                                        }
                                    }}
                                </span>
                            </div>
                        </div>
                        <pre class="console-log">{move || console_log.get().join("\n")}</pre>
                        <div class="chat-input">
                            <input
                                type="text"
                                placeholder=move || i18n::t(&language.get(), "analysis.uci_hint")
                                prop:value=move || console_cmd.get()
                                on:input=move |ev| console_cmd.set(event_target_value(&ev))
                                on:keydown=move |ev| {
                                    if ev.key() == i18n::t(&language.get(), "common.enter") {
                                        console_send();
                                    }
                                }
                            />
                            <button on:click=move |_| console_send() disabled=move || console_busy.get()>
                                {move || i18n::t(&language.get(), "analysis.send")}
                            </button>
                        </div>
                    </div>
                }
                .into_any();
                Some(view! {
                    <div class="engine-dock">
                        <div class="dock-head">
                            <div class="dock-tabs">
                                <button
                                    class=if kind == "ai" { "dock-tab active" } else { "dock-tab" }
                                    on:click=move |_| dock.set("ai".into())
                                >
                                    {move || i18n::t(&language.get(), "analysis.ai")}
                                </button>
                                <button
                                    class=if kind == "console" { "dock-tab active" } else { "dock-tab" }
                                    on:click=move |_| dock.set("console".into())
                                >
                                    {move || i18n::t(&language.get(), "analysis.console")}
                                </button>
                            </div>
                            <button
                                class="dock-close"
                                aria-label={move || i18n::t(&language.get(), "analysis.close_panel")}
                                on:click=move |_| dock.set(String::new())
                            >
                                "×"
                            </button>
                        </div>
                        {if kind == "ai" { ai } else { console }}
                    </div>
                })
            }}
        </section>
    }
    .into_any()
}
