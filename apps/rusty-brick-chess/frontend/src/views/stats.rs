//! Read-only views extracted from the app shell.

use leptos::prelude::*;

use crate::api;
use crate::i18n;

#[derive(Clone, Copy)]
pub struct StatsCtx {
    pub language: RwSignal<String>,
    pub stats: RwSignal<api::Stats>,
    pub db_list: RwSignal<Vec<api::DatabaseInfo>>,
}

pub fn stats_view(ctx: StatsCtx) -> impl IntoView {

    let card = |label: &'static str, value: i64| {
        view! {
            <div class="stat-card">
                <div class="stat-label">{label}</div>
                <div class="stat-value">{value.to_string()}</div>
            </div>
        }
    };
    let s = ctx.stats.get();
    view! {
        <section class="panel">
            <div class="panel-head">
                <h2>{move || i18n::t(&ctx.language.get(), "db.statistics")}</h2>
                <span class="muted">{move || ctx.db_list.get().into_iter().find(|d| d.current).map(|d| d.name).unwrap_or_default()}</span>
            </div>
            <div class="stat-grid">
                {card(i18n::t(&ctx.language.get(), "db.games"), s.games)}
                {card(i18n::t(&ctx.language.get(), "db.white_wins"), s.white_wins)}
                {card(i18n::t(&ctx.language.get(), "db.draws"), s.draws)}
                {card(i18n::t(&ctx.language.get(), "db.black_wins"), s.black_wins)}
                {card(i18n::t(&ctx.language.get(), "db.unfinished"), s.unfinished)}
                {card(i18n::t(&ctx.language.get(), "db.players"), s.players)}
                {card(i18n::t(&ctx.language.get(), "db.events"), s.events)}
                {card(i18n::t(&ctx.language.get(), "db.collections"), s.collections)}
                {card(i18n::t(&ctx.language.get(), "nav.puzzles"), s.puzzles)}
            </div>
            <div class="catalog-row">
                <div class="catalog">
                    <div class="panel-sub">{move || i18n::t(&ctx.language.get(), "db.most_active")}</div>
                    {move || {
                        ctx.stats
                            .get()
                            .top_players
                            .into_iter()
                            .map(|p| {
                                view! {
                                    <span class="muted">
                                        {format!(
                                            "{} — {} games{}",
                                            p.name,
                                            p.games,
                                            p.elo.map(|e| format!(" · {e}")).unwrap_or_default(),
                                        )}
                                    </span>
                                }
                            })
                            .collect_view()
                    }}
                </div>
                <div class="catalog">
                    <div class="panel-sub">{move || i18n::t(&ctx.language.get(), "db.events")}</div>
                    {move || {
                        ctx.stats
                            .get()
                            .top_events
                            .into_iter()
                            .map(|e| view! { <span class="muted">{format!("{} — {}", e.name, e.games)}</span> })
                            .collect_view()
                    }}
                </div>
            </div>
        </section>
    }
    .into_any()
}
