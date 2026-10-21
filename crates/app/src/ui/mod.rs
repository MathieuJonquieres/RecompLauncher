//! Shared visual language for the redesigned launcher.
//!
//! Every colour, radius and spacing value used by the UI comes from
//! [`theme::Tokens`]. Views never hard-code palette values; they ask the
//! current theme for a semantic token instead.

pub mod art;
pub mod components;
pub mod fonts;
pub mod format;
pub mod icons;
pub mod modal;
pub mod text;
pub mod theme;
