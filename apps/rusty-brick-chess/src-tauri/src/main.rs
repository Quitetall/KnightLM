//! RustyBrickChess desktop shell.
//!
//! Starts the local RustyBrickChess server in-process on a random loopback
//! port, behind a per-launch token, and points a native window at it, so the
//! desktop app and the browser app are the same code. Native menus, drag and
//! drop and `.pgn` file association feed PGN files to the running server.
//!
//! Established Tauri mechanisms only: single-instance, window-state, dialog and
//! opener plugins, the app data directory, `fileAssociations` in
//! `tauri.conf.json`, `RunEvent::Exit` for shutdown.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use tauri::menu::{
    AboutMetadata, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder,
};
use tauri::{
    AppHandle, DragDropEvent, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use rusty_brick_chess::launch_guard::{TOKEN_HEADER, TOKEN_PARAM};

/// The running in-process server: where it listens, its token, how to stop it.
struct Server {
    port: u16,
    token: String,
    stop: tokio::sync::watch::Sender<bool>,
    finished: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

fn main() {
    let app = tauri::Builder::default()
        // A second launch (menu, icon, or `.pgn` double click) focuses the
        // running app and hands it the files.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            focus_main(app);
            open_paths(app, pgn_args(argv.iter().skip(1)));
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|window, event| {
            if let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
                open_paths(window.app_handle(), pgn_args(paths.iter()));
            }
        })
        .setup(|app| {
            configure_environment(app.handle());
            let server = start_server()?;
            let url = format!(
                "http://127.0.0.1:{}/?{TOKEN_PARAM}={}",
                server.port, server.token
            );
            println!("rbc-desktop: server on http://127.0.0.1:{}/", server.port);
            if std::env::var_os("RBC_DESKTOP_PRINT_URL").is_some() {
                println!("rbc-desktop: window url {url}");
            }
            app.manage(server);
            build_window(app.handle(), "main", 1400.0, 950.0, &url)?;
            build_menu(app.handle())?;
            quit_on_signal(app.handle().clone());
            open_paths(app.handle(), pgn_args(std::env::args().skip(1)));
            Ok(())
        })
        .on_menu_event(|app, event| match event.id().0.as_str() {
            "open_pgn" => {
                if let Some(path) = pick_file(app, "PGN", &["pgn", "PGN"]) {
                    open_paths(app, vec![path]);
                }
            }
            "open_db" => open_database(app),
            "choose_engine" => choose_engine(app),
            "new_window" => open_window(app),
            "reload" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.eval("window.location.reload()");
                }
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("error while building RustyBrickChess");

    app.run(|app, event| match event {
        // macOS delivers double-clicked `.pgn` files here, not in argv.
        #[cfg(target_os = "macos")]
        RunEvent::Opened { urls } => {
            let paths = urls
                .iter()
                .filter_map(|url| url.to_file_path().ok())
                .collect::<Vec<_>>();
            open_paths(app, pgn_args(paths.iter()));
        }
        RunEvent::Exit => stop_server(app),
        _ => {}
    });
}

/// SIGTERM/SIGINT (logout, `kill`, Ctrl+C in a terminal) quit through the
/// normal exit path so the server and its engines are shut down.
#[cfg(unix)]
fn quit_on_signal(app: AppHandle) {
    use tokio::signal::unix::{SignalKind, signal};
    std::thread::spawn(move || {
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };
        runtime.block_on(async {
            let (Ok(mut term), Ok(mut int)) = (
                signal(SignalKind::terminate()),
                signal(SignalKind::interrupt()),
            ) else {
                return;
            };
            tokio::select! { _ = term.recv() => {}, _ = int.recv() => {} }
        });
        app.exit(0);
    });
}

#[cfg(not(unix))]
fn quit_on_signal(_app: AppHandle) {}

