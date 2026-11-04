//! Assistant domain types for RustyBrickChess.
//!
//! This crate is split by Cargo features:
//!
//! - `protocol` (default): serde DTOs that are safe to compile for the frontend.
//! - `core`: context integration, tools and policy.
//! - `host`: provider I/O, storage and engine adapters.
//!
//! Only the protocol, tool registry and policy layers are implemented in this
//! slice (PKG-020 and PKG-021, partial). There is intentionally no network,
//! database, filesystem or engine access here.

pub mod policy;
pub mod protocol;
pub mod tools;

#[cfg(feature = "host")]
pub mod store;

/// Protocol/schema revision shared by the in-app assistant and MCP.
pub const PROTOCOL_VERSION: &str = "1";
