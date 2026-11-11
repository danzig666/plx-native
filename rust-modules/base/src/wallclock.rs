//! **A time of day** — the value the player's corner clock shows. Only the type lives here (the
//! leaf layer every other layer may name); the local time is read by the
//! application's clock (`app::clock::time_of_day`, through `plx_platform::i18n::local_clock_time`), which the loop publishes into the frame's
//! views. A screen reads the published value and no clock.

/// An hour and a minute of the local day, `0..24` and `0..60`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WallTime {
    pub hour: u8,
    pub minute: u8,
}