/// Point the server at per-user data, assets and an engine before it starts.
fn configure_environment(app: &AppHandle) {
    // SAFETY: called from setup before the server thread exists.
    unsafe {
        // Installed bundles carry the web assets as a resource.
        if let Ok(resource) = app.path().resource_dir() {
            let candidate = resource.join("frontend");
            if candidate.join("index.html").exists() {
                std::env::set_var("RBC_STATIC", candidate);
            }
            // ...and the model server's Python code, for Play AI.
            let code = resource.join("model-runtime");
            if code.join("model/serve/server.py").exists()
                && std::env::var_os("RBC_MODEL_CODE").is_none()
            {
                std::env::set_var("RBC_MODEL_CODE", code);
            }
        }
        // The database lives in the app data directory, never the cwd. An
        // existing database at the browser-mode default stays in use.
        if std::env::var_os("RBC_DB").is_none() {
            let legacy = rusty_brick_chess::default_db_path();
            let chosen = match app.path().app_data_dir() {
                Ok(dir) if !legacy.exists() => dir.join("rusty-brick-chess.db"),
                _ => legacy,
            };
            std::env::set_var("RBC_DB", chosen);
        }
        // Stockfish is not bundled: use the one on PATH unless told otherwise.
        // A path chosen in Settings or the File menu still wins.
        if std::env::var_os("STOCKFISH_PATH").is_none()
            && let Some(engine) = find_engine()
        {
            std::env::set_var("STOCKFISH_PATH", engine);
        }
    }
}

/// `stockfish` on PATH, then the usual install locations.
fn find_engine() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "stockfish.exe"
    } else {
        "stockfish"
    };
    let on_path = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default();
    let usual = [
        "/usr/games",
        "/usr/local/bin",
        "/opt/homebrew/bin",
        "/snap/bin",
    ]
    .map(PathBuf::from);
    on_path
        .into_iter()
        .chain(usual)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

fn start_server() -> Result<Server, Box<dyn std::error::Error>> {
    let token = match std::env::var("RBC_DESKTOP_TOKEN") {
        Ok(token) if token.len() >= 16 => token,
        _ => rusty_brick_chess::launch_guard::new_token()?,
    };
    // Loopback only; port 0 lets the OS pick a free one. RBC_PORT pins it.
    let requested = std::env::var("RBC_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(0);
    let listener = std::net::TcpListener::bind(("127.0.0.1", requested))?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    let (stop, mut stopped) = tokio::sync::watch::channel(false);
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let server_token = token.clone();
    std::thread::Builder::new()
        .name("rbc-server".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            let mut graceful = stopped.clone();
            let result = runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener)?;
                let serve =
                    rusty_brick_chess::serve_listener(listener, Some(server_token), async move {
                        let _ = graceful.changed().await;
                    });
                tokio::select! {
                    result = serve => result,
                    // Open event streams can hold graceful shutdown; bound it.
                    _ = async {
                        let _ = stopped.changed().await;
                        tokio::time::sleep(Duration::from_secs(3)).await;
                    } => Ok(()),
                }
            });
            if let Err(error) = result {
                eprintln!("RustyBrickChess: server stopped: {error}");
            }
            // Dropping the runtime drops server state and its engine children.
            drop(runtime);
            let _ = done_tx.send(());
        })?;
    wait_for_server(port, &token)?;
    Ok(Server {
        port,
        token,
        stop,
        finished: Mutex::new(Some(done_rx)),
    })
}

fn wait_for_server(port: u16, token: &str) -> Result<(), String> {
    let health = format!("http://127.0.0.1:{port}/api/health");
    for _ in 0..150 {
        if ureq::get(&health)
            .header(TOKEN_HEADER, token)
            .call()
            .is_ok()
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!("the server did not answer on port {port}"))
}

/// Stop the server and wait (bounded) for it and its engines to go away.
fn stop_server(app: &AppHandle) {
    // A managed model server must not outlive the app.
    rusty_brick_chess::model_runtime::shutdown_all();
    if let Some(server) = app.try_state::<Server>() {
        let _ = server.stop.send(true);
        let finished = server.finished.lock().ok().and_then(|mut slot| slot.take());
        if let Some(finished) = finished {
            let _ = finished.recv_timeout(Duration::from_secs(5));
        }
    }
}

