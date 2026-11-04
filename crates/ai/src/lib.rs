//! Assistant domain types for RustyBrickChess.
//!
//! This crate is split by Cargo features:
//!
//! - `protocol` (default): serde DTOs that are safe to compile for the frontend.
//! - `core`: context integration, tools and policy.
//! - `provider`: model adapters — the [`provider::Provider`] trait, SSE parsing
//!   and the OpenAI-compatible implementation.
//! - `host`: storage and engine adapters on top of `provider`.
//!
//! Only the protocol, tool registry and policy layers are implemented in this
//! slice (PKG-020 and PKG-021, partial). Nothing outside `provider` and
//! `store` touches the network, the database or the filesystem.

pub mod policy;
pub mod protocol;
pub mod result;
pub mod tools;

#[cfg(feature = "core")]
pub mod glicko;

#[cfg(feature = "core")]
pub mod tournament;

#[cfg(feature = "host")]
pub mod tournament_store;

#[cfg(feature = "host")]
pub mod ratings;

#[cfg(feature = "core")]
pub mod game;

#[cfg(feature = "provider")]
pub mod context_diff;

#[cfg(feature = "provider")]
pub mod play;

#[cfg(feature = "provider")]
pub mod provider;

#[cfg(feature = "provider")]
pub mod agent;

#[cfg(feature = "host")]
pub mod approval;

#[cfg(feature = "host")]
pub mod board;

#[cfg(feature = "host")]
pub mod events;

#[cfg(feature = "host")]
pub mod reads;

#[cfg(feature = "host")]
pub mod store;

/// Protocol/schema revision shared by the in-app assistant and MCP.
pub const PROTOCOL_VERSION: &str = "1";
