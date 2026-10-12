use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};

use uma_race_sim::api::RaceInput;

use super::{Activation, HorseRun, Race, run_once};

/// `n` seeds: `s` first, then deterministic pseudo-random ones derived from it.
pub fn seeds(s: i32, n: usize) -> Vec<i32> {
    let mut x = s as u32 as u64;
    let mut next = move || {
        // splitmix64, top 31 bits (non-negative like the game's seeds).
        x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = x;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 33) as i32
    };
    std::iter::once(s).chain(std::iter::repeat_with(&mut next)).take(n).collect()
}

/// Mean, nearest-rank percentiles, min, max. Empty → all 0.
pub(super) fn dist(mut v: Vec<f32>) -> Dist {
    if v.is_empty() {
        return Dist::default();
    }
    v.sort_by(f32::total_cmp);
    let q = |p: f32| v[((v.len() - 1) as f32 * p).round() as usize];
    Dist {
        mean: v.iter().sum::<f32>() / v.len() as f32,
        p10: q(0.1),
        p25: q(0.25),
        p50: q(0.5),
        p75: q(0.75),
        p90: q(0.9),
        min: v[0],
        max: v[v.len() - 1],
    }
}

/// Progress and cancellation of a long job, shared with the UI thread.
#[derive(Default)]
pub struct Work {
    /// Finished items (runs or paired samples), for a progress bar.
    pub done: AtomicUsize,
    /// Set to stop early; the job then returns only what finished.
    pub cancel: AtomicBool,
}

/// `f(0..n)` on all cores, `(i, f(i))` in index order; bumps `w.done` after each item and stops taking new ones once
/// `w.cancel` is set (then some indices are missing). A panic in `f` is re-raised.
pub(super) fn par_map<T: Send>(n: usize, w: &Work, f: impl Fn(usize) -> T + Sync) -> Vec<(usize, T)> {
    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(1, |t| t.get()).min(n).max(1);
    let mut out: Vec<(usize, T)> = std::thread::scope(|s| {
        let work = || {
            let mut v = Vec::new();
            loop {
                let i = next.fetch_add(1, Relaxed);
                if i >= n || w.cancel.load(Relaxed) {
                    return v;
                }
                v.push((i, f(i)));
                w.done.fetch_add(1, Relaxed);
            }
        };
        let hs: Vec<_> = (0..threads).map(|_| s.spawn(work)).collect();
        hs.into_iter().flat_map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e))).collect()
    });
    out.sort_by_key(|x| x.0);
    out
}

/// Summary statistic over runs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dist {
    pub mean: f32,
    pub p10: f32,
    pub p25: f32,
    pub p50: f32,
    pub p75: f32,
    pub p90: f32,
    pub min: f32,
    pub max: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkillStat {
    pub skill_id: i32,
    /// Fraction of runs it activated at least once.
    pub rate: f32,
    /// Activation distances over all runs (for a strip/histogram plot).
    pub distances: Vec<f32>,
    /// Fraction of runs per `SkillOutcome::kind` (activated, wit check, can't here, conditions not met).
    pub outcomes: [f32; 4],
}

/// Per horse aggregate over a batch.
#[derive(Clone, Debug, PartialEq)]
pub struct HorseStats {
    pub win_rate: f32,
    /// place_hist[k] = fraction of runs finishing (k+1)th.
    pub place_hist: Vec<f32>,
    pub place: Dist,
    pub finish_time: Dist,
    /// 101 entries (0..=100 % of course): placement distribution at that point.
    pub place_by_progress: Vec<Dist>,
    pub full_spurt_rate: f32,
    pub hp_out_rate: f32,
    /// Over runs that spurted.
    pub spurt_start: Dist,
    pub hp_at_goal: Dist,
    pub skills: Vec<SkillStat>,
    /// Per seed (same order as `Batch::seeds`), for paired diffs.
    pub times: Vec<f32>,
    pub places: Vec<u8>,
    /// Speed at the goal (m/s) per seed: turns paired time differences into lengths (`diff_batches`).
    pub goal_speeds: Vec<f32>,
    /// Fractions of runs per `HorseRun::spurt_kind` (full, delayed, reduced, delayed + reduced, none).
    pub spurt_kinds: [f32; 5],
    /// spurt_start − final-leg start (m), per seed that spurted.
    pub spurt_delays: Vec<f32>,
    /// `HorseRun::hp_out_at`, per seed that ran out of HP.
    pub hp_out_distances: Vec<f32>,
}

