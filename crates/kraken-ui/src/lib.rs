//! Kraken's chrome widgets. Each module turns some state into rectangles and
//! text lines for a [`kraken_render::Frame`]; none of them owns a window or
//! handles raw input, so they can be tested and reused on their own.

pub mod agents;
pub mod anim;
pub mod cards;
pub mod gitpop;
pub mod links;
pub mod overview;
pub mod search;
pub mod sheet;
pub mod sidebar;
pub mod splash;
pub mod statusbar;
