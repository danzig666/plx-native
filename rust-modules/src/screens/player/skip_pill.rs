//! The **Skip Intro / Skip Credits** control. While the playhead sits inside one of the playing
//! leaf's server markers (`metadata::playing_markers`, fetched with `?includeMarkers=1`) this
//! button TAKES THE PLACE of the transport's Subtitles/Audio discs — same row, same right edge,
//! same height — and the discs return the moment the segment ends.
//!
//! Replacing them rather than joining them is the design call: one wide unmissable target in the
//! spot the eye already goes for transport controls. The cost is that CC/Audio are unreachable for
//! the length of the segment, and that the button only exists while the HUD is up — which is why
//! `app.rs` RAISES the HUD once when a segment begins and parks focus here, so a bare OK still
//! skips in one press instead of three.
#![allow(dead_code)]
use crate::metadata::{self, MarkerKind};
use crate::ui::consts::{SAFE, SCR_W};
use crate::ui::label::{HAlign, Label, VAlign};
use crate::ui::theme;
use crate::ui::widgets::{Button, ControlGround};
use crate::ui::{Env, Painter, Rect, View};
use std::ffi::{CStr, CString};

/// What pressing the button does. The distinction is the `final` flag on a credits marker: an
/// ordinary segment is a seek and playback continues past it, but a `final` one runs to the end of
/// the item, so "skip" means **the episode is over** — seeking to its end would race the decoder
/// against its own last frames for no benefit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SkipAction {
    /// jump to this position (ns) and keep playing
    Seek(i64),
    /// the item is finished — hand off to the end-of-playback path (next episode, or leave)
    Finish,
}

/// The offer the button is currently making. Carries the marker's TYPED kind, not its label:
/// asking "is the playhead in a credits segment?" by string-comparing user-facing copy made a pure
/// copy-edit — the sort of thing a design pass does — silently disable Up Next and auto-advance,
/// with no compile error and nothing to fail.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Prompt {
    /// the segment itself — carried so activating the button can retire it (`metadata::mark_skipped`)
    pub(crate) marker: metadata::Marker,
    pub(crate) kind: MarkerKind,
    pub(crate) action: SkipAction,
}

impl Prompt {
    /// The button's copy — presentation, derived from the kind at the point of drawing.
    pub(crate) fn label(&self) -> &'static str {
        match self.kind {
            MarkerKind::Intro => "Skip Intro",
            MarkerKind::Credits => "Skip Credits",
        }
    }
}

/// PURE: the offer a given segment makes. Takes the marker rather than reading the playhead, so
/// the precedence it feeds ([`crate::ui::player_hud::slot_for`]) is host-testable and the whole frame
/// decides from ONE playhead sample — `playpos_ns` is written by LG's media thread, and re-reading
/// it per call site let the input path and the draw path disagree within a single frame.
///
/// Markers belong to the PLAYING leaf (`metadata::playing()`), never to `metadata::current()`:
/// during a show-page episode play `current()` is the SHOW, and offering episode 1's intro timing
/// during episode 5 is the same identity bug the track store exists to prevent.
pub(crate) fn prompt_for(m: metadata::Marker) -> Prompt {
    Prompt {
        marker: m,
        kind: m.kind,
        action: match m.kind {
            // a `final` credits segment runs to the end of the item, so "skip" means the episode is
            // over — seeking to its end would race the decoder against its own last frames
            MarkerKind::Credits if m.final_seg => SkipAction::Finish,
            _ => SkipAction::Seek(m.end_ms * 1_000_000),
        },
    }
}

/// **"Skip intros automatically"** (Settings → Playback): the seek an INTRO offer is taken with at
/// once, instead of raising the Skip Intro button, and the segment to retire before it. `None` —
/// the ordinary button — for the switch off, for a credits offer (Up Next and "let it run" are
/// choices the viewer makes), and for anything that is not a seek. PURE, so the rule is
/// host-testable; the frame loop performs it on the offer's fresh edge (`app/run.rs`).
pub(crate) fn auto_skip(
    slot: crate::ui::player_hud::ControlSlot,
    enabled: bool,
) -> Option<(metadata::Marker, i64)> {
    match slot {
        crate::ui::player_hud::ControlSlot::Skip(Prompt {
            marker,
            kind: MarkerKind::Intro,
            action: SkipAction::Seek(ns),
        }) if enabled => Some((marker, ns)),
        _ => None,
    }
}

/// The button's rect — the SHARED control-row slot, so it and Up Next cannot drift apart.
///
/// `row` is the player instance's own [`crate::ui::player_hud::TransportRow`] (restructure phase
/// 9): it carries both the label-width memo this measurement is cached in and the control row's
/// focus springs. It was a module `static mut` on the other side of `ctrl_slot` until then.
pub(crate) fn rect(row: &mut crate::ui::player_hud::TransportRow, pr: Prompt, measure: &dyn crate::ui::machine::Measure) -> Rect {
    crate::ui::player_hud::ctrl_slot(row, pr.label(), measure)
}

/// Draw the button in the control row. Called by `player_hud` INSTEAD of the two discs.
pub(crate) fn draw(row: &mut crate::ui::player_hud::TransportRow, p: Painter, pr: Prompt, focused: bool, measure: &dyn crate::ui::machine::Measure) {
    let Ok(label) = CString::new(pr.label()) else {
        return;
    };
    // No leading icon: the label alone carries it, and a chevron on a control that does not
    // navigate anywhere was reading as "more" rather than "skip".
    // Its slot's only item, so index 0 — the pop is the control ROW's (`TransportRow::scale`),
    // shared with the transport discs this pill stands in for.
    let slot = rect(row, pr, measure);
    let pop = row.scale(0);
    Button::new(label.as_ptr(), theme::size::BODY, slot)
        .scale(pop)
        .focused(focused)
        // It stands in the transport discs' own slot, over the video plane and on the HUD's ramp,
        // so it wears their ground as well as their pop — see `ControlGround`.
        .ground(ControlGround::Unkeyed)
        .draw(&Env::inert(), p);
}

