//! What survives a restart, in `%LOCALAPPDATA%/ulala/`: the window bounds (window.json) and the race being worked on
//! (session.json): its save (None = custom), the setup, field size, trainer names and the pinned baseline; the NPC
//! setup (npcs.json); setups saved by name (setups/<name>.json).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uma_race_sim::api::{HorseInput, RaceInput, SkillInput};

use super::{NpcSetup, Race, app_dir, load_race, new_race};

/// Window bounds in logical px: the normal (not maximized or minimized) size and top-left, which it restores to.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct WindowSize {
    pub width: f64,
    pub height: f64,
    pub maximized: bool,
    /// None in files from before positions were kept: placed centered.
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
}

pub fn load_window() -> Option<WindowSize> {
    read("window.json")
}

pub fn save_window(w: &WindowSize) {
    write("window.json", w);
}

/// The NPC setup (npcs.json): kept across races, not part of one.
pub fn load_npcs() -> NpcSetup {
    read("npcs.json").unwrap_or_default()
}

pub fn save_npcs(n: &NpcSetup) {
    write("npcs.json", n);
}

/// The last session's race, setup and baseline.
pub struct Restored {
    pub race: Race,
    pub input: RaceInput,
    pub trainers: Vec<String>,
    pub baseline: Option<RaceInput>,
    /// 0 in files from before it was kept.
    pub field_size: usize,
    /// The saved race (`setups/`) it is, as last saved; None when it isn't one, or that file is gone.
    pub setup: Option<SavedSetup>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Session {
    save: Option<PathBuf>,
    input: Input,
    trainers: Vec<String>,
    baseline: Option<Input>,
    field_size: usize,
    /// The saved race being worked on (session.json only).
    #[serde(skip_serializing_if = "Option::is_none")]
    setup_file: Option<PathBuf>,
}

pub fn save_session(
    save: Option<&Path>, input: &RaceInput, trainers: &[String], baseline: Option<&RaceInput>, field_size: usize, setup_file: Option<&Path>,
) {
    let s = Session {
        save: save.map(Path::to_path_buf),
        input: input.into(),
        trainers: trainers.to_vec(),
        baseline: baseline.map(Input::from),
        field_size,
        setup_file: setup_file.map(Path::to_path_buf),
    };
    write("session.json", &s);
}

/// The last session: its save reloaded (or a new custom race) with the setup and baseline put back. None when there
/// is none, or its save can't be loaded any more (moved, deleted): start over with the default race.
pub fn restore_session() -> Option<Restored> {
    let s: Session = read("session.json")?;
    let race = match &s.save {
        Some(p) => load_race(p).ok()?,
        None => new_race(),
    };
    let input: RaceInput = s.input.into();
    if input.horses.is_empty() {
        return None;
    }
    let mut trainers = s.trainers;
    trainers.resize(input.horses.len(), String::new());
    let setup = s.setup_file.as_deref().and_then(read_setup);
    Some(Restored { race, input, trainers, baseline: s.baseline.map(RaceInput::from), field_size: s.field_size, setup })
}

/// A setup saved by name (`setups/<name>.json`): the race it is on, the setup, trainer names, baseline, field size
/// and NPCs.
#[derive(Clone, Debug)]
pub struct SavedSetup {
    pub path: PathBuf,
    pub name: String,
    /// The saved race it was made on (None = custom).
    pub source: Option<PathBuf>,
    pub input: RaceInput,
    pub trainers: Vec<String>,
    pub baseline: Option<RaceInput>,
    pub field_size: usize,
    pub npcs: NpcSetup,
    /// The Champions Meeting preset its course and conditions were (number, name), when saved on one.
    pub cm: Option<(i32, String)>,
    /// When the file was last written, "YYYY-MM-DD" (UTC).
    pub date: String,
    /// Seconds since 1970 of that, for sorting.
    pub mtime: u64,
}

/// A setup file: `session.json`'s fields plus a marker, the name and the NPCs.
#[derive(Serialize, Deserialize)]
struct SetupFile {
    /// Format version; tells a setup file from a saved race when opening any file.
    ulala_setup: u32,
    name: String,
    #[serde(flatten)]
    session: Session,
    #[serde(default)]
    npcs: NpcSetup,
    #[serde(default)]
    cm: Option<(i32, String)>,
}

/// Where setups saved by name live.
pub fn setups_dir() -> Option<PathBuf> {
    Some(app_dir()?.join("setups"))
}

/// The file a setup named `name` saves to (characters Windows rejects in file names become '_').
pub fn setup_path(name: &str) -> Option<PathBuf> {
    let stem: String = name.trim().chars().map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c }).take(80).collect();
    let stem = stem.trim_end_matches(['.', ' ']);
    (!stem.is_empty()).then(|| setups_dir()).flatten().map(|d| d.join(format!("{stem}.json")))
}

