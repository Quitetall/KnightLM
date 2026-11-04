//! Stable tool specifications, the built-in registry and argument validation.
//!
//! Tool names, versions and JSON schemas are shared verbatim by the in-app
//! assistant and MCP. The registry deliberately contains chess and study tools
//! only: no Lua execution, no app lifecycle/settings, and no AI chat-session
//! tools. Those boundaries are part of the MCP security model.
//!
//! Validation here is intentionally small and dependency-free: it checks that
//! arguments are an object, that every key is known, that required keys are
//! present, and that primitive types/enums match. It is not a general JSON
//! Schema implementation; the host adapters still enforce domain rules such as
//! revision conflicts.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::protocol::SessionMode;

/// Version stamped on every built-in tool schema.
pub const TOOL_SCHEMA_VERSION: &str = "1.0.0";

/// What a tool can do, used by the permission policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolEffect {
    /// Reads state and never mutates it.
    Read,
    /// Mutates board content: moves, shapes, annotations, notes, board creation.
    BoardWrite,
    /// Mutates session-owned structure: studies, chapters, study prose.
    SessionWrite,
    /// Irreversible operations. Never auto-approved by a board or study grant.
    Destructive,
    /// Queries an external source (engine, opening explorer, tablebase, database).
    External,
}

impl ToolEffect {
    /// Stable wire/database spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::BoardWrite => "board-write",
            Self::SessionWrite => "session-write",
            Self::Destructive => "destructive",
            Self::External => "external",
        }
    }

    /// Whether the effect can change persisted state.
    pub const fn is_write(self) -> bool {
        matches!(
            self,
            Self::BoardWrite | Self::SessionWrite | Self::Destructive
        )
    }
}

impl fmt::Display for ToolEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One versioned tool contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    /// Stable snake_case name, for example `play_move`.
    pub name: String,
    /// Schema version. Built-ins all use [`TOOL_SCHEMA_VERSION`].
    pub version: String,
    /// One sentence for the model and the tool card.
    pub description: String,
    /// JSON Schema (object) describing the arguments.
    pub parameters: Value,
    pub effects: ToolEffect,
    /// Interaction modes in which an in-app session may call this tool.
    /// MCP is not mode-gated.
    pub modes: Vec<SessionMode>,
}

impl ToolSpec {
    /// Check the spec's internal consistency.
    ///
    /// This is used when the registry is built and when a spec arrives from a
    /// plugin or test; it does not validate a call's arguments.
    pub fn validate(&self) -> Result<(), RegistryError> {
        if !valid_tool_name(&self.name) {
            return Err(self.invalid("name must be lowercase snake_case"));
        }
        if !valid_version(&self.version) {
            return Err(self.invalid("version must be a dotted numeric version"));
        }
        if self.description.trim().is_empty() {
            return Err(self.invalid("description must not be empty"));
        }

        let schema = self
            .parameters
            .as_object()
            .ok_or_else(|| self.invalid("parameters must be an object schema"))?;
        if schema.get("type").and_then(Value::as_str) != Some("object") {
            return Err(self.invalid("parameters.type must be \"object\""));
        }
        if schema.get("additionalProperties").and_then(Value::as_bool) != Some(false) {
            return Err(self.invalid("parameters.additionalProperties must be false"));
        }

        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .ok_or_else(|| self.invalid("parameters.properties must be an object"))?;
        let required = schema
            .get("required")
            .and_then(Value::as_array)
            .ok_or_else(|| self.invalid("parameters.required must be an array"))?;

        let mut required_names = HashSet::new();
        for value in required {
            let Some(name) = value.as_str() else {
                return Err(self.invalid("parameters.required entries must be strings"));
            };
            if !properties.contains_key(name) {
                return Err(self.invalid("required argument is absent from properties"));
            }
            required_names.insert(name);
        }

        // Revisioned writes must never silently default to "latest revision".
        if properties.contains_key("expected_revision")
            && !required_names.contains("expected_revision")
        {
            return Err(self.invalid("expected_revision must be a required argument"));
        }

        for (name, property) in properties {
            let property = property
                .as_object()
                .ok_or_else(|| self.invalid("each property must be an object schema"))?;
            let Some(kind) = property.get("type") else {
                return Err(self.invalid("each property must declare a type"));
            };
            if !valid_schema_type(kind) {
                return Err(self.invalid("property type must be a JSON type or type array"));
            }
            if name == "expected_revision"
                && property.get("type").and_then(Value::as_str) != Some("integer")
            {
                return Err(self.invalid("expected_revision must be an integer"));
            }
        }

        if self.modes.is_empty() {
            return Err(self.invalid("modes must not be empty"));
        }
        let mut modes = HashSet::new();
        for mode in &self.modes {
            if !modes.insert(*mode) {
                return Err(self.invalid("modes must not contain duplicates"));
            }
        }

        Ok(())
    }

