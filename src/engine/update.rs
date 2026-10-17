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

/// CHANGELOG.md's (version, lines) the first time this version runs after an update (once: it's marked seen here):
/// every version since the last one shown, newest first, so skipping releases loses nothing.
pub fn unseen_changelog() -> Option<Vec<(&'static str, Vec<&'static str>)>> {
    let since = super::mark_seen(VERSION)?;
    Some(changelog(include_str!("../../CHANGELOG.md"), since.as_deref(), VERSION)).filter(|v| !v.is_empty())
}

/// The `## {v}` sections after `since` up to `version` (just `version`'s when `since` is None), newest first: each
/// one's lines up to the next heading, without blanks and list markers. Empty sections are left out.
fn changelog<'a>(md: &'a str, since: Option<&str>, version: &str) -> Vec<(&'a str, Vec<&'a str>)> {
    let mut out: Vec<(&str, Vec<&str>)> = Vec::new();
    for l in md.lines() {
        if let Some(v) = l.strip_prefix("## ") {
            out.push((v.trim(), Vec::new()));
        } else if let Some((_, lines)) = out.last_mut() {
            let l = l.trim().trim_start_matches(['-', '*']).trim();
            if !l.is_empty() {
                lines.push(l);
            }
        }
    }
    out.retain(|(v, lines)| !lines.is_empty() && (*v == version || since.is_some_and(|s| is_newer(v, s) && is_newer(version, v))));
    out.sort_by(|a, b| if is_newer(a.0, b.0) { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater });
    out
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
    assert_eq!(changelog(md, None, "0.1.8"), [("0.1.8", vec!["one", "two"])]);
    assert!(changelog(md, None, "0.1.6").is_empty());
    // Skipped releases: everything after the last one shown, newest first.
    assert_eq!(changelog(md, Some("0.1.6"), "0.1.9"), [("0.1.9", vec!["later"]), ("0.1.8", vec!["one", "two"]), ("0.1.7", vec!["old"])]);
    assert_eq!(changelog(md, Some("0.1.7"), "0.1.8"), [("0.1.8", vec!["one", "two"])]);
}
