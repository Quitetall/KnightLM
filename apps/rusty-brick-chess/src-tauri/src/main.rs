//! RustyBrickChess desktop shell.
//!
//! Starts the local RustyBrickChess server in-process and points a native
//! window at it, so the desktop app and the browser app are the same code.
//! Native menu items open PGN files and databases through the OS file picker.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
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
            let new_window = MenuItemBuilder::with_id("new_window", "New window")
                .accelerator("CmdOrCtrl+N")
                .build(handle)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit")
                .accelerator("CmdOrCtrl+Q")
                .build(handle)?;
            let file = SubmenuBuilder::new(handle, "File")
                .item(&open_pgn)
                .item(&open_db)
                .separator()
                .item(&quit)
                .build()?;
            let view = SubmenuBuilder::new(handle, "View")
                .item(&new_window)
                .separator()
                .item(&reload)
                .build()?;
            let menu = MenuBuilder::new(handle).item(&file).item(&view).build()?;
            app.set_menu(menu)?;
            Ok(())
        })
        .on_menu_event(|app, event| {
            let port = server_port();
            match event.id().0.as_str() {
                "open_pgn" | "open_db" => {
                    // The blocking picker dispatches back to the UI thread.
                    // Keep that thread free to show the dialog and handle input.
                    let app = app.clone();
                    let database = event.id().0 == "open_db";
                    std::thread::spawn(move || {
                        if database {
                            open_database(&app, port);
                        } else {
                            open_pgn(&app, port);
                        }
                    });
                }
                "new_window" => open_window(app),
                "quit" => app.exit(0),
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

/// Open another window onto the same running server.
fn open_window(app: &AppHandle) {
    let port = server_port();
    let Ok(url) = format!("http://127.0.0.1:{port}/").parse() else {
        return;
    };
    let label = format!(
        "main-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let _ = WebviewWindowBuilder::new(app, label, WebviewUrl::External(url))
        .title("RustyBrickChess")
        .inner_size(1200.0, 850.0)
        .min_inner_size(900.0, 600.0)
        .build();
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
        let _ = window.eval(format!(
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
            notify(
                app,
                &format!("imported {imported} game(s) from {}", path.display()),
            );
        }
        Err(error) => notify(app, &format!("import failed: {error}")),
    }
}

fn open_database(app: &AppHandle, port: u16) {
    let Some(path) = pick_file(app, "SQLite database", &["db", "sqlite", "sqlite3"]) else {
        return;
    };
    let Some(name) = path
        .file_stem()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
    else {
        notify(app, "database file has no name");
        return;
    };
    let name = name.trim();
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ' ')
    {
        notify(
            app,
            "database names may use letters, digits, spaces, '-' and '_'",
        );
        return;
    }
    // The server takes a stem and manages .db files, including when the
    // selected external file uses .sqlite or .sqlite3.
    let target = rusty_brick_chess::db_dir_from_env().join(format!("{name}.db"));
    let same_file = match (std::fs::canonicalize(&path), std::fs::canonicalize(&target)) {
        (Ok(source), Ok(target)) => source == target,
        _ => false,
    };
    if !same_file {
        let copy = || -> std::io::Result<()> {
            let mut source = std::fs::File::open(&path)?;
            // Import must never overwrite an existing user database.
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            if let Err(error) = std::io::copy(&mut source, &mut output) {
                let _ = std::fs::remove_file(&target);
                return Err(error);
            }
            Ok(())
        };
        if let Err(error) = copy() {
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
