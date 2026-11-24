//! The managed KnightLM runtime (src/model_runtime.rs), offline: the downloads come from a
//! localhost fixture server, the server it starts is `model.serve.fake_server`.
//!
//! Needs Python with fastapi and uvicorn for the start/stop tests (`KNIGHTLM_FAKE_SERVER_PYTHON`,
//! else `python3`); without them those tests skip unless `RBC_REQUIRE_FAKE_SERVER=1` (CI sets it).

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use rusty_brick_chess::model_runtime::{
    AdapterSet, BaseSet, Config, FileSpec, Manifest, Phase, Runtime, extract_adapter,
    install_runtime, sha256_file, verify_file,
};
use sha2::{Digest, Sha256};

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rbc-runtime-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const ADAPTER_DIR: &str = "knightlm-run1-short-adapter";
const ADAPTER_FILES: [(&str, &[u8]); 2] = [
    ("adapter_config.json", b"{\"r\": 8}"),
    ("adapter_model.safetensors", b"not really weights"),
];

fn adapter_tgz() -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    for (name, body) in ADAPTER_FILES {
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, format!("{ADAPTER_DIR}/{name}"), body)
            .unwrap();
    }
    let raw = tar.into_inner().unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&raw).unwrap();
    gz.finish().unwrap()
}

fn spec(name: &str, body: &[u8]) -> FileSpec {
    FileSpec {
        name: name.into(),
        size: body.len() as u64,
        sha256: sha(body),
    }
}

struct Fixture {
    base: String,
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    ranges: Arc<Mutex<Vec<String>>>,
    manifest: Manifest,
}

type Shared = (
    Arc<Mutex<HashMap<String, Vec<u8>>>>,
    Arc<Mutex<Vec<String>>>,
);

async fn serve_file(
    State((files, ranges)): State<Shared>,
    UrlPath(name): UrlPath<String>,
    headers: HeaderMap,
) -> Response {
    let Some(body) = files.lock().unwrap().get(&name).cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if let Some(range) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) {
        ranges.lock().unwrap().push(range.to_string());
        let start: usize = range
            .trim_start_matches("bytes=")
            .trim_end_matches('-')
            .parse()
            .unwrap();
        return Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .body(Body::from(body[start..].to_vec()))
            .unwrap();
    }
    Response::new(Body::from(body))
}

