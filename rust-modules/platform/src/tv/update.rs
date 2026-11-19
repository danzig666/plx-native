//! **Installing a newer PlxNative through Homebrew Channel** — the one door the app's updater
//! (`app::update`) uses. The app may not install a package itself; Homebrew Channel's service
//! (`luna://org.webosbrew.hbchannel.service/install`, `{ipkUrl, ipkHash, id}`) downloads it,
//! checks its SHA-256 and hands it to the system installer, streaming its progress back.
//!
//! What the service sends, read from its source (`webosbrew/webos-homebrew-channel`,
//! `services/service.ts`, the `install` method): `{"statusText":"Downloading…"}`, then
//! `{"statusText":"Downloading…","progress":41.7}` every 300 ms while it downloads,
//! `{"statusText":"Verifying…"}`, `{"statusText":"Installing…"}` and finally
//! `{"returnValue":true,"statusText":"Finished.","finished":true}`; any failure (a download that
//! answered an error, a checksum that does not match, an install the system refused) is
//! `{"returnValue":false,"errorText":"…"}`. Its cancel handler does nothing (`TODO: support
//! cancellation`), so once handed over an install runs to its end whatever the caller does —
//! and the system installer stops the running app while it replaces it, so on success this
//! process usually never sees `finished`.
//!
//! The bus call is the webOS port's ([`crate::webos::hbc_install`], through [`super::Port`]); it
//! blocks for the whole install, so [`install`] runs on a worker.

/// What the port reports when the call itself could not be made or went quiet. Not the
/// service's own refusal, which arrives as a reply and is read by [`read_reply`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortFail {
    /// No bus on this platform (the simulator, a test).
    Unsupported,
    /// The bus would not register this process or send the call.
    NoBus,
    /// The call was sent and the service stopped answering before it finished.
    Silent,
}

/// One step of an install, as the updater shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// Homebrew Channel is downloading the package; the percentage when it knows the size.
    Downloading(Option<u32>),
    /// The download is complete and its SHA-256 is being checked.
    Verifying,
    /// The system installer has the package. The app is normally stopped during this step.
    Installing,
    /// The install completed and this process is still running.
    Finished,
    Failed(Failure),
}

impl Progress {
    /// An answer after which the service sends nothing more.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Progress::Finished | Progress::Failed(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// This TV has no Homebrew Channel service (the bus answered for a missing service).
    NoHomebrewChannel,
    /// The service refused, in its own words (a download error, `Invalid file checksum (…)`, an
    /// installer error).
    Refused(String),
    /// The call was never sent (no bus): nothing was handed over, so it can be tried again.
    NotSent,
    /// The service went quiet before it finished. Its install may still be running (its cancel
    /// does nothing), so it is not offered again in this run.
    NoAnswer,
    /// Not on a TV.
    Unsupported,
}

const SERVICE_ABSENT: [&str; 2] = ["does not exist", "Unknown service"];

/// One reply of the service, read into a step; `None` for a reply that says nothing new (a bare
/// `{"returnValue":true}`, a status this updater does not name).
pub fn read_reply(reply: &str) -> Option<Progress> {
    let v: serde_json::Value = serde_json::from_str(reply).ok()?;
    let error = v.get("errorText").and_then(serde_json::Value::as_str);
    if v.get("returnValue") == Some(&serde_json::Value::Bool(false)) || error.is_some() {
        let text = error.unwrap_or("").trim();
        if SERVICE_ABSENT.iter().any(|absent| text.contains(absent)) {
            return Some(Progress::Failed(Failure::NoHomebrewChannel));
        }
        return Some(Progress::Failed(Failure::Refused(text.to_owned())));
    }
    if v.get("finished") == Some(&serde_json::Value::Bool(true)) {
        return Some(Progress::Finished);
    }
    let status = v.get("statusText").and_then(serde_json::Value::as_str)?;
    if status.starts_with("Download") {
        let percent = v.get("progress").and_then(serde_json::Value::as_f64)
            .filter(|p| p.is_finite())
            .map(|p| p.clamp(0.0, 100.0) as u32);
        return Some(Progress::Downloading(percent));
    }
    if status.starts_with("Verif") {
        return Some(Progress::Verifying);
    }
    if status.starts_with("Install") || status.starts_with("Self-update") {
        return Some(Progress::Installing);
    }
    None
}

