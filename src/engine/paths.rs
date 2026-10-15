use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use serde::Deserialize;

/// horseACT's saved-races folder: `<outputPath>/Saved Races`, falling back to horseACT's default output
/// (`%USERPROFILE%/Documents`) when its config wasn't found.
pub fn saved_races_dir() -> PathBuf {
    paths().output.clone().unwrap_or_else(|| PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default()).join("Documents")).join("Saved Races")
}

/// What stops Ulala finding horseACT's saves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Setup {
    Ok,
    /// The game isn't in a Steam library (or the folder set).
    NoGame,
    /// The game is there but has no `hachimi/horseACTConfig.json` (horseACT not installed).
    NoHorseAct,
}

pub fn setup() -> Setup {
    let p = paths();
    match (&p.output, &p.game) {
        (Some(_), _) => Setup::Ok,
        (None, None) => Setup::NoGame,
        (None, Some(_)) => Setup::NoHorseAct,
    }
}

/// The game folder found in the Steam libraries or set by the user.
pub fn game_dir() -> Option<PathBuf> {
    paths().game.clone()
}

/// No settings file yet: the first launch (the UI explains a failed lookup once).
pub fn first_run() -> bool {
    paths().first
}

/// The game folder (holding `UmamusumePrettyDerby.exe`) as chosen by the user: remembered, and horseACT looked up there.
pub fn set_game_dir(dir: &Path) -> Result<Setup, String> {
    if !is_game(dir) {
        return Err(format!("{} has no {GAME_EXE}", dir.display()));
    }
    let mut p = paths();
    p.game = Some(dir.to_path_buf());
    p.output = horseact_output(dir);
    save(&p);
    drop(p);
    Ok(setup())
}

/// The field shown as compact rows (else cards); compact until the user picks.
pub fn compact_pref() -> bool {
    paths().compact.unwrap_or(true)
}

pub fn set_compact_pref(on: bool) {
    let mut p = paths();
    p.compact = Some(on);
    save(&p);
}

/// Share of logical processors long jobs run on, in percent; half until the user picks (the other half stays free for
/// the game, the browser, ...).
pub fn cpu_pref() -> u8 {
    paths().cpu.unwrap_or(50)
}

pub fn set_cpu_pref(pct: u8) {
    let mut p = paths();
    p.cpu = Some(pct);
    save(&p);
}

/// Records `version` as the last one whose changelog was shown, returning whether it's new to this settings file
/// (never on the first launch: a fresh install has nothing to compare against).
pub(super) fn mark_seen(version: &str) -> bool {
    let mut p = paths();
    if p.seen.as_deref() == Some(version) {
        return false;
    }
    p.seen = Some(version.to_string());
    save(&p);
    !p.first
}

/// Worker threads for long jobs.
pub fn worker_threads() -> usize {
    threads_at(cpu_pref())
}

/// `pct` % of the logical processors (rounded up), at least 1.
pub fn threads_at(pct: u8) -> usize {
    let all = std::thread::available_parallelism().map_or(1, |t| t.get());
    (all * pct as usize).div_ceil(100).max(1)
}

/// `%LOCALAPPDATA%/ulala`
pub(super) fn app_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("ulala"))
}

#[derive(serde::Serialize, Deserialize, Default)]
struct Settings {
    /// Set by the user when the game isn't in a Steam library.
    #[serde(default)]
    game_dir: Option<PathBuf>,
    horseact_output: Option<PathBuf>,
    #[serde(default)]
    compact: Option<bool>,
    #[serde(default)]
    cpu: Option<u8>,
    /// The last version whose changelog was shown.
    #[serde(default)]
    seen: Option<String>,
}

fn settings_file() -> Option<PathBuf> {
    app_dir().map(|d| d.join("settings.json"))
}

fn save(p: &Paths) {
    save_settings(&Settings { game_dir: p.game.clone(), horseact_output: p.output.clone(), compact: p.compact, cpu: p.cpu, seen: p.seen.clone() });
}

fn save_settings(s: &Settings) {
    if let Some(f) = settings_file() {
        let _ = std::fs::create_dir_all(f.parent().unwrap());
        let _ = std::fs::write(f, serde_json::to_vec_pretty(s).unwrap());
    }
}

