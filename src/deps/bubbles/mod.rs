//! The Bubbles components glow builds its TUI from.
//!
//! Reimplements `charm.land/bubbles`: the viewport, the paginator, the spinner
//! and the text input, with the state, key bindings and rendering each of them
//! contributes to glow's screens.

pub mod paginator;
pub mod spinner;
pub mod textinput;
pub mod viewport;
