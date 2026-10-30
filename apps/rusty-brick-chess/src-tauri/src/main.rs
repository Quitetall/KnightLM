//! RustyBrickChess desktop shell.
//!
//! Starts the local RustyBrickChess server in-process and points a native
//! window at it, so the desktop app and the browser app are the same code.
//! Native menu items open PGN files and databases through the OS file picker.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;

const DEFAULT_PORT: u16 = 8090;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Installed bundles carry the web assets as a resource.
            if let Ok(resource) = app.path().resource_dir() {
                let candidate = resource.join("frontend");
                if candidate.join("index.html").exists() {
                    // SAFETY: no other thread reads the environment yet.
                    unsafe { std::env::set_var("RBC_STATIC", candidate) };
                }
            }
            let port = start_server();
            wait_for_server(port);
            let url = format!("http://127.0.0.1:{port}/");
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url.parse()?))
                .title("RustyBrickChess")
                .inner_size(1400.0, 950.0)
                .min_inner_size(900.0, 600.0)
                .build()?;

            let handle = app.handle();
            let open_pgn = MenuItemBuilder::with_id("open_pgn", "Open PGN…")
                .accelerator("CmdOrCtrl+O")
                .build(handle)?;
            let open_db = MenuItemBuilder::with_id("open_db", "Open database…")
                .accelerator("CmdOrCtrl+Shift+O")
                .build(handle)?;
            let reload = MenuItemBuilder::with_id("reload", "Reload")
                .accelerator("CmdOrCtrl+R")
                .build(handle)?;
            let quit = PredefinedMenuItem::quit(handle, Some("Quit"))?;
            let file = SubmenuBuilder::new(handle, "File")
                .item(&open_pgn)
                .item(&open_db)
                .separator()
                .item(&quit)
                .build()?;
            let view = SubmenuBuilder::new(handle, "View").item(&reload).build()?;
            let menu = MenuBuilder::new(handle).item(&file).item(&view).build()?;
            app.set_menu(menu)?;
            Ok(())
        })
        .on_menu_event(|app, event| {
            let port = server_port();
            match event.id().0.as_str() {
                "open_pgn" => open_pgn(app, port),
                "open_db" => open_database(app, port),
                "reload" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.eval("window.location.reload()");
                    }
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running RustyBrickChess");
}

fn server_port() -> u16 {
    std::env::var("RBC_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

fn start_server() -> u16 {
    let port = server_port();
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("RustyBrickChess: cannot start runtime: {error}");
                return;
            }
        };
        if let Err(error) =
            runtime.block_on(rusty_brick_chess::serve_on(&format!("127.0.0.1:{port}")))
        {
            eprintln!("RustyBrickChess: server stopped: {error}");
        }
    });
    port
}

fn wait_for_server(port: u16) {
    let health = format!("http://127.0.0.1:{port}/api/health");
    for _ in 0..150 {
        if ureq::get(&health).call().is_ok() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    eprintln!("RustyBrickChess: server did not answer on port {port}");
}

fn notify(app: &AppHandle, message: &str) {
    if let Some(window) = app.get_webview_window("main") {
        let detail = serde_json::to_string(message).unwrap_or_else(|_| "\"\"".into());
        let _ = window.eval(&format!(
            "window.dispatchEvent(new CustomEvent('rbc-imported', {{ detail: {detail} }}))"
        ));
    }
}

fn open_pgn(app: &AppHandle, port: u16) {
    let Some(path) = pick_file(app, "PGN", &["pgn", "PGN"]) else {
        return;
    };
    let pgn = match std::fs::read_to_string(&path) {
        Ok(pgn) => pgn,
        Err(error) => {
            notify(app, &format!("could not read {}: {error}", path.display()));
            return;
        }
    };
    let url = format!("http://127.0.0.1:{port}/api/games");
    match ureq::post(&url).send_json(serde_json::json!({ "pgn": pgn })) {
        Ok(mut response) => {
            let imported = response
                .body_mut()
                .read_json::<serde_json::Value>()
                .ok()
                .and_then(|value| {
                    value
                        .get("imported")
                        .and_then(|ids| ids.as_array())
                        .map(|ids| ids.len())
                })
                .unwrap_or(0);
            notify(app, &format!("imported {imported} game(s) from {}", path.display()));
        }
        Err(error) => notify(app, &format!("import failed: {error}")),
    }
}

fn open_database(app: &AppHandle, port: u16) {
    let Some(path) = pick_file(app, "SQLite database", &["db", "sqlite", "sqlite3"]) else {
        return;
    };
    let Some(name) = path.file_name().and_then(|name| name.to_str()).map(str::to_owned) else {
        notify(app, "database file has no name");
        return;
    };
    let target = rusty_brick_chess::db_dir_from_env().join(&name);
    if target != path {
        if let Err(error) = std::fs::copy(&path, &target) {
            notify(app, &format!("could not copy database: {error}"));
            return;
        }
    }
    let url = format!("http://127.0.0.1:{port}/api/databases/switch");
    match ureq::post(&url).send_json(serde_json::json!({ "name": name })) {
        Ok(_) => notify(app, &format!("database: {name}")),
        Err(error) => notify(app, &format!("could not open database: {error}")),
    }
}

fn pick_file(app: &AppHandle, label: &str, extensions: &[&str]) -> Option<std::path::PathBuf> {
    app.dialog()
        .file()
        .add_filter(label, extensions)
        .blocking_pick_file()
        .and_then(|file| file.as_path().map(std::path::Path::to_path_buf))
}