/// Writes setup `name` to `path`, replacing any file there.
#[allow(clippy::too_many_arguments)]
pub fn write_setup(
    path: &Path, name: &str, save: Option<&Path>, input: &RaceInput, trainers: &[String], baseline: Option<&RaceInput>, field_size: usize,
    npcs: &NpcSetup, cm: Option<(i32, String)>,
) -> Result<(), String> {
    let f = SetupFile {
        ulala_setup: 1,
        name: name.trim().to_string(),
        session: Session {
            save: save.map(Path::to_path_buf),
            input: input.into(),
            trainers: trainers.to_vec(),
            baseline: baseline.map(Input::from),
            field_size,
            setup_file: None,
        },
        npcs: npcs.clone(),
        cm,
    };
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("Couldn't create {}: {e}", d.display()))?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(&f).unwrap()).map_err(|e| format!("Couldn't save {}: {e}", path.display()))
}

/// The setup in `path`, None when it isn't a setup file (a saved race, say).
pub fn read_setup(path: &Path) -> Option<SavedSetup> {
    let f: SetupFile = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let input: RaceInput = f.session.input.into();
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    let mut trainers = f.session.trainers;
    trainers.resize(input.horses.len(), String::new());
    Some(SavedSetup {
        path: path.to_path_buf(),
        name: f.name,
        source: f.session.save,
        input,
        trainers,
        baseline: f.session.baseline.map(RaceInput::from),
        field_size: f.session.field_size,
        npcs: f.npcs,
        cm: f.cm,
        date: ymd(mtime),
        mtime,
    })
}

/// Every setup saved by name, newest first.
pub fn list_setups() -> Vec<SavedSetup> {
    let Some(d) = setups_dir() else { return vec![] };
    let mut v: Vec<SavedSetup> = std::fs::read_dir(d).into_iter().flatten().flatten().filter_map(|e| read_setup(&e.path())).collect();
    v.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    v
}

/// The race a setup is on: its saved race when that still loads, else a custom race.
pub fn setup_race(s: &SavedSetup) -> Race {
    s.source.as_deref().and_then(|p| load_race(p).ok()).unwrap_or_else(new_race)
}

/// "YYYY-MM-DD" of `secs` since 1970 (UTC).
fn ymd(secs: u64) -> String {
    // Days to civil date (Howard Hinnant's algorithm).
    let z = (secs / 86400) as i64 + 719468;
    let (era, doe) = (z.div_euclid(146097), z.rem_euclid(146097));
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    format!("{:04}-{m:02}-{d:02}", yoe + era * 400 + (m <= 2) as i64)
}

fn read<T: for<'a> Deserialize<'a>>(name: &str) -> Option<T> {
    serde_json::from_slice(&std::fs::read(app_dir()?.join(name)).ok()?).ok()
}

fn write(name: &str, v: &impl Serialize) {
    if let Some(d) = app_dir() {
        let _ = std::fs::create_dir_all(&d);
        let _ = std::fs::write(d.join(name), serde_json::to_vec(v).unwrap());
    }
}

