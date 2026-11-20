//! The desktop shell serves behind a per-launch token; browser mode does not.

use std::io::{Read, Write};

fn request(port: u16, head: &str) -> String {
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .write_all(format!("{head}\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").as_bytes())
        .unwrap();
    let mut out = String::new();
    stream.read_to_string(&mut out).unwrap();
    out
}

async fn start(token: Option<String>) -> (u16, tokio::sync::oneshot::Sender<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = rusty_brick_chess::serve_listener(listener, token, async {
            let _ = stopped.await;
        })
        .await;
    });
    (port, stop)
}

#[tokio::test(flavor = "multi_thread")]
async fn token_gates_the_api_and_browser_mode_is_open() {
    let dir = std::env::temp_dir().join(format!("rbc-token-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: this is the only test in this binary; nothing else reads the environment.
    unsafe { std::env::set_var("RBC_DB", dir.join("t.db")) };

    let token = rusty_brick_chess::launch_guard::new_token().unwrap();
    let (port, stop) = start(Some(token.clone())).await;
    let (open_port, open_stop) = start(None).await;
    let token_c = token.clone();
    tokio::task::spawn_blocking(move || {
        for _ in 0..50 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let none = request(port, "GET /api/health HTTP/1.1");
        assert!(none.starts_with("HTTP/1.1 401"), "{none}");
        let wrong = request(port, "GET /api/health HTTP/1.1\r\nx-rbc-token: nope");
        assert!(wrong.starts_with("HTTP/1.1 401"), "{wrong}");
        let post = request(port, "POST /api/games HTTP/1.1\r\nContent-Length: 0");
        assert!(post.starts_with("HTTP/1.1 401"), "{post}");
        let ok = request(
            port,
            &format!("GET /api/health HTTP/1.1\r\nx-rbc-token: {token_c}"),
        );
        assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
        let page = request(port, &format!("GET /?rbc_token={token_c} HTTP/1.1"));
        assert!(
            page.starts_with("HTTP/1.1 303") && page.to_lowercase().contains("set-cookie"),
            "{page}"
        );
        // Browser mode: no token configured, no token needed.
        let browser = request(open_port, "GET /api/health HTTP/1.1");
        assert!(browser.starts_with("HTTP/1.1 200"), "{browser}");
    })
    .await
    .unwrap();
    let _ = stop.send(());
    let _ = open_stop.send(());
    let _ = std::fs::remove_dir_all(dir);
}