fn build_window(
    app: &AppHandle,
    label: &str,
    width: f64,
    height: f64,
    url: &str,
) -> tauri::Result<()> {
    let nav_handle = app.clone();
    let window = WebviewWindowBuilder::new(
        app,
        label,
        WebviewUrl::External(
            url.parse()
                .map_err(|error: url::ParseError| tauri::Error::Anyhow(error.into()))?,
        ),
    )
    .title("RustyBrickChess")
    .inner_size(width, height)
    .min_inner_size(900.0, 600.0)
    // Links to other sites open in the system browser, not in this window.
    .on_navigation(move |target| {
        let local = (matches!(target.scheme(), "http" | "https")
            && target.host_str() == Some("127.0.0.1"))
            || matches!(target.scheme(), "about" | "data" | "blob" | "tauri");
        if !local && matches!(target.scheme(), "http" | "https" | "mailto") {
            let _ = nav_handle.opener().open_url(target.as_str(), None::<&str>);
        }
        local
    })
    .build()?;
    if let Some(icon) = app.default_window_icon() {
        let _ = window.set_icon(icon.clone());
    }
    Ok(())
}

/// Another window onto the same running server.
fn open_window(app: &AppHandle) {
    let Some(server) = app.try_state::<Server>() else {
        return;
    };
    let url = format!(
        "http://127.0.0.1:{}/?{TOKEN_PARAM}={}",
        server.port, server.token
    );
    let label = format!(
        "main-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let _ = build_window(app, &label, 1200.0, 850.0, &url);
}

fn focus_main(app: &AppHandle) {
    let window = app
        .get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next());
    if let Some(window) = window {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn build_menu(app: &AppHandle) -> tauri::Result<()> {
    let about = AboutMetadata {
        name: Some("RustyBrickChess".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        license: Some("AGPL-3.0-only".into()),
        comments: Some("Chess database, analysis and training.".into()),
        ..Default::default()
    };
    let open_pgn = MenuItemBuilder::with_id("open_pgn", "Open PGN…")
        .accelerator("CmdOrCtrl+O")
        .build(app)?;
    let open_db = MenuItemBuilder::with_id("open_db", "Open database…")
        .accelerator("CmdOrCtrl+Shift+O")
        .build(app)?;
    let engine = MenuItemBuilder::with_id("choose_engine", "Choose engine…").build(app)?;
    let new_window = MenuItemBuilder::with_id("new_window", "New window")
        .accelerator("CmdOrCtrl+N")
        .build(app)?;
    let reload = MenuItemBuilder::with_id("reload", "Reload")
        .accelerator("CmdOrCtrl+R")
        .build(app)?;

    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut menu = MenuBuilder::new(app);
    #[cfg(target_os = "macos")]
    {
        let app_menu = SubmenuBuilder::new(app, "RustyBrickChess")
            .item(&PredefinedMenuItem::about(app, None, Some(about.clone()))?)
            .separator()
            .item(&PredefinedMenuItem::hide(app, None)?)
            .item(&PredefinedMenuItem::hide_others(app, None)?)
            .item(&PredefinedMenuItem::show_all(app, None)?)
            .separator()
            .item(&PredefinedMenuItem::quit(app, None)?)
            .build()?;
        menu = menu.item(&app_menu);
    }
    let mut file = SubmenuBuilder::new(app, "File")
        .item(&open_pgn)
        .item(&open_db)
        .separator()
        .item(&engine)
        .separator()
        .item(&PredefinedMenuItem::close_window(app, None)?);
    if !cfg!(target_os = "macos") {
        file = file.item(&PredefinedMenuItem::quit(app, None)?);
    }
    let edit = SubmenuBuilder::new(app, "Edit")
        .item(&PredefinedMenuItem::undo(app, None)?)
        .item(&PredefinedMenuItem::redo(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, None)?)
        .item(&PredefinedMenuItem::copy(app, None)?)
        .item(&PredefinedMenuItem::paste(app, None)?)
        .item(&PredefinedMenuItem::select_all(app, None)?)
        .build()?;
    let view = SubmenuBuilder::new(app, "View")
        .item(&new_window)
        .separator()
        .item(&reload)
        .item(&PredefinedMenuItem::fullscreen(app, None)?)
        .build()?;
    let window = SubmenuBuilder::new(app, "Window")
        .item(&PredefinedMenuItem::minimize(app, None)?)
        .item(&PredefinedMenuItem::maximize(app, None)?)
        .build()?;
    let mut help = SubmenuBuilder::new(app, "Help");
    if !cfg!(target_os = "macos") {
        help = help.item(&PredefinedMenuItem::about(app, None, Some(about))?);
    }
    let menu = menu
        .item(&file.build()?)
        .item(&edit)
        .item(&view)
        .item(&window)
        .item(&help.build()?)
        .build()?;
    app.set_menu(menu)?;
    Ok(())
}

/// Keep `.pgn` paths out of a launcher's arguments (flags and other files drop out).
fn pgn_args<I, P>(args: I) -> Vec<PathBuf>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    args.into_iter()
        .map(|arg| arg.as_ref().to_path_buf())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("pgn"))
                && path.is_file()
        })
        .collect()
}

