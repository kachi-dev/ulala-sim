use uma_race_sim::api::{RaceInput, WizAlt, span_kind, wiz_mechanic};

use super::{HorseRun, Race, Work, gap_lengths, par_map, run_once};

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
    /// Lengths the race as set up finished ahead of this one (+ = the Wit change gained); 0 for the race as set up.
    pub lengths: f32,
    pub full_spurt: bool,
    pub rushed: bool,
    /// Skills activated.
    pub skills: u8,
    /// Course distance (m) where the last spurt started; -1 = none.
    pub spurt_start: f32,
    /// Seconds in each of `WIT_MODES`.
    pub modes: [f32; WIT_MODES.len()],
}

/// The Wit tab's measurement of one horse.
#[derive(Clone, Debug, PartialEq)]
pub struct Wit {
    /// Per seed, the race as set up.
    pub cur: Vec<WitSample>,
    /// Per `wiz_mechanic` bit, then all of them at once: per seed, that mechanic on the old Wit.
    pub alt: Vec<Vec<WitSample>>,
}

fn sample(h: &HorseRun, lengths: f32) -> WitSample {
    WitSample {
        lengths,
        full_spurt: h.full_spurt,
        rushed: h.state_time[span_kind::RUSHED as usize] > 0.0,
        skills: h.skills.iter().filter(|c| c.outcome.kind() == 0).count() as u8,
        spurt_start: h.spurt_start,
        modes: WIT_MODES.map(|(k, _)| h.state_time[k as usize]),
    }
}

/// Horse `i` of `input` on `seeds`, as set up and with each Wit mechanic alone (then all) reading `old_wiz` instead: a
/// pair differs only in what that mechanic does with the Wit change (plus the knock-on once the races diverge).
pub fn wit(race: &Race, input: &RaceInput, i: usize, old_wiz: i32, seeds: &[i32], w: &Work) -> Wit {
    let masks: Vec<u8> = (0..wiz_mechanic::COUNT).map(|k| 1 << k).chain([wiz_mechanic::ALL]).collect();
    let res = par_map(seeds.len(), w, |s| {
        let run = |mechanics: u8| {
            let mut inp = RaceInput { random_seed: seeds[s], ..input.clone() };
            inp.horses[i].viewer_id = 1; // dense frames near the goal (recording only), for `gap_lengths`
            if mechanics != 0 {
                inp.horses[i].wiz_alt = Some(WizAlt { wiz: old_wiz, mechanics });
            }
            run_once(race, &inp)
        };
        let a = run(0);
        let alt: Vec<WitSample> = masks
            .iter()
            .map(|&m| {
                let b = run(m);
                sample(&b.horses[i], gap_lengths(&a.out.frames, &b.out.frames, i, a.goal))
            })
            .collect();
        (sample(&a.horses[i], 0.0), alt)
    });
    Wit {
        cur: res.iter().map(|r| r.1.0).collect(),
        alt: (0..masks.len()).map(|k| res.iter().map(|r| r.1.1[k]).collect()).collect(),
    }
}