    /// Whether an argument is required by the schema.
    pub fn requires_argument(&self, argument: &str) -> bool {
        self.parameters
            .get("required")
            .and_then(Value::as_array)
            .is_some_and(|required| {
                required
                    .iter()
                    .any(|value| value.as_str() == Some(argument))
            })
    }

    /// Whether this tool requires `expected_revision`.
    pub fn requires_expected_revision(&self) -> bool {
        self.requires_argument("expected_revision")
    }

    /// Whether the tool mutates persisted state.
    pub const fn is_write(&self) -> bool {
        self.effects.is_write()
    }

    fn invalid(&self, reason: &str) -> RegistryError {
        RegistryError::InvalidTool {
            tool: self.name.clone(),
            reason: reason.to_string(),
        }
    }
}

/// Why a registry could not be constructed or validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    DuplicateTool(String),
    SchemaVersion {
        tool: String,
        expected: String,
        found: String,
    },
    InvalidTool {
        tool: String,
        reason: String,
    },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateTool(name) => write!(f, "duplicate tool {name:?}"),
            Self::SchemaVersion {
                tool,
                expected,
                found,
            } => write!(
                f,
                "tool {tool:?} uses schema version {found:?}, expected {expected:?}"
            ),
            Self::InvalidTool { tool, reason } => {
                write!(f, "invalid tool {tool:?}: {reason}")
            }
        }
    }
}

impl std::error::Error for RegistryError {}

/// Why an argument object was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgumentError {
    /// Arguments must be a JSON object.
    NotObject,
    /// A key is not declared by the tool's schema.
    UnknownArgument(String),
    /// A required key is absent or null.
    MissingRequired(String),
    /// A value does not match its declared primitive type.
    WrongType { argument: String, expected: String },
    /// A value is not one of the schema's allowed enum values.
    InvalidEnum {
        argument: String,
        allowed: Vec<String>,
    },
    /// The tool spec itself is malformed (a programming error).
    InvalidSchema(String),
}

impl fmt::Display for ArgumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotObject => f.write_str("arguments must be a JSON object"),
            Self::UnknownArgument(name) => write!(f, "unknown argument {name:?}"),
            Self::MissingRequired(name) => write!(f, "missing required argument {name:?}"),
            Self::WrongType { argument, expected } => {
                write!(f, "argument {argument:?} must be of type {expected}")
            }
            Self::InvalidEnum { argument, allowed } => write!(
                f,
                "argument {argument:?} must be one of [{}]",
                allowed.join(", ")
            ),
            Self::InvalidSchema(reason) => write!(f, "invalid tool schema: {reason}"),
        }
    }
}

impl std::error::Error for ArgumentError {}

/// Why a named call was refused before execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallError {
    /// No such tool is registered.
    UnknownTool(String),
    /// The tool exists but the arguments are invalid.
    InvalidArguments(ArgumentError),
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTool(name) => write!(f, "unknown tool {name:?}"),
            Self::InvalidArguments(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for CallError {}

impl From<ArgumentError> for CallError {
    fn from(error: ArgumentError) -> Self {
        Self::InvalidArguments(error)
    }
}

/// An ordered catalog of tool specs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolRegistry {
    tools: Vec<ToolSpec>,
}

impl ToolRegistry {
    /// Build a registry, rejecting duplicates, mismatched versions and malformed
    /// specs.
    pub fn new(tools: Vec<ToolSpec>) -> Result<Self, RegistryError> {
        let registry = Self { tools };
        registry.validate()?;
        Ok(registry)
    }