fn notify(app: &AppHandle, message: &str) {
    if let Some(window) = app.get_webview_window("main") {
        let detail = serde_json::to_string(message).unwrap_or_else(|_| "\"\"".into());
        let _ = window.eval(format!(
            "window.dispatchEvent(new CustomEvent('rbc-imported', {{ detail: {detail} }}))"
        ));
    }
}

fn api_post(
    app: &AppHandle,
    path: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let server = app.try_state::<Server>().ok_or("server not running")?;
    let url = format!("http://127.0.0.1:{}{path}", server.port);
    let mut response = ureq::post(&url)
        .header(TOKEN_HEADER, &server.token)
        .send_json(body)
        .map_err(|error| error.to_string())?;
    response
        .body_mut()
        .read_json::<serde_json::Value>()
        .map_err(|error| error.to_string())
}

/// Import PGN files into the open database, off the UI thread.
fn open_paths(app: &AppHandle, paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        for path in paths {
            let pgn = match std::fs::read_to_string(&path) {
                Ok(pgn) => pgn,
                Err(error) => {
                    notify(&app, &format!("could not read {}: {error}", path.display()));
                    continue;
                }
            };
            match api_post(&app, "/api/games", serde_json::json!({ "pgn": pgn })) {
                Ok(value) => {
                    let imported = value
                        .get("imported")
                        .and_then(|ids| ids.as_array())
                        .map_or(0, Vec::len);
                    notify(
                        &app,
                        &format!("imported {imported} game(s) from {}", path.display()),
                    );
                }
                Err(error) => notify(&app, &format!("import failed: {error}")),
            }
        }
    });
}

fn open_database(app: &AppHandle) {
    let Some(path) = pick_file(app, "SQLite database", &["db", "sqlite", "sqlite3"]) else {
        return;
    };
    let Some(name) = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
    else {
        notify(app, "database file has no name");
        return;
    };
    let target = rusty_brick_chess::db_dir_from_env().join(&name);
    if target != path
        && let Err(error) = std::fs::copy(&path, &target)
    {
        notify(app, &format!("could not copy database: {error}"));
        return;
    }
    match api_post(
        app,
        "/api/databases/switch",
        serde_json::json!({ "name": name }),
    ) {
        Ok(_) => notify(app, &format!("database: {name}")),
        Err(error) => notify(app, &format!("could not open database: {error}")),
    }
}

/// Let the user point at a UCI engine; saved in the app's own settings.
fn choose_engine(app: &AppHandle) {
    let Some(path) = pick_file(app, "UCI engine", &["*"]) else {
        return;
    };
    let server = app.try_state::<Server>();
    let Some(server) = server else { return };
    let url = format!("http://127.0.0.1:{}/api/settings", server.port);
    let result = ureq::put(&url)
        .header(TOKEN_HEADER, &server.token)
        .send_json(serde_json::json!({ "engine_path": path.display().to_string() }));
    match result {
        Ok(_) => notify(app, &format!("engine: {}", path.display())),
        Err(error) => notify(app, &format!("could not set engine: {error}")),
    }
}

fn pick_file(app: &AppHandle, label: &str, extensions: &[&str]) -> Option<PathBuf> {
    let mut dialog = app.dialog().file();
    if extensions != ["*"] {
        dialog = dialog.add_filter(label, extensions);
    }
    dialog
        .blocking_pick_file()
        .and_then(|file| file.as_path().map(Path::to_path_buf))
}
