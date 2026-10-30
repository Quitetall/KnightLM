pub mod app;
pub mod board;
pub use app::App;

use leptos::prelude::*;

/// WASM entry point — cargo-leptos builds the lib target to wasm, so the
/// mount must live here, not in main.rs.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <App/> });
}
