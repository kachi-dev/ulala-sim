//! What survives a restart, in `%LOCALAPPDATA%/ulala/`: the window bounds (window.json) and the race being worked on
//! (session.json): its save (None = custom), the setup, trainer names and the pinned baseline.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uma_race_sim::api::{HorseInput, RaceInput, SkillInput};

use super::{Race, app_dir, load_race, new_race};

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

/// The last session's race, setup and baseline.
pub struct Restored {
    pub race: Race,
    pub input: RaceInput,
    pub trainers: Vec<String>,
    pub baseline: Option<RaceInput>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Session {
    save: Option<PathBuf>,
    input: Input,
    trainers: Vec<String>,
    baseline: Option<Input>,
}

pub fn save_session(save: Option<&Path>, input: &RaceInput, trainers: &[String], baseline: Option<&RaceInput>) {
    let s = Session { save: save.map(Path::to_path_buf), input: input.into(), trainers: trainers.to_vec(), baseline: baseline.map(Input::from) };
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
    Some(Restored { race, input, trainers, baseline: s.baseline.map(RaceInput::from) })
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
fn input_round_trip() {
    let mut input = new_race().input;
    input.horses[0].skills.push(SkillInput { skill_id: 200331, level: 2 });
    input.horses[0].popularity_mark_rank = vec![0, 2, 1];
    let json = serde_json::to_string(&Input::from(&input)).unwrap();
    let back: RaceInput = serde_json::from_str::<Input>(&json).unwrap().into();
    assert_eq!(format!("{back:?}"), format!("{input:?}"));
}
