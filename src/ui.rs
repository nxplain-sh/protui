//! protui's terminal UI, built on ratatui.
//!
//! The update loop performs no I/O: every pass-cli call is returned as a
//! command, executed off the loop by the runtime, and reported back as a
//! message. Only public key material ever reaches this module.

pub use runtime::run;
pub use theme::no_color;

mod create_form;
mod key_list;
mod keymap;
mod message;
mod model;
mod runtime;
mod text;
mod text_input;
mod theme;
mod update;
mod view;

#[cfg(test)]
mod tests;