async fn fixture() -> Fixture {
    let base_files: [(&str, &[u8]); 2] = [
        ("model.safetensors", b"0123456789abcdef-fake-weights"),
        ("config.json", b"{\"model\": 1}"),
    ];
    let tgz = adapter_tgz();
    let mut served: HashMap<String, Vec<u8>> = HashMap::new();
    for (name, body) in base_files {
        served.insert(name.into(), body.to_vec());
    }
    served.insert("adapter.tar.gz".into(), tgz.clone());
    let files = Arc::new(Mutex::new(served));
    let ranges = Arc::new(Mutex::new(Vec::new()));
    let app = axum::Router::new()
        .route("/files/{name}", get(serve_file))
        .with_state((files.clone(), ranges.clone()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/files/", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let manifest = Manifest {
        schema: 1,
        base: BaseSet {
            repo: "test/fake".into(),
            revision: "0".into(),
            license: "test".into(),
            url_prefix: base.clone(),
            files: base_files.iter().map(|(n, b)| spec(n, b)).collect(),
        },
        adapter: AdapterSet {
            url: format!("{base}adapter.tar.gz"),
            sha256: sha(&tgz),
            size: tgz.len() as u64,
            dir: ADAPTER_DIR.into(),
            files: ADAPTER_FILES.iter().map(|(n, b)| spec(n, b)).collect(),
        },
        python: Vec::new(),
    };
    Fixture {
        base,
        files,
        ranges,
        manifest,
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn python() -> String {
    std::env::var("KNIGHTLM_FAKE_SERVER_PYTHON").unwrap_or_else(|_| "python3".into())
}

/// Whether the fake server's Python is usable; panics in CI when it is not.
fn have_python() -> bool {
    let ready = Command::new(python())
        .args(["-c", "import fastapi, uvicorn"])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !ready {
        assert!(
            std::env::var("RBC_REQUIRE_FAKE_SERVER").map_or(true, |v| v != "1"),
            "RBC_REQUIRE_FAKE_SERVER=1 but python lacks fastapi/uvicorn"
        );
        eprintln!("skipped: python lacks fastapi or uvicorn");
    }
    ready
}

fn config(manifest: &Manifest) -> Config {
    Config {
        manifest: manifest.clone(),
        unpinned: true,
        fake: true,
        dev_base_dir: None,
        python: Some(PathBuf::from(python())),
        code_dir: Some(repo_root()),
    }
}

fn runtime(dir: &Path, manifest: &Manifest) -> Arc<Runtime> {
    install_runtime(dir, config(manifest))
}

fn db() -> rbc_db::Db {
    rbc_db::Db::open_memory().unwrap()
}

#[cfg(target_os = "linux")]
fn alive(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| {
            stat.rsplit_once(')')
                .map(|(_, rest)| rest.trim_start().starts_with(|c| c != 'Z'))
        })
        .unwrap_or(false)
}

// --- the manifest ---------------------------------------------------------------------------

#[test]
fn the_embedded_manifest_is_pinned_and_other_urls_are_refused() {
    let manifest = Manifest::pinned();
    manifest.check_pinned().unwrap();
    assert_eq!(
        manifest.base.revision,
        "a7da5b986cb59b408707209984f360a5f4ad7e47"
    );
    assert_eq!(
        manifest.adapter.sha256,
        "c67de2b96740b748ceddc3e9662789250e08f226a2a5d792c2dda8bf8452fed6"
    );
    assert!(
        manifest
            .base
            .files
            .iter()
            .any(|f| f.name == "model.safetensors"
                && f.sha256 == "d05b567eeaf534e83d375551f068ed57b5f52d37c657197f644af5ef9db091a2")
    );
    let mut other = manifest.clone();
    other.adapter.url = "https://example.com/adapter.tar.gz".into();
    assert!(other.check_pinned().is_err());
    let mut other = manifest.clone();
    other.base.url_prefix = "https://evil.example/".into();
    assert!(other.check_pinned().is_err());
    let mut other = manifest;
    other.base.files[0].name = "../escape".into();
    assert!(other.check_shape().is_err());
}

#[test]
fn a_pinned_runtime_will_not_run_an_unpinned_manifest() {
    let dir = scratch("unpinned");
    let mut config = Config {
        manifest: Manifest::pinned(),
        unpinned: false,
        fake: true,
        dev_base_dir: None,
        python: None,
        code_dir: None,
    };
    config.manifest.adapter.url = "http://127.0.0.1:1/x.tgz".into();
    let runtime = install_runtime(&dir, config);
    let error = runtime.run_install().unwrap_err();
    assert!(format!("{error:#}").contains("pinned"), "{error:#}");
    let _ = std::fs::remove_dir_all(&dir);
}

// --- hashes --------------------------------------------------------------------------------

#[test]
fn a_corrupt_file_is_refused() {
    let dir = scratch("verify");
    let file = dir.join("f.bin");
    std::fs::write(&file, b"hello").unwrap();
    verify_file(&file, 5, &sha(b"hello")).unwrap();
    assert!(verify_file(&file, 5, &sha(b"hellp")).is_err(), "wrong hash");
    assert!(verify_file(&file, 6, &sha(b"hello")).is_err(), "wrong size");
    assert_eq!(sha256_file(&file).unwrap(), sha(b"hello"));
    let _ = std::fs::remove_dir_all(&dir);
}

// --- the tarball -----------------------------------------------------------------------------

fn raw_tar(entries: &[(&[u8], u8, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (name, kind, body) in entries {
        let mut header = tar::Header::new_gnu();
        header.as_gnu_mut().unwrap().name[..name.len()].copy_from_slice(name);
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::new(*kind));
        if *kind == b'2' {
            header.set_link_name("/etc/passwd").unwrap();
        }
        header.set_cksum();
        builder.append(&header, *body).unwrap();
    }
    let raw = builder.into_inner().unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&raw).unwrap();
    gz.finish().unwrap()
}

#[tokio::test]
async fn extraction_refuses_traversal_links_and_surprises() {
    let manifest = fixture().await.manifest;
    let dir = scratch("tar");
    let good = |name: &str| format!("{ADAPTER_DIR}/{name}").into_bytes();
    type Case = (&'static str, Vec<(Vec<u8>, u8, Vec<u8>)>);
    let cases: Vec<Case> = vec![
        (
            "dotdot",
            vec![(
                format!("{ADAPTER_DIR}/../evil").into_bytes(),
                b'0',
                b"x".to_vec(),
            )],
        ),
        (
            "absolute",
            vec![(b"/tmp/evil".to_vec(), b'0', b"x".to_vec())],
        ),
        (
            "outside",
            vec![(b"other/adapter_config.json".to_vec(), b'0', b"x".to_vec())],
        ),
        (
            "symlink",
            vec![(good("adapter_config.json"), b'2', Vec::new())],
        ),
        ("unlisted", vec![(good("run_me.sh"), b'0', b"x".to_vec())]),
        (
            "nested",
            vec![(
                format!("{ADAPTER_DIR}/sub/adapter_config.json").into_bytes(),
                b'0',
                b"x".to_vec(),
            )],
        ),
        (
            "missing",
            vec![(
                good("adapter_config.json"),
                b'0',
                ADAPTER_FILES[0].1.to_vec(),
            )],
        ),
        (
            "wrong content",
            vec![
                (good("adapter_config.json"), b'0', b"tampered".to_vec()),
                (
                    good("adapter_model.safetensors"),
                    b'0',
                    ADAPTER_FILES[1].1.to_vec(),
                ),
            ],
        ),
    ];
    for (name, entries) in cases {
        let borrowed: Vec<(&[u8], u8, &[u8])> = entries
            .iter()
            .map(|(n, k, b)| (n.as_slice(), *k, b.as_slice()))
            .collect();
        let tgz = dir.join(format!("{name}.tgz"));
        std::fs::write(&tgz, raw_tar(&borrowed)).unwrap();
        let dest = dir.join(format!("out-{name}"));
        assert!(
            extract_adapter(&tgz, &dest, &manifest.adapter).is_err(),
            "{name} must be refused"
        );
    }
    assert!(!dir.join("evil").exists() && !Path::new("/tmp/evil").exists());
    // The honest archive extracts.
    let tgz = dir.join("good.tgz");
    std::fs::write(&tgz, adapter_tgz()).unwrap();
    extract_adapter(&tgz, &dir.join("out-good"), &manifest.adapter).unwrap();
    assert!(dir.join("out-good/adapter_config.json").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

// --- install -----------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn install_downloads_verifies_and_only_then_is_ready() {
    let fx = fixture().await;
    let dir = scratch("install");
    let rt = runtime(&dir, &fx.manifest);
    assert_eq!(rt.phase(), Phase::NotInstalled);
    let started = rt.clone();
    tokio::task::spawn_blocking(move || started.install())
        .await
        .unwrap()
        .unwrap();
    // The state machine: in flight until done, never "ready" before the marker exists.
    let mut saw_ready = false;
    for _ in 0..200 {
        match rt.phase() {
            Phase::Ready => {
                assert!(
                    dir.join("knightlm/installed.json").is_file(),
                    "ready means the marker exists"
                );
                saw_ready = true;
                break;
            }
            Phase::Error { code, message } => panic!("{code}: {message}"),
            Phase::NotInstalled
            | Phase::Downloading { .. }
            | Phase::Verifying
            | Phase::SettingUp { .. } => {
                assert!(!dir.join("knightlm/installed.json").is_file());
            }
            other => panic!("unexpected {other:?}"),
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(saw_ready);
    assert!(rt.files_installed());
    assert_eq!(
        std::fs::read(dir.join("knightlm/base/config.json")).unwrap(),
        b"{\"model\": 1}"
    );
    assert!(
        dir.join(format!(
            "knightlm/adapter/{ADAPTER_DIR}/adapter_config.json"
        ))
        .is_file()
    );
    assert!(
        !dir.join("knightlm/staging").exists() && !dir.join("knightlm/partial").exists(),
        "no scratch left"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_corrupt_download_is_refused_and_never_marked_ready() {
    let fx = fixture().await;
    fx.files.lock().unwrap().insert(
        "model.safetensors".into(),
        b"0123456789abcdef-EVIL-weights".to_vec(),
    );
    let dir = scratch("corrupt");
    let rt = runtime(&dir, &fx.manifest);
    let error = tokio::task::spawn_blocking({
        let rt = rt.clone();
        move || rt.run_install()
    })
    .await
    .unwrap()
    .unwrap_err();
    assert!(
        format!("{error:#}").contains("pinned checksum"),
        "{error:#}"
    );
    assert!(!rt.files_installed());
    assert!(!dir.join("knightlm/installed.json").exists());
    assert!(
        !dir.join("knightlm/base").exists(),
        "nothing half-installed is visible"
    );
    assert!(
        !dir.join("knightlm/partial/model.safetensors.part").exists(),
        "the bad bytes are discarded"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_partial_download_is_not_ready_and_resumes() {
    let fx = fixture().await;
    let dir = scratch("resume");
    let rt = runtime(&dir, &fx.manifest);
    // An interrupted earlier run: half of one file, no marker.
    let partial = dir.join("knightlm/partial");
    std::fs::create_dir_all(&partial).unwrap();
    std::fs::write(partial.join("model.safetensors.part"), b"0123456789").unwrap();
    assert!(!rt.files_installed());
    assert_eq!(rt.phase(), Phase::NotInstalled);
    tokio::task::spawn_blocking({
        let rt = rt.clone();
        move || rt.run_install()
    })
    .await
    .unwrap()
    .unwrap();
    assert!(rt.files_installed());
    assert_eq!(
        fx.ranges.lock().unwrap().as_slice(),
        ["bytes=10-"],
        "resumed, not restarted"
    );
    let _ = fx.base;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unreachable_source_is_a_download_error_not_a_ready_state() {
    let mut fx = fixture().await;
    fx.manifest.base.url_prefix = "http://127.0.0.1:9/".into();
    let dir = scratch("down");
    let rt = runtime(&dir, &fx.manifest);
    let error = tokio::task::spawn_blocking({
        let rt = rt.clone();
        move || rt.run_install()
    })
    .await
    .unwrap()
    .unwrap_err();
    assert!(format!("{error:#}").contains("resumes"), "{error:#}");
    assert!(!rt.files_installed());
    let _ = std::fs::remove_dir_all(&dir);
}

// --- start and stop with the fake server ----------------------------------------------------------

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn start_registers_the_provider_and_stop_leaves_no_process() {
    if !have_python() {
        return;
    }
    let fx = fixture().await;
    let dir = scratch("start");
    let rt = runtime(&dir, &fx.manifest);
    let db = db();
    // Not installed: a clear refusal.
    let early = tokio::task::spawn_blocking({
        let (rt, db) = (rt.clone(), db.clone());
        move || rt.start(&db)
    })
    .await
    .unwrap();
    assert!(format!("{:#}", early.unwrap_err()).contains("Get the model"));
    tokio::task::spawn_blocking({
        let rt = rt.clone();
        move || rt.run_install()
    })
    .await
    .unwrap()
    .unwrap();
    let url = tokio::task::spawn_blocking({
        let (rt, db) = (rt.clone(), db.clone());
        move || rt.start(&db)
    })
    .await
    .unwrap()
    .unwrap();
    assert!(url.starts_with("http://127.0.0.1:"));
    assert!(matches!(rt.phase(), Phase::Running { .. }));
    let rows = db.list_providers().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].kind.as_str(), rows[0].base_url.as_str()),
        ("knightlm", url.as_str())
    );
    let pid = rt.child_pid().unwrap();
    assert!(alive(pid));
    // Starting again is idempotent and registers no second row.
    let again = tokio::task::spawn_blocking({
        let (rt, db) = (rt.clone(), db.clone());
        move || rt.start(&db)
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(again, url);
    assert_eq!(db.list_providers().unwrap().len(), 1);
    tokio::task::spawn_blocking({
        let rt = rt.clone();
        move || rt.stop()
    })
    .await
    .unwrap();
    assert!(!alive(pid), "stop ends the server");
    assert_eq!(rt.phase(), Phase::Ready);
    // A restart picks a fresh port and updates the same row.
    let second = tokio::task::spawn_blocking({
        let (rt, db) = (rt.clone(), db.clone());
        move || rt.start(&db)
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(db.list_providers().unwrap().len(), 1);
    assert_eq!(db.list_providers().unwrap()[0].base_url, second);
    rt.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_server_exits_when_the_app_goes_away() {
    if !have_python() {
        return;
    }
    let fx = fixture().await;
    let dir = scratch("orphan");
    let rt = runtime(&dir, &fx.manifest);
    let db = db();
    tokio::task::spawn_blocking({
        let rt = rt.clone();
        move || rt.run_install()
    })
    .await
    .unwrap()
    .unwrap();
    tokio::task::spawn_blocking({
        let (rt, db) = (rt.clone(), db.clone());
        move || rt.start(&db)
    })
    .await
    .unwrap()
    .unwrap();
    let pid = rt.child_pid().unwrap();
    assert!(alive(pid));
    // What a crashing app does: the pipe closes, nothing kills the child.
    rt.abandon();
    let mut gone = false;
    for _ in 0..100 {
        if !alive(pid) {
            gone = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(gone, "the server must not outlive the app");
    let _ = std::fs::remove_dir_all(&dir);
}

// --- the routes ---------------------------------------------------------------------------------------

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_routes_drive_the_whole_flow() {
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    if !have_python() {
        return;
    }
    let fx = fixture().await;
    let dir = scratch("routes");
    install_runtime(&dir, config(&fx.manifest));
    let state = rusty_brick_chess::api::AppState {
        db: db(),
        db_dir: dir.clone(),
        data_dir: dir.clone(),
        engine_path: None,
        model_url: None,
        default_depth: 12,
        console: Default::default(),
        turn_cancels: Default::default(),
        lamu_child: Default::default(),
    };
    let call = |method: &'static str, path: &'static str| {
        let state = state.clone();
        async move {
            let response = rusty_brick_chess::router(state)
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            (
                status,
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap_or_default(),
            )
        }
    };
    let (status, body) = call("GET", "/api/knightlm/runtime").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["phase"]["state"], "not_installed");
    assert_eq!(body["mode"], "fake");
    let (status, _) = call("POST", "/api/knightlm/runtime/install").await;
    assert_eq!(status, StatusCode::OK);
    let mut state_name = String::new();
    for _ in 0..200 {
        let (_, body) = call("GET", "/api/knightlm/runtime").await;
        state_name = body["phase"]["state"].as_str().unwrap().to_string();
        if state_name == "ready" || state_name == "error" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(state_name, "ready");
    let (status, body) = call("POST", "/api/knightlm/runtime/start").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["phase"]["state"], "running");
    assert!(body["provider_id"].is_string());
    let (_, body) = call("POST", "/api/knightlm/runtime/stop").await;
    assert_eq!(body["phase"]["state"], "ready");
    let _ = std::fs::remove_dir_all(&dir);
}
