//! **This fork's self-updater** (Settings › Software update), after the owner's ytnative app: a
//! check reads the fork's Homebrew Channel repository file — the same one Homebrew Channel lists the
//! fork from ([`FEED`], written by `ci/fork-homebrew-repo.py` at every fork release) — and when it
//! names a newer version than this build, Settings offers to install it. The install is
//! Homebrew Channel's (`plx_platform::tv::update`): the app may not install a package itself, and
//! the system installer stops the app while it replaces it.
//!
//! **When it checks.** Once at start-up when the last check is older than [`AUTO_EVERY_SECS`]
//! (the stamp is a file in the runtime directory, so a reboot also counts as "long ago"), and
//! whenever the viewer asks. A start-up check that finds a newer version says so once, through the
//! television's own toast, as `clock_notice` does: it names where to install it.
//!
//! **Only the stable package updates itself.** A development install (`com.beb.plxnative.debug`
//! and the other flavours) is a different app id; installing the release package from it would
//! install a second app rather than update this one, so there the page says the updater is not
//! available.
//!
//! Every request runs on a worker; the frame loop only drains their answers ([`Updater::tick`]),
//! and the Bridge publishes the state with each frame's views (`AppViews::update`), which the pages
//! read; they ask nothing, so a recording of Settings holds no effect of this module's.

use plx_screens::registry::{UpdateCmd, UpdatePhase, UpdateStatus};
use std::sync::mpsc;

/// The fork's Homebrew Channel repository file.
pub(crate) const FEED: &str = "https://raw.githubusercontent.com/danzig666/plx-native/homebrew/repo.json";
/// The repository file is a few hundred bytes; anything past this is not it.
const MAX_FEED_BYTES: usize = 64 * 1024;
/// A start-up check runs when the last one is at least this old.
const AUTO_EVERY_SECS: u64 = 20 * 3600;
/// The stamp of the last check that read the feed, in Unix seconds.
const STAMP: &str = "plxnative-update-checked";

/// A release the feed names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Release {
    pub(crate) version: String,
    pub(crate) ipk_url: String,
    pub(crate) sha256: String,
}

/// What a worker reports to the frame loop.
enum Msg {
    /// A check finished: the newest release when the feed could be read.
    Checked { found: Result<Release, String>, auto: bool },
    /// A start-up check found its stamp too recent and asked nothing.
    NotDue,
    Install(plx_platform::tv::update::Progress),
}

pub(crate) struct Updater {
    /// The stable package on a television: anything else answers [`UpdatePhase::Unavailable`].
    enabled: bool,
    current: &'static str,
    phase: UpdatePhase,
    /// The newer release the last check found, kept through a failed install to try again.
    found: Option<Release>,
    tx: mpsc::Sender<Msg>,
    rx: mpsc::Receiver<Msg>,
    /// A start-up check found a newer version: the toast is owed once.
    tell: Option<String>,
    /// The state as the frames publish it, rebuilt after every change ([`Self::view`]).
    view: UpdateStatus,
}

impl Updater {
    pub(crate) fn new(enabled: bool, current: &'static str) -> Self {
        let (tx, rx) = mpsc::channel();
        let phase = if enabled { UpdatePhase::Idle } else { UpdatePhase::Unavailable };
        let mut updater = Self { enabled, current, phase, found: None, tx, rx, tell: None,
            view: UpdateStatus::UNKNOWN };
        updater.publish();
        updater
    }

    /// The updater of a live run (`live` is the Bridge's own live-or-controlled answer): the stable
    /// package updates itself; a development flavour, a controlled run and a test host do not.
    pub(crate) fn for_run(live: bool) -> Self {
        Self::new(live && plx_base::paths::app_id() == plx_base::paths::STABLE_APP_ID,
            plx_plex::plex::identity::version())
    }

    /// The state this frame publishes.
    pub(crate) fn view(&self) -> &UpdateStatus {
        &self.view
    }

    fn publish(&mut self) {
        self.view = UpdateStatus {
            phase: self.phase,
            current: self.current.to_owned(),
            latest: self.found.as_ref().map(|r| r.version.clone()),
        };
    }

    #[cfg(test)]
    fn status(&self) -> UpdateStatus {
        self.view.clone()
    }

    /// A page's command. The state it leaves is published at once ([`Self::view`]).
    pub(crate) fn command(&mut self, cmd: UpdateCmd) {
        match cmd {
            UpdateCmd::Check => self.check(false),
            UpdateCmd::Install => self.install(),
        }
        self.publish();
    }

    /// The start-up check, when the last one is old enough (decided on the worker, which reads the
    /// stamp).
    pub(crate) fn start(&mut self) {
        self.check(true);
        self.publish();
    }

