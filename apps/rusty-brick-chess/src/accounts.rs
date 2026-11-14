//! Lichess and Chess.com account links: profile lookup and game import.
//!
//! Tokens are optional for Lichess (public games are readable). The app never
//! persists a token: the settings table holds `lichess_token_ref`, the NAME of an
//! environment variable or a `keyring:<service>/<account>` entry, resolved through
//! `providers::resolve_secret` at call time (the same pattern as AI provider
//! `key_ref`). A database written by an older build may still hold a plaintext
//! `lichess_token`; `migrate_plaintext_token` moves it into the keyring.

use anyhow::{Context, Result, bail};
use std::time::Duration;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct Profile {
    pub username: String,
    pub name: Option<String>,
    pub url: Option<String>,
    pub rating: Option<i32>,
    pub games: Option<i64>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Account {
    pub provider: String,
    pub linked: bool,
    pub username: Option<String>,
    pub has_token: bool,
    pub profile: Option<Profile>,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .user_agent("RustyBrickChess/0.1 (local chess app)")
        .build()
        .into()
}

fn get_json(url: &str, token: Option<&str>) -> Result<serde_json::Value> {
    let mut request = agent().get(url).header("Accept", "application/json");
    if let Some(token) = token.filter(|t| !t.trim().is_empty()) {
        request = request.header("Authorization", &format!("Bearer {}", token.trim()));
    }
    let mut response = request.call().with_context(|| format!("GET {url}"))?;
    let body = response
        .body_mut()
        .with_config()
        .limit(2 << 20)
        .read_to_string()?;
    Ok(serde_json::from_str(&body)?)
}

fn get_text(url: &str, token: Option<&str>) -> Result<String> {
    let mut request = agent().get(url).header("Accept", "application/x-chess-pgn");
    if let Some(token) = token.filter(|t| !t.trim().is_empty()) {
        request = request.header("Authorization", &format!("Bearer {}", token.trim()));
    }
    let mut response = request.call().with_context(|| format!("GET {url}"))?;
    Ok(response
        .body_mut()
        .with_config()
        .limit(64 << 20)
        .read_to_string()?)
}

pub fn lichess_profile(username: &str, token: Option<&str>) -> Result<Profile> {
    let username = username.trim();
    if username.is_empty() {
        bail!("username is required");
    }
    let value = if token.filter(|t| !t.trim().is_empty()).is_some() {
        get_json("https://lichess.org/api/account", token)?
    } else {
        get_json(&format!("https://lichess.org/api/user/{username}"), None)?
    };
    let id = value
        .get("id")
        .or_else(|| value.get("username"))
        .and_then(|v| v.as_str())
        .unwrap_or(username)
        .to_owned();
    let rating = value
        .get("perfs")
        .and_then(|perfs| perfs.get("blitz").or_else(|| perfs.get("rapid")))
        .and_then(|perf| perf.get("rating"))
        .and_then(|v| v.as_i64())
        .map(|v| v as i32);
    Ok(Profile {
        username: id.clone(),
        name: value
            .get("profile")
            .and_then(|p| p.get("realName"))
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .filter(|v| !v.is_empty()),
        url: Some(format!("https://lichess.org/@/{id}")),
        rating,
        games: value
            .get("count")
            .and_then(|c| c.get("all"))
            .and_then(|v| v.as_i64()),
        status: value
            .get("online")
            .and_then(|v| v.as_bool())
            .map(|online| if online { "online" } else { "offline" }.to_owned()),
    })
}

pub fn lichess_games(username: &str, token: Option<&str>, max: u32) -> Result<String> {
    let username = username.trim();
    if username.is_empty() {
        bail!("username is required");
    }
    get_text(
        &format!(
            "https://lichess.org/api/games/user/{username}?max={}&clocks=false&evals=false&opening=false",
            max.clamp(1, 500)
        ),
        token,
    )
}

pub fn chesscom_profile(username: &str) -> Result<Profile> {
    let username = username.trim();
    if username.is_empty() {
        bail!("username is required");
    }
    let player = get_json(
        &format!("https://api.chess.com/pub/player/{username}"),
        None,
    )?;
    let stats = get_json(
        &format!("https://api.chess.com/pub/player/{username}/stats"),
        None,
    )
    .ok();
    let rating = stats.as_ref().and_then(|stats| {
        ["chess_blitz", "chess_rapid", "chess_bullet"]
            .iter()
            .find_map(|key| {
                stats
                    .get(*key)
                    .and_then(|v| v.get("last"))
                    .and_then(|v| v.get("rating"))
                    .and_then(|v| v.as_i64())
            })
            .map(|v| v as i32)
    });
    let id = player
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or(username);
    Ok(Profile {
        username: id.to_owned(),
        name: player
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .filter(|v| !v.is_empty()),
        url: Some(format!("https://www.chess.com/member/{id}")),
        rating,
        games: None,
        status: player
            .get("status")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
    })
}

/// Keep only the last `max` games of a multi-game PGN.
pub fn trim_games(pgn: &str, max: u32) -> String {
    let parts: Vec<&str> = pgn.split("\n\n[Event ").collect();
    if parts.len() <= 1 {
        return pgn.to_owned();
    }
    let total = parts.len();
    let keep = (max as usize).clamp(1, total);
    let start = total - keep;
    let mut out = String::new();
    for (index, part) in parts.iter().enumerate().skip(start) {
        if index == start {
            if start > 0 {
                out.push_str("[Event ");
            }
            out.push_str(part);
        } else {
            out.push_str("\n\n[Event ");
            out.push_str(part);
        }
    }
    out
}

/// PGN of the most recent monthly archive (Chess.com publishes one per month).
pub fn chesscom_latest_pgn(username: &str) -> Result<String> {
    let username = username.trim();
    let archives = get_json(
        &format!("https://api.chess.com/pub/player/{username}/games/archives"),
        None,
    )?;
    let latest = archives
        .get("archives")
        .and_then(|v| v.as_array())
        .and_then(|list| list.last())
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("no archives for {username}"))?;
    get_text(&format!("{latest}/pgn"), None)
}

