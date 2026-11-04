//! Mode/origin permission policy for tool calls.
//!
//! The policy answers one question: **may this call run without a person
//! approving it?** Approval-required and denied calls are different:
//!
//! - [`PolicyDecision::Allow`] — execute immediately.
//! - [`PolicyDecision::RequiresApproval`] — queue a changeset; execute only after
//!   approval.
//! - [`PolicyDecision::Deny`] — the tool is not available in this mode at all.
//!
//! Defaults (see [`PermissionProfile`]):
//!
//! | effect | in-app Chat | in-app Play/Watch | MCP |
//! |---|---|---|---|
//! | `read` | allow | allow | allow |
//! | `external` | allow | allow | allow |
//! | `board-write` | approval unless `board-write` is auto-approved | allow | approval unless `board-write` is auto-approved |
//! | `session-write` | approval unless `study-content` is auto-approved | denied by tool modes | approval unless `study-content` is auto-approved |
//! | `destructive` | approval unless `destructive` is auto-approved | approval unless `destructive` is auto-approved | approval unless `destructive` is auto-approved |
//!
//! Notes:
//!
//! - Play and Watch need moves and drawing without a prompt, so in-game board
//!   writes are auto-approved there. Watch is not write-free: model moves,
//!   arrows and circles are allowed; structural and study writes keep `Chat`-only
//!   tool modes and are denied in Play/Watch.
//! - MCP has no session mode, so mode gating is skipped for [`Origin::Mcp`]. Its
//!   writes always require approval unless the MCP-specific scope is granted.
//! - A `board-write` grant never authorizes study content or destruction; each
//!   class has its own scope.

use serde::{Deserialize, Serialize};

use crate::tools::{ToolEffect, ToolSpec};

pub use crate::protocol::SessionMode as Mode;

/// Where a tool call came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// The assistant running inside the RustyBrickChess UI.
    InApp,
    /// An external MCP client.
    Mcp,
}

impl Origin {
    /// Stable wire/database spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InApp => "in_app",
            Self::Mcp => "mcp",
        }
    }
}

impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Auto-approve class. Scopes are independent: granting one never grants another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionScope {
    /// In-game board content: moves, shapes, annotations and notes.
    BoardWrite,
    /// Study structure and study prose.
    StudyContent,
    /// Deletes, imports and other irreversible operations. There is intentionally
    /// no built-in tool in this class in R1, but the policy supports it.
    Destructive,
}

impl PermissionScope {
    /// Stable wire/database spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BoardWrite => "board-write",
            Self::StudyContent => "study-content",
            Self::Destructive => "destructive",
        }
    }
}

impl std::fmt::Display for PermissionScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The policy decision for one tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecision {
    /// Execute immediately.
    Allow,
    /// Create an approval job and wait for a person.
    RequiresApproval,
    /// The tool is not available in this mode/origin at all.
    Deny,
}

impl PolicyDecision {
    /// Whether the call may run without approval.
    pub const fn is_allowed(self) -> bool {
        matches!(self, Self::Allow)
    }
}

/// Auto-approve scopes, kept per origin as the settings UI describes.
///
/// The default profile grants nothing: reads still run, every write prompts.
/// This is deliberately the safe default for both in-app and MCP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PermissionProfile {
    /// Scopes auto-approved for calls made from the app.
    pub in_app: Vec<PermissionScope>,
    /// Scopes auto-approved for MCP calls. Empty means every MCP write prompts.
    pub mcp: Vec<PermissionScope>,
}

impl PermissionProfile {
    /// The scopes granted for one origin.
    pub fn scopes(&self, origin: Origin) -> &[PermissionScope] {
        match origin {
            Origin::InApp => &self.in_app,
            Origin::Mcp => &self.mcp,
        }
    }

    /// Whether `scope` is auto-approved for `origin`.
    pub fn grants(&self, origin: Origin, scope: PermissionScope) -> bool {
        self.scopes(origin).contains(&scope)
    }

