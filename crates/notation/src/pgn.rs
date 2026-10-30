//! PGN reading (via `pgn-reader`) and a small writer.
//!
//! `pgn-reader` is the streaming parser: tags, SAN, NAGs, comments, recursive
//! variations and outcomes. We collect them into a small AST (`PgnGame`) that
//! keeps the fidelity the viewer needs, and keep a compact writer for export.

use std::collections::BTreeMap;
use std::ops::ControlFlow;

use serde::{Deserialize, Serialize};

use pgn_reader::{Nag, Outcome, RawComment, RawTag, Reader, SanPlus, Skip, Visitor};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PgnNode {
    pub san: String,
    pub nags: Vec<u32>,
    pub pre_comment: Option<String>,
    pub post_comment: Option<String>,
    pub variations: Vec<Vec<PgnNode>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PgnGame {
    pub tags: BTreeMap<String, String>,
    pub movetext: Vec<PgnNode>,
    pub result: Option<String>,
}

#[derive(Debug)]
pub struct PgnError(pub String);

impl std::fmt::Display for PgnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for PgnError {}

#[derive(Default)]
struct Collector {
    tags: BTreeMap<String, String>,
    result: Option<String>,
    /// Stack of movetexts: index 0 is the main line; each variation pushes one.
    stack: Vec<Vec<PgnNode>>,
    /// A comment seen before the next move.
    pending_pre: Option<String>,
}

impl Collector {
    fn push_node(&mut self, node: PgnNode) {
        if let Some(level) = self.stack.last_mut() {
            level.push(node);
        }
    }
    fn last_mut(&mut self) -> Option<&mut PgnNode> {
        self.stack.last_mut().and_then(|v| v.last_mut())
    }
}

impl Visitor for Collector {
    type Tags = ();
    type Movetext = ();
    type Output = PgnGame;

    fn begin_tags(&mut self) -> ControlFlow<Self::Output, Self::Tags> {
        self.tags.clear();
        self.result = None;
        self.stack.clear();
        self.pending_pre = None;
        ControlFlow::Continue(())
    }

    fn tag(
        &mut self,
        _tags: &mut Self::Tags,
        name: &[u8],
        value: RawTag<'_>,
    ) -> ControlFlow<Self::Output> {
        self.tags.insert(
            String::from_utf8_lossy(name).into_owned(),
            String::from_utf8_lossy(value.as_bytes()).into_owned(),
        );
        ControlFlow::Continue(())
    }

    fn begin_movetext(&mut self, _tags: Self::Tags) -> ControlFlow<Self::Output, Self::Movetext> {
        self.stack = vec![Vec::new()];
        ControlFlow::Continue(())
    }

    fn san(&mut self, _movetext: &mut Self::Movetext, san_plus: SanPlus) -> ControlFlow<Self::Output> {
        let pre = self.pending_pre.take();
        self.push_node(PgnNode {
            san: san_plus.to_string(),
            nags: Vec::new(),
            pre_comment: pre,
            post_comment: None,
            variations: Vec::new(),
        });
        ControlFlow::Continue(())
    }

    fn nag(&mut self, _movetext: &mut Self::Movetext, nag: Nag) -> ControlFlow<Self::Output> {
        if let Some(last) = self.last_mut() {
            last.nags.push(u32::from(nag.0));
        }
        ControlFlow::Continue(())
    }

    fn comment(&mut self, _movetext: &mut Self::Movetext, comment: RawComment<'_>) -> ControlFlow<Self::Output> {
        let text = String::from_utf8_lossy(comment.as_bytes()).trim().to_owned();
        if let Some(last) = self.last_mut() {
            match &mut last.post_comment {
                Some(prev) => {
                    prev.push(' ');
                    prev.push_str(&text);
                }
                None => last.post_comment = Some(text),
            }
        } else {
            self.pending_pre = Some(text);
        }
        ControlFlow::Continue(())
    }

    fn begin_variation(&mut self, _movetext: &mut Self::Movetext) -> ControlFlow<Self::Output, Skip> {
        // Descend into the variation rather than skipping it.
        self.stack.push(Vec::new());
        ControlFlow::Continue(Skip(false))
    }

    fn end_variation(&mut self, _movetext: &mut Self::Movetext) -> ControlFlow<Self::Output> {
        if self.stack.len() > 1 {
            let variation = self.stack.pop().unwrap_or_default();
            if let Some(parent_last) = self.stack.last_mut().and_then(|v| v.last_mut()) {
                parent_last.variations.push(variation);
            }
        }
        ControlFlow::Continue(())
    }

    fn outcome(&mut self, _movetext: &mut Self::Movetext, outcome: Outcome) -> ControlFlow<Self::Output> {
        self.result = Some(outcome.to_string());
        ControlFlow::Continue(())
    }

    fn end_game(&mut self, _movetext: Self::Movetext) -> Self::Output {
        let movetext = if self.stack.is_empty() {
            Vec::new()
        } else {
            self.stack.remove(0)
        };
        PgnGame {
            tags: std::mem::take(&mut self.tags),
            movetext,
            result: self.result.take(),
        }
    }
}

/// Parse a single PGN game.
pub fn parse(input: &str) -> Result<PgnGame, PgnError> {
    let mut reader = Reader::new(input.as_bytes());
    let mut collector = Collector::default();
    match reader.read_game(&mut collector) {
        Ok(Some(game)) => Ok(game),
        Ok(None) => Ok(PgnGame {
            tags: BTreeMap::new(),
            movetext: Vec::new(),
            result: None,
        }),
        Err(e) => Err(PgnError(format!("PGN parse error: {e}"))),
    }
}

/// Parse every game in a potentially multi-game PGN stream.
pub fn parse_all(input: &[u8]) -> Result<Vec<PgnGame>, PgnError> {
    let mut reader = Reader::new(input);
    let mut collector = Collector::default();
    let mut games = Vec::new();
    for game in reader.read_games(&mut collector) {
        match game {
            Ok(g) => games.push(g),
            Err(e) => return Err(PgnError(format!("PGN parse error: {e}"))),
        }
    }
    Ok(games)
}

pub fn write(game: &PgnGame) -> String {
    let mut out = String::new();
    for (k, v) in &game.tags {
        out.push_str(&format!("[{} \"{}\"]\n", k, v));
    }
    if !game.tags.is_empty() {
        out.push('\n');
    }
    write_nodes(&mut out, &game.movetext, 0);
    if let Some(r) = &game.result {
        out.push(' ');
        out.push_str(r);
    }
    out.push('\n');
    out
}

fn write_nodes(out: &mut String, nodes: &[PgnNode], start_ply: usize) {
    let mut ply = start_ply;
    let mut line_start = true;
    let mut need_space = false;
    for node in nodes {
        if need_space {
            out.push(' ');
        }
        let number = ply / 2 + 1;
        if ply % 2 == 0 {
            out.push_str(&format!("{number}. "));
        } else if line_start {
            out.push_str(&format!("{number}... "));
        }
        if let Some(pc) = &node.pre_comment {
            out.push_str(&format!("{{{}}} ", pc));
        }
        out.push_str(&node.san);
        for nag in &node.nags {
            out.push_str(&format!(" ${}", nag));
        }
        if let Some(c) = &node.post_comment {
            out.push_str(&format!(" {{{}}}", c));
        }
        if !node.variations.is_empty() {
            for var in &node.variations {
                out.push_str(" (");
                write_nodes(out, var, ply);
                out.push(')');
            }
            // The next mainline move follows a variation, so it needs its
            // move number spelled out again (PGN requires it after ")").
            line_start = true;
        } else {
            line_start = false;
        }
        need_space = true;
        ply += 1;
    }
}
