use leptos::prelude::*;
use leptos::task::spawn_local;
use crate::board::{Keybinds};
use crate::api;
use crate::i18n;
use crate::views::AppCtx;

pub fn settings(ctx: AppCtx) -> impl IntoView {
    let AppCtx {
        animate_last_move,
        custom_sets,
        engine_dialog,
        engine_test,
        engine_test_busy,
        engines,
        keybinds,
        language,
        new_engine_name,
        new_engine_path,
        piece_animation_ms,
        piece_set_msg,
        piece_set_name,
        piece_upload_busy,
        puzzle_rating,
        settings_coords,
        settings_dark,
        settings_depth,
        settings_engine,
        settings_hash,
        settings_light,
        settings_model,
        settings_pieces,
        settings_status,
        settings_syzygy,
        settings_theme,
        settings_threads,
        settings_variant_engine,
        sound_enabled,
        sound_msg,
        sound_theme,
        sound_volume,
        vision,
        vision_busy,
        vision_msg,
        ..
    } = ctx;
    let add_engine = move |a| ctx.add_engine.run(a);
    let download_vision = move |a| ctx.download_vision.run(a);
    let persist_settings = move || ctx.persist_settings.run(());
    let run_engine_test = move |a| ctx.run_engine_test.run(a);
    let set_engine_option = move |a, b, c| ctx.set_engine_option.run((a, b, c));
    let upload_piece_set = move |a| ctx.upload_piece_set.run(a);
    let upload_sound = move |a| ctx.upload_sound.run(a);
    view! {
        <section class="panel">
            <div class="panel-head">
                <h2>{move || i18n::t(&language.get(), "settings.title")}</h2>
                <span class="muted">{move || format!("Puzzle rating {}", puzzle_rating.get())}</span>
            </div>
            <div class="settings-form">
                <label>
                    {move || i18n::t(&language.get(), "settings.language")}
                    <select
                        prop:value=move || language.get()
                        on:change=move |ev| {
                            language.set(event_target_value(&ev));
                            persist_settings();
                        }
                    >
                        {i18n::LANGS
                            .into_iter()
                            .map(|(code, name)| {
                                view! { <option value=code>{name}</option> }
                            })
                            .collect_view()}
                    </select>
                </label>
                <label>
                    {move || i18n::t(&language.get(), "settings.engine_binary")}
                    <input
                        type="text"
                        placeholder="/path/to/stockfish"
                        prop:value=move || settings_engine.get()
                        on:input=move |ev| settings_engine.set(event_target_value(&ev))
                    />
                </label>
                <label>
                    {move || i18n::t(&language.get(), "settings.model_url")}
                    <input
                        type="text"
                        placeholder="http://127.0.0.1:8000/v1/chat"
                        prop:value=move || settings_model.get()
                        on:input=move |ev| settings_model.set(event_target_value(&ev))
                    />
                </label>
                <label>
                    {move || i18n::t(&language.get(), "settings.depth")}
                    <input
                        type="number"
                        min="1"
                        max="30"
                        prop:value=move || settings_depth.get().to_string()
                        on:input=move |ev| {
                            settings_depth.set(event_target_value(&ev).parse().unwrap_or(15))
                        }
                    />
                </label>
                <div class="settings-row">
                    <label>
                        {move || i18n::t(&language.get(), "settings.board_theme")}
                        <select
                            prop:value=move || settings_theme.get()
                            on:change=move |ev| settings_theme.set(event_target_value(&ev))
                        >
                            <option value="green">{move || i18n::t(&language.get(), "theme.green")}</option>
                            <option value="brown">{move || i18n::t(&language.get(), "theme.brown")}</option>
                            <option value="blue">{move || i18n::t(&language.get(), "theme.blue")}</option>
                            <option value="custom">"Custom"</option>
                        </select>
                    </label>
                    <label>
                        {move || i18n::t(&language.get(), "settings.piece_set")}
                        <select
                            prop:value=move || settings_pieces.get()
                            on:change=move |ev| settings_pieces.set(event_target_value(&ev))
                        >
                            {brick_chess::pieces::PieceSet::ALL
                                .into_iter()
                                .map(|s| view! { <option value=s.slug()>{s.label()}</option> })
                                .collect_view()}
                            {move || {
                                custom_sets
                                    .get()
                                    .into_iter()
                                    .map(|set| {
                                        view! {
                                            <option value=format!("custom:{}", set.name)>
                                                {format!("{} (custom)", set.name)}
                                            </option>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </select>
                    </label>
                    <label>
                        {move || i18n::t(&language.get(), "settings.light")}
                        <input
                            type="color"
                            prop:value=move || settings_light.get()
                            on:input=move |ev| {
                                settings_light.set(event_target_value(&ev));
                                settings_theme.set("custom".into());
                            }
                        />
                    </label>
                    <label>
                        {move || i18n::t(&language.get(), "settings.dark")}
                        <input
                            type="color"
                            prop:value=move || settings_dark.get()
                            on:input=move |ev| {
                                settings_dark.set(event_target_value(&ev));
                                settings_theme.set("custom".into());
                            }
                        />
                    </label>
                    <label class="checkline">
                        <input
                            type="checkbox"
                            prop:checked=move || settings_coords.get()
                            on:change=move |ev| settings_coords.set(event_target_checked(&ev))
                        />
                        {move || i18n::t(&language.get(), "settings.coordinates")}
                    </label>
                </div>
                <div class="panel-sub">{move || i18n::t(&language.get(), "settings.custom_pieces")}</div>
                <div class="piece-upload">
                    <input
                        type="text"
                        placeholder="set name (a-z, 0-9, -)"
                        prop:value=move || piece_set_name.get()
                        on:input=move |ev| piece_set_name.set(event_target_value(&ev))
                    />
                    <label class="file-button">
                        {move || i18n::t(&language.get(), "settings.upload_pieces")}
                        <input
                            type="file"
                            multiple
                            accept=".svg,image/svg+xml"
                            on:change=upload_piece_set
                        />
                    </label>
                    <span class="muted">{move || piece_set_msg.get()}</span>
                    <span class="status">
                        {move || if piece_upload_busy.get() { "uploading…" } else { "" }}
                    </span>
                </div>
                <div class="muted">
                    {move || i18n::t(&language.get(), "settings.piece_files_hint")}
                </div>
                {move || {
                    let sets = custom_sets.get();
                    (!sets.is_empty()).then(|| {
                        view! {
                            <div class="custom-sets">
                                {sets
                                    .into_iter()
                                    .map(|set| {
                                        let name_for_delete = set.name.clone();
                                        view! {
                                            <div class="custom-set">
                                                <span>{format!("{} · {} pieces", set.name, set.pieces)}</span>
                                                <button
                                                    class="link"
                                                    on:click=move |_| {
                                                        let name = name_for_delete.clone();
                                                        spawn_local(async move {
                                                            match api::delete_piece_set(&name).await {
                                                                Ok(list) => {
                                                                    custom_sets.set(list);
                                                                    if settings_pieces
                                                                        .get_untracked()
                                                                        .starts_with(&format!("custom:{name}"))
                                                                    {
                                                                        settings_pieces.set("cburnett".into());
                                                                        persist_settings();
                                                                    }
                                                                    piece_set_msg.set(i18n::tf(&language.get(), "status.d39", &[("name", name.clone())]));
                                                                }
                                                                Err(error) => piece_set_msg.set(error),
                                                            }
                                                        });
                                                    }
                                                >
                                                    {move || i18n::t(&language.get(), "db.delete")}
                                                </button>
                                            </div>
                                        }
                                    })
                                    .collect_view()}
                            </div>
                        }
                    })
                }}
                <div class="row-btns">
                    <button on:click=move |_| persist_settings()>{move || i18n::t(&language.get(), "common.save")}</button>
                    <span class="status">{move || settings_status.get()}</span>
                </div>
                <div class="settings-row">
                    <label>
                        {move || i18n::t(&language.get(), "settings.threads")}
                        <input
                            type="number"
                            min="0"
                            max="256"
                            prop:value=move || settings_threads.get().to_string()
                            on:input=move |ev| {
                                settings_threads.set(event_target_value(&ev).parse().unwrap_or(0))
                            }
                        />
                    </label>
                    <label>
                        {move || i18n::t(&language.get(), "settings.hash")}
                        <input
                            type="number"
                            min="0"
                            max="65536"
                            prop:value=move || settings_hash.get().to_string()
                            on:input=move |ev| {
                                settings_hash.set(event_target_value(&ev).parse().unwrap_or(0))
                            }
                        />
                    </label>
                </div>
                <label>
                    {move || i18n::t(&language.get(), "settings.syzygy")}
                    <input
                        type="text"
                        placeholder="/path/to/syzygy"
                        prop:value=move || settings_syzygy.get()
                        on:input=move |ev| settings_syzygy.set(event_target_value(&ev))
                    />
                </label>
                {move || {
                    let index = engine_dialog.get()?;
                    let entry = engines.get().get(index).cloned()?;
                    let option = |key: &'static str, label: &'static str, numeric: bool| {
                        let stored = entry.options.get(key).cloned().unwrap_or_default();
                        let value = if stored.is_empty() {
                            match key {
                                "MultiPV" => "1".to_owned(),
                                "Hash" | "Threads" => "0".to_owned(),
                                _ => String::new(),
                            }
                        } else {
                            stored
                        };
                        view! {
                            <label>
                                {label}
                                <input
                                    type=if numeric { "number" } else { "text" }
                                    prop:value=value
                                    on:change=move |ev| {
                                        set_engine_option(index, key, event_target_value(&ev))
                                    }
                                />
                            </label>
                        }
                    };
                    let test = engine_test.get();
                    Some(view! {
                        <div class="engine-dialog">
                            <div class="panel-head">
                                <h2>{format!("Engine: {}", entry.name)}</h2>
                                <button on:click=move |_| {
                                    engine_dialog.set(None);
                                    engine_test.set(None);
                                }>{move || i18n::t(&language.get(), "common.close")}</button>
                            </div>
                            <div class="engine-fields">
                                <label>
                                    {move || i18n::t(&language.get(), "settings.name")}
                                    <input
                                        type="text"
                                        prop:value=entry.name.clone()
                                        on:change=move |ev| {
                                            let value = event_target_value(&ev);
                                            engines.update(|list| {
                                                if let Some(item) = list.get_mut(index) {
                                                    item.name = value;
                                                }
                                            });
                                        }
                                    />
                                </label>
                                <label>
                                    {move || i18n::t(&language.get(), "settings.path")}
                                    <input
                                        type="text"
                                        prop:value=entry.path.clone()
                                        on:change=move |ev| {
                                            let value = event_target_value(&ev);
                                            engines.update(|list| {
                                                if let Some(item) = list.get_mut(index) {
                                                    item.path = value;
                                                }
                                            });
                                        }
                                    />
                                </label>
                            </div>
                            <div class="engine-fields">
                                {option(i18n::t(&language.get(), "settings.hash_label"), i18n::t(&language.get(), "settings.hash_mb"), true)}
                                {option(i18n::t(&language.get(), "settings.threads_label"), i18n::t(&language.get(), "settings.threads_label"), true)}
                                {option(i18n::t(&language.get(), "settings.multipv"), i18n::t(&language.get(), "settings.multipv"), true)}
                                {option("Skill Level", "Skill level", true)}
                                {option("UCI_Elo", "UCI Elo", true)}
                                {option("SyzygyPath", "Syzygy path", false)}
                            </div>
                            <div class="muted">
                                {move || i18n::t(&language.get(), "settings.engine_options_hint")}
                            </div>
                            <div class="row-btns">
                                <button on:click=run_engine_test disabled=move || engine_test_busy.get()>
                                    {move || if engine_test_busy.get() {
                                    i18n::t(&language.get(), "settings.testing")
                                } else {
                                    i18n::t(&language.get(), "settings.test_engine")
                                }}
                                </button>
                                <button on:click=move |_| {
                                    persist_settings();
                                    settings_status.set(i18n::t(&language.get(), "status.m08").into());
                                }>{move || i18n::t(&language.get(), "common.save")}</button>
                                <span class="status">{move || settings_status.get()}</span>
                            </div>
                            {test.map(|info| view! {
                                <div class="engine-test">
                                    <div class=if info.ok { "best" } else { "error" }>
                                        {if info.ok {
                                            format!("{} — {}", info.name, info.author)
                                        } else {
                                            format!("could not start engine: {}", info.error.unwrap_or_default())
                                        }}
                                    </div>
                                    <div class="muted">
                                        {format!("{} UCI option(s)", info.options.len())}
                                    </div>
                                </div>
                            })}
                        </div>
                    })
                }}
                <label>
                    {move || i18n::t(&language.get(), "settings.variant_engine")}
                    <input
                        type="text"
                        placeholder="/path/to/fairy-stockfish"
                        prop:value=move || settings_variant_engine.get()
                        on:input=move |ev| settings_variant_engine.set(event_target_value(&ev))
                    />
                </label>
                <div class="panel-sub">{move || i18n::t(&language.get(), "settings.vision_model")}</div>
                {move || {
                    vision
                        .get()
                        .map(|v| {
                            let size = v.size as f64 / 1_048_576.0;
                            view! {
                                <div class="engine-body">
                                    <div class="muted">
                                        {format!("{} · {} · {:.0} MB", v.name, v.license, size)}
                                    </div>
                                    <div class="muted">{v.sha256.clone()}</div>
                                    <div class="row-btns">
                                        <button
                                            on:click=download_vision
                                            disabled=move || vision_busy.get()
                                        >
                                            {move || {
                                                if vision.get().map(|v| v.installed).unwrap_or(false) {
                                                    i18n::t(&language.get(), "settings.redownload_model")
                                                } else {
                                                    i18n::t(&language.get(), "settings.download_model")
                                                }
                                            }}
                                        </button>
                                        <span class="status">
                                            {move || {
                                                if vision.get().map(|v| v.installed).unwrap_or(false) {
                                                    "installed".to_owned()
                                                } else {
                                                    "not installed".to_owned()
                                                }
                                            }}
                                        </span>
                                    </div>
                                    <div class="muted">{move || vision_msg.get()}</div>
                                </div>
                            }
                        })
                }}
                <div class="panel-sub">{move || i18n::t(&language.get(), "settings.engines")}</div>
                <div class="engine-list">
                    {move || {
                        engines
                            .get()
                            .into_iter()
                            .enumerate()
                            .map(|(i, e)| {
                                let active = settings_engine.get() == e.path;
                                let path = e.path.clone();
                                view! {
                                    <div class="engine-row">
                                        <span class=if active { "engine-name active" } else { "engine-name" }>
                                            {format!("{} — {}", e.name, e.path)}
                                        </span>
                                        <button
                                            on:click=move |_| {
                                                engine_test.set(None);
                                                engine_dialog.set(Some(i));
                                            }
                                        >
                                            {move || i18n::t(&language.get(), "settings.configure")}
                                        </button>
                                        <button
                                            on:click=move |_| {
                                                settings_engine.set(path.clone());
                                                persist_settings();
                                            }
                                        >
                                            {if active {
                                                i18n::t(&language.get(), "common.active")
                                            } else {
                                                i18n::t(&language.get(), "common.use")
                                            }}
                                        </button>
                                        <button
                                            on:click=move |_| {
                                                engines.update(|v| {
                                                    if i < v.len() {
                                                        v.remove(i);
                                                    }
                                                });
                                                persist_settings();
                                            }
                                        >
                                            {move || i18n::t(&language.get(), "settings.remove")}
                                        </button>
                                    </div>
                                }
                            })
                            .collect_view()
                    }}
                </div>
                <div class="row-btns">
                    <input
                        type="text"
                        placeholder={move || i18n::t(&language.get(), "settings.engine_name")}
                        style="max-width:9rem"
                        prop:value=move || new_engine_name.get()
                        on:input=move |ev| new_engine_name.set(event_target_value(&ev))
                    />
                    <input
                        type="text"
                        placeholder="/path/to/engine"
                        prop:value=move || new_engine_path.get()
                        on:input=move |ev| new_engine_path.set(event_target_value(&ev))
                    />
                    <button on:click=add_engine>{move || i18n::t(&language.get(), "settings.add_engine")}</button>
                </div>
                <label class="checkline">
                    <input
                        type="checkbox"
                        prop:checked=move || animate_last_move.get()
                        on:change=move |ev| animate_last_move.set(event_target_checked(&ev))
                    />
                    {move || i18n::t(&language.get(), "settings.animate_last")}
                </label>
                <div class="settings-row">
                    <label class="checkline">
                        <input
                            type="checkbox"
                            prop:checked=move || sound_enabled.get()
                            on:change=move |ev| sound_enabled.set(event_target_checked(&ev))
                        />
                        {move || i18n::t(&language.get(), "settings.sounds")}
                    </label>
                    <label>
                        {move || i18n::t(&language.get(), "settings.sound_theme")}
                        <select
                            prop:value=move || sound_theme.get()
                            on:change=move |ev| sound_theme.set(event_target_value(&ev))
                        >
                            <option value="sfx">"SFX"</option>
                            <option value="piano">"Piano"</option>
                            <option value="nes">"NES"</option>
                            <option value="futuristic">"Futuristic"</option>
                            <option value="custom">"Custom"</option>
                        </select>
                    </label>
                    <label>
                        {move || i18n::t(&language.get(), "settings.volume")}
                        <input
                            type="range"
                            min="0"
                            max="100"
                            prop:value=move || sound_volume.get().to_string()
                            on:input=move |ev| sound_volume.set(event_target_value(&ev).parse().unwrap_or(70))
                        />
                    </label>
                    <label>
                        {move || i18n::t(&language.get(), "settings.animation")}
                        <input
                            type="range"
                            min="0"
                            max="400"
                            step="10"
                            prop:value=move || piece_animation_ms.get().to_string()
                            on:input=move |ev| piece_animation_ms.set(event_target_value(&ev).parse().unwrap_or(0))
                        />
                    </label>
                </div>
                <label class="checkline">
                    {move || i18n::t(&language.get(), "settings.custom_sound_hint")}
                    <input
                        type="file"
                        accept=".ogg,.mp3,.wav,audio/*"
                        on:change=upload_sound
                    />
                </label>
                <div class="muted">{move || sound_msg.get()}</div>
                <div class="panel-sub">{move || i18n::t(&language.get(), "settings.keybinds")}</div>
                <div class="keybinds">
                    <label>
                        {move || i18n::t(&language.get(), "settings.previous")}
                        <input
                            type="text"
                            prop:value=move || keybinds.get().previous
                            on:change=move |ev| {
                                let v = event_target_value(&ev);
                                keybinds.update(|k| k.previous = v);
                            }
                        />
                    </label>
                    <label>
                        {move || i18n::t(&language.get(), "settings.next")}
                        <input
                            type="text"
                            prop:value=move || keybinds.get().next
                            on:change=move |ev| {
                                let v = event_target_value(&ev);
                                keybinds.update(|k| k.next = v);
                            }
                        />
                    </label>
                    <label>
                        {move || i18n::t(&language.get(), "common.start")}
                        <input
                            type="text"
                            prop:value=move || keybinds.get().start
                            on:change=move |ev| {
                                let v = event_target_value(&ev);
                                keybinds.update(|k| k.start = v);
                            }
                        />
                    </label>
                    <label>
                        {move || i18n::t(&language.get(), "settings.end")}
                        <input
                            type="text"
                            prop:value=move || keybinds.get().end
                            on:change=move |ev| {
                                let v = event_target_value(&ev);
                                keybinds.update(|k| k.end = v);
                            }
                        />
                    </label>
                    <label>
                        {move || i18n::t(&language.get(), "settings.flip")}
                        <input
                            type="text"
                            prop:value=move || keybinds.get().flip
                            on:change=move |ev| {
                                let v = event_target_value(&ev);
                                keybinds.update(|k| k.flip = v);
                            }
                        />
                    </label>
                </div>
                <div class="muted">
                    {move || i18n::t(&language.get(), "settings.keybind_hint")}
                </div>
                <div class="row-btns">
                    <button on:click=move |_| persist_settings()>{move || i18n::t(&language.get(), "common.save")}</button>
                    <button on:click=move |_| {
                        keybinds.set(Keybinds::default());
                    }>{move || i18n::t(&language.get(), "settings.reset_keybinds")}</button>
                    <span class="status">{move || settings_status.get()}</span>
                </div>
            </div>
        </section>
    }
    .into_any()
}
