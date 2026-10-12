use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Deserialize;
use uma_race_sim::api::RaceInput;
use uma_race_sim::data::{DATA_DIR, DataEra, GameData};

use super::app_dir;

pub(super) const STYLES: [&str; 4] = ["Front", "Pace", "Late", "End"];
pub(super) const DISTANCES: [&str; 4] = ["Sprint", "Mile", "Medium", "Long"];
pub(super) const PHASES: [&str; 4] = ["Early race", "Mid race", "Late race", "Last spurt"];
pub(super) const SURFACES: [&str; 2] = ["Turf", "Dirt"];
pub(super) const TURNS: [&str; 4] = ["Right", "Left", "Straight", "Straight"];
/// Global names for RaceDefine.GroundCondition Good/Soft/Hard/Bad.
pub(super) const GROUNDS: [&str; 4] = ["Firm", "Good", "Soft", "Heavy"];
pub(super) const WEATHERS: [&str; 4] = ["Sunny", "Cloudy", "Rainy", "Snowy"];
pub(super) const SEASONS: [&str; 5] = ["Spring", "Summer", "Fall", "Winter", "Spring"];

/// `v[i - 1]` (game enums are 1-based).
pub(super) fn pick(v: &[&'static str], i: i32) -> Option<&'static str> {
    v.get(usize::try_from(i - 1).ok()?).copied()
}

#[derive(Clone, Debug, PartialEq)]
pub struct CardInfo {
    pub card_id: i32,
    pub chara_id: i32,
    /// "Haru Urara"
    pub name: String,
    /// "[New Year ♪ New Urara!]"
    pub title: String,
    pub unique_skill: i32,
}

/// Every playable card, sorted by name, then card id (built once).
pub fn cards() -> &'static [CardInfo] {
    static CARDS: OnceLock<Vec<CardInfo>> = OnceLock::new();
    CARDS.get_or_init(|| {
        let mut v: Vec<CardInfo> = names().card.keys().filter_map(|&id| card_info(id)).collect();
        v.sort_by(|a, b| a.name.cmp(&b.name).then(a.card_id.cmp(&b.card_id)));
        v
    })
}

pub fn card_info(card_id: i32) -> Option<CardInfo> {
    let c = names().card.get(&card_id)?;
    Some(CardInfo { card_id, chara_id: c.1, name: chara_name(c.1), title: c.0.clone(), unique_skill: c.2 })
}

/// Display name ("Haru Urara"), "#<id>" when unknown.
pub fn chara_name(id: i32) -> String {
    names().chara.get(&id).cloned().unwrap_or_else(|| format!("#{id}"))
}

/// Where the game data comes from: GitHub, so game updates reach users without a new app build.
const DATA_REPO: &str = "kachi-dev/uma-race-simulator";
const DATA_BRANCH: &str = "master";
/// Why the last `sync_data` failed, for the load error when there is no cached copy either.
static SYNC_ERR: OnceLock<String> = OnceLock::new();

/// `ULALA_DATA_DIR` at runtime, else the sim checkout this was built from (source builds), else the copy of the sim
/// repo's `data/` in `%LOCALAPPDATA%/ulala/data`, synced from GitHub on the first call of each launch (`sync_data`).
pub(super) fn data_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(d) = std::env::var_os("ULALA_DATA_DIR") {
            return d.into();
        }
        let Some(dir) = app_dir().map(|d| d.join("data")).filter(|_| !Path::new(DATA_DIR).is_dir()) else { return DATA_DIR.into() };
        if let Err(e) = sync_data(&dir) {
            let _ = SYNC_ERR.set(e);
        }
        dir
    })
    .clone()
}

/// Brings `dir` in line with `data/` on the sim repo's `DATA_BRANCH`: one API call lists it with git blob hashes, then
/// every file whose local copy hashes differently is downloaded (first launch: all ~57 MB; a game update: usually a
/// few hundred KB). Each file is checked against its hash and swapped in whole, so a failure keeps the old copy.
// ponytail: whatever lands on master reaches every user's next launch, including a data change that needs newer sim
// code than this build; a `data` release branch (or a format number in the repo) would gate that.
fn sync_data(dir: &Path) -> Result<(), String> {
    #[derive(Deserialize)]
    struct Tree {
        tree: Vec<Entry>,
    }
    #[derive(Deserialize)]
    struct Entry {
        path: String,
        sha: String,
        r#type: String,
    }
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(120))).build().into();
    let err = |e: ureq::Error| e.to_string();
    let url = format!("https://api.github.com/repos/{DATA_REPO}/git/trees/{DATA_BRANCH}?recursive=1");
    let json = agent.get(&url).header("User-Agent", "ulala").call().map_err(err)?.body_mut().read_to_string().map_err(err)?;
    let tree: Tree = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let todo: Vec<(&str, &str)> = tree
        .tree
        .iter()
        .filter(|e| e.r#type == "blob")
        .filter_map(|e| Some((e.path.strip_prefix("data/")?, e.sha.as_str())))
        .filter(|(p, sha)| std::fs::read(dir.join(p)).map_or(true, |b| blob_sha(&b) != *sha))
        .collect();
    let fetch = |path: &str, sha: &str| -> Result<(), String> {
        let url = format!("https://raw.githubusercontent.com/{DATA_REPO}/{DATA_BRANCH}/data/{path}");
        let body = agent.get(&url).call().map_err(err)?.body_mut().with_config().limit(u64::MAX).read_to_vec().map_err(err)?;
        // raw.githubusercontent caches for a few minutes after a push: a stale file waits for the next launch.
        if blob_sha(&body) != sha {
            return Err(format!("{path}: hash mismatch"));
        }
        let to = dir.join(path);
        let part = to.with_file_name(format!("{}.part", to.file_name().unwrap_or_default().to_string_lossy()));
        std::fs::create_dir_all(to.parent().unwrap_or(dir)).map_err(|e| e.to_string())?;
        std::fs::write(&part, body).and_then(|_| std::fs::rename(&part, &to)).map_err(|e| format!("{}: {e}", to.display()))
    };
    let res = super::par_map(todo.len(), &super::Work::default(), |i| fetch(todo[i].0, todo[i].1));
    res.into_iter().find_map(|r| r.1.err()).map_or(Ok(()), Err)
}

