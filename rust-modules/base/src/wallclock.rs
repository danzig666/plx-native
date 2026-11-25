//! **A time of day** — the value the player's corner clock shows (this fork). Only the type lives
//! here (the leaf layer every other layer may name); the app publishes each frame's wall time
//! (`AppViews::wall`, a `plx_ui::fmt::Wall`), and the player turns it into this with
//! `Wall::time_of_day`. A screen reads the published value and no clock.

/// An hour and a minute of the local day, `0..24` and `0..60`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WallTime {
    pub hour: u8,
    pub minute: u8,
}
