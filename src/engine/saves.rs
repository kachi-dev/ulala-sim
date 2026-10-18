use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use uma_race_sim::data::DataEra;

use super::{Work, app_dir, course_info, game_data, par_map};

/// One entry of the saved-races browser (cheap: from the filename only, no parsing).
#[derive(Clone, Debug, PartialEq)]
pub struct SaveEntry {
    pub path: PathBuf,
    /// Subfolder, e.g. "Champions meeting".
    pub category: String,
    /// Filename stem part before the time, e.g. "Oguri Cap".
    pub winner: String,
    /// e.g. "94.8875s"
    pub time: String,
    /// "YYYY-MM-DD"
    pub date: String,
}

/// A save's entry from its path: category = its folder; winner, time and date from horseACT's file name
/// ("<Winner>-<time>s-<YYYYMMDD>"; a headless room match's "<id>-<YYYYMMDD>" has no winner; else the whole name is the
/// winner).
pub fn save_entry(path: &Path) -> SaveEntry {
    let category = path.parent().and_then(|p| p.file_name()).map_or(String::new(), |s| s.to_string_lossy().into());
    let stem = path.file_stem().map_or(String::new(), |s| s.to_string_lossy().into());
    // The winner may contain '-'.
    let parts: Vec<&str> = stem.rsplitn(3, '-').collect();
    let ymd = |d: &str| (d.len() == 8 && d.bytes().all(|b| b.is_ascii_digit())).then(|| format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..]));
    let (winner, time, date) = match parts[..] {
        [d, t, w] if ymd(d).is_some() && t.ends_with('s') => (w.to_string(), t.to_string(), ymd(d).unwrap_or_default()),
        [d, _] if ymd(d).is_some() => (String::new(), String::new(), ymd(d).unwrap_or_default()),
        _ => (stem.clone(), String::new(), String::new()),
    };
    SaveEntry { path: path.to_path_buf(), category, winner, time, date }
}

/// All `*.json` saves under `dir/*/`, newest date first.
pub fn list_saves(dir: &Path) -> Vec<SaveEntry> {
    let ls = |d: &Path| std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path());
    let mut v: Vec<SaveEntry> = ls(dir)
        .filter(|d| d.is_dir())
        .flat_map(|d| ls(&d).collect::<Vec<_>>())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")))
        .map(|p| save_entry(&p))
        .collect();
    v.sort_by(|a, b| b.date.cmp(&a.date));
    v
}

/// What the saved-races list shows beyond the file name: from a light parse of the save, cached on disk.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, serde::Serialize)]
pub struct SaveMeta {
    /// e.g. "Tokyo Dirt 1600m", "" when unknown.
    pub course: String,
    /// raceCourseSet id.
    pub course_id: i32,
    /// The winner's (card, chara); (0, 0) = unknown.
    pub winner: (i32, i32),
    /// The save owner's umas: the team (3 in Champions Meeting), else just playerHorseIndex.
    pub team: Vec<TeamMember>,
    /// Everyone else in the field as (card, chara), gate order.
    pub rivals: Vec<(i32, i32)>,
    pub player_trainer: String,
    /// One of `team` won.
    pub won: bool,
    /// Every uma, gate order (the replay rivals' pool, `rival_pool`).
    pub field: Vec<FieldUma>,
    /// Ground, weather, season, time of day (`RaceInput`'s values); 0 = unknown.
    pub conds: [i32; 4],
}

/// One uma of a saved race's field.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, serde::Serialize)]
pub struct FieldUma {
    pub card: i32,
    pub chara: i32,
    /// `trained_chara_id`, numbered per player account; 0 = unknown.
    pub trained: i32,
    /// Running style, 1 ..= 4.
    pub style: i32,
    /// `team_id`, 0 = no team.
    pub team: i32,
    pub trainer: String,
    /// Finishing place, 0 = unknown.
    pub place: usize,
}

/// One of the save owner's umas in a saved race.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, serde::Serialize)]
pub struct TeamMember {
    pub card: i32,
    pub chara: i32,
    /// `trained_chara_id`: the trained uma, which tells a retrained copy of the same card apart. 0 = unknown.
    pub trained: i32,
    /// Running style, 1 ..= 4.
    pub style: i32,
    /// Field index (gate order).
    pub horse: usize,
    /// Finishing place, 0 = unknown.
    pub place: usize,
}

