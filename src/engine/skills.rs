use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use uma_race_sim::data::{DataEra, GameData, SkillData};

use super::{DISTANCES, PHASES, Race, STYLES, SURFACES, SkillInput, SkillName, game_data, names, pick};

/// Skill display info.
#[derive(Clone, Debug, PartialEq)]
pub struct SkillInfo {
    pub id: i32,
    pub name: String,
    pub description: String,
    /// skill_data.rarity: 1 white, 2 gold, 3+ unique/evolved.
    pub rarity: i32,
    /// Icon id (assets), 0 = unknown.
    pub icon_id: i32,
    /// What it does, from its ability types: names in `SKILL_EFFECTS`.
    pub effects: Vec<&'static str>,
    /// When / for whom, from its tag ids and conditions: names in `SKILL_TAGS`.
    pub tags: Vec<&'static str>,
    /// Inherited version of a unique (9xxxxx).
    pub inherited: bool,
    /// For uniques (and their inherited versions): the card it belongs to.
    pub owner_card: Option<i32>,
}

/// Every `SkillInfo::effects` value. "Debuff" = harmful to other horses ("Stamina drain" = an HP debuff), "Negative" =
/// harmful to the owner, "Stats" = stat bonus (green skills), "Start" = start delay.
pub const SKILL_EFFECTS: &[&str] =
    &["Speed", "Acceleration", "Recovery", "Stats", "Lane", "Vision", "Start", "Debuff", "Stamina drain", "Negative", "Other"];

/// Every `SkillInfo::tags` value: running style, distance, surface, phase, location.
pub const SKILL_TAGS: &[&str] = &[
    "Front", "Pace", "Late", "End", "Sprint", "Mile", "Medium", "Long", "Turf", "Dirt", "Early race", "Mid race", "Late race", "Last spurt",
    "Corner", "Straight", "Uphill", "Downhill",
];

/// Info for one skill id with the current data (name falls back to "Skill <id>").
pub fn skill_info(id: i32) -> SkillInfo {
    let table = game_data(DataEra::current()).map(|d| skill_table(&d));
    table.ok().and_then(|t| t.by_id.get(&id).map(|&i| t.all[i].clone())).unwrap_or_else(|| describe(id, None))
}

/// An uma's skills (with their index) in the game's list order: skill_data.disp_order ascending, ties as given
/// (the client's `PartsSingleModeSkillList` sorts with a stable OrderBy on it). Display only: the sim's order stays.
pub fn game_order(skills: &[SkillInput]) -> Vec<(usize, SkillInput)> {
    let order = |id: i32| names().skill.get(&id).map_or(i32::MAX, |s| s.4);
    let mut v: Vec<(usize, SkillInput)> = skills.iter().copied().enumerate().collect();
    v.sort_by_key(|&(_, k)| order(k.skill_id));
    v
}

/// Every skill the race's GameData knows, sorted by name (for the skill picker).
pub fn all_skills(race: &Race) -> Arc<Vec<SkillInfo>> {
    skill_table(&race.data).all.clone()
}

/// Every skill of an era described, sorted by name then id, plus an id → index map.
struct SkillTable {
    all: Arc<Vec<SkillInfo>>,
    by_id: HashMap<i32, usize>,
}

/// Per era, cached.
fn skill_table(data: &GameData) -> Arc<SkillTable> {
    static CACHE: OnceLock<Mutex<HashMap<DataEra, Arc<SkillTable>>>> = OnceLock::new();
    let mut c = CACHE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    c.entry(data.era.clone())
        .or_insert_with(|| {
            let mut v: Vec<SkillInfo> = data.skills.values().map(|s| describe(s.id, Some(s))).collect();
            v.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
            let by_id = v.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
            Arc::new(SkillTable { all: Arc::new(v), by_id })
        })
        .clone()
}

