//! Every sound the player can request must ship as real Ogg audio, in both the
//! source assets and the public/ copy that deploy.sh produces. Upstream stores
//! some files as symlinks; a checkout without symlinks leaves a few bytes of
//! text ("Check.ogg", "404: Not Found") that the browser cannot decode.
use std::path::Path;

const THEMES: [&str; 4] = ["sfx", "piano", "nes", "futuristic"];
const EVENTS: [&str; 11] = [
    "Move",
    "Capture",
    "Check",
    "Checkmate",
    "LowTime",
    "Error",
    "Victory",
    "Defeat",
    "Draw",
    "Select",
    "Confirmation",
];

#[test]
fn every_bundled_sound_is_real_ogg() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("frontend");
    let mut broken = Vec::new();
    for base in ["assets/sounds", "public/sounds"] {
        for theme in THEMES {
            for event in EVENTS {
                let path = root.join(base).join(theme).join(format!("{event}.ogg"));
                match std::fs::read(&path) {
                    Ok(b) if b.len() >= 1000 && b.starts_with(b"OggS") => {}
                    Ok(b) => broken.push(format!("{} ({} bytes)", path.display(), b.len())),
                    Err(e) => broken.push(format!("{}: {e}", path.display())),
                }
            }
        }
    }
    assert!(broken.is_empty(), "invalid bundled sounds: {broken:#?}");
}

#[test]
fn public_sounds_match_assets() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("frontend");
    for theme in THEMES {
        for event in EVENTS {
            let rel = format!("{theme}/{event}.ogg");
            let a = std::fs::read(root.join("assets/sounds").join(&rel)).unwrap();
            let p = std::fs::read(root.join("public/sounds").join(&rel)).unwrap();
            assert!(
                a == p,
                "public/sounds/{rel} differs from assets/sounds/{rel}"
            );
        }
    }
}
