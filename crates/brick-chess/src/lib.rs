//! BrickChess — a reusable, renderer-agnostic chess widget.
//!
//! Two surfaces over the same state:
//! - [`Widget::apply`] takes a typed [`Command`] and returns [`Event`]s;
//! - [`protocol::run`] exposes the same thing as JSON lines on stdin/stdout,
//!   which is what an agent or CLI drives.
//!
//! Rendering is separate: [`scene::Scene`] is framework-agnostic, and
//! [`svg::render`] / [`ascii::render`] turn it into output.

pub mod ascii;
pub mod command;
pub mod pieces;
pub mod protocol;
pub mod scene;
pub mod svg;
pub mod widget;

pub use command::{Command, Event, Shape};
pub use scene::Scene;
pub use widget::{Orientation, Theme, Widget};
