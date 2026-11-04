//! Annotations attached to a board or supplied as evidence.

use serde::{Deserialize, Serialize};

/// A single arrow between two squares, rendered as `from->to`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Arrow {
    pub from: String,
    pub to: String,
}

/// One annotated ply.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Annotation {
    pub ply: u32,
    pub san: Option<String>,
    pub nag: Option<u8>,
    pub comment: Option<String>,
    pub arrows: Vec<Arrow>,
    pub circles: Vec<String>,
}

/// A collection of annotations. Input annotations and evidence annotations are
/// merged and sorted by `(ply, san, nag, comment)` before rendering.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Annotations {
    pub items: Vec<Annotation>,
}

impl Annotations {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Sort arrows and circles so a renderer never depends on caller order.
    pub fn normalized(&self) -> Vec<Annotation> {
        let mut items = self.items.clone();
        for item in &mut items {
            item.arrows
                .sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
            item.circles.sort();
        }
        items.sort_by(|a, b| annotation_key(a).cmp(&annotation_key(b)));
        items
    }

    /// Canonical copy of this collection.
    pub fn sorted(&self) -> Self {
        Self {
            items: self.normalized(),
        }
    }

    /// Merge two sets, dropping exact duplicates after canonical ordering.
    pub fn merged(&self, other: &Self) -> Self {
        let mut items = self.normalized();
        items.extend(other.normalized());
        items.sort_by(|a, b| annotation_key(a).cmp(&annotation_key(b)));
        items.dedup();
        Self { items }
    }
}

fn annotation_key(item: &Annotation) -> (u32, &str, u8, &str) {
    (
        item.ply,
        item.san.as_deref().unwrap_or(""),
        item.nag.unwrap_or(0),
        item.comment.as_deref().unwrap_or(""),
    )
}
