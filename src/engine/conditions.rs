//! Skill conditions and effects in compact form: "Final corner or later + order 4th-7th + overtaking".

use uma_race_sim::data::{DataEra, SkillAbilityData};

use super::{DISTANCES, GROUNDS, PHASES, SEASONS, STYLES, SURFACES, TURNS, WEATHERS, game_data, pick};

/// One effect half of a skill (`*_1` / `*_2` columns), for display.
#[derive(Clone, Debug, PartialEq)]
pub struct SkillPart {
    /// Precondition and condition as OR-groups (`condition_groups`); empty = none.
    pub pre: Vec<String>,
    pub cond: Vec<String>,
    /// The raw strings, precondition first.
    pub raw: String,
    pub effects: Vec<String>,
    /// Base seconds (× course distance / 1000 in a race); < 0 = whole race, 0 = instant.
    pub duration: f32,
    /// Base seconds, same scaling; 0 = none.
    pub cooldown: f32,
    /// Duration scales with something besides the distance.
    pub duration_varies: bool,
}

/// A skill's parts with the current data (empty for an unknown id).
pub fn skill_parts(id: i32) -> Vec<SkillPart> {
    let Ok(data) = game_data(DataEra::current()) else { return vec![] };
    let Some(s) = data.skills.get(&id) else { return vec![] };
    s.details
        .iter()
        .filter(|d| !d.condition.is_empty() || d.abilities.iter().any(|a| a.ability_type != 0))
        .map(|d| SkillPart {
            pre: condition_groups(&d.precondition),
            cond: condition_groups(&d.condition),
            raw: [d.precondition.as_str(), d.condition.as_str()].iter().filter(|t| !t.is_empty()).copied().collect::<Vec<_>>().join(" → "),
            effects: d.abilities.iter().filter(|a| a.ability_type != 0).map(effect_text).collect(),
            duration: d.float_ability_time as f32 * 1e-4,
            cooldown: d.float_cooldown_time as f32 * 1e-4,
            duration_varies: d.ability_time_usage > 1,
        })
        .collect()
}

/// Whether the skill needs the pre-race wit check.
pub fn wit_checked(id: i32) -> bool {
    game_data(DataEra::current()).ok().and_then(|d| d.skills.get(&id).map(|s| s.activate_lot == 1)).unwrap_or(false)
}

/// A skill's activation condition on one line: groups joined by " | ", precondition first ("pre → cond").
pub fn condition_text(id: i32) -> String {
    let Some(p) = skill_parts(id).into_iter().next() else { return String::new() };
    let (pre, cond) = (p.pre.join(" | "), p.cond.join(" | "));
    if pre.is_empty() { cond } else { format!("{pre} → {cond}") }
}

/// One condition term, compact.
pub fn term_text(term: &str) -> String {
    group_text(term)
}

/// A condition string (`@` = or, `&` = and) as compact OR-groups.
pub fn condition_groups(text: &str) -> Vec<String> {
    text.split('@').filter(|g| !g.is_empty()).map(group_text).collect()
}

/// Bounds on one keyword within an AND-group. `lo`/`hi`: (value, inclusive).
#[derive(Default)]
struct Bounds {
    eq: Option<i32>,
    ne: Option<i32>,
    lo: Option<(i32, bool)>,
    hi: Option<(i32, bool)>,
}

/// Keywords compared as places (1st, 2nd, ...): strict bounds become inclusive ones.
const PLACES: [(&str, &str); 4] = [("order", "order"), ("order_lastspurt", "order at spurt"), ("order_final_corner_end", "order after final corner"), ("popularity", "popularity")];

/// Conditions are shown for a Champions Meeting field.
const FIELD: i32 = 9;

/// Field-relative keywords and the place keyword they become for a known field size.
const RATES: [(&str, &str); 5] = [
    ("order_rate", "order"),
    ("order_rate_lastspurt", "order_lastspurt"),
    ("order_rate_final_corner_end", "order_final_corner_end"),
    ("popularity_rate", "popularity"),
    ("gate_number_rate", "gate_number"),
];

