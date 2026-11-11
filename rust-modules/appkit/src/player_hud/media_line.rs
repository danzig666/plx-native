//! **The media line** — what is playing, in the words a spec sheet uses: `"4K · Dolby Vision ·
//! HEVC · TrueHD Atmos 7.1"` after the kicker over the title (`classic::draw_player_title_at`),
//! or `"Converting · H.264 · AAC"` on a re-encode. Pure functions over the session and the
//! playing item, so the draw and its tests read one answer.

/// `"4K · Dolby Vision · HEVC · TrueHD Atmos 7.1"` — the picture and the sound of THIS playback,
/// drawn after the kicker line over the title. Source facts on direct play (and on a remux, which
/// copies both streams); on a re-encode the codecs the server is actually sending, said as such,
/// because then the source's spec sheet is not what reaches the panel.
pub fn media_line(
    ps: &plx_media::route::PlaybackSession,
    meta: plx_data::metadata::MetadataView<'_>,
) -> String {
    let Some(item) = meta.playing() else {
        return String::new();
    };
    if plx_media::route::is_transcoding(ps) && !plx_media::route::is_remux(ps) {
        let mut parts = vec!["Converting".to_string()];
        let v = crate::info_panel::video_codec_name(&plx_media::route::stream_vcodec(ps));
        if !v.is_empty() {
            parts.push(v);
        }
        let a = audio_abbrev(&plx_media::route::stream_acodec(ps), "", "", 0);
        if !a.is_empty() {
            parts.push(a);
        }
        return parts.join(" \u{b7} ");
    }
    let audio = playing_audio(&item.audio, plx_media::route::cur_audio_sid(ps));
    source_line(
        item.width,
        item.height,
        item.dovi.present,
        item.hdr,
        &plx_media::route::source_vcodec(ps),
        audio,
    )
}

/// PURE: the direct-play / remux line from the source's facts.
fn source_line(
    w: i64,
    h: i64,
    dovi: bool,
    hdr: bool,
    vcodec: &str,
    audio: Option<&plx_data::metadata::Stream>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    let res = resolution_class(w, h);
    if !res.is_empty() {
        parts.push(res.to_string());
    }
    if dovi {
        parts.push("Dolby Vision".to_string());
    } else if hdr {
        parts.push("HDR".to_string());
    }
    let v = crate::info_panel::video_codec_name(vcodec);
    if !v.is_empty() {
        parts.push(v);
    }
    if let Some(s) = audio {
        let a = audio_abbrev(&s.codec, &s.profile, &s.layout, s.channels);
        if !a.is_empty() {
            parts.push(a);
        }
    }
    parts.join(" \u{b7} ")
}

/// The audio track on the wire: the one the viewer picked, else the server's selection, else the
/// file's default, else the first — the same ladder the track menu's checkmark walks.
fn playing_audio(
    audio: &[plx_data::metadata::Stream],
    picked: i64,
) -> Option<&plx_data::metadata::Stream> {
    audio
        .iter()
        .find(|s| picked > 0 && s.id == picked)
        .or_else(|| audio.iter().find(|s| s.selected))
        .or_else(|| audio.iter().find(|s| s.default))
        .or_else(|| audio.first())
}

/// The stored frame's class in the words a shelf badge uses: `4K` / `1080p` / `720p` / `SD`;
/// empty when the size is unknown. Judged on EITHER axis clearing the class, because a
/// scope-cropped 3840×1602 file is 4K and a 1920×800 one is 1080p.
pub fn resolution_class(w: i64, h: i64) -> &'static str {
    if w <= 0 || h <= 0 {
        ""
    } else if w >= 3800 || h >= 2100 {
        "4K"
    } else if w >= 1900 || h >= 1060 {
        "1080p"
    } else if w >= 1260 || h >= 700 {
        "720p"
    } else {
        "SD"
    }
}

/// `"DTS-HD MA 7.1"`, `"DD+ Atmos 5.1"`, `"AAC 2.0"` — a codec in its spec-sheet abbreviation,
/// then the layout. Deliberately NOT `metadata::friendly_codec`'s long names ("Dolby Digital
/// Plus"): this run shares one line with the kicker, so it uses the short forms every AV spec
/// sheet and Plex's own badges use. The profile carries what the codec name alone cannot —
/// MA / HRA / X for DTS, Atmos for the Dolby family.
pub fn audio_abbrev(codec: &str, profile: &str, layout: &str, channels: i64) -> String {
    let codec = codec.to_ascii_lowercase();
    let profile = profile.to_ascii_lowercase();
    let atmos = profile.contains("atmos");
    let mut name = match codec.as_str() {
        "truehd" => "TrueHD",
        "eac3" | "ec-3" | "e-ac-3" => "DD+",
        "ac3" | "ac-3" => "DD",
        "dts" | "dca" => {
            if profile.contains("ma") || profile.contains("master") {
                "DTS-HD MA"
            } else if profile.contains("hra") || profile.contains("high") {
                "DTS-HD HRA"
            } else if profile == "x" || profile.contains("dts:x") || profile.contains("dtsx") {
                "DTS:X"
            } else if profile.contains("es") {
                "DTS-ES"
            } else if profile.contains("96") {
                "DTS 96/24"
            } else {
                "DTS"
            }
        }
        "aac" => "AAC",
        "flac" => "FLAC",
        "opus" => "Opus",
        "mp3" => "MP3",
        "mp2" => "MP2",
        "vorbis" => "Vorbis",
        "alac" => "ALAC",
        c if c.starts_with("pcm") => "PCM",
        _ => "",
    }
    .to_string();
    if name.is_empty() && !codec.is_empty() {
        name = codec.to_uppercase();
    }
    if atmos && !name.is_empty() {
        name.push_str(" Atmos");
    }
    let lay = layout_short(layout, channels);
    match (name.is_empty(), lay.is_empty()) {
        (false, false) => format!("{name} {lay}"),
        (false, true) => name,
        (true, false) => lay,
        (true, true) => String::new(),
    }
}