struct Paths {
    game: Option<PathBuf>,
    /// horseACT's output folder.
    output: Option<PathBuf>,
    first: bool,
    compact: Option<bool>,
    cpu: Option<u8>,
    seen: Option<String>,
}

/// Stored in `%LOCALAPPDATA%/ulala/settings.json`; looked up again while missing or gone (e.g. horseACT installed
/// later).
fn paths() -> MutexGuard<'static, Paths> {
    static P: OnceLock<Mutex<Paths>> = OnceLock::new();
    P.get_or_init(|| {
        let saved: Option<Settings> = settings_file().and_then(|f| std::fs::read(f).ok()).and_then(|b| serde_json::from_slice(&b).ok());
        let first = saved.is_none();
        let saved = saved.unwrap_or_default();
        let game = saved.game_dir.clone().filter(|d| is_game(d)).or_else(find_game);
        let (compact, cpu, seen) = (saved.compact, saved.cpu, saved.seen);
        if let Some(out) = saved.horseact_output.filter(|p| p.is_dir()) {
            return Mutex::new(Paths { game, output: Some(out), first, compact, cpu, seen });
        }
        let output = game.as_deref().and_then(horseact_output);
        save_settings(&Settings { game_dir: saved.game_dir, horseact_output: output.clone(), compact, cpu, seen: seen.clone() });
        Mutex::new(Paths { game, output, first, compact, cpu, seen })
    })
    .lock()
    .unwrap_or_else(|e| e.into_inner())
}

pub const GAME_EXE: &str = "UmamusumePrettyDerby.exe";

fn is_game(dir: &Path) -> bool {
    dir.join(GAME_EXE).is_file()
}

/// `steamapps/common/UmamusumePrettyDerby` in any Steam library.
pub(super) fn find_game() -> Option<PathBuf> {
    steam_libraries().into_iter().map(|lib| lib.join("steamapps").join("common").join("UmamusumePrettyDerby")).find(|d| is_game(d))
}

/// `outputPath` from `<game>/hachimi/horseACTConfig.json`.
pub(super) fn horseact_output(game: &Path) -> Option<PathBuf> {
    let cfg: serde_json::Value = serde_json::from_slice(&std::fs::read(game.join("hachimi").join("horseACTConfig.json")).ok()?).ok()?;
    // Relative paths are taken from the game folder (the game's working directory).
    Some(game.join(expand_env(cfg["outputPath"].as_str()?))).filter(|p| p.is_dir())
}

/// `%NAME%` → the environment variable (unknown names kept as is).
pub(super) fn expand_env(s: &str) -> String {
    s.split('%').enumerate().map(|(i, p)| if i % 2 == 1 { std::env::var(p).unwrap_or_else(|_| format!("%{p}%")) } else { p.to_string() }).collect()
}

/// Steam's own folder plus every library in `steamapps/libraryfolders.vdf`.
fn steam_libraries() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = steam_path().into_iter().collect();
    roots.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
    let mut libs = roots.clone();
    for r in &roots {
        let vdf = std::fs::read_to_string(r.join("steamapps").join("libraryfolders.vdf")).unwrap_or_default();
        // Lines like `"path"		"F:\\SteamLibrary"`.
        libs.extend(vdf.lines().filter_map(|l| l.trim().strip_prefix("\"path\"")).map(|v| PathBuf::from(v.trim().trim_matches('"').replace(r"\\", r"\"))));
    }
    libs
}

#[cfg(windows)]
fn steam_path() -> Option<PathBuf> {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};
    use windows::core::w;
    let mut buf = [0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    let ok = unsafe { RegGetValueW(HKEY_CURRENT_USER, w!(r"Software\Valve\Steam"), w!("SteamPath"), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut len)) }.is_ok();
    ok.then(|| PathBuf::from(String::from_utf16_lossy(&buf[..(len as usize / 2).saturating_sub(1)])))
}

#[cfg(not(windows))]
fn steam_path() -> Option<PathBuf> {
    None
}