/// Git's blob hash of `bytes` (what the GitHub tree lists as `sha`).
pub(super) fn blob_sha(bytes: &[u8]) -> String {
    use sha1::{Digest, Sha1};
    let mut h = Sha1::new();
    h.update(format!("blob {}\0", bytes.len()));
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// GameData per era, loaded once. The load (seconds) runs outside the lock, which the UI thread takes for other eras
/// meanwhile; two concurrent loads of one era just waste one.
pub(super) fn game_data(era: DataEra) -> Result<Arc<GameData>, String> {
    static CACHE: OnceLock<Mutex<HashMap<DataEra, Arc<GameData>>>> = OnceLock::new();
    let cache = || CACHE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(d) = cache().get(&era) {
        return Ok(d.clone());
    }
    let d = GameData::load(&data_dir(), era.clone()).map_err(|e| match SYNC_ERR.get() {
        Some(why) => format!("Couldn't download the game data ({why}). {e}"),
        None => e,
    })?;
    let d = Arc::new(d);
    Ok(cache().entry(era).or_insert(d).clone())
}

/// assets/names.json (tools/gen_names.py, from master.mdb text_data).
#[derive(Deserialize)]
pub(super) struct Names {
    pub(super) card: HashMap<i32, CardRow>,
    pub(super) chara: HashMap<i32, String>,
    pub(super) skill: HashMap<i32, SkillName>,
    /// By race_instance id.
    pub(super) race: HashMap<i32, String>,
    pub(super) track: HashMap<i32, String>,
    pub(super) cm: Vec<CmPreset>,
}

/// A Champions Meeting's course and conditions (all its rounds share them).
#[derive(Deserialize)]
pub struct CmPreset {
    /// CM number (champions_schedule id).
    pub id: i32,
    pub name: String,
    /// Start date, "YYYY-MM-DD".
    pub start: String,
    pub course_set_id: i32,
    pub race_instance_id: i32,
    pub ground: i32,
    pub weather: i32,
    pub season: i32,
    pub time: i32,
}

/// Champions Meetings, newest first.
pub fn cm_presets() -> &'static [CmPreset] {
    &names().cm
}

/// The CM a Champions Meeting save is from: the latest one on its course that started by `date` ("YYYY-MM-DD"), which
/// tells repeat cups (same course) apart.
pub fn cm_of_save(course_set_id: i32, date: &str) -> Option<&'static CmPreset> {
    cm_presets().iter().find(|p| p.course_set_id == course_set_id && p.start.as_str() <= date)
}

/// Index into `cm_presets` of the CM whose course and conditions `input` has.
pub fn cm_preset_of(input: &RaceInput) -> Option<usize> {
    let key = (input.course_set_id, input.ground_condition, input.weather, input.season, input.time);
    cm_presets().iter().position(|p| (p.course_set_id, p.ground, p.weather, p.season, p.time) == key)
}

/// [title, chara_id, unique skill, running style, 10 aptitudes: short mile middle long, nige senko sashi oikomi, turf dirt]
#[derive(Deserialize)]
#[serde(from = "(String, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32)")]
pub(super) struct CardRow(pub(super) String, pub(super) i32, pub(super) i32, pub(super) i32, pub(super) [i32; 10]);

impl From<(String, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32)> for CardRow {
    fn from(r: (String, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32)) -> Self {
        CardRow(r.0, r.1, r.2, r.3, [r.4, r.5, r.6, r.7, r.8, r.9, r.10, r.11, r.12, r.13])
    }
}

/// [name, description, rarity, icon_id, disp_order (both optional)]
#[derive(Deserialize)]
pub(super) struct SkillName(
    pub(super) String,
    pub(super) String,
    pub(super) i32,
    #[serde(default)] pub(super) i32,
    #[serde(default)] pub(super) i32,
);

pub(super) fn names() -> &'static Names {
    static N: OnceLock<Names> = OnceLock::new();
    N.get_or_init(|| serde_json::from_str(include_str!("../../assets/names.json")).expect("assets/names.json"))
}