    fn check(&mut self, auto: bool) {
        if !self.enabled || self.phase.busy() {
            return;
        }
        self.phase = UpdatePhase::Checking;
        let tx = self.tx.clone();
        let current = self.current;
        let started = plx_base::task::spawn_small("update-check", move || {
            let now = plx_telemetry::telemetry::crashreport::now_secs();
            let stamp = plx_base::paths::in_runtime_dir(STAMP);
            let last = std::fs::read_to_string(&stamp).ok().and_then(|s| s.trim().parse().ok());
            let msg = if auto && !due(last, now) {
                Msg::NotDue
            } else {
                let found = fetch();
                // Only a check that read the feed holds the next start-up check off: one made
                // before the network was up is tried again at the next start.
                if found.is_ok() {
                    let _ = std::fs::write(&stamp, now.to_string());
                }
                match &found {
                    Ok(release) => plx_base::eventlog::log(&format!(
                        "update: the feed names {} (this build {current})", release.version)),
                    Err(why) => plx_base::eventlog::log(&format!("update: check failed: {why}")),
                }
                Msg::Checked { found, auto }
            };
            let _ = tx.send(msg);
            plx_machine::present::wake_from_worker();
        });
        if !started {
            self.phase = UpdatePhase::CheckFailed;
        }
    }

    fn install(&mut self) {
        let Some(release) = self.found.clone() else { return };
        if !self.enabled || self.phase.busy() || matches!(self.phase, UpdatePhase::Installed | UpdatePhase::Unconfirmed) {
            return;
        }
        self.phase = UpdatePhase::Downloading(None);
        plx_base::eventlog::log(&format!("update: installing {} through Homebrew Channel", release.version));
        let tx = self.tx.clone();
        let started = plx_base::task::spawn_small("update-install", move || {
            plx_platform::tv::update::install(&release.ipk_url, &release.sha256, plx_base::paths::STABLE_APP_ID,
                &mut |step| {
                    let _ = tx.send(Msg::Install(step));
                    plx_machine::present::wake_from_worker();
                });
        });
        if !started {
            self.phase = UpdatePhase::InstallFailed;
        }
    }

    /// One frame's look at the workers' answers. `true` when the state moved.
    pub(crate) fn tick(&mut self) -> bool {
        let mut moved = false;
        while let Ok(msg) = self.rx.try_recv() {
            moved = true;
            self.land(msg);
        }
        if moved {
            self.publish();
        }
        if let Some(version) = self.tell.take() {
            let message = plx_platform::i18n::msg::settings_update_toast(&version);
            plx_base::task::spawn_small("update notice", move || {
                let outcome = plx_platform::tv::toast::toast(&message);
                plx_base::eventlog::log(&format!("update: toast {outcome:?}"));
            });
        }
        moved
    }

    fn land(&mut self, msg: Msg) {
        use plx_platform::tv::update::{Failure, Progress};
        match msg {
            Msg::NotDue => self.phase = UpdatePhase::Idle,
            Msg::Checked { found: Ok(release), auto } => {
                if newer(&release.version, self.current) {
                    if auto {
                        self.tell = Some(release.version.clone());
                    }
                    self.found = Some(release);
                    self.phase = UpdatePhase::Available;
                } else {
                    self.found = None;
                    self.phase = UpdatePhase::UpToDate;
                }
            }
            Msg::Checked { found: Err(_), .. } => {
                // A start-up check that could not reach the feed stays quiet.
                self.phase = if self.found.is_some() { UpdatePhase::Available } else { UpdatePhase::CheckFailed };
            }
            Msg::Install(step) => {
                // A late answer never turns an ended install back into progress.
                if matches!(self.phase, UpdatePhase::Installed | UpdatePhase::InstallFailed
                    | UpdatePhase::NoHomebrewChannel | UpdatePhase::Unconfirmed) {
                    return;
                }
                self.phase = match step {
                    Progress::Downloading(percent) => UpdatePhase::Downloading(percent.map(|p| p.min(100) as u8)),
                    Progress::Verifying => UpdatePhase::Verifying,
                    Progress::Installing => UpdatePhase::Installing,
                    Progress::Finished => UpdatePhase::Installed,
                    Progress::Failed(failure) => {
                        plx_base::eventlog::log(&format!("update: install failed: {failure:?}"));
                        match failure {
                            Failure::NoHomebrewChannel => UpdatePhase::NoHomebrewChannel,
                            Failure::NoAnswer => UpdatePhase::Unconfirmed,
                            Failure::Refused(_) | Failure::NotSent | Failure::Unsupported => UpdatePhase::InstallFailed,
                        }
                    }
                };
            }
        }
    }
}