    /// The registry shared by the assistant and MCP.
    pub fn builtin() -> Self {
        let registry = Self {
            tools: builtin_specs(),
        };
        registry
            .validate()
            .expect("built-in tool registry must be valid");
        registry
    }

    /// Validate every spec and enforce registry-wide invariants.
    pub fn validate(&self) -> Result<(), RegistryError> {
        let mut names = HashSet::new();
        for spec in &self.tools {
            if !names.insert(spec.name.as_str()) {
                return Err(RegistryError::DuplicateTool(spec.name.clone()));
            }
            if spec.version != TOOL_SCHEMA_VERSION {
                return Err(RegistryError::SchemaVersion {
                    tool: spec.name.clone(),
                    expected: TOOL_SCHEMA_VERSION.to_string(),
                    found: spec.version.clone(),
                });
            }
            spec.validate()?;
        }
        Ok(())
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&ToolSpec> {
        self.tools.iter().find(|spec| spec.name == name)
    }

    /// Look up a tool only if it matches a specific schema version.
    pub fn get_version(&self, name: &str, version: &str) -> Option<&ToolSpec> {
        self.get(name).filter(|spec| spec.version == version)
    }

    /// All tool names in registration order.
    pub fn names(&self) -> Vec<&str> {
        self.tools.iter().map(|spec| spec.name.as_str()).collect()
    }

    /// Iterate over specs in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &ToolSpec> {
        self.tools.iter()
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Resolve a tool and validate its arguments with [`validate_arguments`].
    pub fn validate_call(&self, name: &str, arguments: &Value) -> Result<&ToolSpec, CallError> {
        let spec = self
            .get(name)
            .ok_or_else(|| CallError::UnknownTool(name.to_string()))?;
        validate_arguments(spec, arguments)?;
        Ok(spec)
    }
}

/// Validate arguments against the small, supported subset of JSON Schema.
///
/// Refuses: non-object arguments, unknown keys, missing/null required keys,
/// primitive type mismatches, and enum violations. Nested object properties are
/// type-checked as objects but not descended into, because the built-in schemas
/// are flat.
pub fn validate_arguments(spec: &ToolSpec, arguments: &Value) -> Result<(), ArgumentError> {
    let object = arguments.as_object().ok_or(ArgumentError::NotObject)?;
    let schema = spec
        .parameters
        .as_object()
        .ok_or_else(|| ArgumentError::InvalidSchema("parameters must be an object".into()))?;
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| ArgumentError::InvalidSchema("properties must be an object".into()))?;
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .ok_or_else(|| ArgumentError::InvalidSchema("required must be an array".into()))?;

    for key in object.keys() {
        if !properties.contains_key(key) {
            return Err(ArgumentError::UnknownArgument(key.clone()));
        }
    }

    for name in required {
        let name = name
            .as_str()
            .ok_or_else(|| ArgumentError::InvalidSchema("required entry is not a string".into()))?;
        if object.get(name).is_none_or(Value::is_null) {
            return Err(ArgumentError::MissingRequired(name.to_string()));
        }
    }

    for (key, value) in object {
        let property = properties
            .get(key)
            .expect("unknown keys were refused above");
        validate_value(key, value, property)?;
    }

    Ok(())
}

fn validate_value(name: &str, value: &Value, schema: &Value) -> Result<(), ArgumentError> {
    // Optional nulls are treated as absent.
    if value.is_null() {
        return Ok(());
    }

    if let Some(kind) = schema.get("type")
        && !matches_schema_type(kind, value)
    {
        return Err(ArgumentError::WrongType {
            argument: name.to_string(),
            expected: type_label(kind),
        });
    }

    if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
        && !allowed.iter().any(|candidate| candidate == value)
    {
        return Err(ArgumentError::InvalidEnum {
            argument: name.to_string(),
            allowed: allowed
                .iter()
                .map(|candidate| match candidate {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                })
                .collect(),
        });
    }

    Ok(())
}

fn matches_schema_type(kind: &Value, value: &Value) -> bool {
    match kind {
        Value::String(kind) => matches_type(kind, value),
        Value::Array(kinds) => kinds
            .iter()
            .filter_map(Value::as_str)
            .any(|kind| matches_type(kind, value)),
        _ => true,
    }
}

fn matches_type(kind: &str, value: &Value) -> bool {
    match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true,
    }
}

