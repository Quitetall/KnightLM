use leptos::prelude::*;
use crate::i18n;
use crate::views::AppCtx;

pub fn database(ctx: AppCtx) -> impl IntoView {
    let AppCtx {
        active_collection,
        collection_name,
        collections,
        db_list,
        event_filter,
        events,
        filter_date_from,
        filter_date_to,
        filter_elo_max,
        filter_elo_min,
        filter_result,
        filter_variant,
        games,
        import_text,
        language,
        manage_collection,
        new_collection,
        new_database,
        online_max,
        online_provider,
        online_user,
        partial_search,
        players,
        position_fen,
        position_variant,
        query,
        recent_files,
        sort_by,
        total,
        ..
    } = ctx;
    let add_collection_clicked = move |a| ctx.add_collection_clicked.run(a);
    let create_db = move |a| ctx.create_db.run(a);
    let delete_collection = move |a| ctx.delete_collection.run(a);
    let import = move |a| ctx.import.run(a);
    let import_online = move |a| ctx.import_online.run(a);
    let on_file_pick = move |a| ctx.on_file_pick.run(a);
    let open_game = move |a| ctx.open_game.run(a);
    let reload = move || ctx.reload.run(());
    let rename_collection = move |a| ctx.rename_collection.run(a);
    let search_position = move |a| ctx.search_position.run(a);
    let switch_db = move |a| ctx.switch_db.run(a);
    view! {
        <section class="panel">
            <div class="panel-head">
                <h2>{move || i18n::t(&language.get(), "db.database")}</h2>
                <span class="muted">{move || format!("{} games", total.get())}</span>
            </div>
            <div class="dbs">
                <label>
                    {move || i18n::t(&language.get(), "db.database")}
                    <select
                        prop:value=move || {
                            db_list
                                .get()
                                .into_iter()
                                .find(|d| d.current)
                                .map(|d| d.name)
                                .unwrap_or_default()
                        }
                        on:change=switch_db
                    >
                        {move || {
                            db_list
                                .get()
                                .into_iter()
                                .map(|d| {
                                    let name = d.name;
                                    view! { <option value=name.clone()>{name.clone()}</option> }
                                })
                                .collect_view()
                        }}
                    </select>
                </label>
                <input
                    type="text"
                    placeholder=move || i18n::t(&language.get(), "db.new_database")
                    prop:value=move || new_database.get()
                    on:input=move |ev| new_database.set(event_target_value(&ev))
                />
                <button on:click=create_db>{move || i18n::t(&language.get(), "common.create")}</button>
            </div>
            <div class="import">
                <textarea
                    placeholder=move || i18n::t(&language.get(), "db.paste_hint")
                    prop:value=move || import_text.get()
                    on:input=move |ev| import_text.set(event_target_value(&ev))
                ></textarea>
                <div class="import-actions">
                    <button on:click=import>{move || i18n::t(&language.get(), "common.import")}</button>
                    <label class="file-button">
                        {move || i18n::t(&language.get(), "db.import_file")}
                        <input
                            type="file"
                            accept=".pgn,application/x-chess-pgn,text/plain"
                            multiple
                            on:change=on_file_pick
                        />
                    </label>
                </div>
            </div>
            {move || {
                let files = recent_files.get();
                (!files.is_empty()).then(|| {
                    view! {
                        <div class="recent-files">
                            <span class="panel-sub">{move || i18n::t(&language.get(), "db.recent_files")}</span>
                            {files
                                .into_iter()
                                .map(|(name, games)| {
                                    view! {
                                        <div class="muted">
                                            {format!("{name} · {games} game(s)")}
                                        </div>
                                    }
                                })
                                .collect_view()}
                        </div>
                    }
                })
            }}
            <div class="collections">
                <button
                    class=move || if active_collection.get().is_none() { "chip active" } else { "chip" }
                    on:click=move |_| {
                        active_collection.set(None);
                        reload();
                    }
                >
                    {move || i18n::t(&language.get(), "db.all_games")}
                </button>
                {move || {
                    collections
                        .get()
                        .into_iter()
                        .map(|c| {
                            let id = c.id;
                            view! {
                                <button
                                    class=move || {
                                        if active_collection.get() == Some(id) { "chip active" } else { "chip" }
                                    }
                                    on:click=move |_| {
                                        active_collection.set(Some(id));
                                        reload();
                                    }
                                >
                                    {format!("{} ({})", c.name, c.games)}
                                </button>
                            }
                        })
                        .collect_view()
                }}
                <input
                    type="text"
                    placeholder=move || i18n::t(&language.get(), "db.new_collection")
                    prop:value=move || new_collection.get()
                    on:input=move |ev| new_collection.set(event_target_value(&ev))
                />
                <button on:click=add_collection_clicked>{move || i18n::t(&language.get(), "common.create")}</button>
            </div>
            <div class="dbs">
                <select
                    prop:value=move || manage_collection.get().to_string()
                    on:change=move |ev| {
                        manage_collection.set(event_target_value(&ev).parse().unwrap_or(0))
                    }
                >
                    <option value="0">{move || i18n::t(&language.get(), "db.manage")}</option>
                    {move || {
                        collections
                            .get()
                            .into_iter()
                            .map(|c| {
                                view! { <option value=c.id.to_string()>{c.name.clone()}</option> }
                            })
                            .collect_view()
                    }}
                </select>
                <input
                    type="text"
                    placeholder=move || i18n::t(&language.get(), "db.new_name")
                    prop:value=move || collection_name.get()
                    on:input=move |ev| collection_name.set(event_target_value(&ev))
                />
                <button on:click=rename_collection>{move || i18n::t(&language.get(), "common.rename")}</button>
                <button on:click=delete_collection>{move || i18n::t(&language.get(), "common.delete")}</button>
            </div>
            <div class="load">
                <input
                    type="text"
                    placeholder=move || i18n::t(&language.get(), "db.position_ph")
                    prop:value=move || position_fen.get()
                    on:input=move |ev| position_fen.set(event_target_value(&ev))
                />
                <select
                    prop:value=move || position_variant.get()
                    on:change=move |ev| position_variant.set(event_target_value(&ev))
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
                <label class="checkline">
                    <input
                        type="checkbox"
                        prop:checked=move || partial_search.get()
                        on:change=move |ev| partial_search.set(event_target_checked(&ev))
                    />
                    {move || i18n::t(&language.get(), "db.partial")}
                </label>
                <button on:click=search_position>
                    {move || i18n::t(&language.get(), "db.find_position")}
                </button>
                <button on:click=move |_| {
                    position_fen.set(String::new());
                    event_filter.set(None);
                    query.set(String::new());
                    filter_date_from.set(String::new());
                    filter_date_to.set(String::new());
                    filter_result.set(String::new());
                    filter_elo_min.set(String::new());
                    filter_elo_max.set(String::new());
                    filter_variant.set("any".into());
                    sort_by.set("newest".into());
                    reload();
                }>
                    {move || i18n::t(&language.get(), "db.clear_filters")}
                </button>
            </div>
            <div class="filters">
                <label>
                    {move || i18n::t(&language.get(), "db.from")}
                    <input
                        type="date"
                        prop:value=move || filter_date_from.get()
                        on:change=move |ev| filter_date_from.set(event_target_value(&ev))
                    />
                </label>
                <label>
                    {move || i18n::t(&language.get(), "db.to")}
                    <input
                        type="date"
                        prop:value=move || filter_date_to.get()
                        on:change=move |ev| filter_date_to.set(event_target_value(&ev))
                    />
                </label>
                <label>
                    {move || i18n::t(&language.get(), "db.result")}
                    <select
                        prop:value=move || filter_result.get()
                        on:change=move |ev| filter_result.set(event_target_value(&ev))
                    >
                        <option value="">"Any"</option>
                        <option value="1-0">{move || i18n::t(&language.get(), "db.white_wins")}</option>
                        <option value="0-1">{move || i18n::t(&language.get(), "db.black_wins")}</option>
                        <option value="1/2-1/2">{move || i18n::t(&language.get(), "db.draw")}</option>
                        <option value="*">{move || i18n::t(&language.get(), "db.unfinished")}</option>
                    </select>
                </label>
                <label>
                    {move || i18n::t(&language.get(), "db.elo")}
                    <input
                        type="number"
                        placeholder="min"
                        min="0"
                        max="4000"
                        prop:value=move || filter_elo_min.get()
                        on:input=move |ev| filter_elo_min.set(event_target_value(&ev))
                    />
                    <input
                        type="number"
                        placeholder="max"
                        min="0"
                        max="4000"
                        prop:value=move || filter_elo_max.get()
                        on:input=move |ev| filter_elo_max.set(event_target_value(&ev))
                    />
                </label>
                <label>
                    {move || i18n::t(&language.get(), "db.variant")}
                    <select
                        prop:value=move || filter_variant.get()
                        on:change=move |ev| filter_variant.set(event_target_value(&ev))
                    >
                        <option value="any">"Any"</option>
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
                <label>
                    {move || i18n::t(&language.get(), "db.sort")}
                    <select
                        prop:value=move || sort_by.get()
                        on:change=move |ev| sort_by.set(event_target_value(&ev))
                    >
                        <option value="newest">"Newest"</option>
                        <option value="oldest">{move || i18n::t(&language.get(), "db.sort_oldest")}</option>
                        <option value="date">{move || i18n::t(&language.get(), "db.sort_date")}</option>
                        <option value="white">"White"</option>
                        <option value="black">"Black"</option>
                        <option value="length">{move || i18n::t(&language.get(), "db.sort_longest")}</option>
                    </select>
                </label>
                <button on:click=move |_| reload()>
                    {move || i18n::t(&language.get(), "db.apply_filters")}
                </button>
            </div>
            {move || {
                event_filter
                    .get()
                    .map(|e| {
                        view! {
                            <div class="muted">
                                {format!("Event: {e}")}
                                <button
                                    class="link"
                                    on:click=move |_| {
                                        event_filter.set(None);
                                        reload();
                                    }
                                >
                                    "clear"
                                </button>
                            </div>
                        }
                    })
            }}
            <div class="dbs">
                <select
                    prop:value=move || online_provider.get()
                    on:change=move |ev| online_provider.set(event_target_value(&ev))
                >
                    <option value="lichess">"Lichess"</option>
                    <option value="chesscom">"Chess.com"</option>
                </select>
                <input
                    type="text"
                    placeholder=move || i18n::t(&language.get(), "db.username")
                    prop:value=move || online_user.get()
                    on:input=move |ev| online_user.set(event_target_value(&ev))
                />
                <input
                    type="number"
                    min="1"
                    max="200"
                    style="max-width:6rem"
                    prop:value=move || online_max.get().to_string()
                    on:input=move |ev| online_max.set(event_target_value(&ev).parse().unwrap_or(30))
                />
                <button on:click=import_online>{move || i18n::t(&language.get(), "db.import_online")}</button>
            </div>
            <div class="table">
                <div class="row head">
                    <span>"#"</span><span>"White"</span><span>"Black"</span>
                    <span>"Result"</span><span>{move || i18n::t(&language.get(), "db.event")}</span>
                </div>
                {move || {
                    games
                        .get()
                        .into_iter()
                        .map(|g| {
                            let clicked = g.clone();
                            view! {
                                <button class="row game" data-game-id=g.id.to_string() on:click=move |_| open_game(clicked.clone())>
                                    <span>{g.id.to_string()}</span>
                                    <span>{g.white.clone()}</span>
                                    <span>{g.black.clone()}</span>
                                    <span>{g.result.clone()}</span>
                                    <span>{g.event.clone().unwrap_or_default()}</span>
                                </button>
                            }
                        })
                        .collect_view()
                }}
            </div>
            <div class="catalog-row">
                <div class="catalog">
                    <div class="panel-sub">{move || i18n::t(&language.get(), "db.players")}</div>
                    {move || {
                        players
                            .get()
                            .into_iter()
                            .take(8)
                            .map(|p| {
                                let name = p.name.clone();
                                view! {
                                    <button
                                        class="link"
                                        on:click=move |_| {
                                            query.set(name.clone());
                                            reload();
                                        }
                                    >
                                        {format!("{} ({})", p.name, p.games)}
                                    </button>
                                }
                            })
                            .collect_view()
                    }}
                </div>
                <div class="catalog">
                    <div class="panel-sub">{move || i18n::t(&language.get(), "db.events")}</div>
                    {move || {
                        events
                            .get()
                            .into_iter()
                            .take(8)
                            .map(|e| {
                                let name = e.name.clone();
                                let games = e.games;
                                let clicked = name.clone();
                                view! {
                                    <button
                                        class="link"
                                        on:click=move |_| {
                                            event_filter.set(Some(clicked.clone()));
                                            reload();
                                        }
                                    >
                                        {format!("{name} ({games})")}
                                    </button>
                                }
                            })
                            .collect_view()
                    }}
                </div>
            </div>
        </section>
    }
    .into_any()
}