/// Read the feed and name its release of the stable package.
fn fetch() -> Result<Release, String> {
    let resp = plx_net::net::https_get_public_bounded(FEED, MAX_FEED_BYTES).map_err(|e| format!("{e:?}"))?;
    if resp.status != 200 {
        return Err(format!("HTTP {}", resp.status));
    }
    parse_feed(&String::from_utf8_lossy(&resp.body)).ok_or_else(|| "unreadable repository file".to_owned())
}

/// The stable package's release in a Homebrew Channel repository file
/// (`{"packages":[{"id":…,"manifest":{"version","ipkUrl","ipkHash":{"sha256"}}}]}`).
pub(crate) fn parse_feed(json: &str) -> Option<Release> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let package = v["packages"].as_array()?.iter().find(|p| p["id"] == plx_base::paths::STABLE_APP_ID)?;
    let manifest = &package["manifest"];
    let version = manifest["version"].as_str()?;
    let ipk_url = manifest["ipkUrl"].as_str()?;
    let sha256 = manifest["ipkHash"]["sha256"].as_str()?;
    let hex = sha256.len() == 64 && sha256.bytes().all(|b| b.is_ascii_hexdigit());
    (hex && ipk_url.starts_with("https://") && plx_data::hudpacks::Semver::parse(version).is_some()).then(|| Release {
        version: version.to_owned(),
        ipk_url: ipk_url.to_owned(),
        sha256: sha256.to_ascii_lowercase(),
    })
}