/// Just the fields `SaveMeta` and `load_race` need; serde skips the rest (incl. the big replay string) without
/// allocating it.
#[derive(Deserialize)]
pub(super) struct SaveHead {
    #[serde(rename = "raceCourseSet", alias = "<RaceCourseSet>k__BackingField")]
    course: HeadId,
    #[serde(rename = "raceHorse", alias = "<RaceHorse>k__BackingField")]
    pub(super) horses: Vec<HeadHorse>,
    #[serde(default, rename = "playerHorseIndex", alias = "<PlayerHorseIndex>k__BackingField")]
    pub(super) player: Option<i32>,
    #[serde(default, rename = "playerTeamMemberArray", alias = "<PlayerTeamMemberArray>k__BackingField")]
    team: Vec<HeadMember>,
    /// Horse indices, winner first.
    #[serde(default, rename = "horseIndexByFinishOrder", alias = "<HorseIndexByFinishOrder>k__BackingField")]
    finish: Vec<i32>,
    #[serde(default, rename = "groundCondition", alias = "<GroundCondition>k__BackingField")]
    ground: Option<HeadEnum>,
    #[serde(default, rename = "weather", alias = "<Weather>k__BackingField")]
    weather: Option<HeadEnum>,
    #[serde(default, rename = "season", alias = "<Season>k__BackingField")]
    season: Option<HeadEnum>,
    #[serde(default, rename = "time", alias = "<Time>k__BackingField")]
    time: Option<HeadEnum>,
    #[serde(default, rename = "raceInstanceMaster", alias = "_raceInstanceMaster")]
    instance: Option<HeadInstance>,
}

/// A condition saved by number or by name (the sim's corpus names).
#[derive(Deserialize)]
#[serde(untagged)]
enum HeadEnum {
    Int(i32),
    Name(String),
}

impl HeadEnum {
    fn val(&self, names: &[&str]) -> i32 {
        match self {
            HeadEnum::Int(n) => *n,
            HeadEnum::Name(s) => names.iter().position(|n| n == s).map_or(0, |i| i as i32 + 1),
        }
    }
}

#[derive(Deserialize)]
struct HeadInstance {
    #[serde(default)]
    time: Option<i32>,
}

#[derive(Deserialize)]
struct HeadMember {
    #[serde(rename = "horseIndex")]
    horse: i32,
}

#[derive(Deserialize)]
struct HeadId {
    #[serde(alias = "Id")]
    id: i32,
}

#[derive(Deserialize)]
pub(super) struct HeadHorse {
    #[serde(rename = "responseHorseData", alias = "_responseHorseData")]
    pub(super) r: HeadHorseData,
}

#[derive(Deserialize)]
pub(super) struct HeadHorseData {
    chara_id: i32,
    card_id: i32,
    #[serde(default)]
    pub(super) trainer_name: Option<String>,
    #[serde(default)]
    trained_chara_id: i32,
    #[serde(default)]
    running_style: i32,
    #[serde(default)]
    team_id: i32,
}

/// "Tokyo Dirt 1600m" for raceCourseSet `id` with the current data ("" when unknown).
pub fn course_label(id: i32) -> String {
    let data = game_data(DataEra::current()).ok();
    data.filter(|d| d.master.race_course_set.contains_key(&id)).map_or(String::new(), |d| course_info(&d, id).label())
}