fn type_label(kind: &Value) -> String {
    match kind {
        Value::String(kind) => kind.clone(),
        Value::Array(kinds) => kinds
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("|"),
        _ => "value".to_string(),
    }
}

fn valid_schema_type(kind: &Value) -> bool {
    match kind {
        Value::String(kind) => is_json_type(kind),
        Value::Array(kinds) => {
            !kinds.is_empty() && kinds.iter().filter_map(Value::as_str).all(is_json_type)
        }
        _ => false,
    }
}

fn is_json_type(kind: &str) -> bool {
    matches!(
        kind,
        "object" | "array" | "string" | "number" | "integer" | "boolean" | "null"
    )
}

fn valid_tool_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn valid_version(version: &str) -> bool {
    !version.is_empty()
        && version.split('.').count() <= 3
        && version
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

// --- Built-in specs -------------------------------------------------------

const ALL_MODES: &[SessionMode] = &[SessionMode::Chat, SessionMode::Play, SessionMode::Watch];
const CHAT_ONLY: &[SessionMode] = &[SessionMode::Chat];

fn spec(
    name: &str,
    description: &str,
    parameters: Value,
    effects: ToolEffect,
    modes: &[SessionMode],
) -> ToolSpec {
    ToolSpec {
        name: name.to_string(),
        version: TOOL_SCHEMA_VERSION.to_string(),
        description: description.to_string(),
        parameters,
        effects,
        modes: modes.to_vec(),
    }
}

fn read(name: &str, description: &str, parameters: Value) -> ToolSpec {
    spec(name, description, parameters, ToolEffect::Read, ALL_MODES)
}

fn external(name: &str, description: &str, parameters: Value) -> ToolSpec {
    spec(
        name,
        description,
        parameters,
        ToolEffect::External,
        ALL_MODES,
    )
}

fn board_write(
    name: &str,
    description: &str,
    parameters: Value,
    modes: &[SessionMode],
) -> ToolSpec {
    spec(name, description, parameters, ToolEffect::BoardWrite, modes)
}

fn session_write(name: &str, description: &str, parameters: Value) -> ToolSpec {
    spec(
        name,
        description,
        parameters,
        ToolEffect::SessionWrite,
        CHAT_ONLY,
    )
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn id_prop() -> Value {
    json!({"type": "string", "minLength": 1})
}

fn optional_id_prop() -> Value {
    json!({"type": "string"})
}

fn string_prop() -> Value {
    json!({"type": "string"})
}

fn revision_prop() -> Value {
    json!({"type": "integer", "minimum": 0})
}

fn version_prop() -> Value {
    json!({"type": "integer", "minimum": 1})
}

fn color_prop() -> Value {
    json!({"type": "string", "enum": ["green", "red", "blue", "yellow"]})
}

fn board_kind_prop() -> Value {
    json!({
        "type": "string",
        "enum": [
            "analysis",
            "game",
            "puzzle",
            "study_primary",
            "study_additional",
            "compare"
        ]
    })
}

fn opening_source_prop() -> Value {
    json!({"type": "string", "enum": ["local", "lichess", "masters"]})
}

fn builtin_specs() -> Vec<ToolSpec> {
    vec![
        // --- Read ---------------------------------------------------------
        read(
            "get_position",
            "Read the current position, FEN, turn and revision of a board.",
            object(
                json!({
                    "board_id": id_prop()
                }),
                &["board_id"],
            ),
        ),
        read(
            "get_history",
            "Read the move history of a board as SAN/UCI with clocks.",
            object(
                json!({
                    "board_id": id_prop(),
                    "from": {"type": "integer", "minimum": 0},
                    "to": {"type": "integer", "minimum": 0}
                }),
                &["board_id"],
            ),
        ),
        read(
            "legal_moves",
            "List legal moves for the current position, sorted SAN and UCI.",
            object(
                json!({
                    "board_id": id_prop()
                }),
                &["board_id"],
            ),
        ),
        external(
            "analyze",
            "Run the engine on a board and return lines, score, PV and fingerprint.",
            object(
                json!({
                    "board_id": id_prop(),
                    "depth": {"type": "integer", "minimum": 1}
                }),
                &["board_id"],
            ),
        ),
        external(
            "evaluate",
            "Return the current evaluation summary for a board.",
            object(
                json!({
                    "board_id": id_prop()
                }),
                &["board_id"],
            ),
        ),
        external(
            "opening_explorer",
            "Look up opening moves from the local, Lichess or masters source.",
            object(
                json!({
                    "board_id": id_prop(),
                    "source": opening_source_prop(),
                    "fen": string_prop()
                }),
                &["board_id"],
            ),
        ),
        external(
            "search_database",
            "Search the game database and return aggregate counts and samples.",
            object(
                json!({
                    "query": {"type": "string", "minLength": 1}
                }),
                &["query"],
            ),
        ),
        external(
            "tablebase",
            "Query tablebase win/draw/loss and distance-to-zero for a board.",
            object(
                json!({
                    "board_id": id_prop()
                }),
                &["board_id"],
            ),
        ),
        read(
            "get_game",
            "Read a stored game's tags, PGN and moves.",
            object(
                json!({
                    "game_id": id_prop()
                }),
                &["game_id"],
            ),
        ),
        read(
            "list_workspaces",
            "List workspaces with ids, kinds and board counts.",
            object(json!({}), &[]),
        ),
        read(
            "list_boards",
            "List boards, optionally scoped to one workspace.",
            object(
                json!({
                    "workspace_id": optional_id_prop()
                }),
                &[],
            ),
        ),
        read(
            "get_board",
            "Read a board's FEN, turn, history, annotations, shapes and notes.",
            object(
                json!({
                    "board_id": id_prop()
                }),
                &["board_id"],
            ),
        ),
        read(
            "list_studies",
            "List studies with ids, titles and current versions.",
            object(json!({}), &[]),
        ),
        read(
            "get_study",
            "Read a study's chapters, boards and metadata.",
            object(
                json!({
                    "study_id": id_prop()
                }),
                &["study_id"],
            ),
        ),
        read(
            "get_chapter",
            "Read one chapter, its primary board and its notes.",
            object(
                json!({
                    "study_id": id_prop(),
                    "chapter_id": id_prop()
                }),
                &["study_id", "chapter_id"],
            ),
        ),
        read(
            "export_pgn",
            "Export a board as PGN.",
            object(
                json!({
                    "board_id": id_prop()
                }),
                &["board_id"],
            ),
        ),
        // --- Board writes -------------------------------------------------
        board_write(
            "play_move",
            "Play one legal move on a board. Returns applied, illegal or conflict.",
            object(
                json!({
                    "board_id": id_prop(),
                    "move": {"type": "string", "minLength": 1},
                    "expected_revision": revision_prop()
                }),
                &["board_id", "move", "expected_revision"],
            ),
            ALL_MODES,
        ),
        board_write(
            "draw_arrow",
            "Draw an arrow on a board.",
            object(
                json!({
                    "board_id": id_prop(),
                    "from": {"type": "string", "minLength": 1},
                    "to": {"type": "string", "minLength": 1},
                    "color": color_prop(),
                    "expected_revision": revision_prop()
                }),
                &["board_id", "from", "to", "expected_revision"],
            ),
            ALL_MODES,
        ),
        board_write(
            "draw_circle",
            "Draw a circle on a board square.",
            object(
                json!({
                    "board_id": id_prop(),
                    "square": {"type": "string", "minLength": 1},
                    "color": color_prop(),
                    "expected_revision": revision_prop()
                }),
                &["board_id", "square", "expected_revision"],
            ),
            ALL_MODES,
        ),
        board_write(
            "clear_shapes",
            "Remove all arrows and circles from a board.",
            object(
                json!({
                    "board_id": id_prop(),
                    "expected_revision": revision_prop()
                }),
                &["board_id", "expected_revision"],
            ),
            ALL_MODES,
        ),
        board_write(
            "annotate",
            "Add or replace an annotation at a ply.",
            object(
                json!({
                    "board_id": id_prop(),
                    "ply": {"type": "integer", "minimum": 0},
                    "nag": {"type": "string"},
                    "comment": {"type": "string"},
                    "expected_revision": revision_prop()
                }),
                &["board_id", "ply", "expected_revision"],
            ),
            ALL_MODES,
        ),
        board_write(
            "write_note",
            "Replace a board's markdown notes.",
            object(
                json!({
                    "board_id": id_prop(),
                    "markdown": {"type": "string"},
                    "expected_revision": revision_prop()
                }),
                &["board_id", "markdown", "expected_revision"],
            ),
            ALL_MODES,
        ),
        board_write(
            "create_board",
            "Create a new board in a workspace.",
            object(
                json!({
                    "workspace_id": id_prop(),
                    "fen": {"type": "string"},
                    "title": {"type": "string"},
                    "kind": board_kind_prop()
                }),
                &["workspace_id"],
            ),
            CHAT_ONLY,
        ),
        board_write(
            "copy_board",
            "Copy a board, creating a new id with provenance.",
            object(
                json!({
                    "board_id": id_prop(),
                    "workspace_id": optional_id_prop(),
                    "title": {"type": "string"}
                }),
                &["board_id"],
            ),
            CHAT_ONLY,
        ),
        // --- Study writes -------------------------------------------------
        session_write(
            "create_study",
            "Create a new study in draft state.",
            object(
                json!({
                    "title": {"type": "string", "minLength": 1},
                    "description": {"type": "string"}
                }),
                &["title"],
            ),
        ),
        session_write(
            "add_chapter",
            "Append a chapter to a study.",
            object(
                json!({
                    "study_id": id_prop(),
                    "title": {"type": "string", "minLength": 1},
                    "fen": {"type": "string"}
                }),
                &["study_id", "title"],
            ),
        ),
        session_write(
            "add_board_to_chapter",
            "Add a board to an existing chapter.",
            object(
                json!({
                    "study_id": id_prop(),
                    "chapter_id": id_prop(),
                    "fen": {"type": "string"},
                    "title": {"type": "string"}
                }),
                &["study_id", "chapter_id"],
            ),
        ),
        session_write(
            "reorder_chapters",
            "Set the chapter order of a study.",
            object(
                json!({
                    "study_id": id_prop(),
                    "chapter_ids": {
                        "type": "array",
                        "items": {"type": "string"},
                        "minItems": 1
                    }
                }),
                &["study_id", "chapter_ids"],
            ),
        ),
        session_write(
            "write_study_text",
            "Replace a study's description or a chapter's markdown notes.",
            object(
                json!({
                    "study_id": id_prop(),
                    "chapter_id": optional_id_prop(),
                    "markdown": {"type": "string"},
                    "expected_revision": revision_prop()
                }),
                &["study_id", "markdown", "expected_revision"],
            ),
        ),
        session_write(
            "publish_study",
            "Publish the current study draft as a frozen version.",
            object(
                json!({
                    "study_id": id_prop(),
                    "expected_version": version_prop()
                }),
                &["study_id", "expected_version"],
            ),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPECTED_TOOLS: &[&str] = &[
        "get_position",
        "get_history",
        "legal_moves",
        "analyze",
        "evaluate",
        "opening_explorer",
        "search_database",
        "tablebase",
        "get_game",
        "list_workspaces",
        "list_boards",
        "get_board",
        "list_studies",
        "get_study",
        "get_chapter",
        "export_pgn",
        "play_move",
        "draw_arrow",
        "draw_circle",
        "clear_shapes",
        "annotate",
        "write_note",
        "create_board",
        "copy_board",
        "create_study",
        "add_chapter",
        "add_board_to_chapter",
        "reorder_chapters",
        "write_study_text",
        "publish_study",
    ];

    const REVISIONED_WRITES: &[&str] = &[
        "play_move",
        "draw_arrow",
        "draw_circle",
        "clear_shapes",
        "annotate",
        "write_note",
        "write_study_text",
    ];

    fn builtin() -> ToolRegistry {
        ToolRegistry::builtin()
    }

    fn sample_value(property: &Value) -> Value {
        if let Some(first) = property
            .get("enum")
            .and_then(Value::as_array)
            .and_then(|values| values.first())
        {
            return first.clone();
        }
        match property
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("string")
        {
            "integer" => json!(property.get("minimum").and_then(Value::as_u64).unwrap_or(0)),
            "number" => json!(0),
            "boolean" => json!(true),
            "array" => json!(["x"]),
            "object" => json!({}),
            _ => json!("x"),
        }
    }

    fn sample_arguments(spec: &ToolSpec) -> Value {
        let properties = spec.parameters["properties"]
            .as_object()
            .expect("properties");
        let required = spec.parameters["required"].as_array().expect("required");
        let mut arguments = serde_json::Map::new();
        for name in required {
            let name = name.as_str().expect("required name");
            let property = properties.get(name).expect("required property");
            arguments.insert(name.to_string(), sample_value(property));
        }
        Value::Object(arguments)
    }

    #[test]
    fn registry_contains_expected_tools() {
        let registry = builtin();
        let mut names = registry.names();
        names.sort_unstable();
        let mut expected = EXPECTED_TOOLS.to_vec();
        expected.sort_unstable();
        assert_eq!(names, expected);
        assert_eq!(registry.len(), EXPECTED_TOOLS.len());
        registry.validate().expect("built-in registry is valid");
    }

    #[test]
    fn registry_contains_no_lua_app_or_chat_tools() {
        let registry = builtin();
        for name in registry.names() {
            let tokens: Vec<&str> = name.split('_').collect();
            assert!(!tokens.contains(&"lua"), "unexpected Lua tool {name}");
            assert!(!tokens.contains(&"chat"), "unexpected chat tool {name}");
            assert!(
                !tokens.contains(&"session"),
                "unexpected session tool {name}"
            );
            assert!(
                !tokens.contains(&"setting") && !tokens.contains(&"settings"),
                "unexpected settings tool {name}"
            );
            assert!(
                !tokens.contains(&"app"),
                "unexpected app-lifecycle tool {name}"
            );
        }
        for forbidden in [
            "run_lua",
            "eval_lua",
            "get_settings",
            "update_settings",
            "list_sessions",
            "create_session",
            "delete_session",
            "get_session",
            "shutdown_app",
        ] {
            assert!(
                registry.get(forbidden).is_none(),
                "{forbidden} must not be registered"
            );
        }
    }

    #[test]
    fn unknown_tool_is_refused() {
        let registry = builtin();
        assert!(registry.get("run_lua").is_none());
        assert_eq!(
            registry.validate_call("run_lua", &json!({})),
            Err(CallError::UnknownTool("run_lua".into()))
        );
    }

    #[test]
    fn unknown_argument_is_refused() {
        let registry = builtin();
        let error = registry
            .validate_call("get_board", &json!({"board_id": "b1", "bogus": 1}))
            .expect_err("unknown argument must fail");
        assert_eq!(
            error,
            CallError::InvalidArguments(ArgumentError::UnknownArgument("bogus".into()))
        );
        // Even tools with no arguments refuse extra keys.
        let error = registry
            .validate_call("list_workspaces", &json!({"workspace_id": "w1"}))
            .expect_err("unknown argument must fail");
        assert_eq!(
            error,
            CallError::InvalidArguments(ArgumentError::UnknownArgument("workspace_id".into()))
        );
    }

    #[test]
    fn required_arguments_are_enforced() {
        let registry = builtin();
        let error = registry
            .validate_call("get_board", &json!({}))
            .expect_err("missing board_id");
        assert_eq!(
            error,
            CallError::InvalidArguments(ArgumentError::MissingRequired("board_id".into()))
        );
    }

    #[test]
    fn argument_types_are_checked() {
        let registry = builtin();
        let play = registry.get("play_move").unwrap();
        let mut arguments = sample_arguments(play);
        arguments["move"] = json!(5);
        assert_eq!(
            validate_arguments(play, &arguments),
            Err(ArgumentError::WrongType {
                argument: "move".into(),
                expected: "string".into(),
            })
        );

        // Wrong enum value.
        let arrow = registry.get("draw_arrow").unwrap();
        let mut arguments = sample_arguments(arrow);
        arguments["color"] = json!("purple");
        assert_eq!(
            validate_arguments(arrow, &arguments),
            Err(ArgumentError::InvalidEnum {
                argument: "color".into(),
                allowed: vec!["green".into(), "red".into(), "blue".into(), "yellow".into()],
            })
        );
    }

    #[test]
    fn non_object_arguments_are_refused() {
        let registry = builtin();
        let error = registry
            .validate_call("get_board", &json!("board-1"))
            .expect_err("scalar arguments");
        assert_eq!(error, CallError::InvalidArguments(ArgumentError::NotObject));
    }

    #[test]
    fn optional_arguments_may_be_omitted_or_null() {
        let registry = builtin();
        let analyze = registry.get("analyze").unwrap();
        assert!(validate_arguments(analyze, &json!({"board_id": "b1"})).is_ok());
        assert!(validate_arguments(analyze, &json!({"board_id": "b1", "depth": null})).is_ok());
        let chapter = registry.get("get_chapter").unwrap();
        assert!(
            validate_arguments(chapter, &json!({"study_id": "s1", "chapter_id": "c1"})).is_ok()
        );
    }

    #[test]
    fn expected_revision_is_required_on_revisioned_writes() {
        let registry = builtin();
        for name in REVISIONED_WRITES {
            let spec = registry.get(name).unwrap_or_else(|| panic!("{name}"));
            assert!(
                spec.requires_expected_revision(),
                "{name} must require expected_revision"
            );
            let mut arguments = sample_arguments(spec)
                .as_object()
                .expect("sample object")
                .clone();
            arguments.remove("expected_revision");
            assert_eq!(
                validate_arguments(spec, &Value::Object(arguments)),
                Err(ArgumentError::MissingRequired("expected_revision".into())),
                "{name} accepted a write without expected_revision"
            );
        }
        // Publishing is versioned rather than revisioned.
        let publish = registry.get("publish_study").unwrap();
        assert!(publish.requires_argument("expected_version"));
        assert!(!publish.requires_expected_revision());
    }

    #[test]
    fn every_builtin_accepts_well_typed_arguments() {
        let registry = builtin();
        for spec in registry.iter() {
            let arguments = sample_arguments(spec);
            validate_arguments(spec, &arguments)
                .unwrap_or_else(|error| panic!("{} sample rejected: {error}", spec.name));
        }
    }

    #[test]
    fn every_write_is_approval_relevant() {
        let registry = builtin();
        for spec in registry.iter() {
            if !spec.is_write() {
                continue;
            }
            assert!(!spec.modes.is_empty(), "{} has no modes", spec.name);
        }
    }

    #[test]
    fn duplicate_names_are_refused() {
        let registry = builtin();
        let spec = registry.get("get_board").unwrap().clone();
        assert_eq!(
            ToolRegistry::new(vec![spec.clone(), spec]),
            Err(RegistryError::DuplicateTool("get_board".into()))
        );
    }

    #[test]
    fn schema_version_mismatch_is_refused() {
        let registry = builtin();
        let mut spec = registry.get("get_board").unwrap().clone();
        spec.version = "2.0.0".into();
        assert_eq!(
            ToolRegistry::new(vec![spec]),
            Err(RegistryError::SchemaVersion {
                tool: "get_board".into(),
                expected: TOOL_SCHEMA_VERSION.into(),
                found: "2.0.0".into(),
            })
        );
    }

    #[test]
    fn malformed_specs_are_refused() {
        let registry = builtin();

        let mut open = registry.get("get_board").unwrap().clone();
        open.parameters["additionalProperties"] = json!(true);
        assert!(matches!(
            ToolRegistry::new(vec![open]),
            Err(RegistryError::InvalidTool { .. })
        ));

        let mut optional_revision = registry.get("play_move").unwrap().clone();
        optional_revision.parameters["required"] = json!(["board_id", "move"]);
        assert!(matches!(
            ToolRegistry::new(vec![optional_revision]),
            Err(RegistryError::InvalidTool { .. })
        ));

        let mut bad_name = registry.get("get_board").unwrap().clone();
        bad_name.name = "GetBoard".into();
        assert!(matches!(
            ToolRegistry::new(vec![bad_name]),
            Err(RegistryError::InvalidTool { .. })
        ));
    }

    #[test]
    fn specs_round_trip_through_serde() {
        let registry = builtin();
        let encoded = serde_json::to_string(&registry).unwrap();
        let decoded: ToolRegistry = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, registry);
        assert_eq!(
            decoded.get("play_move").unwrap().effects,
            ToolEffect::BoardWrite
        );
    }

    #[test]
    fn versioned_lookup_checks_the_version() {
        let registry = builtin();
        assert!(
            registry
                .get_version("get_board", TOOL_SCHEMA_VERSION)
                .is_some()
        );
        assert!(registry.get_version("get_board", "0.9.0").is_none());
    }
}