// ---- the notice an automatic skip leaves behind ----------------------------------------------

/// How long the "Intro skipped" notice stays up after an automatic skip, its fades included. Long
/// enough to be read across a room; short enough that it is gone before the first scene settles.
pub(crate) const SKIP_TOAST_MS: u32 = 2_500;
/// Each fade's share of [`SKIP_TOAST_MS`] — in at the start, out at the end.
const SKIP_TOAST_FADE_MS: u32 = 200;
const SKIP_TOAST_TEXT: &CStr = c"Intro skipped";
const SKIP_TOAST_H: f32 = 64.0;
const SKIP_TOAST_PAD: f32 = 32.0;

/// PURE: the notice's opacity `since_ms` after the skip — a linear fade in, a hold at 1, a linear
/// fade out, and 0 from [`SKIP_TOAST_MS`] on.
pub(crate) fn skip_toast_alpha(since_ms: u32) -> f32 {
    if since_ms >= SKIP_TOAST_MS {
        0.0
    } else if since_ms < SKIP_TOAST_FADE_MS {
        since_ms as f32 / SKIP_TOAST_FADE_MS as f32
    } else {
        ((SKIP_TOAST_MS - since_ms) as f32 / SKIP_TOAST_FADE_MS as f32).min(1.0)
    }
}

/// The notice's frame for a label `text_w` wide: centred at the top of the safe area, where
/// nothing else on the player route draws — the transport and the captions own the bottom, the
/// diagnostics read-out the top-left corner.
fn skip_toast_rect(text_w: f32) -> Rect {
    let w = text_w + 2.0 * SKIP_TOAST_PAD;
    Rect::new((SCR_W - w) * 0.5, SAFE.y, w, SKIP_TOAST_H)
}

/// Draw "Intro skipped" at `alpha` ([`skip_toast_alpha`]); nothing at 0. Not a control: it takes
/// no focus and no keys, and every key keeps doing what it did while it is up. Its own opaque
/// ground, like the read-outs', because on this route the UI plane is cleared transparent and
/// the pill stands on whatever frame of video is under it.
pub(crate) fn draw_skip_toast(alpha: f32, measure: &dyn crate::ui::machine::Measure) {
    if alpha <= 0.0 {
        return;
    }
    let r = skip_toast_rect(measure.width(SKIP_TOAST_TEXT, theme::size::BODY, true));
    let p = Painter::root().alpha(alpha);
    p.rect(r, r.h * 0.5, theme::PANEL_TOP, theme::PANEL_BOT, 0.0);
    Label::new(SKIP_TOAST_TEXT.as_ptr(), theme::size::BODY, theme::TEXT_PRIMARY)
        .bold()
        .h(HAlign::Center)
        .v(VAlign::Middle)
        .draw(p, r);
}

#[cfg(test)]
mod auto_skip_tests {
    use super::*;
    use crate::ui::player_hud::ControlSlot;

    fn seg(kind: MarkerKind, final_seg: bool) -> metadata::Marker {
        metadata::Marker { kind, start_ms: 10_000, end_ms: 70_000, final_seg }
    }

    /// Only an intro is taken automatically, only with the switch on, and at the segment's end.
    #[test]
    fn only_an_intro_is_skipped_automatically_and_only_when_asked() {
        let intro = seg(MarkerKind::Intro, false);
        assert_eq!(
            auto_skip(ControlSlot::Skip(prompt_for(intro)), true),
            Some((intro, 70_000 * 1_000_000))
        );
        assert_eq!(auto_skip(ControlSlot::Skip(prompt_for(intro)), false), None, "switch off");
        for credits in [seg(MarkerKind::Credits, false), seg(MarkerKind::Credits, true)] {
            assert_eq!(auto_skip(ControlSlot::Skip(prompt_for(credits)), true), None);
        }
        assert_eq!(auto_skip(ControlSlot::Discs, true), None);
    }

    /// The notice fades in, holds, fades out and is GONE at its deadline — the deadline is also
    /// what the page's clock fingerprint keys the last present on.
    #[test]
    fn the_skip_notice_fades_in_holds_and_is_gone_at_its_deadline() {
        assert_eq!(skip_toast_alpha(0), 0.0);
        assert!(skip_toast_alpha(SKIP_TOAST_FADE_MS / 2) > 0.0);
        assert_eq!(skip_toast_alpha(SKIP_TOAST_FADE_MS), 1.0);
        assert_eq!(skip_toast_alpha(SKIP_TOAST_MS / 2), 1.0);
        assert!(skip_toast_alpha(SKIP_TOAST_MS - SKIP_TOAST_FADE_MS / 2) < 1.0);
        assert_eq!(skip_toast_alpha(SKIP_TOAST_MS), 0.0);
        assert_eq!(skip_toast_alpha(u32::MAX), 0.0);
    }

    /// It is read from across a room, so it must sit inside the overscan frame at any label width.
    #[test]
    fn the_skip_notice_stays_inside_the_safe_area() {
        for w in [0.0, 200.0, 600.0] {
            let r = skip_toast_rect(w);
            assert!(crate::ui::consts::inside_safe(r), "label width {w}: {r:?}");
        }
    }
}