    /// Grant a scope without duplicating it.
    pub fn grant(&mut self, origin: Origin, scope: PermissionScope) {
        let scopes = match origin {
            Origin::InApp => &mut self.in_app,
            Origin::Mcp => &mut self.mcp,
        };
        if !scopes.contains(&scope) {
            scopes.push(scope);
        }
    }

    /// Revoke a scope if it is present.
    pub fn revoke(&mut self, origin: Origin, scope: PermissionScope) {
        let scopes = match origin {
            Origin::InApp => &mut self.in_app,
            Origin::Mcp => &mut self.mcp,
        };
        scopes.retain(|candidate| *candidate != scope);
    }

    /// The scope granted for this spec, if any, translated to the profile's origin.
    fn granted_scope(&self, origin: Origin, spec: &ToolSpec) -> Option<PermissionScope> {
        let scope = scope_for(spec)?;
        self.grants(origin, scope).then_some(scope)
    }

    /// Decide a call using this profile.
    pub fn decide(&self, spec: &ToolSpec, mode: Mode, origin: Origin) -> PolicyDecision {
        decide(spec, mode, origin, self.granted_scope(origin, spec))
    }

    /// Whether a call may run without approval under this profile.
    pub fn allows(&self, spec: &ToolSpec, mode: Mode, origin: Origin) -> bool {
        self.decide(spec, mode, origin).is_allowed()
    }
}

/// Map a tool class to the scope that would auto-approve it.
///
/// Reads and external queries need no scope. Structural workspace/study writes
/// (`session-write`) are study content for approval purposes.
pub fn scope_for(spec: &ToolSpec) -> Option<PermissionScope> {
    match spec.effects {
        ToolEffect::Read | ToolEffect::External => None,
        ToolEffect::BoardWrite => Some(PermissionScope::BoardWrite),
        ToolEffect::SessionWrite => Some(PermissionScope::StudyContent),
        ToolEffect::Destructive => Some(PermissionScope::Destructive),
    }
}

/// Decide a call, where `granted_scope` is the auto-approve scope (if any) the
/// caller's profile grants for this origin.
///
/// This is the primitive behind [`PermissionProfile::decide`]. Most callers
/// should use the profile method; this function is public so adapters can test
/// policy without building a profile.
pub fn decide(
    spec: &ToolSpec,
    mode: Mode,
    origin: Origin,
    granted_scope: Option<PermissionScope>,
) -> PolicyDecision {
    // MCP has no session mode, so its calls are not mode-gated. In-app calls must
    // use a mode the tool advertises; this is what keeps structural/study writes
    // out of Play/Watch.
    if origin == Origin::InApp && !spec.modes.contains(&mode) {
        return PolicyDecision::Deny;
    }

    match spec.effects {
        // Reading a board or querying an external source does not mutate anything.
        ToolEffect::Read | ToolEffect::External => PolicyDecision::Allow,

        ToolEffect::BoardWrite => {
            // Moves, arrows, circles, annotations and notes are the flow of a
            // game, so Play/Watch never interrupt with a prompt. Chat writes
            // need `board-write` auto-approval.
            if origin == Origin::InApp && matches!(mode, Mode::Play | Mode::Watch) {
                PolicyDecision::Allow
            } else if granted_scope == Some(PermissionScope::BoardWrite) {
                PolicyDecision::Allow
            } else {
                PolicyDecision::RequiresApproval
            }
        }

        // Study content is always review-by-default, in every mode and origin.
        ToolEffect::SessionWrite => {
            if granted_scope == Some(PermissionScope::StudyContent) {
                PolicyDecision::Allow
            } else {
                PolicyDecision::RequiresApproval
            }
        }

        // Destructive actions need their own grant. A board-write or
        // study-content grant never authorizes them.
        ToolEffect::Destructive => {
            if granted_scope == Some(PermissionScope::Destructive) {
                PolicyDecision::Allow
            } else {
                PolicyDecision::RequiresApproval
            }
        }
    }
}

