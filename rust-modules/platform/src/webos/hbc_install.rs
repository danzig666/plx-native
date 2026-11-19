//! The bus half of [`crate::tv::update`]: Homebrew Channel's `install`, streamed. The service's
//! replies and what they mean are documented there; this module only carries them.

use crate::tv::update::PortFail;

#[cfg(all(not(feature = "hostsim"), not(any(test, feature = "test-support"))))]
const INSTALL_URI: &str = "luna://org.webosbrew.hbchannel.service/install";

/// How long the service may say nothing before the install is given up as silent. It reports a
/// download every 300 ms; the checksum and the system installer are the quiet steps, and the
/// installer normally stops this app during its own.
#[cfg(all(not(feature = "hostsim"), not(any(test, feature = "test-support"))))]
const IDLE: std::time::Duration = std::time::Duration::from_secs(180);
/// The whole install's ceiling, a slow download included.
#[cfg(all(not(feature = "hostsim"), not(any(test, feature = "test-support"))))]
const TOTAL: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// Send `payload` to the service and hand every reply to `on_reply` until it has its answer.
#[cfg(all(not(feature = "hostsim"), not(any(test, feature = "test-support"))))]
pub fn install(payload: &str, on_reply: &mut dyn FnMut(&str) -> bool) -> Result<(), PortFail> {
    let registration = super::ls2::register().map_err(|_| PortFail::NoBus)?;
    registration.stream(INSTALL_URI, payload, IDLE, TOTAL, on_reply).map_err(|failure| match failure {
        super::ls2::Fail::Timeout => PortFail::Silent,
        super::ls2::Fail::Setup { stage, detail, .. } => {
            plx_base::eventlog::log(&format!("update: install call not sent ({stage}: {detail})"));
            PortFail::NoBus
        }
    })
}

#[cfg(any(feature = "hostsim", test, feature = "test-support"))]
pub fn install(_payload: &str, _on_reply: &mut dyn FnMut(&str) -> bool) -> Result<(), PortFail> {
    Err(PortFail::Unsupported)
}