/// Is `candidate` a later release than this build? A build with a suffix (`0.11.0-dev`) is
/// compared by its numbers.
pub(crate) fn newer(candidate: &str, current: &str) -> bool {
    use plx_data::hudpacks::Semver;
    match (Semver::parse(candidate), Semver::app_release(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

/// Is a start-up check due, given the last one's stamp? A stamp from the future (a clock that was
/// wrong) does not hold checks off.
fn due(last: Option<u64>, now: u64) -> bool {
    match last {
        Some(last) if last <= now => now - last >= AUTO_EVERY_SECS,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED_JSON: &str = r#"{"packages":[{"id":"com.beb.plxnative","manifest":{"id":"com.beb.plxnative","version":"0.10.25",
        "ipkUrl":"https://github.com/danzig666/plx-native/releases/download/v0.10.0-fork.25/com.beb.plxnative_0.10.25_arm.ipk",
        "ipkHash":{"sha256":"A17B81A8FBCB602F5A9941DE17AC1258FDFB454B3EF64E53363467A4A3F2032F"},"ipkSize":21111612}}]}"#;

    #[test]
    fn the_feed_names_the_stable_packages_release() {
        let release = parse_feed(FEED_JSON).expect("the fork's repository file reads");
        assert_eq!(release.version, "0.10.25");
        assert!(release.ipk_url.ends_with("com.beb.plxnative_0.10.25_arm.ipk"));
        assert_eq!(release.sha256, "a17b81a8fbcb602f5a9941de17ac1258fdfb454b3ef64e53363467a4a3f2032f", "the hash is lowercased");
    }

    #[test]
    fn a_feed_missing_anything_or_naming_another_app_names_nothing() {
        assert!(parse_feed("{}").is_none());
        assert!(parse_feed(&FEED_JSON.replace(r#""id":"com.beb.plxnative","manifest""#, r#""id":"org.other","manifest""#)).is_none());
        assert!(parse_feed(&FEED_JSON.replace("https://github.com", "http://github.com")).is_none(), "only https");
        assert!(parse_feed(&FEED_JSON.replace("A17B81", "Z17B81")).is_none(), "not a hash");
        assert!(parse_feed(&FEED_JSON.replace("0.10.25", "0.10")).is_none(), "not a version");
    }

    #[test]
    fn versions_compare_by_their_numbers() {
        assert!(newer("0.10.25", "0.10.24"));
        assert!(newer("0.10.10", "0.10.9"));
        assert!(newer("0.11.0", "0.10.99"));
        assert!(!newer("0.10.24", "0.10.24"));
        assert!(!newer("0.10.23", "0.10.24"));
        assert!(!newer("0.10.25", "0.11.0-dev"), "a development build is ahead of every fork release");
        assert!(!newer("garbage", "0.10.24"));
    }

    #[test]
    fn a_start_up_check_waits_for_its_interval() {
        assert!(due(None, 1_000), "never checked");
        assert!(!due(Some(1_000), 1_000 + AUTO_EVERY_SECS - 1));
        assert!(due(Some(1_000), 1_000 + AUTO_EVERY_SECS));
        assert!(due(Some(5_000), 1_000), "a stamp from the future holds nothing off");
    }

    /// Land one answer as `tick` does, and publish.
    fn landed(u: &mut Updater, msg: Msg) {
        u.land(msg);
        u.publish();
    }

    fn rig() -> Updater {
        Updater::new(true, "0.10.24")
    }

    fn release(version: &str) -> Release {
        Release { version: version.into(), ipk_url: "https://x/a.ipk".into(), sha256: "ab".into() }
    }

    #[test]
    fn a_newer_release_is_offered_and_a_start_up_find_is_told_once() {
        let mut u = rig();
        u.phase = UpdatePhase::Checking;
        landed(&mut u, Msg::Checked { found: Ok(release("0.10.25")), auto: true });
        let s = u.status();
        assert_eq!((s.phase, s.latest.as_deref(), s.current.as_str()), (UpdatePhase::Available, Some("0.10.25"), "0.10.24"));
        assert_eq!(u.tell.as_deref(), Some("0.10.25"), "the toast is owed");
        let mut v = rig();
        landed(&mut v, Msg::Checked { found: Ok(release("0.10.25")), auto: false });
        assert!(v.tell.is_none(), "a check the viewer asked for is answered on the page, not by a toast");
    }

    #[test]
    fn the_same_or_an_older_release_is_up_to_date_and_a_failed_check_keeps_a_known_find() {
        let mut u = rig();
        landed(&mut u, Msg::Checked { found: Ok(release("0.10.24")), auto: false });
        assert_eq!(u.status().phase, UpdatePhase::UpToDate);
        landed(&mut u, Msg::Checked { found: Err("offline".into()), auto: false });
        assert_eq!(u.status().phase, UpdatePhase::CheckFailed);
        landed(&mut u, Msg::Checked { found: Ok(release("0.10.26")), auto: false });
        landed(&mut u, Msg::Checked { found: Err("offline".into()), auto: false });
        assert_eq!((u.status().phase, u.status().latest.as_deref()), (UpdatePhase::Available, Some("0.10.26")));
        landed(&mut u, Msg::NotDue);
        assert_eq!(u.status().phase, UpdatePhase::Idle);
    }

    #[test]
    fn install_steps_land_in_order_and_a_late_one_never_revives_an_ended_install() {
        use plx_platform::tv::update::{Failure, Progress};
        let mut u = rig();
        u.found = Some(release("0.10.25"));
        u.phase = UpdatePhase::Downloading(None);
        landed(&mut u, Msg::Install(Progress::Downloading(Some(41))));
        assert_eq!(u.status().phase, UpdatePhase::Downloading(Some(41)));
        landed(&mut u, Msg::Install(Progress::Verifying));
        landed(&mut u, Msg::Install(Progress::Installing));
        assert_eq!(u.status().phase, UpdatePhase::Installing);
        landed(&mut u, Msg::Install(Progress::Failed(Failure::Refused("Invalid file checksum".into()))));
        assert_eq!(u.status().phase, UpdatePhase::InstallFailed);
        landed(&mut u, Msg::Install(Progress::Downloading(Some(90))));
        assert_eq!(u.status().phase, UpdatePhase::InstallFailed, "a late answer stays out");
        let mut v = rig();
        v.found = Some(release("0.10.25"));
        landed(&mut v, Msg::Install(Progress::Failed(Failure::NoHomebrewChannel)));
        assert_eq!(v.status().phase, UpdatePhase::NoHomebrewChannel);
    }

    /// Homebrew Channel's cancel does nothing, so an install that went quiet may still be running:
    /// it is not offered again in this run. One that was never sent can be tried again.
    #[test]
    fn a_silent_install_is_never_started_twice_and_an_unsent_one_can_be_retried() {
        use plx_platform::tv::update::{Failure, Progress};
        let mut u = rig();
        u.found = Some(release("0.10.25"));
        landed(&mut u, Msg::Install(Progress::Downloading(Some(10))));
        landed(&mut u, Msg::Install(Progress::Failed(Failure::NoAnswer)));
        assert_eq!(u.view().phase, UpdatePhase::Unconfirmed);
        u.command(UpdateCmd::Install);
        assert_eq!(u.view().phase, UpdatePhase::Unconfirmed, "no second install");
        let mut v = rig();
        v.found = Some(release("0.10.25"));
        landed(&mut v, Msg::Install(Progress::Failed(Failure::NotSent)));
        assert_eq!(v.view().phase, UpdatePhase::InstallFailed, "nothing was handed over: Install is offered again");
    }

    #[test]
    fn a_development_install_has_no_updater_and_asks_nothing() {
        let mut u = Updater::new(false, "0.11.0-dev");
        u.command(UpdateCmd::Check);
        assert_eq!(u.view().phase, UpdatePhase::Unavailable);
        u.command(UpdateCmd::Install);
        assert_eq!(u.view().phase, UpdatePhase::Unavailable);
        assert!(!u.tick());
    }

    #[test]
    fn install_without_a_find_does_nothing() {
        let mut u = rig();
        u.command(UpdateCmd::Install);
        assert_eq!(u.view().phase, UpdatePhase::Idle);
    }
}
