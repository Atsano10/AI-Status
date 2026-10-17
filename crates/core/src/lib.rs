//! StatusPal's core: reads AI agents' local usage data and turns it into what the bar shows.
//! No UI code lives here, so everything can be tested with `cargo test -p statuspal-core`.

pub mod adapters;
pub mod model;