/// `SaveMeta` for every save, reported through `each(index, meta)` as they come (cached ones first). The cache is
/// `%LOCALAPPDATA%/ulala/saves_index5.json`, keyed by path + modification time (bump the name when `SaveMeta` changes).
pub fn index_saves(saves: &[SaveEntry], each: impl Fn(usize, SaveMeta) + Sync) {
    let cache_path = app_dir().map(|d| d.join("saves_index5.json"));
    let mut cache: HashMap<String, (u64, SaveMeta)> =
        cache_path.as_ref().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let mtime = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    let keys: Vec<(String, u64)> = saves.iter().map(|e| (e.path.display().to_string(), mtime(&e.path))).collect();
    let mut todo = vec![];
    for (i, k) in keys.iter().enumerate() {
        match cache.get(&k.0) {
            Some((t, m)) if *t == k.1 => each(i, m.clone()),
            _ => todo.push(i),
        }
    }
    if todo.is_empty() {
        return;
    }
    let data = game_data(DataEra::current()).ok();
    let course = |id: i32| data.as_ref().filter(|d| d.master.race_course_set.contains_key(&id)).map_or(String::new(), |d| course_info(d, id).label());
    let parsed = par_map(todo.len(), &Work::default(), |j| {
        let i = todo[j];
        let bytes = std::fs::read(&saves[i].path).ok();
        let head: Option<SaveHead> = bytes.as_ref().and_then(|b| serde_json::from_slice(&b).ok());
        let meta = head.map(|mut h| {
            // Saves without the finish order (horseACT's headless room matches): the replay's results have it.
            if h.finish.is_empty() {
                let replay = bytes.as_deref().and_then(|b| std::str::from_utf8(b).ok()).and_then(|t| uma_race_sim::data::corpus::parse_race(t).ok());
                if let Some((_, r)) = replay {
                    let mut by: Vec<(i32, i32)> = r.results.iter().enumerate().map(|(i, x)| (x.finish_order, i as i32)).collect();
                    by.sort();
                    h.finish = by.into_iter().map(|x| x.1).collect();
                }
            }
            let horse = |i: i32| usize::try_from(i).ok().and_then(|i| h.horses.get(i)).map(|h| &h.r);
            let ids: Vec<i32> = if h.team.is_empty() { h.player.into_iter().collect() } else { h.team.iter().map(|m| m.horse).collect() };
            let winner = h.finish.first().copied();
            let place = |i: i32| h.finish.iter().position(|&f| f == i).map_or(0, |p| p + 1);
            SaveMeta {
                course: course(h.course.id),
                course_id: h.course.id,
                winner: winner.and_then(horse).map_or((0, 0), |r| (r.card_id, r.chara_id)),
                team: ids
                    .iter()
                    .filter_map(|&i| {
                        let r = horse(i)?;
                        let (trained, style, place) = (r.trained_chara_id, r.running_style, place(i));
                        Some(TeamMember { card: r.card_id, chara: r.chara_id, trained, style, horse: i as usize, place })
                    })
                    .collect(),
                rivals: (0..h.horses.len() as i32).filter(|i| !ids.contains(i)).filter_map(horse).map(|r| (r.card_id, r.chara_id)).collect(),
                player_trainer: h.player.and_then(horse).and_then(|r| r.trainer_name.clone()).unwrap_or_default(),
                won: winner.is_some_and(|w| ids.contains(&w)),
                conds: [
                    h.ground.as_ref().map_or(0, |x| x.val(&["Good", "Soft", "Hard", "Bad"])),
                    h.weather.as_ref().map_or(0, |x| x.val(&["Sunny", "Cloudy", "Rainy", "Snow"])),
                    h.season.as_ref().map_or(0, |x| x.val(&["Spring", "Summer", "Fall", "Winter", "CherryBlossom"])),
                    h.time.as_ref().map(|x| x.val(&["Morning", "Daytime", "Evening", "Night"])).or(h.instance.as_ref().and_then(|i| i.time)).unwrap_or(0),
                ],
                field: h
                    .horses
                    .iter()
                    .enumerate()
                    .map(|(i, x)| {
                        let r = &x.r;
                        let trainer = r.trainer_name.clone().unwrap_or_default();
                        let (trained, style, team) = (r.trained_chara_id, r.running_style, r.team_id);
                        FieldUma { card: r.card_id, chara: r.chara_id, trained, style, team, trainer, place: place(i as i32) }
                    })
                    .collect(),
            }
        });
        if let Some(m) = &meta {
            each(i, m.clone());
        }
        meta
    });
    for (j, m) in parsed {
        if let Some(m) = m {
            let (k, t) = keys[todo[j]].clone();
            cache.insert(k, (t, m));
        }
    }
    if let Some(p) = cache_path {
        let _ = std::fs::create_dir_all(p.parent().unwrap_or(Path::new(".")));
        let _ = serde_json::to_vec(&cache).map(|b| std::fs::write(p, b));
    }
}
