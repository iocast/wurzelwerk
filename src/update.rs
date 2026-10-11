//! Update check against the latest GitHub release.

use std::time::Duration;

use serde::Deserialize;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPO_URL: &str = "https://github.com/iocast/wurzelwerk";
const API_URL: &str = "https://api.github.com/repos/iocast/wurzelwerk/releases/latest";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    UpToDate,
    Available(Release),
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
}

/// Blocking - run it off the UI thread ([`check_async`]).
fn check() -> Result<Check, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let release: ApiRelease = agent
        .get(API_URL)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", concat!("wurzelwerk/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .read_json()
        .map_err(|e| e.to_string())?;

    let version = release.tag_name.trim_start_matches(['v', 'V']).to_owned();
    Ok(if is_newer(&version, VERSION) {
        Check::Available(Release {
            version,
            url: release.html_url,
        })
    } else {
        Check::UpToDate
    })
}

pub async fn check_async() -> Result<Check, String> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(check());
    });
    rx.await.map_err(|_| "update check was interrupted".to_owned())?
}

/// Numeric `major.minor.patch` comparison; a pre-release suffix (`-rc1`) sorts below the release.
fn is_newer(candidate: &str, current: &str) -> bool {
    parse(candidate) > parse(current)
}

fn parse(v: &str) -> (Vec<u64>, bool) {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let (core, pre) = match v.split_once(['-', '+']) {
        Some((core, _)) => (core, true),
        None => (v, false),
    };
    let mut nums: Vec<u64> = core.split('.').map(|p| p.parse().unwrap_or(0)).collect();
    nums.resize(nums.len().max(3), 0);
    (nums, !pre)
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("0.10.0", "0.9.5"));
        assert!(is_newer("1.0.0", "1.0.0-rc1"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
    }
}