/// Whether a call may run without approval.
///
/// This is the four-argument convenience form requested by the package spec.
/// It is exactly `decide(..) == PolicyDecision::Allow`; a `false` result may
/// mean either "needs approval" or "denied", so callers that need to tell those
/// apart should use [`decide`].
pub fn allows(
    spec: &ToolSpec,
    mode: Mode,
    origin: Origin,
    granted_scope: Option<PermissionScope>,
) -> bool {
    decide(spec, mode, origin, granted_scope).is_allowed()
}

/// Whether a call needs a person's approval (as opposed to being denied).
pub fn requires_approval(
    spec: &ToolSpec,
    mode: Mode,
    origin: Origin,
    granted_scope: Option<PermissionScope>,
) -> bool {
    decide(spec, mode, origin, granted_scope) == PolicyDecision::RequiresApproval
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolRegistry;
    use serde_json::json;

    fn destructive_spec() -> ToolSpec {
        ToolSpec {
            name: "delete_board".into(),
            version: crate::tools::TOOL_SCHEMA_VERSION.into(),
            description: "Delete a board.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "board_id": {"type": "string"},
                    "expected_revision": {"type": "integer"}
                },
                "required": ["board_id", "expected_revision"],
                "additionalProperties": false
            }),
            effects: ToolEffect::Destructive,
            modes: vec![Mode::Chat, Mode::Play, Mode::Watch],
        }
    }

    #[test]
    fn reads_are_allowed_everywhere() {
        let registry = ToolRegistry::builtin();
        let spec = registry.get("get_board").unwrap();
        for origin in [Origin::InApp, Origin::Mcp] {
            for mode in [Mode::Chat, Mode::Play, Mode::Watch] {
                assert_eq!(
                    decide(spec, mode, origin, None),
                    PolicyDecision::Allow,
                    "get_board {mode} {origin}"
                );
            }
        }
    }

    #[test]
    fn external_queries_are_allowed() {
        let registry = ToolRegistry::builtin();
        for name in [
            "analyze",
            "evaluate",
            "opening_explorer",
            "search_database",
            "tablebase",
        ] {
            let spec = registry.get(name).unwrap();
            assert_eq!(
                decide(spec, Mode::Chat, Origin::InApp, None),
                PolicyDecision::Allow,
                "{name}"
            );
        }
    }

    #[test]
    fn play_move_is_allowed_in_play_and_watch() {
        let registry = ToolRegistry::builtin();
        let spec = registry.get("play_move").unwrap();
        assert!(allows(spec, Mode::Play, Origin::InApp, None));
        assert!(allows(spec, Mode::Watch, Origin::InApp, None));
        // In Chat a move is a board write and needs a prompt or a grant.
        assert_eq!(
            decide(spec, Mode::Chat, Origin::InApp, None),
            PolicyDecision::RequiresApproval
        );
        assert!(allows(
            spec,
            Mode::Chat,
            Origin::InApp,
            Some(PermissionScope::BoardWrite)
        ));
    }

    #[test]
    fn drawing_is_allowed_in_watch() {
        let registry = ToolRegistry::builtin();
        for name in ["draw_arrow", "draw_circle", "clear_shapes"] {
            let spec = registry.get(name).unwrap();
            assert!(
                allows(spec, Mode::Watch, Origin::InApp, None),
                "{name} should be allowed while watching"
            );
        }
    }

    #[test]
    fn chat_board_write_requires_board_scope() {
        let registry = ToolRegistry::builtin();
        let spec = registry.get("draw_arrow").unwrap();
        assert!(!allows(spec, Mode::Chat, Origin::InApp, None));
        assert!(requires_approval(spec, Mode::Chat, Origin::InApp, None));
        assert!(allows(
            spec,
            Mode::Chat,
            Origin::InApp,
            Some(PermissionScope::BoardWrite)
        ));
        // Wrong scope does not help.
        assert!(!allows(
            spec,
            Mode::Chat,
            Origin::InApp,
            Some(PermissionScope::StudyContent)
        ));
    }

    #[test]
    fn mcp_defaults_to_approval() {
        let registry = ToolRegistry::builtin();
        let spec = registry.get("play_move").unwrap();
        assert!(!allows(spec, Mode::Chat, Origin::Mcp, None));
        assert!(requires_approval(spec, Mode::Chat, Origin::Mcp, None));
        // MCP also ignores Play/Watch auto-approval.
        assert!(!allows(spec, Mode::Play, Origin::Mcp, None));
        // An explicit MCP grant is required.
        assert!(allows(
            spec,
            Mode::Chat,
            Origin::Mcp,
            Some(PermissionScope::BoardWrite)
        ));
    }

    #[test]
    fn study_content_requires_approval_by_default() {
        let registry = ToolRegistry::builtin();
        let spec = registry.get("add_chapter").unwrap();
        assert!(!allows(spec, Mode::Chat, Origin::InApp, None));
        assert!(!allows(spec, Mode::Chat, Origin::Mcp, None));
        // A board-write grant is not a study grant.
        assert!(!allows(
            spec,
            Mode::Chat,
            Origin::InApp,
            Some(PermissionScope::BoardWrite)
        ));
        assert!(allows(
            spec,
            Mode::Chat,
            Origin::InApp,
            Some(PermissionScope::StudyContent)
        ));
    }

    #[test]
    fn destructive_denied_without_destructive_scope() {
        let spec = destructive_spec();
        assert!(!allows(&spec, Mode::Chat, Origin::InApp, None));
        assert!(requires_approval(&spec, Mode::Chat, Origin::InApp, None));
        assert!(!allows(
            &spec,
            Mode::Chat,
            Origin::InApp,
            Some(PermissionScope::BoardWrite)
        ));
        assert!(!allows(
            &spec,
            Mode::Chat,
            Origin::InApp,
            Some(PermissionScope::StudyContent)
        ));
        assert!(allows(
            &spec,
            Mode::Chat,
            Origin::InApp,
            Some(PermissionScope::Destructive)
        ));
        // Destructive MCP calls still need the destructive scope.
        assert!(!allows(&spec, Mode::Chat, Origin::Mcp, None));
        assert!(allows(
            &spec,
            Mode::Chat,
            Origin::Mcp,
            Some(PermissionScope::Destructive)
        ));
    }

    #[test]
    fn structural_writes_are_denied_outside_chat() {
        let registry = ToolRegistry::builtin();
        for name in ["create_board", "copy_board", "create_study", "add_chapter"] {
            let spec = registry.get(name).unwrap();
            assert_eq!(
                decide(spec, Mode::Play, Origin::InApp, None),
                PolicyDecision::Deny,
                "{name} must not run during a game"
            );
            assert_eq!(
                decide(spec, Mode::Watch, Origin::InApp, None),
                PolicyDecision::Deny,
                "{name} must not run while watching"
            );
        }
    }

    #[test]
    fn profile_grants_are_per_origin() {
        let registry = ToolRegistry::builtin();
        let spec = registry.get("play_move").unwrap();
        let mut profile = PermissionProfile::default();
        assert!(!profile.allows(spec, Mode::Chat, Origin::InApp));
        profile.grant(Origin::InApp, PermissionScope::BoardWrite);
        assert!(profile.allows(spec, Mode::Chat, Origin::InApp));
        assert!(!profile.allows(spec, Mode::Chat, Origin::Mcp));
        profile.grant(Origin::Mcp, PermissionScope::BoardWrite);
        assert!(profile.allows(spec, Mode::Chat, Origin::Mcp));
        profile.revoke(Origin::Mcp, PermissionScope::BoardWrite);
        assert!(!profile.allows(spec, Mode::Chat, Origin::Mcp));
    }

    #[test]
    fn profile_defaults_are_empty() {
        let profile: PermissionProfile = serde_json::from_str("{}").unwrap();
        assert_eq!(profile, PermissionProfile::default());
        assert!(profile.scopes(Origin::InApp).is_empty());
        assert!(profile.scopes(Origin::Mcp).is_empty());
    }
}
