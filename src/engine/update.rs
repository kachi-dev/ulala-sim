//! App updates from the GitHub releases (.github/workflows/release.yml publishes ulala.exe with every tag).

use serde::Deserialize;
use std::time::Duration;

const REPO: &str = "kachi-dev/ulala-sim";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const RELEASES_URL: &str = "https://github.com/kachi-dev/ulala-sim/releases";

fn agent(secs: u64) -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(secs))).build().into()
}

/// The latest release's version when it's newer than this build; None when up to date, offline, or a debug build
/// (an update would swap the dev exe for a release one).
pub fn newer_release() -> Option<String> {
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
    }
    if cfg!(debug_assertions) {
        return None;
    }
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let json = agent(15).get(&url).header("User-Agent", "ulala").call().ok()?.body_mut().read_to_string().ok()?;
    let v = serde_json::from_str::<Release>(&json).ok()?.tag_name.trim_start_matches('v').to_string();
    is_newer(&v, VERSION).then_some(v)
}

fn is_newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| v.split('.').map(|p| p.parse::<u32>().ok()).collect::<Option<Vec<_>>>();
    matches!((parse(a), parse(b)), (Some(a), Some(b)) if a > b)
}

/// Installs release `v` and starts it; the caller then exits. An installed copy (Inno's uninstaller sits next to the
/// exe) runs the release's installer silently: it reuses the install's folder and per-user / all-users mode (asking
/// for admin when that's all-users), updates the version in Windows' app list, and relaunches Ulala (ulala.iss).
/// The plain exe swaps itself: Windows lets a running exe be renamed but not overwritten, so the old one moves to
/// `ulala.exe.old` and `clean_old_exe` deletes it on the next launch.
pub fn install_release(v: &str) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let installed = exe.with_file_name("unins000.exe").exists();
    let asset = if installed { "ulala-setup.exe" } else { "ulala.exe" };
    let err = |e: ureq::Error| e.to_string();
    let url = format!("https://github.com/{REPO}/releases/download/v{v}/{asset}");
    let body = agent(600).get(&url).call().map_err(err)?.body_mut().with_config().limit(u64::MAX).read_to_vec().map_err(err)?;
    if !body.starts_with(b"MZ") {
        return Err("the download isn't an exe".into());
    }
    if installed {
        let setup = std::env::temp_dir().join(asset);
        let io = |e: std::io::Error| format!("{}: {e}", setup.display());
        std::fs::write(&setup, body).map_err(io)?;
        std::process::Command::new(&setup).args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART"]).spawn().map_err(io)?;
        return Ok(());
    }
    let (new, old) = (exe.with_extension("exe.new"), exe.with_extension("exe.old"));
    let _ = std::fs::remove_file(&old);
    let io = |e: std::io::Error| format!("{}: {e}", exe.display());
    std::fs::write(&new, body).map_err(io)?;
    std::fs::rename(&exe, &old).map_err(io)?;
    if let Err(e) = std::fs::rename(&new, &exe) {
        let _ = std::fs::rename(&old, &exe);
        return Err(io(e));
    }
    std::process::Command::new(&exe).spawn().map_err(io)?;
    Ok(())
}

/// This version's CHANGELOG.md lines, the first time it runs after an update (once: it's marked seen here).
pub fn unseen_changelog() -> Option<Vec<&'static str>> {
    super::mark_seen(VERSION).then(|| changelog(include_str!("../../CHANGELOG.md"), VERSION)).filter(|l| !l.is_empty())
}

/// The lines under `## {version}` up to the next heading, without blanks and list markers.
fn changelog<'a>(md: &'a str, version: &str) -> Vec<&'a str> {
    let head = format!("## {version}");
    let lines = md.lines().skip_while(|l| l.trim() != head).skip(1).take_while(|l| !l.starts_with("## "));
    lines.map(|l| l.trim().trim_start_matches(['-', '*']).trim()).filter(|l| !l.is_empty()).collect()
}

/// Deletes the exe an update replaced. Retries for a while: right after an update the old process is still exiting.
pub fn clean_old_exe() {
    let Ok(exe) = std::env::current_exe() else { return };
    let old = exe.with_extension("exe.old");
    for _ in 0..20 {
        if !old.exists() || std::fs::remove_file(&old).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[test]
fn newer() {
    assert!(is_newer("0.1.10", "0.1.9"));
    assert!(is_newer("0.2.0", "0.1.7"));
    assert!(!is_newer("0.1.7", "0.1.7"));
    assert!(!is_newer("0.1.6", "0.1.7"));
    assert!(!is_newer("0.1.8-rc1", "0.1.7"));
}

#[test]
fn changelog_section() {
    let md = "# Changelog

## 0.1.9
- later

## 0.1.8
- one
* two

## 0.1.7
- old
";
    assert_eq!(changelog(md, "0.1.8"), ["one", "two"]);
    assert!(changelog(md, "0.1.6").is_empty());
}
