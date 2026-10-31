use leptos::prelude::*;
use crate::i18n;
use crate::model::*;
use crate::views::AppCtx;

pub fn home(ctx: AppCtx) -> impl IntoView {
    let AppCtx {
        account_busy,
        account_import_max,
        account_msg,
        accounts,
        chesscom_user_input,
        events: _,
        games: _,
        home_recent,
        language,
        lichess_token_input,
        lichess_user_input,
        players,
        profile,
        profile_name,
        profile_player,
        stats,
        status,
        tab,
        ..
    } = ctx;
    let import_account = move |a| ctx.import_account.run(a);
    let link_account = move |a| ctx.link_account.run(a);
    let new_workspace = move || ctx.new_workspace.run(());
    let open_game = move |a| ctx.open_game.run(a);
    let persist_settings = move || ctx.persist_settings.run(());
    let switch_tab = move |a| ctx.switch_tab.run(a);
    let unlink_account = move |a| ctx.unlink_account.run(a);
    let stat = move |value: i64, label: &'static str| {
        view! {
            <div class="home-stat">
                <div class="home-stat-value">{value}</div>
                <div class="muted">{label}</div>
            </div>
        }
    };
    view! {
        <section class="home">
            <div class="panel home-hero">
                <div>
                    <div class="panel-sub">{move || i18n::t(&language.get(), "home.local_account")}</div>
                    <h2 class="home-greeting">
                        {move || format!("Welcome back, {}", profile_name.get())}
                    </h2>
                    <div class="muted">
                        {move || i18n::t(&language.get(), "home.local_hint")}
                    </div>
                </div>
                <div class="home-hero-actions">
                    <input
                        type="text"
                        placeholder=move || i18n::t(&language.get(), "home.display_name")
                        prop:value=move || profile_name.get()
                        on:input=move |ev| profile_name.set(event_target_value(&ev))
                    />
                    <button on:click=move |_| {
                        persist_settings();
                        status.set(i18n::t(&language.get(), "status.m21").into());
                    }>{move || i18n::t(&language.get(), "home.save_profile")}</button>
                </div>
            </div>

            <div class="panel accounts">
                <div class="panel-head"><h2>{move || i18n::t(&language.get(), "home.accounts")}</h2></div>
                <div class="account-grid">
                    {move || {
                        let list = accounts.get();
                        let lichess = list.iter().find(|a| a.provider == "lichess");
                        let profile = lichess.and_then(|a| a.profile.clone());
                        if lichess.map(|a| a.linked).unwrap_or(false) {
                            view! {
                                <div class="account-card">
                                    <div class="account-head">
                                        <span class="account-name">"Lichess"</span>
                                        <span class="best">{profile.as_ref().map(|p| p.username.clone()).unwrap_or_default()}</span>
                                    </div>
                                    <div class="muted">
                                        {profile
                                            .as_ref()
                                            .map(|p| {
                                                let mut parts = Vec::new();
                                                if let Some(name) = p.name.clone() { parts.push(name); }
                                                if let Some(rating) = p.rating { parts.push(format!("{rating} blitz/rapid")); }
                                                if let Some(games) = p.games { parts.push(format!("{games} games")); }
                                                parts.join(" · ")
                                            })
                                            .unwrap_or_default()}
                                    </div>
                                    <div class="row-btns">
                                        <button
                                            disabled=move || account_busy.get()
                                            on:click=move |_| import_account("lichess")
                                        >
                                            {move || i18n::t(&language.get(), "home.import_games")}
                                        </button>
                                        <button on:click=move |_| unlink_account("lichess")>{move || i18n::t(&language.get(), "home.unlink")}</button>
                                    </div>
                                </div>
                            }
                                .into_any()
                        } else {
                            view! {
                                <div class="account-card">
                                    <div class="account-head">
                                        <span class="account-name">"Lichess"</span>
                                    </div>
                                    <div class="account-form">
                                        <input
                                            type="text"
                                            placeholder=move || i18n::t(&language.get(), "home.lichess_user")
                                            prop:value=move || lichess_user_input.get()
                                            on:input=move |ev| lichess_user_input.set(event_target_value(&ev))
                                        />
                                        <input
                                            type="password"
                                            placeholder=move || i18n::t(&language.get(), "home.token")
                                            prop:value=move || lichess_token_input.get()
                                            on:input=move |ev| lichess_token_input.set(event_target_value(&ev))
                                        />
                                        <button
                                            disabled=move || account_busy.get()
                                            on:click=move |_| link_account("lichess")
                                        >
                                            {move || i18n::t(&language.get(), "home.link")}
                                        </button>
                                    </div>
                                    <div class="muted">
                                        {move || i18n::t(&language.get(), "home.lichess_hint")}
                                    </div>
                                </div>
                            }
                                .into_any()
                        }
                    }}
                    {move || {
                        let list = accounts.get();
                        let chesscom = list.iter().find(|a| a.provider == "chesscom");
                        let profile = chesscom.and_then(|a| a.profile.clone());
                        if chesscom.map(|a| a.linked).unwrap_or(false) {
                            view! {
                                <div class="account-card">
                                    <div class="account-head">
                                        <span class="account-name">"Chess.com"</span>
                                        <span class="best">{profile.as_ref().map(|p| p.username.clone()).unwrap_or_default()}</span>
                                    </div>
                                    <div class="muted">
                                        {profile
                                            .as_ref()
                                            .map(|p| {
                                                let mut parts = Vec::new();
                                                if let Some(name) = p.name.clone() { parts.push(name); }
                                                if let Some(rating) = p.rating { parts.push(format!("{rating} blitz/rapid")); }
                                                if let Some(status) = p.status.clone() { parts.push(status); }
                                                parts.join(" · ")
                                            })
                                            .unwrap_or_default()}
                                    </div>
                                    <div class="row-btns">
                                        <button
                                            disabled=move || account_busy.get()
                                            on:click=move |_| import_account("chesscom")
                                        >
                                            {move || i18n::t(&language.get(), "home.import_month")}
                                        </button>
                                        <button on:click=move |_| unlink_account("chesscom")>{move || i18n::t(&language.get(), "home.unlink")}</button>
                                    </div>
                                </div>
                            }
                                .into_any()
                        } else {
                            view! {
                                <div class="account-card">
                                    <div class="account-head">
                                        <span class="account-name">"Chess.com"</span>
                                    </div>
                                    <div class="account-form">
                                        <input
                                            type="text"
                                            placeholder=move || i18n::t(&language.get(), "home.chesscom_user")
                                            prop:value=move || chesscom_user_input.get()
                                            on:input=move |ev| chesscom_user_input.set(event_target_value(&ev))
                                        />
                                        <button
                                            disabled=move || account_busy.get()
                                            on:click=move |_| link_account("chesscom")
                                        >
                                            {move || i18n::t(&language.get(), "home.link")}
                                        </button>
                                    </div>
                                    <div class="muted">
                                        {move || i18n::t(&language.get(), "home.chesscom_hint")}
                                    </div>
                                </div>
                            }
                                .into_any()
                        }
                    }}
                </div>
                <div class="account-actions">
                    <label>
                        {move || i18n::t(&language.get(), "home.import_count")}
                        <input
                            type="number"
                            min="1"
                            max="500"
                            prop:value=move || account_import_max.get().to_string()
                            on:input=move |ev| {
                                account_import_max
                                    .set(event_target_value(&ev).parse().unwrap_or(20))
                            }
                        />
                    </label>
                    <span class="muted">{move || account_msg.get()}</span>
                </div>
            </div>

            <div class="home-stats">
                {move || stat(stats.get().games, "games")}
                {move || stat(stats.get().players, "players")}
                {move || stat(stats.get().events, "events")}
                {move || stat(stats.get().puzzles, "puzzles")}
            </div>

            <div class="home-grid">
                <div class="panel">
                    <div class="panel-head"><h2>{move || i18n::t(&language.get(), "home.recent")}</h2></div>
                    <div class="table">
                        <div class="row head">
                            <span>"ID"</span><span>"White"</span><span>"Black"</span>
                            <span>"Result"</span><span>{move || i18n::t(&language.get(), "db.event")}</span>
                        </div>
                        {move || {
                            home_recent
                                .get()
                                .into_iter()
                                .map(|g| {
                                    let summary = g.clone();
                                    let title = format!("{} vs {} · {}", g.white, g.black, g.result);
                                    view! {
                                        <button
                                            class="row game"
                                            data-game-id=g.id.to_string()
                                            on:click=move |_| {
                                                open_game(summary.clone());
                                                tab.set(Tab::Analysis);
                                            }
                                        >
                                            <span>{g.id.to_string()}</span>
                                            <span>{g.white}</span>
                                            <span>{g.black}</span>
                                            <span>{g.result}</span>
                                            <span class="muted">{g.event.unwrap_or_default()}</span>
                                            <span class="hidden-title">{title}</span>
                                        </button>
                                    }
                                })
                                .collect_view()
                        }}
                    </div>
                    <div class="muted">
                        {move || {
                            if home_recent.get().is_empty() {
                                i18n::t(&language.get(), "home.no_games")
                            } else {
                                ""
                            }
                        }}
                    </div>
                </div>

                <div class="panel">
                    <div class="panel-head"><h2>{move || i18n::t(&language.get(), "home.profile")}</h2></div>
                    <select
                        prop:value=move || profile_player.get()
                        on:change=move |ev| profile_player.set(event_target_value(&ev))
                    >
                        <option value="">{move || i18n::t(&language.get(), "home.choose_player")}</option>
                        {move || {
                            players
                                .get()
                                .into_iter()
                                .map(|p| {
                                    view! {
                                        <option value=p.name.clone()>
                                            {format!("{} ({} games)", p.name, p.games)}
                                        </option>
                                    }
                                })
                                .collect_view()
                        }}
                    </select>
                    {move || {
                        profile
                            .get()
                            .map(|p| {
                                let decided = p.wins + p.draws + p.losses;
                                view! {
                                    <div class="profile-card">
                                        <div class="profile-name">{p.name.clone()}</div>
                                        <div class="profile-record">
                                            {format!("{}W · {}D · {}L", p.wins, p.draws, p.losses)}
                                        </div>
                                        <div class="profile-score">
                                            {format!("{:.1}% score · {} games", p.score_pct, decided)}
                                        </div>
                                        <div class="muted">
                                            {format!("{} as White · {} as Black · {} unfinished", p.as_white, p.as_black, p.unfinished)}
                                        </div>
                                    </div>
                                }
                            })
                    }}
                    <div class="row-btns">
                        <button on:click=move |_| {
                            persist_settings();
                            profile_player.set(profile_player.get_untracked());
                            status.set(i18n::t(&language.get(), "status.m18").into());
                        }>{move || i18n::t(&language.get(), "home.use_account")}</button>
                    </div>
                    <div class="muted">
                        {move || {
                            if profile_player.get().trim().is_empty() {
                                i18n::t(&language.get(), "home.pick_player")
                            } else {
                                ""
                            }
                        }}
                    </div>
                </div>
            </div>

            <div class="panel">
                <div class="panel-head"><h2>{move || i18n::t(&language.get(), "home.quick")}</h2></div>
                <div class="row-btns">
                    <button on:click=move |_| switch_tab(Tab::Database)>{move || i18n::t(&language.get(), "home.import")}</button>
                    <button on:click=move |_| new_workspace()>{move || i18n::t(&language.get(), "home.new_analysis")}</button>
                    <button on:click=move |_| switch_tab(Tab::Puzzles)>{move || i18n::t(&language.get(), "home.solve")}</button>
                    <button on:click=move |_| switch_tab(Tab::Openings)>{move || i18n::t(&language.get(), "home.train")}</button>
                    <button on:click=move |_| switch_tab(Tab::Settings)>{move || i18n::t(&language.get(), "nav.settings")}</button>
                </div>
            </div>
        </section>
    }
}
