use leptos::prelude::*;
use crate::board::Board;

#[component]
pub fn App() -> impl IntoView {
    view! {
        <main class="knightlm-viewer">
            <Board/>
        </main>
    }
}
