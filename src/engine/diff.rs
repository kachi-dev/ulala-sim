

use super::{Batch, Dist, Run, dist, gap_lengths};

/// Per horse change between two single runs (baseline → edited).
#[derive(Clone, Debug, PartialEq)]
pub struct RunDiff {
    pub place: (usize, usize),
    pub finish_time: (f32, f32),
    pub spurt_start: (f32, f32),
    pub full_spurt: (bool, bool),
    pub hp_at_goal: (f32, f32),
    /// Skills that activated only in the baseline / only in the edited run.
    pub skills_lost: Vec<i32>,
    pub skills_gained: Vec<i32>,
    /// Edited run's gain in lengths (2.5 m) at the moment the faster of the two crosses the goal; + = edited ahead.
    pub lengths: f32,
}

pub fn diff_runs(a: &Run, b: &Run) -> Vec<RunDiff> {
    let ids = |r: &Run, h: usize| {
        let mut v: Vec<i32> = r.activations.iter().filter(|x| x.horse == h).map(|x| x.skill_id).collect();
        v.sort();
        v.dedup();
        v
    };
    let minus = |x: &[i32], y: &[i32]| x.iter().filter(|i| !y.contains(i)).copied().collect();
    a.horses
        .iter()
        .zip(&b.horses)
        .enumerate()
        .map(|(h, (x, y))| {
            let (ia, ib) = (ids(a, h), ids(b, h));
            RunDiff {
                place: (x.place, y.place),
                finish_time: (x.finish_time, y.finish_time),
                spurt_start: (x.spurt_start, y.spurt_start),
                full_spurt: (x.full_spurt, y.full_spurt),
                hp_at_goal: (x.hp_at_goal, y.hp_at_goal),
                skills_lost: minus(&ia, &ib),
                skills_gained: minus(&ib, &ia),
                lengths: gap_lengths(&b.out.frames, &a.out.frames, h, a.goal),
            }
        })
        .collect()
}

/// Per horse change between two batches over the same seeds (paired).
#[derive(Clone, Debug, PartialEq)]
pub struct BatchDiff {
    pub win_rate: (f32, f32),
    pub mean_place: (f32, f32),
    /// Per seed (edited − baseline) finish time; negative = faster.
    pub time_delta: Dist,
    pub full_spurt_rate: (f32, f32),
    /// Per seed edited gain in lengths (2.5 m) when the faster run finishes; + = edited ahead.
    pub lengths: Dist,
    pub length_deltas: Vec<f32>,
}

pub fn diff_batches(a: &Batch, b: &Batch) -> Vec<BatchDiff> {
    a.horses
        .iter()
        .zip(&b.horses)
        .map(|(x, y)| {
            let d: Vec<f32> = y.times.iter().zip(&x.times).map(|(tb, ta)| tb - ta).collect();
            // The slower run's remaining distance when the faster one finishes = time gap x its goal speed (speed is
            // ~constant over that fraction of a second; within cm of `gap_lengths` on the frames, which batches drop).
            let l: Vec<f32> = d
                .iter()
                .enumerate()
                .map(|(s, &dt)| -dt * if dt > 0.0 { y.goal_speeds[s] } else { x.goal_speeds[s] } / 2.5)
                .collect();
            BatchDiff {
                lengths: dist(l.clone()),
                length_deltas: l,
                win_rate: (x.win_rate, y.win_rate),
                mean_place: (x.place.mean, y.place.mean),
                time_delta: dist(d),
                full_spurt_rate: (x.full_spurt_rate, y.full_spurt_rate),
            }
        })
        .collect()
}
