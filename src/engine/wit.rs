use uma_race_sim::api::{RaceInput, WizAlt, mechanic, span_kind, wiz_mechanic};

use super::{HorseRun, Race, Rivals, SkillOutcome, Work, at_seed, gap_lengths, par_map, run_drawn};

/// `wiz_mechanic` bits in order, as the Wit tab names them.
pub const WIT_MECHANICS: [&str; wiz_mechanic::COUNT] = ["Section rolls", "Position keep", "Spurt candidates", "Skills", "Rushed", "Downhill", "Course bonus"];

/// The `span_kind`s the Wit tab times, as it names them: (kind, label).
/// Downhill first, then the position keep modes (front runners speed up / overtake, the rest pace up / down).
pub const WIT_MODES: [(u8, &str); 5] = [
    (span_kind::DOWNHILL, "Downhill"),
    (span_kind::SPEED_UP, "Speed up"),
    (span_kind::OVERTAKE, "Overtake"),
    (span_kind::PACE_UP, "Pace up"),
    (span_kind::PACE_DOWN, "Pace down"),
];

/// One race of the measured horse.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WitSample {
    pub full_spurt: bool,
    pub rushed: bool,
    /// Skills activated.
    pub skills: u8,
    /// Course distance (m) where the last spurt started; -1 = none.
    pub spurt_start: f32,
    /// Seconds in each of `WIT_MODES`.
    pub modes: [f32; WIT_MODES.len()],
    /// Seconds of the released conserve-power boost.
    pub conserve: f32,
}

/// One seed of one mechanic: the measured horse's race with the mechanic on the new Wit and on the old Wit.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WitPair {
    /// Lengths the new-Wit race finished ahead (+ = the Wit change gained).
    pub gain: f32,
    pub new: WitSample,
    pub old: WitSample,
}

/// The Wit tab's measurement of one horse.
#[derive(Clone, Debug, PartialEq)]
pub struct Wit {
    /// Per `wiz_mechanic` bit (`rows[k]` is bit `1 << k`): the pair per seed.
    pub rows: Vec<Vec<WitPair>>,
    /// Every mechanic at once: the whole Wit change.
    pub all: Vec<WitPair>,
}

fn sample(h: &HorseRun) -> WitSample {
    WitSample {
        full_spurt: h.full_spurt,
        rushed: h.state_time[span_kind::RUSHED as usize] > 0.0,
        skills: h.skills.iter().filter(|c| c.outcome == SkillOutcome::Activated).count() as u8,
        spurt_start: h.spurt_start,
        modes: WIT_MODES.map(|(k, _)| h.state_time[k as usize]),
        conserve: h.state_time[span_kind::CONSERVE_POWER as usize],
    }
}

/// Horse `i` of `input` on `seeds` at its baseline Wit `old_wiz`, against the same with each Wit mechanic alone (then all)
/// on its Wit as set up: a pair differs only in what that mechanic does with the Wit change (plus the knock-on once the
/// races diverge).
pub fn wit(race: &Race, input: &RaceInput, rivals: Option<&Rivals>, i: usize, old_wiz: i32, seeds: &[i32], w: &Work) -> Wit {
    let new_wiz = input.horses[i].wiz;
    let masks: Vec<u8> = (0..wiz_mechanic::COUNT).map(|k| 1 << k).chain([wiz_mechanic::ALL]).collect();
    let res = par_map(seeds.len(), w, |s| {
        let run = |mechanics: u8| {
            let mut inp = at_seed(input, seeds[s], rivals);
            inp.horses[i].viewer_id = 1; // dense frames near the goal (recording only), for `gap_lengths`
            inp.horses[i].wiz = old_wiz;
            if mechanics != 0 {
                inp.horses[i].wiz_alt = Some(WizAlt { wiz: new_wiz, mechanics });
            }
            run_drawn(race, &inp)
        };
        let a = run(0);
        masks
            .iter()
            .map(|&m| {
                let b = run(m);
                WitPair { gain: gap_lengths(&b.out.frames, &a.out.frames, i, a.goal), new: sample(&b.horses[i]), old: sample(&a.horses[i]) }
            })
            .collect::<Vec<_>>()
    });
    let column = |k: usize| res.iter().map(|r| r.1[k]).collect();
    Wit { rows: (0..wiz_mechanic::COUNT).map(column).collect(), all: column(wiz_mechanic::COUNT) }
}

/// `mechanic` bits in order, as the Other Mechanics tab names them.
pub const MECHANICS: [&str; mechanic::COUNT] = ["Power conserve"];

/// Horse `i` of `input` on `seeds` as set up, against the same with each `mechanic` muted: `rows[k]` is bit `1 << k`'s
/// pair per seed (new = as set up, old = muted, so + = what the mechanic gained).
pub fn mechanics(race: &Race, input: &RaceInput, rivals: Option<&Rivals>, i: usize, seeds: &[i32], w: &Work) -> Vec<Vec<WitPair>> {
    let res = par_map(seeds.len(), w, |s| {
        let run = |muted: u8| {
            let mut inp = at_seed(input, seeds[s], rivals);
            inp.horses[i].viewer_id = 1; // dense frames near the goal (recording only), for `gap_lengths`
            inp.horses[i].muted_mechanics = muted;
            run_drawn(race, &inp)
        };
        let a = run(0);
        (0..mechanic::COUNT)
            .map(|k| {
                let b = run(1 << k);
                WitPair { gain: gap_lengths(&a.out.frames, &b.out.frames, i, b.goal), new: sample(&a.horses[i]), old: sample(&b.horses[i]) }
            })
            .collect::<Vec<_>>()
    });
    (0..mechanic::COUNT).map(|k| res.iter().map(|r| r.1[k]).collect()).collect()
}
