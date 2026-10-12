use std::path::{Path, PathBuf};
use std::sync::Arc;

use uma_race_sim::api::{HorseInput, RaceInput, SkillInput};
use uma_race_sim::data::{DataEra, GameData, corpus};
use uma_race_sim::replay::Replay;
use uma_race_sim::sim::{defs::RunningStyleEx, horse::running_style_ex};

use super::{GROUNDS, SEASONS, SURFACES, SaveHead, TURNS, WEATHERS, course_info, cm_presets, game_data, names, pick};

/// A race setup: from a saved race (`load_race`) or built by hand (`new_race`). Display strings: `race_title`,
/// `race_subtitle`, `chara_name`; course geometry: `course_view`.
#[derive(Clone)]
pub struct Race {
    /// The save it was loaded from (None = custom).
    pub path: Option<PathBuf>,
    /// The server's replay of the saved race (for the "matches server" check). None = custom.
    pub replay: Option<Replay>,
    /// "YYYYMMDD", "" for custom.
    pub date: String,
    /// Baseline input (a save's exactly as saved, seed included).
    pub input: RaceInput,
    /// Per horse (gate order), "" for NPCs / custom horses.
    pub trainers: Vec<String>,
    /// The save owner's horse (`playerHorseIndex`); its `team_id` is the player's team. None for custom races.
    pub player: Option<usize>,
    pub data: Arc<GameData>,
}

/// Parses a save and loads (cached per data era) the GameData it needs.
pub fn load_race(path: &Path) -> Result<Race, String> {
    let err = |e: String| format!("{}: {e}", path.display());
    let s = std::fs::read_to_string(path).map_err(|e| err(e.to_string()))?;
    let (input, replay) = corpus::parse_race(&s).map_err(err)?;
    let date = corpus::date_from_path(path).unwrap_or_default();
    let data = game_data(DataEra::for_date(&date).unwrap_or_else(|_| DataEra::current())).map_err(err)?;
    // parse_race drops the trainer names; a second, tiny parse picks them up.
    let save: Option<SaveHead> = serde_json::from_str(&s).ok();
    let trainers = (0..input.horses.len())
        .map(|i| save.as_ref().and_then(|s| s.horses.get(i)).and_then(|h| h.r.trainer_name.clone()).unwrap_or_default())
        .collect();
    let player = save.as_ref().and_then(|s| s.player).and_then(|i| usize::try_from(i).ok()).filter(|&i| i < input.horses.len());
    Ok(Race { path: Some(path.to_path_buf()), replay: Some(replay), date, input, trainers, player, data })
}

/// Custom race: current data, a default course, one Haru Urara.
pub fn new_race() -> Race {
    let data = game_data(DataEra::current()).expect("game data");
    let input = RaceInput { random_seed: 1024, ground_condition: 1, weather: 1, season: 4, time: 2, race_type: 8, race_instance_id: 100101, ..Default::default() };
    let mut race = Race { path: None, replay: None, date: String::new(), input, trainers: vec![String::new()], player: None, data };
    let mut input = race.input.clone();
    set_course(&race, &mut input, 10611); // Tokyo dirt 1600, February Stakes: suits Urara (dirt A, mile B)
    input.horses = vec![new_horse(&race, 105201)]; // Haru Urara [Bestest Prize ♪]
    race.input = input;
    race
}

/// A custom horse wearing card `card_id`: its unique skill, default running style and aptitudes, endgame-ish stats.
pub fn new_horse(race: &Race, card_id: i32) -> HorseInput {
    let c = names().card.get(&card_id);
    let apt = |k: usize| c.map_or(7, |c| c.4[k]);
    let unique = c.map_or(0, |c| c.2);
    HorseInput {
        chara_id: c.map_or(card_id / 100, |c| c.1),
        card_id,
        speed: 1200,
        stamina: 1000,
        pow: 1000,
        guts: 600,
        wiz: 1000,
        running_style: c.map_or(2, |c| c.3),
        motivation: 5,
        proper_distance: [apt(0), apt(1), apt(2), apt(3)],
        proper_running_style: [apt(4), apt(5), apt(6), apt(7)],
        proper_ground: [apt(8), apt(9)],
        skills: race.data.skills.contains_key(&unique).then_some(SkillInput { skill_id: unique, level: 4 }).into_iter().collect(),
        popularity: 1, // custom fields get distinct ones at run time, see `custom_popularity`
        // Real player: the dense frame window near the goal (recording only).
        viewer_id: 1,
        rarity: 4,
        talent_level: 4,
        ..Default::default()
    }
}

