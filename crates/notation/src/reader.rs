use std::io::Read;

use anyhow::{Context, Result};
use knightlm_core::state::{
    castling_mode_for_fen, parse_variant_fen, GameRecord, START_FEN,
};
use knightlm_core::SCHEMA_VERSION;
use ruzstd::decoding::FrameDecoder;
use shakmaty::{variant::Variant, san::SanPlus, uci::UciMove, CastlingMode, Position};

use crate::pgn::{self, PgnGame};

/// Source-policy history_complete: a four-field or SetUp/FEN source never
/// yields true (KLM-SAS-RQ-006).
pub fn history_complete_for(game: &PgnGame) -> bool {
    !game.tags.contains_key("SetUp") && !game.tags.contains_key("FEN")
}

pub fn initial_fen_for(game: &PgnGame) -> String {
    if let Some(fen) = game.tags.get("FEN") {
        // six-field not guaranteed; pad to six like core does.
        let fields: Vec<&str> = fen.split_whitespace().collect();
        if fields.len() >= 6 {
            fen.clone()
        } else if fields.len() == 4 {
            format!("{} 0 1", fen)
        } else {
            START_FEN.to_owned()
        }
    } else {
        START_FEN.to_owned()
    }
}

/// Convert a parsed PGN game into a canonical GameRecord, converting SAN
/// movetext to UCI. Fails explicitly on any unplayable move (RQ-004).
pub fn to_game_record(game: &PgnGame, source: &str, game_id: &str) -> Result<GameRecord> {
    let initial_fen = initial_fen_for(game);
    let hc = history_complete_for(game);
    let raw_variant = game
        .tags
        .get("Variant")
        .map(|v| v.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let chess960 = raw_variant.contains("960") || raw_variant.contains("fischer");
    let variant = if chess960 || raw_variant.is_empty() {
        Variant::Chess
    } else {
        Variant::from_ascii(raw_variant.as_bytes()).unwrap_or(Variant::Chess)
    };
    let mode = if variant == Variant::Chess {
        if chess960 {
            CastlingMode::Chess960
        } else {
            castling_mode_for_fen(&initial_fen)
        }
    } else {
        CastlingMode::Standard
    };
    let (mut pos, mode) = parse_variant_fen(&initial_fen, variant, mode)?;

    let mut moves_uci: Vec<String> = Vec::new();
    for n in &game.movetext {
        let parsed = SanPlus::from_ascii(n.san.as_bytes())
            .with_context(|| format!("bad SAN in {}: {}", game_id, n.san))?;
        let mv = parsed
            .san
            .to_move(&pos)
            .with_context(|| format!("illegal SAN in {}: {}", game_id, n.san))?;
        let uci = UciMove::from_move(mv, mode).to_string();
        pos = pos.play(mv).context("play failed")?;
        moves_uci.push(uci);
    }

    Ok(GameRecord {
        schema_version: SCHEMA_VERSION,
        game_id: game_id.to_owned(),
        source: source.to_owned(),
        initial_fen,
        history_complete: hc,
        moves: moves_uci,
        variant: if chess960 {
            "chess960".to_owned()
        } else {
            variant.uci().to_owned()
        },
    })
}

/// Read a (possibly zstd-compressed) PGN stream into GameRecords.
pub fn read_stream<R: Read>(reader: R, source: &str) -> Result<Vec<GameRecord>> {
    // Detect zstd magic (0x28,0xb5,0x2f,0xfd).
    let mut buf = Vec::new();
    let mut r = reader;
    r.read_to_end(&mut buf).context("read input")?;
    let text = if buf.len() >= 4 && buf[0] == 0x28 && buf[1] == 0xb5 && buf[2] == 0x2f && buf[3] == 0xfd {
        let mut dec = FrameDecoder::new();
        dec.init(&buf[..])
            .map_err(|e| anyhow::anyhow!("zstd decode: {e}"))?;
        let mut out = String::new();
        dec.read_to_string(&mut out).context("zstd read")?;
        out
    } else {
        String::from_utf8(buf).context("utf8")?
    };

    // Stream-parse every game with pgn-reader.
    let games = pgn::parse_all(text.as_bytes())?;

    let mut records = Vec::new();
    for (i, g) in games.into_iter().enumerate() {
        let gid = g
            .tags
            .get("GameId")
            .cloned()
            .unwrap_or_else(|| format!("lichess-{}", i));
        match to_game_record(&g, source, &gid) {
            Ok(r) => records.push(r),
            Err(e) => {
                // Explicit refusal: a malformed game is an error, never a silent skip.
                return Err(e).context(format!("game {} in {}", i, source));
            }
        }
    }
    Ok(records)
}
