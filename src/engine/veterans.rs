//! horseACT's veteran list (`<output>/Saved Races/veterans.json`): the user's trained umas, ready to race.

use serde::Deserialize;
use uma_race_sim::api::{HorseInput, SkillInput};

use super::{names, saved_races_dir};

pub struct Veteran {
    /// trained_chara_id
    pub id: i32,
    /// As trained, at Great mood.
    pub horse: HorseInput,
    /// Rank index (1 = G), see `rank_label`.
    pub rank: i32,
    pub rating: i32,
    /// "YYYY-MM-DD HH:MM:SS"
    pub created: String,
}

/// One entry of veterans.json (the game's TrainedChara), just what a race needs.
#[derive(Deserialize)]
struct Row {
    trained_chara_id: i32,
    card_id: i32,
    speed: i32,
    stamina: i32,
    power: i32,
    guts: i32,
    wiz: i32,
    running_style: i32,
    proper_distance_short: i32,
    proper_distance_mile: i32,
    proper_distance_middle: i32,
    proper_distance_long: i32,
    proper_running_style_nige: i32,
    proper_running_style_senko: i32,
    proper_running_style_sashi: i32,
    proper_running_style_oikomi: i32,
    proper_ground_turf: i32,
    proper_ground_dirt: i32,
    skill_array: Vec<SkillRow>,
    rarity: i32,
    talent_level: i32,
    rank: i32,
    rank_score: i32,
    #[serde(default)]
    fans: i32,
    #[serde(default)]
    wins: i32,
    #[serde(default)]
    create_time: String,
}

#[derive(Deserialize)]
struct SkillRow {
    skill_id: i32,
    level: i32,
}

pub fn veterans_file() -> std::path::PathBuf {
    saved_races_dir().join("veterans.json")
}

/// Every veteran in horseACT's list, in file order.
pub fn load_veterans() -> Result<Vec<Veteran>, String> {
    let f = veterans_file();
    let b = std::fs::read(&f).map_err(|e| format!("{}: {e}", f.display()))?;
    parse_veterans(&b).map_err(|e| format!("{}: {e}", f.display()))
}

pub(super) fn parse_veterans(json: &[u8]) -> Result<Vec<Veteran>, String> {
    let rows: Vec<Row> = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(veteran).collect())
}

fn veteran(r: Row) -> Veteran {
    let horse = HorseInput {
        chara_id: names().card.get(&r.card_id).map_or(r.card_id / 100, |c| c.chara_id),
        card_id: r.card_id,
        speed: r.speed,
        stamina: r.stamina,
        pow: r.power,
        guts: r.guts,
        wiz: r.wiz,
        running_style: r.running_style,
        motivation: 5,
        proper_distance: [r.proper_distance_short, r.proper_distance_mile, r.proper_distance_middle, r.proper_distance_long],
        proper_running_style: [r.proper_running_style_nige, r.proper_running_style_senko, r.proper_running_style_sashi, r.proper_running_style_oikomi],
        proper_ground: [r.proper_ground_turf, r.proper_ground_dirt],
        skills: r.skill_array.iter().map(|s| SkillInput { skill_id: s.skill_id, level: s.level }).collect(),
        popularity: 1, // custom fields get distinct ones at run time, see `custom_popularity`
        viewer_id: 1,
        final_grade: r.rank,
        rarity: r.rarity,
        talent_level: r.talent_level,
        single_mode_win_count: r.wins,
        fan_count: r.fans,
        ..Default::default()
    };
    Veteran { id: r.trained_chara_id, horse, rank: r.rank, rating: r.rank_score, created: r.create_time }
}

/// A stat value's letter rank, on `rank_label`'s scale (thresholds as in umalator).
pub fn stat_rank(x: i32) -> i32 {
    1 + match x {
        1201.. => (18 + (x - 1200) / 100 * 10 + x / 10 % 10).min(97),
        1150.. => 17,
        1100.. => 16,
        400.. => 8 + (x - 400) / 100,
        _ => x.max(0) / 50,
    }
}

/// The game's rank letters: 1 = G, 2 = G+ … 18 = SS+, then UG, UG1 … UG9, UF … US9.
pub fn rank_label(rank: i32) -> String {
    const LOW: [&str; 18] = ["G", "G+", "F", "F+", "E", "E+", "D", "D+", "C", "C+", "B", "B+", "A", "A+", "S", "S+", "SS", "SS+"];
    const HIGH: [&str; 8] = ["UG", "UF", "UE", "UD", "UC", "UB", "UA", "US"];
    if let Some(l) = usize::try_from(rank - 1).ok().and_then(|i| LOW.get(i)) {
        return l.to_string();
    }
    let k = rank - 19;
    match usize::try_from(k / 10).ok().and_then(|i| HIGH.get(i)) {
        Some(l) if k % 10 == 0 => l.to_string(),
        Some(l) => format!("{l}{}", k % 10),
        None => "?".into(),
    }
}
