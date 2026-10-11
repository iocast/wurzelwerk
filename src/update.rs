//! Updates: checks for a new release and, in packaged builds, downloads and installs it.
//!
//! Release builds from CI carry the updater endpoint (`latest.json` of the latest GitHub
//! release) and the public key of the installer signatures (`WURZELWERK_UPDATE_ENDPOINT`,
//! `WURZELWERK_UPDATE_PUBLIC_KEY`). They download the signed installer in the background and
//! install it on restart. Other builds (`cargo run`, `.deb`) only ask the GitHub API and link to
//! the release page.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use cargo_packager_updater::{Config, Update, UpdaterBuilder};
use serde::Deserialize;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPO_URL: &str = "https://github.com/iocast/wurzelwerk";
const API_URL: &str = "https://api.github.com/repos/iocast/wurzelwerk/releases/latest";

#[derive(Debug, Clone)]
pub struct Release {
    pub version: String,
    pub url: String,
    /// Signed installer for this platform; `None` when this build cannot update itself.
    pub package: Option<Arc<Update>>,
}

#[derive(Debug, Clone)]
pub enum Check {
    UpToDate,
    Available(Release),
}

/// A verified installer, ready for [`install`].
#[derive(Debug, Clone)]
pub struct Downloaded {
    update: Arc<Update>,
    bytes: Arc<Vec<u8>>,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
}

/// Blocking - run it off the UI thread ([`check_async`]).
fn check() -> Result<Check, String> {
    match updater_config() {
        Some(config) => check_packaged(config),
        None => check_github(),
    }
}

/// The updater settings compiled into release builds, if this install can replace itself.
fn updater_config() -> Option<Config> {
    let endpoint = option_env!("WURZELWERK_UPDATE_ENDPOINT").filter(|s| !s.is_empty())?;
    let pubkey = option_env!("WURZELWERK_UPDATE_PUBLIC_KEY").filter(|s| !s.is_empty())?;
    // only the AppImage updates itself on Linux; a .deb belongs to the package manager
    if cfg!(target_os = "linux") && std::env::var_os("APPIMAGE").is_none() {
        return None;
    }
    Some(Config {
        endpoints: vec![endpoint.parse().ok()?],
        pubkey: pubkey.to_owned(),
        windows: None,
    })
}

fn check_packaged(config: Config) -> Result<Check, String> {
    let current = VERSION.parse().map_err(|e| format!("{e}"))?;
    let update = UpdaterBuilder::new(current, config)
        // also bounds the installer download
        .timeout(Duration::from_secs(300))
        .build()
        .and_then(|updater| updater.check())
        .map_err(|e| e.to_string())?;
    Ok(match update {
        Some(update) => Check::Available(Release {
            version: update.version.clone(),
            url: format!("{REPO_URL}/releases/tag/v{}", update.version),
            package: Some(Arc::new(update)),
        }),
        None => Check::UpToDate,
    })
}

fn check_github() -> Result<Check, String> {
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
            package: None,
        })
    } else {
        Check::UpToDate
    })
}

pub async fn check_async() -> Result<Check, String> {
    off_thread(check, "update check was interrupted").await
}

/// Downloads the installer and verifies its signature.
pub async fn download_async(update: Arc<Update>) -> Result<Downloaded, String> {
    off_thread(
        move || {
            let bytes = update.download().map_err(|e| e.to_string())?;
            Ok(Downloaded {
                update,
                bytes: Arc::new(bytes),
            })
        },
        "update download was interrupted",
    )
    .await
}

async fn off_thread<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, String> + Send + 'static,
    interrupted: &str,
) -> Result<T, String> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(job());
    });
    rx.await.map_err(|_| interrupted.to_owned())?
}

/// Installs the update and restarts Wurzelwerk; returns only if that fails.
/// Pending edits must be saved before.
pub fn install(downloaded: Downloaded) -> Result<Infallible, String> {
    let bytes = Arc::unwrap_or_clone(downloaded.bytes);
    // Windows: starts the installer (which relaunches the app) and exits
    downloaded.update.install(bytes).map_err(|e| e.to_string())?;
    // macOS / AppImage: the files were replaced in place
    let exe = std::env::var_os("APPIMAGE")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_exe().ok());
    if let Some(exe) = exe {
        let _ = std::process::Command::new(exe).spawn();
    }
    std::process::exit(0)
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
