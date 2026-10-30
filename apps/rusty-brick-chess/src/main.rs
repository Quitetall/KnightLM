//! RustyBrickChess — browser/server binary.
//!
//! Runs the local server. `rustybrickchess --open` also opens the app in the
//! default browser (and reuses an instance that is already running), which is
//! what the desktop launcher uses.

const HELP: &str = "\
RustyBrickChess — chess database, analysis and training.

Usage:
  rustybrickchess            serve on RBC_ADDR (default 127.0.0.1:8090)
  rustybrickchess --open     serve, then open the app in the default browser
  rustybrickchess --version  print the version
  rustybrickchess --help     show this message

Environment:
  RBC_DB        SQLite database path (default: $XDG_DATA_HOME/rustybrickchess)
  RBC_DATA      data directory for models and sounds
  RBC_ADDR      bind address (default 127.0.0.1:8090)
  RBC_DEPTH     default analysis depth (default 15)
  STOCKFISH_PATH path to a UCI engine
  MODEL_URL     AI chat endpoint
";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut open = false;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--open" | "open" => open = true,
            "--help" | "-h" => {
                println!("{HELP}");
                return Ok(());
            }
            "--version" | "-V" => {
                println!("rustybrickchess {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            other => {
                eprintln!("unknown argument: {other}\n\n{HELP}");
                std::process::exit(2);
            }
        }
    }

    let addr = std::env::var("RBC_ADDR").unwrap_or_else(|_| "127.0.0.1:8090".into());

    if open {
        if tokio::net::TcpStream::connect(&addr).await.is_ok() {
            println!("RustyBrickChess is already running on http://{addr}");
            open_browser(&addr);
            return Ok(());
        }
        let wait_addr = addr.clone();
        tokio::spawn(async move {
            for _ in 0..150 {
                if tokio::net::TcpStream::connect(&wait_addr).await.is_ok() {
                    open_browser(&wait_addr);
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            eprintln!("RustyBrickChess: server did not start; open http://{wait_addr} manually");
        });
    }

    rusty_brick_chess::serve_from_env().await
}

fn open_browser(addr: &str) {
    let url = format!("http://{addr}/");
    println!("opening {url}");
    let result = {
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open").arg(&url).spawn()
        }
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("cmd")
                .args(["/C", "start", "", &url])
                .spawn()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            std::process::Command::new("xdg-open").arg(&url).spawn()
        }
    };
    if let Err(error) = result {
        eprintln!("could not open a browser ({error}); visit {url}");
    }
}