/// `SkillInfo` from names.json plus the skill's data: effects from ability types / targets (the sim's
/// `is_skill_demerit` for harmful ones), tags from tag_id (101-104 style, 201-204 distance, 301-303 phase, 501/502
/// surface: checked against the conditions of the tagged skills) and from the condition tokens.
fn describe(id: i32, sd: Option<&SkillData>) -> SkillInfo {
    let n = names();
    let (name, description, rarity, icon_id) = match n.skill.get(&id) {
        Some(SkillName(a, b, c, d, _)) => (a.clone(), b.clone(), *c, *d),
        None => (format!("Skill {id}"), String::new(), 0, 0),
    };
    let mut effects = Vec::new();
    let mut tags = Vec::new();
    let add = |v: &mut Vec<&'static str>, x: &'static str| {
        if !v.contains(&x) {
            v.push(x)
        }
    };
    for d in sd.map_or(&[][..], |s| &s.details[..]) {
        for a in d.abilities.iter().filter(|a| a.ability_type != 0) {
            // The sim's demerit flag is always set for 13 / 14 (rushing time, start delay fix): harmful only on others.
            let harmful = match a.ability_type {
                13 | 14 => a.target_type != 1,
                t => uma_race_sim::sim::skill::is_skill_demerit(t, a.float_ability_value as f32 * 0.0001),
            };
            match (harmful, a.target_type) {
                (true, 1) => add(&mut effects, "Negative"),
                (true, _) => {
                    add(&mut effects, "Debuff");
                    if a.ability_type == 9 {
                        add(&mut effects, "Stamina drain");
                    }
                }
                _ => add(&mut effects, match a.ability_type {
                    21 | 22 | 27 => "Speed",
                    31 => "Acceleration",
                    7 | 9 => "Recovery",
                    1..=5 | 32 => "Stats",
                    11 | 12 | 28 | 35 => "Lane",
                    8 => "Vision",
                    10 | 14 => "Start",
                    _ => "Other",
                }),
            }
        }
        for t in d.precondition.split(['&', '@']).chain(d.condition.split(['&', '@'])) {
            // `key op value`, op one of == != >= <= > <.
            let Some(i) = t.find(['=', '!', '<', '>']) else { continue };
            let j = if t[i + 1..].starts_with('=') { i + 2 } else { i + 1 };
            let (k, op, v) = (&t[..i], &t[i..j], t[j..].parse::<i32>().unwrap_or(-1));
            let tag = match (k, op) {
                ("running_style", "==") => pick(&STYLES, v),
                ("distance_type", "==") => pick(&DISTANCES, v),
                ("ground_type", "==") => pick(&SURFACES, v),
                ("is_lastspurt", "==") if v == 1 => Some("Last spurt"),
                ("corner", "!=") | ("corner_random" | "all_corner_random" | "is_finalcorner_random", _) => Some("Corner"),
                ("corner", "==") => Some(if v == 0 { "Straight" } else { "Corner" }),
                ("straight_random" | "while_in_straight_random" | "last_straight_random" | "is_last_straight_onetime" | "straight_front_type", _) => Some("Straight"),
                ("slope", "==") => pick(&["Uphill", "Downhill"], v),
                ("up_slope_random", _) => Some("Uphill"),
                ("down_slope_random", _) => Some("Downhill"),
                ("phase" | "phase_random" | "phase_firsthalf_random" | "phase_laterhalf_random", _) => {
                    let r = match op {
                        "==" => v..=v,
                        ">=" => v..=3,
                        "<=" => 0..=v,
                        _ => continue,
                    };
                    r.filter_map(|p| PHASES.get(p as usize)).for_each(|&p| add(&mut tags, p));
                    None
                }
                _ => None,
            };
            if let Some(x) = tag {
                add(&mut tags, x);
            }
        }
    }
    for t in sd.map_or("", |s| &s.tag_id).split('/').filter_map(|t| t.parse::<i32>().ok()) {
        let tag = match t / 100 {
            1 => pick(&STYLES, t % 100),
            2 => pick(&DISTANCES, t % 100),
            3 => pick(&PHASES[..3], t % 100),
            5 => pick(&SURFACES, t % 100),
            _ => None,
        };
        if let Some(x) = tag {
            add(&mut tags, x);
        }
    }
    tags.sort_by_key(|t| SKILL_TAGS.iter().position(|x| x == t));
    effects.sort_by_key(|t| SKILL_EFFECTS.iter().position(|x| x == t));
    // 9xxxxx = the inherited version of unique 1xxxxx (900521 ↔ 100521).
    let inherited = (900_000..1_000_000).contains(&id);
    let unique = if inherited { id - 800_000 } else { id };
    let owner_card = n.card.iter().find(|(_, c)| c.2 == unique).map(|(&k, _)| k);
    SkillInfo { id, name, description, rarity, icon_id, effects, tags, inherited: inherited && owner_card.is_some(), owner_card }
}
