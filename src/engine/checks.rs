//! Why each skill of a horse did or didn't activate in a run, read from the sim's skill state after the race.

use uma_race_sim::sim::race as sim_race;
use uma_race_sim::sim::skill::trigger::kind::SkillTriggerKind as K;
use uma_race_sim::sim::skill::trigger::{SkillTrigger, SkillTriggerCollection};

use super::term_text;

/// Why a skill did or didn't activate in one run, in the order checked.
#[derive(Clone, Debug, PartialEq)]
pub enum SkillOutcome {
    Activated,
    /// Lost the pre-race wit roll (`chance` %, from the uma's wit).
    WitCheck { chance: f32 },
    /// A fixed condition (running style, distance, surface, track, ...) rules it out in this race. The failing terms.
    CantHere(Vec<String>),
    /// Its precondition never held.
    Precondition,
    /// Its conditions never all held at once. `window`: the randomly drawn stretch (m) it could activate in, if any.
    NotMet { window: Option<(f32, f32)> },
}

impl SkillOutcome {
    /// Index into `OUTCOMES`.
    pub fn kind(&self) -> usize {
        match self {
            Self::Activated => 0,
            Self::WitCheck { .. } => 1,
            Self::CantHere(_) => 2,
            Self::Precondition | Self::NotMet { .. } => 3,
        }
    }
}

/// (label, hint) per `SkillOutcome::kind`.
pub const OUTCOMES: [(&str, &str); 4] = [
    ("Activated", "Activated at least once"),
    ("Wit check", "Failed the wit check"),
    ("Can't here", "A fixed condition (style, distance, surface, track, ...) rules it out in this race"),
    ("Conditions not met", "Passed the wit check, but its conditions never all held at once"),
];

#[derive(Clone, Debug, PartialEq)]
pub struct SkillCheck {
    pub skill_id: i32,
    pub outcome: SkillOutcome,
}

/// Horse `i`'s skills (input order) after the race; `Activated` is left to the caller (it reads the events).
pub(super) fn skill_checks(r: &sim_race::Race, i: usize) -> Vec<SkillCheck> {
    let h = &r.horses[i];
    let sm = &h.skill_manager;
    h.horse_data.skills.iter().map(|s| {
        let outcome = match sm.skills.iter().find(|b| b.id == s.skill_id) {
            None => SkillOutcome::CantHere(vec!["not in this era's skill data".into()]),
            Some(b) if !b.is_activate_enable => {
                // = lot_activate_skill's threshold.
                let p = &r.data.param.Skill;
                let wiz = h.horse_data.race_param.base_wiz;
                let chance = ((p.LotActivatePerVal1 as f64 - p.LotActivatePerVal2 as f64 / wiz as f64) as f32).max(p.ActivatePerMin as f32);
                SkillOutcome::WitCheck { chance: chance.min(100.0) }
            }
            Some(b) => {
                // Detail 0 is the skill's own condition (detail 1 is a follow-up effect).
                let (d, m) = (&b.details[0], &b.skill_master.details[0]);
                // Fixed terms never change during the race, so their final state is the whole race's.
                let (c, text) = if d.is_need_pre_trigger_check { (&d.pre_trigger, &m.precondition) } else { (&d.trigger, &m.condition) };
                let out = ruled_out(&sm.triggers, c, text);
                if !out.is_empty() {
                    SkillOutcome::CantHere(out)
                } else if d.is_need_pre_trigger_check {
                    SkillOutcome::Precondition
                } else {
                    let window = sm.triggers[d.trigger.range()].iter().find_map(|t| match &t.kind {
                        K::Distance(_, w) if w.has_distance => Some((w.distance_min, w.distance_max)),
                        _ => None,
                    });
                    SkillOutcome::NotMet { window }
                }
            }
        };
        SkillCheck { skill_id: s.skill_id, outcome }
    })
    .collect()
}

/// Terms of a condition string in trigger order (`create_trigger_collection` makes one trigger per term of each
/// non-empty `@` group).
fn terms(text: &str) -> impl Iterator<Item = &str> {
    text.split('@').filter(|g| !g.is_empty()).flat_map(|g| g.split('&'))
}

/// The false fixed terms of `c`, if they rule out every OR-group; else empty.
fn ruled_out(arena: &[SkillTrigger], c: &SkillTriggerCollection, text: &str) -> Vec<String> {
    let SkillTriggerCollection::Collection { any_empty_group: false, .. } = c else { return vec![] };
    let ts = &arena[c.range()];
    let bad: Vec<(u16, &str)> = ts.iter().zip(terms(text)).filter(|(t, _)| fixed(&t.kind) && !t.is_activated).map(|(t, s)| (t.group, s)).collect();
    if ts.chunk_by(|a, b| a.group == b.group).all(|g| bad.iter().any(|b| b.0 == g[0].group)) {
        bad.into_iter().map(|b| term_text(b.1)).collect()
    } else {
        vec![]
    }
}

/// Terms that read the race setup or the uma's fixed state, not the running race (`Null` = a keyword the sim doesn't
/// know: never true).
fn fixed(k: &K) -> bool {
    matches!(
        k,
        K::Null | K::CourseCornerCount | K::Grade | K::RaceId | K::Time | K::CourseDistance | K::IsDirtGrade | K::GateNumber | K::PostNumber
            | K::BaseSpeed | K::BaseStamina | K::BasePower | K::BaseGuts | K::BaseWiz | K::GroundType | K::GroundCondition
            | K::CourseRotation | K::CourseDistanceType | K::Weather | K::Season | K::RaceTrackId | K::RunningStyle
            | K::Motivation | K::Popularity | K::Latched(_) | K::ExistCharaId(_) | K::SameSkillHorseCount { .. }
    )
}