/// Settings key holding the token REFERENCE (never the token).
pub const TOKEN_REF_SETTING: &str = "lichess_token_ref";
/// Settings key an older build used for the raw token. Read only for migration.
pub const LEGACY_TOKEN_SETTING: &str = "lichess_token";
const KEYRING_SERVICE: &str = "rusty-brick-chess";
const KEYRING_ACCOUNT: &str = "lichess-token";

fn is_env_name(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with(|c: char| c.is_ascii_digit())
        && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Check that `input` is a reference this app accepts and that it resolves.
/// The error never repeats the input, since a person may have pasted the token.
pub fn validate_token_ref(input: &str) -> Result<String, String> {
    let reference = input.trim();
    const SHAPE: &str = "enter the NAME of an environment variable holding the token, \
        or keyring:<service>/<account>; the token itself is never stored";
    if let Some(rest) = reference.strip_prefix("keyring:") {
        match rest.split_once('/') {
            Some((service, account)) if !service.is_empty() && !account.is_empty() => {}
            _ => return Err(format!("not a valid keyring reference: {SHAPE}")),
        }
    } else if !is_env_name(reference) || reference.starts_with("lip_") {
        return Err(format!("that is not a token reference: {SHAPE}"));
    }
    match crate::providers::resolve_secret(reference)? {
        Some(_) => Ok(reference.to_owned()),
        None => Err(
            "the token reference does not resolve to a value in this process \
            (is the environment variable set, or the keyring entry present?)"
                .to_owned(),
        ),
    }
}

/// The token for an outgoing call, resolved now and held only by the caller.
/// Falls back to a not-yet-migrated plaintext value so a failed migration does
/// not break a working link; that row is reported at startup.
pub fn resolve_lichess_token(
    settings: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    if let Some(reference) = settings
        .get(TOKEN_REF_SETTING)
        .map(|r| r.trim())
        .filter(|r| !r.is_empty())
    {
        return match crate::providers::resolve_secret(reference) {
            Ok(found) => found.map(|(value, _)| value),
            Err(_) => None,
        };
    }
    settings
        .get(LEGACY_TOKEN_SETTING)
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
}

#[derive(Debug, PartialEq, Eq)]
pub enum Migration {
    /// No plaintext token present.
    NotNeeded,
    /// Stored in the keyring and the plaintext row removed.
    Moved(String),
    /// The plaintext row was left in place; the reason holds no secret.
    Kept(String),
}

/// Store `value` in the keyring through `secret-tool` and read it back.
pub fn store_in_keyring(value: &str) -> Result<String, String> {
    use std::io::Write;
    let mut child = std::process::Command::new("secret-tool")
        .args([
            "store",
            "--label=RustyBrickChess Lichess token",
            "service",
            KEYRING_SERVICE,
            "account",
            KEYRING_ACCOUNT,
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| format!("secret-tool is not available: {error}"))?;
    child
        .stdin
        .take()
        .ok_or("secret-tool gave no stdin")?
        .write_all(value.as_bytes())
        .map_err(|error| format!("could not hand the value to secret-tool: {error}"))?;
    let status = child.wait().map_err(|error| error.to_string())?;
    if !status.success() {
        return Err("secret-tool store failed (is a keyring unlocked?)".into());
    }
    let reference = format!("keyring:{KEYRING_SERVICE}/{KEYRING_ACCOUNT}");
    match crate::providers::resolve_secret(&reference)? {
        Some((stored, _)) if stored == value => Ok(reference),
        _ => Err("the keyring entry could not be read back".into()),
    }
}

/// Move a plaintext `lichess_token` row into a keyring reference. The row is
/// deleted only after `store` has stored the value AND read it back, so a
/// failure leaves the person's only copy intact. Nothing here logs the value.
pub fn migrate_plaintext_token(
    db: &rbc_db::Db,
    store: &dyn Fn(&str) -> Result<String, String>,
) -> Result<Migration> {
    let settings = db.settings()?;
    let legacy = settings
        .get(LEGACY_TOKEN_SETTING)
        .map(|t| t.trim().to_owned())
        .unwrap_or_default();
    if legacy.is_empty() {
        // An empty tombstone from an old unlink: tidy it, nothing to protect.
        if settings.contains_key(LEGACY_TOKEN_SETTING) {
            db.delete_setting(LEGACY_TOKEN_SETTING)?;
        }
        return Ok(Migration::NotNeeded);
    }
    match store(&legacy) {
        Ok(reference) => {
            db.set_setting(TOKEN_REF_SETTING, &reference)?;
            db.delete_setting(LEGACY_TOKEN_SETTING)?;
            Ok(Migration::Moved(reference))
        }
        Err(reason) => Ok(Migration::Kept(reason)),
    }
}

/// Startup hook: migrate and tell the person, never printing the value.
pub fn migrate_plaintext_token_at_startup(db: &rbc_db::Db) {
    match migrate_plaintext_token(db, &store_in_keyring) {
        Ok(Migration::NotNeeded) => {}
        Ok(Migration::Moved(reference)) => eprintln!(
            "RustyBrickChess: moved your saved Lichess token into the keyring ({reference}) \
             and removed the plaintext copy from the database."
        ),
        Ok(Migration::Kept(reason)) => eprintln!(
            "RustyBrickChess WARNING: the database still holds a plaintext Lichess token \
             ({reason}). It was left in place. Put the token in an environment variable or \
             the keyring, link it again by reference, and unlink to clear the old copy."
        ),
        Err(error) => eprintln!("RustyBrickChess WARNING: token migration failed: {error}"),
    }
}

#[cfg(test)]
mod token_tests {
    use super::*;
    use std::cell::Cell;

    const SECRET: &str = "lip_not-a-real-token-for-tests";

    #[test]
    fn a_reference_validates_when_it_resolves_and_never_echoes_input() {
        // SAFETY: unique name, only read by the resolver in this test.
        unsafe { std::env::set_var("RBC_TEST_LICHESS_REF", SECRET) };
        assert_eq!(
            validate_token_ref(" RBC_TEST_LICHESS_REF ").expect("ok"),
            "RBC_TEST_LICHESS_REF"
        );
        let unset = validate_token_ref("RBC_TEST_LICHESS_REF_UNSET").expect_err("unset");
        assert!(unset.contains("does not resolve"), "{unset}");
        for pasted in [SECRET, "lip_abc", "has space", "a.b", ""] {
            let error = validate_token_ref(pasted).expect_err("refused");
            assert!(!error.contains(pasted) || pasted.is_empty(), "{error}");
        }
        let error = validate_token_ref("keyring:only-service").expect_err("shape");
        assert!(error.contains("keyring"), "{error}");
    }

    #[test]
    fn resolution_reads_the_environment_through_the_reference() {
        unsafe { std::env::set_var("RBC_TEST_LICHESS_RES", SECRET) };
        let mut settings = std::collections::BTreeMap::new();
        settings.insert(TOKEN_REF_SETTING.to_owned(), "RBC_TEST_LICHESS_RES".into());
        assert_eq!(resolve_lichess_token(&settings).as_deref(), Some(SECRET));
        settings.insert(TOKEN_REF_SETTING.to_owned(), "RBC_TEST_LICHESS_NOPE".into());
        assert_eq!(resolve_lichess_token(&settings), None);
        settings.insert(TOKEN_REF_SETTING.to_owned(), "keyring:no-account".into());
        assert_eq!(resolve_lichess_token(&settings), None);
    }

    #[test]
    fn migration_moves_the_token_then_removes_the_plaintext_row() {
        let db = rbc_db::Db::open_memory().expect("db");
        db.set_setting(LEGACY_TOKEN_SETTING, SECRET).expect("seed");
        let seen = Cell::new(false);
        let outcome = migrate_plaintext_token(&db, &|value| {
            assert_eq!(value, SECRET);
            seen.set(true);
            Ok("keyring:rusty-brick-chess/lichess-token".into())
        })
        .expect("migrate");
        assert!(seen.get());
        assert_eq!(
            outcome,
            Migration::Moved("keyring:rusty-brick-chess/lichess-token".into())
        );
        let settings = db.settings().expect("settings");
        assert!(!settings.contains_key(LEGACY_TOKEN_SETTING));
        assert_eq!(
            settings.get(TOKEN_REF_SETTING).map(String::as_str),
            Some("keyring:rusty-brick-chess/lichess-token")
        );
        assert!(!format!("{settings:?}").contains(SECRET));
    }

    #[test]
    fn a_failed_migration_keeps_the_only_copy_and_leaks_nothing() {
        let db = rbc_db::Db::open_memory().expect("db");
        db.set_setting(LEGACY_TOKEN_SETTING, SECRET).expect("seed");
        let outcome = migrate_plaintext_token(&db, &|_| Err("secret-tool store failed".into()))
            .expect("migrate");
        assert_eq!(outcome, Migration::Kept("secret-tool store failed".into()));
        let settings = db.settings().expect("settings");
        assert_eq!(
            settings.get(LEGACY_TOKEN_SETTING).map(String::as_str),
            Some(SECRET)
        );
        assert!(!settings.contains_key(TOKEN_REF_SETTING));
        // The kept copy still works for calls until the person re-links.
        assert_eq!(resolve_lichess_token(&settings).as_deref(), Some(SECRET));
    }

    #[test]
    fn nothing_to_migrate_is_a_no_op() {
        let db = rbc_db::Db::open_memory().expect("db");
        let outcome = migrate_plaintext_token(&db, &|_| panic!("must not store")).expect("ok");
        assert_eq!(outcome, Migration::NotNeeded);
    }
}