/// An AND-group: terms on the same keyword merge ("order_rate>=40&order_rate<=75" → "order 4th-7th").
fn group_text(g: &str) -> String {
    let mut keys: Vec<(String, Bounds, Vec<&str>)> = Vec::new();
    for term in g.split('&') {
        let Some((k, op, v)) = parse(term) else {
            keys.push((String::new(), Bounds::default(), vec![term]));
            continue;
        };
        // `*_rate` = % of the field, rounded half up to a place (the sim's rescale_rate) in a CM-size field.
        let (k, v) = match RATES.iter().find(|r| r.0 == k) {
            Some(r) => (r.1, (v as f32 / 100.0 * FIELD as f32 + 0.5) as i32),
            None => (k, v),
        };
        let int = PLACES.iter().any(|p| p.0 == k) || matches!(k, "gate_number" | "phase");
        let i = keys.iter().position(|x| x.0 == k).unwrap_or_else(|| {
            keys.push((k.to_string(), Bounds::default(), vec![]));
            keys.len() - 1
        });
        let (_, b, raw) = &mut keys[i];
        raw.push(term);
        let tighter = |old: Option<(i32, bool)>, new: (i32, bool), lo: bool| match old {
            Some(o) if (if lo { o.0 > new.0 } else { o.0 < new.0 }) || (o.0 == new.0 && !o.1) => o,
            _ => new,
        };
        match op {
            "==" => b.eq = Some(v),
            "!=" => b.ne = Some(v),
            ">=" => b.lo = Some(tighter(b.lo, (v, true), true)),
            ">" => b.lo = Some(tighter(b.lo, if int { (v + 1, true) } else { (v, false) }, true)),
            "<=" => b.hi = Some(tighter(b.hi, (v, true), false)),
            _ => b.hi = Some(tighter(b.hi, if int { (v - 1, true) } else { (v, false) }, false)),
        }
    }
    let mut parts: Vec<String> = keys.iter().map(|(k, b, raw)| render(k, b).unwrap_or_else(|| raw.join(" + "))).collect();
    // Capitalised, unless it starts with a raw keyword.
    if let Some(first) = parts.first_mut().filter(|_| render(&keys[0].0, &keys[0].1).is_some()) {
        let mut c = first.chars();
        *first = c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default();
    }
    parts.join(" + ")
}

/// `keyword op value`, op one of == != >= <= > <.
fn parse(t: &str) -> Option<(&str, &str, i32)> {
    let i = t.find(['=', '!', '<', '>'])?;
    let j = if t[i + 1..].starts_with('=') { i + 2 } else { i + 1 };
    Some((&t[..i], &t[i..j], t[j..].trim().parse().ok()?))
}

