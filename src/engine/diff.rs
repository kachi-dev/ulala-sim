

use super::{Batch, Dist, Run, dist, gap_lengths_of};

/// Per horse change between two single runs (baseline → edited).
#[derive(Clone, Debug, PartialEq)]
pub struct RunDiff {
    pub place: (usize, usize),
    pub finish_time: (f32, f32),
    /// Skills that activated only in the baseline / only in the edited run.
    pub skills_lost: Vec<i32>,
    pub skills_gained: Vec<i32>,
    /// Edited run's gain in lengths (2.5 m) at the moment the faster of the two crosses the goal; + = edited ahead.
    pub lengths: f32,
}

/// Per horse of `b` (edited): its change from baseline `a`'s horse `pairs[h]` (None: no counterpart, no diff).
pub fn diff_runs(a: &Run, b: &Run, pairs: &[Option<usize>]) -> Vec<Option<RunDiff>> {
    let ids = |r: &Run, h: usize| {
        let mut v: Vec<i32> = r.activations.iter().filter(|x| x.horse == h).map(|x| x.skill_id).collect();
        v.sort();
        v.dedup();
        v
    };
    let minus = |x: &[i32], y: &[i32]| x.iter().filter(|i| !y.contains(i)).copied().collect();
    (0..b.horses.len())
        .map(|hb| {
            let ha = pairs.get(hb).copied().flatten().filter(|&ha| ha < a.horses.len())?;
            let (x, y) = (&a.horses[ha], &b.horses[hb]);
            let (ia, ib) = (ids(a, ha), ids(b, hb));
            Some(RunDiff {
                place: (x.place, y.place),
                finish_time: (x.finish_time, y.finish_time),
                skills_lost: minus(&ia, &ib),
                skills_gained: minus(&ib, &ia),
                lengths: gap_lengths_of(&b.out.frames, hb, &a.out.frames, ha, a.goal),
            })
        })
        .collect()
}

/// Per horse change between two batches over the same seeds (paired).
#[derive(Clone, Debug, PartialEq)]
pub struct BatchDiff {
    pub win_rate: (f32, f32),
    pub mean_place: (f32, f32),
    pub full_spurt_rate: (f32, f32),
    /// Per seed edited gain in lengths (2.5 m) when the faster run finishes; + = edited ahead.
    pub lengths: Dist,
    pub length_deltas: Vec<f32>,
}

/// Per horse of `b` (edited): its change from baseline `a`'s horse `pairs[h]` (None: no counterpart, no diff).
pub fn diff_batches(a: &Batch, b: &Batch, pairs: &[Option<usize>]) -> Vec<Option<BatchDiff>> {
    (0..b.horses.len())
        .map(|hb| {
            let x = &a.horses[pairs.get(hb).copied().flatten().filter(|&ha| ha < a.horses.len())?];
            let y = &b.horses[hb];
            let d: Vec<f32> = y.times.iter().zip(&x.times).map(|(tb, ta)| tb - ta).collect();
            // The slower run's remaining distance when the faster one finishes = time gap x its goal speed (speed is
            // ~constant over that fraction of a second; within cm of `gap_lengths` on the frames, which batches drop).
            let l: Vec<f32> = d
                .iter()
                .enumerate()
                .map(|(s, &dt)| -dt * if dt > 0.0 { y.goal_speeds[s] } else { x.goal_speeds[s] } / 2.5)
                .collect();
            Some(BatchDiff {
                lengths: dist(l.clone()),
                length_deltas: l,
                win_rate: (x.win_rate, y.win_rate),
                mean_place: (x.place.mean, y.place.mean),
                full_spurt_rate: (x.full_spurt_rate, y.full_spurt_rate),
            })
        })
        .collect()
}