/// Serde mirror of a sim input type (which has no serde): its plain fields by name, plus one converted field. Fields
/// the sim adds later stay at their default until listed here.
macro_rules! mirror {
    ($m:ident <-> $t:ident { $($f:ident: $ty:ty),* $(,)? } $s:ident: $sty:ty, |$a:ident| $to:expr, |$b:ident| $back:expr) => {
        #[derive(Serialize, Deserialize, Default)]
        #[serde(default)]
        struct $m {
            $($f: $ty,)*
            $s: $sty,
        }
        impl From<&$t> for $m {
            fn from($a: &$t) -> Self {
                Self { $($f: $a.$f.clone(),)* $s: $to }
            }
        }
        impl From<$m> for $t {
            #[allow(clippy::needless_update)]
            fn from($b: $m) -> Self {
                Self { $($f: $b.$f,)* $s: $back, ..Default::default() }
            }
        }
    };
}

mirror!(Horse <-> HorseInput {
    chara_id: i32, card_id: i32, speed: i32, stamina: i32, pow: i32, guts: i32, wiz: i32, running_style: i32,
    motivation: i32, proper_distance: [i32; 4], proper_running_style: [i32; 4], proper_ground: [i32; 2],
    item_ids: Vec<i32>, team_id: i32, popularity: i32, npc_type: i32, final_grade: i32, viewer_id: i64, rarity: i32,
    talent_level: i32, frame_order: i32, mob_id: i32, popularity_mark_rank: Vec<i32>, team_member_id: i32,
    team_rank: i32, single_mode_win_count: i32, fan_count: i32, muted_skills: Vec<i32>,
} skills: Vec<(i32, i32)>,
    |h| h.skills.iter().map(|k| (k.skill_id, k.level)).collect(),
    |h| h.skills.into_iter().map(|(skill_id, level)| SkillInput { skill_id, level }).collect());

mirror!(Input <-> RaceInput {
    random_seed: i32, course_set_id: i32, ground_condition: i32, weather: i32, season: i32, time: i32, race_type: i32,
    race_instance_id: i32,
} horses: Vec<Horse>,
    |r| r.horses.iter().map(Horse::from).collect(),
    |r| r.horses.into_iter().map(HorseInput::from).collect());

#[cfg(test)]
#[test]
fn setup_round_trip() {
    assert_eq!((ymd(0), ymd(1_792_195_200), ymd(951_782_400)), ("1970-01-01".into(), "2026-10-17".into(), "2000-02-29".into()));
    let mut input = new_race().input;
    input.random_seed = 1234;
    let npcs = NpcSetup { stats: [1, 2, 3, 4, 5], ..Default::default() };
    let path = std::env::temp_dir().join(format!("ulala-setup-test-{}.json", std::process::id()));
    write_setup(&path, " Mile test ", None, &input, &["Kachi".into()], Some(&input), 12, &npcs, Some((21, "Capricorn Cup".into()))).unwrap();
    let s = read_setup(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!((s.name.as_str(), s.input.random_seed, s.field_size, s.npcs, s.trainers[0].as_str()), ("Mile test", 1234, 12, npcs, "Kachi"));
    assert_eq!(format!("{:?}", s.baseline), format!("{:?}", Some(&s.input)));
    assert_eq!(s.cm, Some((21, "Capricorn Cup".into())));
    assert!(setup_path("a/b:c?").unwrap().ends_with("a_b_c_.json") && setup_path("  ").is_none());
}

#[cfg(test)]
#[test]
fn input_round_trip() {
    let mut input = new_race().input;
    input.horses[0].skills.push(SkillInput { skill_id: 200331, level: 2 });
    input.horses[0].popularity_mark_rank = vec![0, 2, 1];
    let json = serde_json::to_string(&Input::from(&input)).unwrap();
    let back: RaceInput = serde_json::from_str::<Input>(&json).unwrap().into();
    assert_eq!(format!("{back:?}"), format!("{input:?}"));
}