fn ordinal(v: i32) -> String {
    let suffix = match (v % 10, v % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{v}{suffix}")
}

/// Bounds as "399-401m", "≥5s", "≤30%", ">1L", "≠2".
fn range(b: &Bounds, unit: &str) -> String {
    let sym = |x: (i32, bool), incl: &str, excl: &str| format!("{}{}{unit}", if x.1 { incl } else { excl }, x.0);
    match (b.eq, b.ne, b.lo, b.hi) {
        (Some(v), ..) => format!("{v}{unit}"),
        (_, _, Some(lo), Some(hi)) if lo.1 && hi.1 => format!("{}-{}{unit}", lo.0, hi.0),
        (_, _, Some(lo), Some(hi)) => format!("{} {}", sym(lo, "≥", ">"), sym(hi, "≤", "<")),
        (_, _, Some(lo), None) => sym(lo, "≥", ">"),
        (_, _, None, Some(hi)) => sym(hi, "≤", "<"),
        (_, Some(v), ..) => format!("≠{v}{unit}"),
        _ => String::new(),
    }
}

/// One keyword's bounds in words; None = not covered (shown raw).
fn render(k: &str, b: &Bounds) -> Option<String> {
    let phase = |p: i32| PHASES.get(p as usize).copied().unwrap_or("?");
    let flag = |on: &str, off: &str| match b.eq {
        Some(1) => Some(on.to_string()),
        Some(0) => Some(off.to_string()),
        _ => None,
    };
    let num = |name: &str, unit: &str| Some(format!("{name} {}", range(b, unit)));
    let v = b.eq.unwrap_or(-1);
    let style = |s: &str| ["nige", "senko", "sashi", "oikomi"].iter().position(|x| s.ends_with(x)).map(|i| STYLES[i]);
    if let Some(r) = k.strip_prefix("running_style_count_").filter(|r| !r.starts_with("same")) {
        let other = if r.ends_with("_otherself") { "other " } else { "" };
        return style(r.trim_end_matches("_otherself")).map(|st| format!("{other}{st} {}", range(b, "")));
    }
    if let Some(st) = k.strip_prefix("running_style_temptation").and_then(style) {
        return Some(format!("rushing {st} {}", range(b, "")));
    }
    if let Some(&(_, name)) = PLACES.iter().find(|p| p.0 == k) {
        if let Some(v) = b.eq {
            return Some(format!("{name} {}", ordinal(v)));
        }
        if let Some(v) = b.ne {
            return Some(format!("{name} not {}", ordinal(v)));
        }
        let lo = b.lo.map_or(1, |x| x.0);
        let hi = b.hi.map_or(FIELD, |x| x.0);
        return Some(if hi == lo { format!("{name} {}", ordinal(lo)) } else { format!("{name} {}-{}", ordinal(lo), ordinal(hi)) });
    }
    match k {
        "running_style" => pick(&STYLES, v).map(String::from),
        "distance_type" => pick(&DISTANCES, v).map(String::from),
        "ground_type" => pick(&SURFACES, v).map(String::from),
        "ground_condition" => pick(&GROUNDS, v).map(|x| format!("{x} ground")),
        "weather" => pick(&WEATHERS, v).map(String::from),
        "season" => pick(&SEASONS, v).map(String::from),
        "rotation" => pick(&TURNS, v).map(|x| format!("{x}-handed")),
        "motivation" => match b.eq {
            Some(v) => pick(&["Awful", "Bad", "Normal", "Good", "Great"], v).map(|x| format!("{x} mood")),
            None => num("mood", ""),
        },
        "phase" => Some(match (b.eq, b.lo, b.hi) {
            (Some(v), ..) => phase(v).to_string(),
            (_, Some(lo), Some(hi)) => format!("{} to {}", phase(lo.0), phase(hi.0)),
            (_, Some(lo), None) => format!("{} or later", phase(lo.0)),
            (_, None, Some(hi)) => format!("{} or earlier", phase(hi.0)),
            _ => return None,
        }),
        "phase_random" => Some(format!("random point in {}", phase(v))),
        "phase_firsthalf_random" => Some(format!("random point, 1st half of {}", phase(v))),
        "phase_laterhalf_random" => Some(format!("random point, 2nd half of {}", phase(v))),
        "all_phase_random" => Some("random point".into()),
        "distance_rate_after_random" => Some(format!("random point after {v}% of course")),
        "corner_random" => Some(format!("random point on corner {v}")),
        "all_corner_random" => Some("random point on a corner".into()),
        "straight_random" | "while_in_straight_random" => Some("random point on a straight".into()),
        "last_straight_random" => Some("random point on final straight".into()),
        "is_finalcorner_random" => Some("random point on final corner".into()),
        "up_slope_random" => Some("random point uphill".into()),
        "down_slope_random" => Some("random point downhill".into()),
        "corner" => match (b.eq, b.ne) {
            (Some(0), _) => Some("straight".into()),
            (_, Some(0)) => Some("corner".into()),
            (Some(c), _) => Some(format!("corner {c}")),
            _ => None,
        },
        "slope" => ["flat", "uphill", "downhill"].get(v as usize).map(|x| x.to_string()),
        "straight_front_type" => ["", "home straight", "back straight"].get(v as usize).filter(|x| !x.is_empty()).map(|x| x.to_string()),
        "is_finalcorner" => flag("final corner or later", "before final corner"),
        "is_finalcorner_laterhalf" => flag("2nd half of final corner", "not 2nd half of final corner"),
        "is_last_straight_onetime" => flag("entering final straight", "not entering final straight"),
        "is_lastspurt" => flag("last spurt", "not last spurt"),
        "is_overtake" => flag("overtaking", "not overtaking"),
        "is_surrounded" => flag("surrounded", "not surrounded"),
        "is_temptation" => flag("rushing", "not rushing"),
        "is_move_lane" => flag("changing lanes", "not changing lanes"),
        "is_badstart" => flag("bad start", "no bad start"),
        "is_goodstart" => flag("good start", "no good start"),
        "is_hp_empty" => flag("out of HP", "has HP"),
        "is_basis_distance" => flag("standard distance", "non-standard distance"),
        "is_activate_any_skill" => flag("after another skill", "no other skill"),
        "blocked_front" => flag("blocked ahead", "not blocked ahead"),
        "blocked_side" => flag("blocked at side", "not blocked at side"),
        "blocked_all" => flag("blocked", "not blocked"),
        "change_order_onetime" => match (b.lo, b.hi) {
            (_, Some(_)) => Some("passes someone".into()),
            (Some(_), _) => Some("gets passed".into()),
            _ => None,
        },
        "always" => Some("always".into()),
        "random_lot" => Some(format!("{v}% chance")),
        "gate_number" => num("gate", ""),
        "distance_rate" => num("course", "%"),
        "remain_distance" => Some(format!("{} left", range(b, "m"))),
        "hp_per" => num("HP", "%"),
        "accumulatetime" => num("time", "s"),
        "near_count" => num("nearby", ""),
        "bashin_diff_infront" => num("gap ahead", "L"),
        "bashin_diff_behind" => num("gap behind", "L"),
        "distance_diff_top" => num("gap to leader", "m"),
        "infront_near_lane_time" => num("tailing", "s"),
        "behind_near_lane_time" => num("tailed", "s"),
        "overtake_target_time" => num("chasing", "s"),
        "blocked_front_continuetime" => num("blocked ahead", "s"),
        "blocked_side_continuetime" => num("blocked at side", "s"),
        "blocked_all_continuetime" => num("blocked", "s"),
        "temptation_opponent_count_infront" | "temptation_count_infront" => num("rushing ahead", ""),
        "temptation_opponent_count_behind" | "temptation_count_behind" => num("rushing behind", ""),
        "activate_count_all" => num("skills used", ""),
        "activate_count_heal" => num("heals used", ""),
        "activate_count_start" => num("early-race skills", ""),
        "activate_count_middle" => num("mid-race skills", ""),
        "activate_count_end_after" => num("late-race skills", ""),
        "activate_count_later_half" => num("2nd-half skills", ""),
        _ => None,
    }
}

/// "+0.35 m/s target speed", "HP +5.5%", "Speed +60"; "(scaled)" when the value depends on something.
fn effect_text(a: &SkillAbilityData) -> String {
    let v = a.float_ability_value as f32 * 1e-4;
    let f = |x: f32| {
        let s = format!("{x:+.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    let s = match a.ability_type {
        t @ 1..=5 => format!("{} {}", ["Speed", "Stamina", "Power", "Guts", "Wit"][t as usize - 1], f(v)),
        32 => format!("All stats {}", f(v)),
        7 => format!("HP use ×{}", f(v).trim_start_matches('+')),
        8 => format!("Vision {} m", f(v)),
        9 => format!("HP {}%", f(v * 100.0)),
        10 => format!("Start delay ×{}", f(v).trim_start_matches('+')),
        11 => "Moves inside".into(),
        12 => "Moves outside".into(),
        13 => "Ends rushing".into(),
        14 => "Fixes start delay".into(),
        21 => format!("Current speed {} m/s", f(v)),
        22 => format!("Current speed {} m/s (fading)", f(v)),
        27 => format!("Target speed {} m/s", f(v)),
        28 => format!("Lane change speed {}", f(v)),
        29 => format!("Rushing chance {}", f(v)),
        30 => format!("Push {}", f(v)),
        31 => format!("Accel {} m/s²", f(v)),
        35 => format!("Target lane {}", f(v)),
        36 | 37 => format!("Activates {} random skills", v as i32),
        38 => "Cancels debuffs".into(),
        t => format!("Type {t} {}", f(v)),
    };
    let target = if !matches!(a.target_type, 0 | 1) { " (others)" } else { "" };
    let scaled = if a.ability_value_usage > 1 { " (scaled)" } else { "" };
    format!("{s}{target}{scaled}")
}