/// The `install` call's payload. `id` is the service's fallback for the package name when it
/// cannot read the package's own.
pub fn payload(ipk_url: &str, sha256: &str, app_id: &str) -> String {
    serde_json::json!({ "ipkUrl": ipk_url, "ipkHash": sha256, "id": app_id, "subscribe": true }).to_string()
}

/// Hand the package to Homebrew Channel and report each step to `report`, ending with a
/// terminal one ([`Progress::is_terminal`]) unless the process is stopped by the install first.
/// Blocks for the whole install: call it from a worker.
pub fn install(ipk_url: &str, sha256: &str, app_id: &str, report: &mut dyn FnMut(Progress)) {
    let _block = plx_base::task::assert_may_block(
        const { &plx_base::task::BlockingLabel::new("Homebrew Channel install") });
    let mut ended = false;
    let result = (super::port().install_package)(&payload(ipk_url, sha256, app_id), &mut |reply| {
        match read_reply(reply) {
            Some(step) => {
                ended = step.is_terminal();
                report(step);
                !ended
            }
            None => true,
        }
    });
    if ended {
        return;
    }
    report(Progress::Failed(match result {
        Err(PortFail::Unsupported) => Failure::Unsupported,
        Err(PortFail::NoBus) => Failure::NotSent,
        Err(PortFail::Silent) | Ok(()) => Failure::NoAnswer,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_reply_the_service_sends_reads_as_its_step() {
        assert_eq!(read_reply(r#"{"statusText":"Downloading…"}"#), Some(Progress::Downloading(None)));
        assert_eq!(read_reply(r#"{"statusText":"Downloading…","progress":41.7}"#), Some(Progress::Downloading(Some(41))));
        assert_eq!(read_reply(r#"{"statusText":"Downloading…","progress":140}"#), Some(Progress::Downloading(Some(100))));
        assert_eq!(read_reply(r#"{"statusText":"Verifying…"}"#), Some(Progress::Verifying));
        assert_eq!(read_reply(r#"{"statusText":"Installing…"}"#), Some(Progress::Installing));
        assert_eq!(read_reply(r#"{"returnValue":true,"statusText":"Finished.","finished":true}"#), Some(Progress::Finished));
        assert_eq!(read_reply(r#"{"returnValue":true}"#), None, "a bare success says nothing new");
        assert_eq!(read_reply("not json"), None);
    }

    #[test]
    fn a_refusal_keeps_the_services_words_and_a_missing_service_is_named() {
        assert_eq!(
            read_reply(r#"{"returnValue":false,"errorText":"Invalid file checksum (ab expected, got cd"}"#),
            Some(Progress::Failed(Failure::Refused("Invalid file checksum (ab expected, got cd".into())))
        );
        assert_eq!(
            read_reply(r#"{"returnValue":false,"errorCode":-1,"errorText":"Service does not exist: org.webosbrew.hbchannel.service."}"#),
            Some(Progress::Failed(Failure::NoHomebrewChannel))
        );
        assert_eq!(read_reply(r#"{"returnValue":false}"#), Some(Progress::Failed(Failure::Refused(String::new()))));
        assert!(Progress::Failed(Failure::NoAnswer).is_terminal() && Progress::Finished.is_terminal());
        assert!(!Progress::Installing.is_terminal());
    }

    #[test]
    fn the_payload_asks_for_progress_and_names_the_package() {
        let v: serde_json::Value = serde_json::from_str(&payload("https://x/a.ipk", "ab", "com.beb.plxnative")).unwrap();
        assert_eq!(v["ipkUrl"], "https://x/a.ipk");
        assert_eq!(v["ipkHash"], "ab");
        assert_eq!(v["id"], "com.beb.plxnative");
        assert_eq!(v["subscribe"], true);
    }

    /// With no port installed (a test, the simulator) an install ends at once, as Unsupported.
    #[test]
    fn without_a_bus_an_install_ends_unsupported() {
        let mut steps = Vec::new();
        install("https://x/a.ipk", "ab", "com.beb.plxnative", &mut |s| steps.push(s));
        assert_eq!(steps, vec![Progress::Failed(Failure::Unsupported)]);
    }
}
