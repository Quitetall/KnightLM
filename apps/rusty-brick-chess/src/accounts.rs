//! Lichess and Chess.com account links: profile lookup and game import.
//!
//! Tokens are optional for Lichess (public games are readable) and stored in
//! the local SQLite settings. Nothing is uploaded to a third party.

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
    let body = response.body_mut().with_config().limit(2 << 20).read_to_string()?;
    Ok(serde_json::from_str(&body)?)
}

fn get_text(url: &str, token: Option<&str>) -> Result<String> {
    let mut request = agent()
        .get(url)
        .header("Accept", "application/x-chess-pgn");
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