/// Runaway (Oonige): a Front runner with an Oonige skill, derived by the sim the way the game does.
pub fn runaway(race: &Race, h: &HorseInput) -> bool {
    running_style_ex(&race.data.skills, h) == RunningStyleEx::Oonige
}

/// Switches costume or uma: card/chara id, and the old card's unique skill becomes the new card's (same level;
/// appended at level 4 if it wasn't there; dropped if the new one is already owned). Stats and other skills stay;
/// aptitudes and running style stay for a costume, and become the new uma's when the character changes.
pub fn set_card(h: &mut HorseInput, card_id: i32) {
    let Some(c) = names().card.get(&card_id) else { return };
    let old = names().card.get(&h.card_id).map(|o| o.2);
    let has_new = h.skills.iter().any(|s| s.skill_id == c.2);
    match h.skills.iter().position(|s| Some(s.skill_id) == old) {
        Some(i) if !has_new => h.skills[i].skill_id = c.2,
        Some(i) if old != Some(c.2) => {
            h.skills.remove(i);
        }
        None if !has_new => h.skills.push(SkillInput { skill_id: c.2, level: 4 }),
        _ => {}
    }
    if c.1 != h.chara_id {
        let apt = |k: usize| c.4[k];
        h.proper_distance = [apt(0), apt(1), apt(2), apt(3)];
        h.proper_running_style = [apt(4), apt(5), apt(6), apt(7)];
        h.proper_ground = [apt(8), apt(9)];
        h.running_style = c.3;
    }
    (h.card_id, h.chara_id) = (card_id, c.1);
}

/// Level for a skill a horse doesn't own yet: uniques at 4, everything else 1.
pub(super) fn default_level(race: &Race, id: i32) -> i32 {
    if race.data.skills.get(&id).is_some_and(|s| s.rarity >= 3) { 4 } else { 1 }
}

/// `input`'s race instance exists and runs on `input`'s course.
fn instance_on_course(race: &Race, input: &RaceInput) -> bool {
    let m = &race.data.master;
    m.race_instance.get(&input.race_instance_id).and_then(|i| m.race.get(&i.race_id)).is_some_and(|r| r.course_set == input.course_set_id)
}

/// Switches `input` to `course_set_id` (also picks a race instance the sim accepts).
pub fn set_course(race: &Race, input: &mut RaceInput, course_set_id: i32) {
    input.course_set_id = course_set_id;
    if instance_on_course(race, input) {
        return;
    }
    // Highest grade instance on the course (grade 100 = G1; 0 = ungraded, last). None → 0: the sim treats a missing
    // instance as "no grade / race id" for skill conditions.
    let m = &race.data.master;
    input.race_instance_id = m
        .race_instance
        .values()
        .filter_map(|i| Some((i, m.race.get(&i.race_id).filter(|r| r.course_set == course_set_id)?)))
        .min_by_key(|(i, r)| (r.grade == 0, r.grade, i.id))
        .map_or(0, |(i, _)| i.id);
}

/// Race instance name when `input` is a real instance on its course, else e.g. "Tokyo Turf 2400m".
pub fn race_title(race: &Race, input: &RaceInput) -> String {
    // A CM preset's race (rounds, not finals): just the cup's name.
    if let Some(p) = cm_presets().iter().find(|p| p.race_instance_id == input.race_instance_id && instance_on_course(race, input)) {
        return p.name.clone();
    }
    let name = names().race.get(&input.race_instance_id).filter(|_| instance_on_course(race, input));
    name.cloned().unwrap_or_else(|| course_info(&race.data, input.course_set_id).label())
}

/// e.g. "Nakayama · Turf 2500m · Right · Good · Sunny · Winter".
pub fn race_subtitle(race: &Race, input: &RaceInput) -> String {
    let c = course_info(&race.data, input.course_set_id);
    let p = |v: &[&'static str], i| pick(v, i).unwrap_or("?");
    [
        c.track.as_str(),
        format!("{} {}m", p(&SURFACES, c.surface), c.distance).as_str(),
        p(&TURNS, c.turn),
        p(&GROUNDS, input.ground_condition),
        p(&WEATHERS, input.weather),
        p(&SEASONS, input.season),
    ]
    .join(" · ")
}