/// `"5.1"`, `"7.1"`, `"2.0"`, `"1.0"` — the numeric layout every spec sheet uses, from the
/// server's ffmpeg-style layout (`5.1(side)`, `stereo`, `mono`), else from the channel count.
fn layout_short(layout: &str, channels: i64) -> String {
    let base = layout.split('(').next().unwrap_or("").trim().to_ascii_lowercase();
    let by_count = || match channels {
        0 => String::new(),
        1 => "1.0".to_string(),
        2 => "2.0".to_string(),
        6 => "5.1".to_string(),
        8 => "7.1".to_string(),
        n => format!("{n} ch"),
    };
    match base.as_str() {
        "" => by_count(),
        "mono" => "1.0".to_string(),
        "stereo" | "downmix" => "2.0".to_string(),
        b if b.starts_with(|c: char| c.is_ascii_digit()) => b.to_string(),
        _ => by_count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The media line's two vocabularies: the resolution CLASS a badge uses (either axis, so a
    /// scope-cropped file keeps its class) and the spec-sheet audio abbreviation with its
    /// layout, profile-aware for the DTS and Dolby families.
    #[test]
    fn the_media_line_speaks_spec_sheet() {
        assert_eq!(resolution_class(3840, 1602), "4K");
        assert_eq!(resolution_class(1920, 800), "1080p");
        assert_eq!(resolution_class(1440, 1080), "1080p");
        assert_eq!(resolution_class(1280, 534), "720p");
        assert_eq!(resolution_class(720, 576), "SD");
        assert_eq!(resolution_class(0, 0), "");
        assert_eq!(audio_abbrev("truehd", "dolby truehd + dolby atmos", "7.1", 8), "TrueHD Atmos 7.1");
        assert_eq!(audio_abbrev("eac3", "", "5.1(side)", 6), "DD+ 5.1");
        assert_eq!(audio_abbrev("ac3", "", "5.1", 6), "DD 5.1");
        assert_eq!(audio_abbrev("dca", "ma", "7.1", 8), "DTS-HD MA 7.1");
        assert_eq!(audio_abbrev("dca", "", "", 6), "DTS 5.1", "no layout: the count says it");
        assert_eq!(audio_abbrev("aac", "lc", "stereo", 2), "AAC 2.0");
        assert_eq!(audio_abbrev("pcm_s16le", "", "mono", 1), "PCM 1.0");
        assert_eq!(audio_abbrev("", "", "", 0), "");
        assert_eq!(audio_abbrev("wma", "", "", 2), "WMA 2.0", "unknown codec: upper-cased as is");
    }

    /// The whole direct-play line, and the audio half naming the track actually playing: the
    /// viewer's pick, else the server's selection, else the file default, else the first.
    #[test]
    fn the_media_line_names_the_picture_and_the_track_on_the_wire() {
        use plx_data::metadata::Stream;
        let t = |id, codec: &str, selected, default| Stream {
            id,
            codec: codec.into(),
            channels: 6,
            selected,
            default,
            ..Default::default()
        };
        let audio = vec![t(1, "ac3", false, false), t(2, "eac3", false, true), t(3, "dca", true, false)];
        assert_eq!(playing_audio(&audio, 1).map(|s| s.id), Some(1), "the pick wins");
        assert_eq!(playing_audio(&audio, 0).map(|s| s.id), Some(3), "then the selection");
        assert_eq!(playing_audio(&audio[..2], 0).map(|s| s.id), Some(2), "then the default");
        assert!(playing_audio(&[], 0).is_none());
        assert_eq!(
            source_line(1920, 800, false, false, "h264", playing_audio(&audio[..2], 0)),
            "1080p \u{b7} H.264 \u{b7} DD+ 5.1"
        );
        assert_eq!(source_line(3840, 2160, true, true, "hevc", None), "4K \u{b7} Dolby Vision \u{b7} HEVC");
        assert_eq!(source_line(3840, 2160, false, true, "hevc", None), "4K \u{b7} HDR \u{b7} HEVC");
    }
}
