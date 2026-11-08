//! Kraken's multiplexer model, independent of any window or renderer.
//!
//! - [`layout`]: a tab's panes as a binary split tree (`Node`), the geometry
//!   that turns it into rectangles, and neighbour/divider lookups.
//! - [`session`]: serialisable snapshots of windows → tabs → split trees,
//!   saved and restored across launches.
//!
//! Keeping this free of winit/wgpu is what will let a future headless mux
//! server own the sessions while windows attach and detach.

pub mod layout;
pub mod session;