#[derive(Clone)]
pub struct Batch {
    pub seeds: Vec<i32>,
    pub horses: Vec<HorseStats>,
}

/// Fractions of `runs` per `SkillOutcome::kind` for skill `id`. A disabled skill's conditions are never checked, so a
/// lost wit roll hides "can't here"; fixed conditions are the same every seed, so then those runs can't either.
fn outcomes(runs: &[&HorseRun], id: i32) -> [f32; 4] {
    let k = runs.len().max(1) as f32;
    let mut o: [f32; 4] = std::array::from_fn(|kind| runs.iter().filter(|x| x.skills.iter().any(|c| c.skill_id == id && c.outcome.kind() == kind)).count() as f32 / k);
    if o[2] > 0.0 {
        (o[2], o[1]) = (o[2] + o[1], 0.0);
    }
    o
}

/// Runs `input` once per seed on all cores, counting finished runs in `w.done`. Once `w.cancel` is set it stops early
/// and the result covers only the finished seeds (callers discard it).
pub fn run_batch(race: &Race, input: &RaceInput, seeds: &[i32], w: &Work) -> Batch {
    // Keep only the summaries: full frames for thousands of runs would be ~100 MB.
    let runs: Vec<_> = par_map(seeds.len(), w, |i| {
        let r = run_once(race, &RaceInput { random_seed: seeds[i], ..input.clone() });
        (r.horses, r.activations)
    })
    .into_iter()
    .map(|x| x.1)
    .collect();
    let k = runs.len().max(1) as f32;
    let n = input.horses.len();
    let horses = (0..n)
        .map(|h| {
            let hr: Vec<&HorseRun> = runs.iter().map(|r| &r.0[h]).collect();
            let rate = |f: &dyn Fn(&HorseRun) -> bool| hr.iter().filter(|x| f(x)).count() as f32 / k;
            let fired = |a: &Activation, id: i32| a.horse == h && a.skill_id == id;
            // The horse's skills in input order, then anything else it activated.
            let mut ids: Vec<i32> = input.horses[h].skills.iter().map(|s| s.skill_id).collect();
            for a in runs.iter().flat_map(|r| &r.1).filter(|a| a.horse == h) {
                if !ids.contains(&a.skill_id) {
                    ids.push(a.skill_id);
                }
            }
            HorseStats {
                win_rate: rate(&|x| x.place == 1),
                place_hist: (1..=n).map(|p| rate(&|x| x.place == p)).collect(),
                place: dist(hr.iter().map(|x| x.place as f32).collect()),
                finish_time: dist(hr.iter().map(|x| x.finish_time).collect()),
                place_by_progress: (0..=100).map(|p| dist(hr.iter().map(|x| x.place_by_progress[p] as f32).collect())).collect(),
                full_spurt_rate: rate(&|x| x.full_spurt),
                hp_out_rate: rate(&|x| x.hp_out),
                spurt_start: dist(hr.iter().map(|x| x.spurt_start).filter(|&d| d >= 0.0).collect()),
                hp_at_goal: dist(hr.iter().map(|x| x.hp_at_goal).collect()),
                skills: ids
                    .into_iter()
                    .map(|id| SkillStat {
                        skill_id: id,
                        rate: runs.iter().filter(|r| r.1.iter().any(|a| fired(a, id))).count() as f32 / k,
                        distances: runs.iter().flat_map(|r| &r.1).filter(|a| fired(a, id)).map(|a| a.distance).collect(),
                        outcomes: outcomes(&hr, id),
                    })
                    .collect(),
                times: hr.iter().map(|x| x.finish_time).collect(),
                places: hr.iter().map(|x| x.place as u8).collect(),
                goal_speeds: hr.iter().map(|x| x.goal_speed).collect(),
                spurt_kinds: std::array::from_fn(|kind| rate(&|x| x.spurt_kind as usize == kind)),
                spurt_delays: hr.iter().filter(|x| x.spurt_kind < 4).map(|x| x.spurt_delay).collect(),
                hp_out_distances: hr.iter().filter_map(|x| x.hp_out_at).collect(),
            }
        })
        .collect();
    Batch { seeds: seeds.to_vec(), horses }
}
