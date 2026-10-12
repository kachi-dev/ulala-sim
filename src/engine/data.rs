use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use serde::Deserialize;
use uma_race_sim::api::RaceInput;
use uma_race_sim::data::{DATA_DIR, DataEra, GameData};

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

/// `ULALA_DATA_DIR` at runtime, else the sim's build-time `DATA_DIR`.
pub(super) fn data_dir() -> PathBuf {
    std::env::var_os("ULALA_DATA_DIR").map_or_else(|| PathBuf::from(DATA_DIR), PathBuf::from)
}

/// GameData per era, loaded once. The load (seconds) runs outside the lock, which the UI thread takes for other eras
/// meanwhile; two concurrent loads of one era just waste one.
pub(super) fn game_data(era: DataEra) -> Result<Arc<GameData>, String> {
    static CACHE: OnceLock<Mutex<HashMap<DataEra, Arc<GameData>>>> = OnceLock::new();
    let cache = || CACHE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(d) = cache().get(&era) {
        return Ok(d.clone());
    }
    let d = Arc::new(GameData::load(&data_dir(), era.clone())?);
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
