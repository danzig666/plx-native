//! **The time of day on this television** — the one wall-clock read the app makes, for the
//! player's corner clock.
//!
//! `localtime_r` over the C library's zone (`TZ`, else `/etc/localtime`). That this zone follows
//! the TV's own Time settings is what the clock assumes and a television has to confirm; nothing
//! here asks the settings service. It is read by the application's loop at
//! the frame split, never by a screen: a screen sees the [`WallTime`] the frame was published
//! with, so a replay or a test supplies its own and draws the same picture every run.

/// An hour and a minute of the local day, `0..24` and `0..60`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WallTime {
    pub hour: u8,
    pub minute: u8,
}

/// The local time now, or `None` when the C library cannot say (a clock before 1970, a zone it
/// failed to load into a broken-down time).
pub fn local_now() -> Option<WallTime> {
    // SAFETY: `time(NULL)` only returns a value; `localtime_r` writes the caller's own `tm` and
    // reads the process's zone, which nothing in the app changes after boot.
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    if now < 0 {
        return None;
    }
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&now, &mut tm) }.is_null() {
        return None;
    }
    let (hour, minute) = (u8::try_from(tm.tm_hour).ok()?, u8::try_from(tm.tm_min).ok()?);
    (hour < 24 && minute < 60).then_some(WallTime { hour, minute })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_local_time_is_an_hour_and_a_minute_of_the_day() {
        let t = local_now().expect("the host has a clock and a zone");
        assert!(t.hour < 24 && t.minute < 60, "{t:?}");
    }
}
