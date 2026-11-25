//! The bus half of `plx_os::tv::update`: Homebrew Channel's `install`, streamed. The service's
//! replies and what they mean are documented there; this module only carries them.

use plx_os::tv::update::PortFail;

#[cfg(not(test))]
const INSTALL_URI: &str = "luna://org.webosbrew.hbchannel.service/install";

/// How long the service may say nothing before the install is given up as silent. It reports a
/// download every 300 ms; the checksum and the system installer are the quiet steps, and the
/// installer normally stops this app during its own.
#[cfg(not(test))]
const IDLE: std::time::Duration = std::time::Duration::from_secs(180);
/// The whole install's ceiling, a slow download included.
#[cfg(not(test))]
const TOTAL: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// Send `payload` to the service and hand every reply to `on_reply` until it has its answer.
#[cfg(not(test))]
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

#[cfg(test)]
pub fn install(_payload: &str, _on_reply: &mut dyn FnMut(&str) -> bool) -> Result<(), PortFail> {
    Err(PortFail::Unsupported)
}
