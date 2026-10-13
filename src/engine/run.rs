use uma_race_sim::api::{Frame, HorseFrame, RaceInput, SimOutput, event_kind, span_kind};
use uma_race_sim::sim::{defs::last_spurt_calc_result, race as sim_race, record, tick};

use super::{Race, SkillCheck, SkillOutcome, checks::skill_checks};

/// One skill activation in a run.
#[derive(Clone, Debug, PartialEq)]
pub struct Activation {
    pub horse: usize,
    pub skill_id: i32,
    pub time: f32,
    /// Activating horse's distance at that time (interpolated from frames).
    pub distance: f32,
    /// Horses it targeted: bit `g` = horse (gate) `g`.
    pub targets: u32,
}

/// How a last spurt went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpurtKind {
    /// Max spurt speed from the final-leg start.
    Full,
    /// Max speed, but started after the final-leg start.
    Delayed,
    /// Started on time, but below max speed.
    Reduced,
    DelayedReduced,
    /// Never switched to a last spurt.
    NoSpurt,
}

impl SpurtKind {
    /// Every kind, in the order `HorseStats::spurt_kinds` holds them.
    pub const ALL: [SpurtKind; 5] = [SpurtKind::Full, SpurtKind::Delayed, SpurtKind::Reduced, SpurtKind::DelayedReduced, SpurtKind::NoSpurt];

    pub fn delayed(self) -> bool {
        matches!(self, SpurtKind::Delayed | SpurtKind::DelayedReduced)
    }

    pub fn reduced(self) -> bool {
        matches!(self, SpurtKind::Reduced | SpurtKind::DelayedReduced)
    }
}

/// Per horse summary of one run.
#[derive(Clone, Debug, PartialEq)]
pub struct HorseRun {
    /// 1-based.
    pub place: usize,
    pub finish_time: f32,
    /// Seconds behind the winner.
    pub behind: f32,
    /// -1 = no spurt.
    pub spurt_start: f32,
    /// spurt_start − final-leg start (m); meaningless without a spurt (`spurt_kind` 4).
    pub spurt_delay: f32,
    pub full_spurt: bool,
    pub hp_at_goal: f32,
    pub hp_out: bool,
    /// 0..=100 → placement (1-based) when this horse reached that % of the course.
    pub place_by_progress: Vec<u8>,
    /// 0..=100 → HP when this horse reached that % of the course.
    pub hp_by_progress: Vec<f32>,
    /// 0..=100 → speed (m/s) when this horse reached that % of the course.
    pub speed_by_progress: Vec<f32>,
    /// HP needed at the final-leg start for a full spurt (max spurt speed to the goal).
    pub spurt_need_hp: f32,
    /// HP when this horse reached the final-leg start.
    pub hp_at_final_leg: f32,
    /// Speed (m/s) when crossing the goal, interpolated from frames.
    pub goal_speed: f32,
    /// Lengths (2.5 m) behind the winner when the winner crossed the goal (exact from frames; 0 for the winner).
    pub behind_lengths: f32,
    /// Target speed the last-spurt calc chose (m/s).
    pub spurt_speed: f32,
    /// The calc's max spurt speed for this horse (m/s).
    pub max_spurt_speed: f32,
    /// `full_spurt` = `SpurtKind::Full`.
    pub spurt_kind: SpurtKind,
    /// Course distance where HP first hit 0 (None = never). See `hp_out_distance` for the accuracy.
    pub hp_out_at: Option<f32>,
    /// Each of its skills (input order): activated, or why not.
    pub skills: Vec<SkillCheck>,
    /// Seconds in each `span_kind` state (duel, spot struggle, rushed, blocked, conserve power).
    pub state_time: [f32; span_kind::COUNT],
    /// Those states as stretches of the course: (`span_kind`, from m, to m), in start order.
    pub spans: Vec<(u8, f32, f32)>,
}

/// One simulated race.
#[derive(Clone)]
pub struct Run {
    pub seed: i32,
    /// Course distance (m).
    pub goal: f32,
    pub out: SimOutput,
    pub horses: Vec<HorseRun>,
    pub activations: Vec<Activation>,
}

/// Simulates `input` once (its own `random_seed`).
pub fn run_once(race: &Race, input: &RaceInput) -> Run {
    let custom;
    let input = if race.replay.is_none() && input.horses.len() > 1 {
        custom = custom_popularity(input);
        &custom
    } else {
        input
    };
    // = sim::simulate, keeping the spurt / HP internals before the race is consumed.
    let mut r = sim_race::init(input, &race.data);
    tick::run(&mut r);
    let goal = r.info.phase_distances.course_distance;
    let final_leg = r.info.phase_distances.phase_end_start_distance;
    // HP stops changing once a horse finishes, so the final HP is the HP at the goal.
    let internals: Vec<(bool, f32, f32, f32, SpurtKind, f32)> = (0..r.horses.len())
        .map(|i| {
            let (max_v, need_hp) = max_spurt_speed(&mut r, i);
            let h = &r.horses[i];
            let ai = &h.ai;
            let kind = if ai.last_spurt_start_distance < 0.0 || ai.last_spurt_calc_result & last_spurt_calc_result::TRUE_FLAGS == 0 {
                SpurtKind::NoSpurt
            } else {
                // The calc starts a spurt on entering the final leg (within 2 steps < 3.5 m); TrueExceedNeedMaxHp sets
                // exactly max speed, True picks a (start, speed) candidate that may be both later and slower.
                let delayed = ai.last_spurt_start_distance > final_leg + SPURT_ON_TIME_M;
                let reduced = ai.last_spurt_calc_result != last_spurt_calc_result::TRUE_EXCEED_NEED_MAX_HP && ai.last_spurt_target_speed < max_v * (1.0 - 1e-5);
                match (delayed, reduced) {
                    (false, false) => SpurtKind::Full,
                    (true, false) => SpurtKind::Delayed,
                    (false, true) => SpurtKind::Reduced,
                    (true, true) => SpurtKind::DelayedReduced,
                }
            };
            (h.is_hp_empty_on_race, h.hp.max(0.0), ai.last_spurt_target_speed, max_v, kind, need_hp)
        })
        .collect();
    let mut checks: Vec<Vec<SkillCheck>> = (0..r.horses.len()).map(|i| skill_checks(&r, i)).collect();
    let out = record::into_output(r);
    let n = out.results.len();
    let frames = &out.frames;
    let fin: Vec<i32> = out.results.iter().map(|r| r.finish_order).collect();
    let winner = fin.iter().position(|&o| o == 0).unwrap_or(0);
    let winner_time = out.results.get(winner).map_or(0.0, |r| r.finish_time_raw);
    // t[h][p]: time horse h reached p% of the course (t[h][100]: the goal; course distances are whole metres, so
    // `goal * 100 / 100` is exact). Ranking by distance at h's time = ranking by these times.
    let t: Vec<Vec<f32>> = (0..n).map(|h| (0..=100).map(|p| cross_time(frames, h, goal * p as f32 / 100.0)).collect()).collect();
    let ahead = |j: usize, h: usize, p: usize| {
        if p == 100 {
            return fin[j] < fin[h];
        }
        if t[j][p] != t[h][p] {
            return t[j][p] < t[h][p];
        }
        // Tie (the start): whoever is in front at the first later frame where they differ.
        frames.iter().filter(|f| f.time >= t[h][p]).map(|f| (f.horses[j].distance, f.horses[h].distance)).find(|(a, b)| a != b).map_or(j < h, |(a, b)| a > b)
    };
    let activations: Vec<Activation> = out
        .events
        .iter()
        // Skill event params: [horse, skill id, .., .., target flags].
        .filter(|e| e.kind == event_kind::SKILL && e.params.len() >= 2 && (e.params[0] as usize) < n)
        .map(|e| {
            let horse = e.params[0] as usize;
            let targets = e.params.get(4).map_or(0, |&t| t as u32);
            Activation { horse, skill_id: e.params[1], time: e.time, distance: distance_at(frames, horse, e.time), targets }
        })
        .collect();
    for a in &activations {
        for c in checks[a.horse].iter_mut().filter(|c| c.skill_id == a.skill_id) {
            c.outcome = SkillOutcome::Activated;
        }
    }
    let mut state_time = vec![[0f32; span_kind::COUNT]; n];
    for s in &out.spans {
        state_time[s.horse][s.kind as usize] += s.end - s.start;
    }
    let horses = (0..n)
        .map(|h| {
            let res = &out.results[h];
            let (hp_out, hp_at_goal, spurt_speed, max_spurt_speed, spurt_kind, spurt_need_hp) = internals[h];
            HorseRun {
                place: res.finish_order as usize + 1,
                finish_time: res.finish_time_raw,
                behind: res.finish_time_raw - winner_time,
                spurt_start: res.last_spurt_start_distance,
                spurt_delay: res.last_spurt_start_distance - final_leg,
                full_spurt: spurt_kind == SpurtKind::Full,
                hp_at_goal,
                hp_out,
                place_by_progress: (0..=100).map(|p| 1 + (0..n).filter(|&j| j != h && ahead(j, h, p)).count() as u8).collect(),
                // At the goal: the exact final HP (the frame after it is past the finish, where HP stops dropping).
                hp_by_progress: (0..=100).map(|p| if p == 100 { hp_at_goal } else { frame_at(frames, h, t[h][p], |f| f.hp).max(0.0) }).collect(),
                speed_by_progress: (0..=100).map(|p| frame_at(frames, h, t[h][p], |f| f.speed)).collect(),
                spurt_need_hp,
                hp_at_final_leg: frame_at(frames, h, cross_time(frames, h, final_leg), |f| f.hp).max(0.0),
                goal_speed: frame_at(frames, h, t[h][100], |f| f.speed),
                behind_lengths: if h == winner { 0.0 } else { ((goal - distance_at(frames, h, t[winner][100])) / 2.5).max(0.0) },
                spurt_speed,
                max_spurt_speed,
                spurt_kind,
                hp_out_at: if hp_out { hp_out_distance(frames, h) } else { None },
                skills: std::mem::take(&mut checks[h]),
                state_time: state_time[h],
                spans: out.spans.iter().filter(|s| s.horse == h).map(|s| (s.kind, distance_at(frames, h, s.start), distance_at(frames, h, s.end))).collect(),
            }
        })
        .collect();
    Run { seed: input.random_seed, goal, out, horses, activations }
}

/// A spurt starting within this many metres after the final-leg start is on time (the calc runs on entering the final
/// leg; two 1/15 s steps at < 26 m/s are < 3.5 m).
pub const SPURT_ON_TIME_M: f32 = 3.5;

/// Distance where horse `h`'s HP first hit 0. The sim clamps HP at 0 and records no distance for it, so: the last frame
/// with HP > 0, extended at the HP-per-metre rate of the frame interval before it, capped at the first 0 frame.
/// Accurate to ~1 m while consumption is steady over those ~2 s (a recovery skill or speed change in that window
/// shifts it, at most to the 0 frame, ≤ one frame interval).
fn hp_out_distance(frames: &[Frame], h: usize) -> Option<f32> {
    let k = frames.iter().position(|f| f.horses[h].hp <= 0.0)?;
    let at = |i: usize| (frames[i].horses[h].distance, frames[i].horses[h].hp);
    let (dk, _) = at(k);
    let Some(k1) = k.checked_sub(1) else { return Some(dk) };
    let (d1, hp1) = at(k1);
    let per_m = match k1.checked_sub(1).map(at) {
        Some((d2, hp2)) if hp2 > hp1 && d1 > d2 => (hp2 - hp1) / (d1 - d2),
        _ => return Some(dk),
    };
    Some((d1 + hp1 / per_m).min(dk))
}

/// The last-spurt calc's max speed for horse `i`: the last-spurt calculation on its phase-End params with unlimited HP
/// (the TrueExceedNeedMaxHp path: max speed, no RNG draw), as compare.rs's need_hp_per_m does. Also the HP that path
/// needs at the final-leg start (the calc's own test: max speed over the remaining distance less the goal buffer).
// ponytail: run on the finished race, so stats are the race-end ones (a temporary stat buff active at the final-leg
// calc is missed); exposing the calc's max_v from the sim would make it exact.
pub(super) fn max_spurt_speed(r: &mut sim_race::Race, i: usize) -> (f32, f32) {
    use uma_race_sim::sim::defs::Phase;
    use uma_race_sim::sim::physics::{hp, last_spurt, speed, stats};
    let h = &r.horses[i];
    let p = last_spurt::LastSpurtCalcParam {
        cur_distance: r.info.phase_distances.phase_end_start_distance,
        hp: f32::MAX,
        phase: Phase::End,
        speed: stats::speed(h),
        guts: stats::guts(h),
        wiz: stats::wiz(h),
        base_target_speed: speed::base_target_speed_without_random(r, i, Phase::End),
        ground_modifier_multi_hp_sub: h.ground_modifier_param.multi_hp_sub,
        proper_distance_coef: h.proper_distance_coef_speed,
        course_distance: r.info.race_course_set.distance,
        race_base_speed: r.info.base_speed,
    };
    let mut calc = last_spurt::new_last_spurt(r, r.info.base_speed);
    calc.calc_param = p;
    last_spurt::calc_last_spurt(r, &mut calc);
    let v = calc.last_spurt_target_speed;
    let remain = (p.course_distance as f32 - p.cur_distance - calc.last_spurt_distance_goal_buffer as f32).max(0.0);
    (v, hp::calc_need_hp(r, p.race_base_speed, p.ground_modifier_multi_hp_sub, Phase::End, p.guts, v, remain))
}

/// The game's margin label for a gap in lengths: Nose, Head, Neck, 1/2L, 3/4L, 1L, 1 1/4L, 1 1/2L, 1 3/4L, 2L,
/// 2 1/2L, 3L, 3 1/2L, 4L, 5L .. 10L, Large (RaceDefine.HorseLength).
// ponytail: the game buckets `diff time × 15 m/s` with RaceDefineStatic.HORSE_LENGTH_DISTANCE, whose values live in
// global-metadata (not in the sim repo); these are midpoints between the labels' nominal lengths, Nose/Head/Neck at
// the usual JRA ~0.1 / 0.2 / 0.35 lengths. Swap in the real table when it's extracted.
pub fn margin_label(lengths: f32) -> String {
    const T: &[(f32, &str)] = &[
        (0.1, "Nose"), (0.2, "Head"), (0.35, "Neck"), (0.625, "1/2L"), (0.875, "3/4L"), (1.125, "1L"), (1.375, "1 1/4L"),
        (1.625, "1 1/2L"), (1.875, "1 3/4L"), (2.25, "2L"), (2.75, "2 1/2L"), (3.25, "3L"), (3.75, "3 1/2L"), (4.5, "4L"),
        (5.5, "5L"), (6.5, "6L"), (7.5, "7L"), (8.5, "8L"), (9.5, "9L"), (10.5, "10L"),
    ];
    T.iter().find(|t| lengths <= t.0).map_or("Large", |t| t.1).to_string()
}

/// Custom fields: distinct popularity 1..=n by total stats, ties by gate (the favourite is roughly the strongest horse).
/// `new_horse`'s shared 1 would satisfy every popularity skill condition for everyone.
pub(super) fn custom_popularity(input: &RaceInput) -> RaceInput {
    let mut out = input.clone();
    let mut order: Vec<usize> = (0..input.horses.len()).collect();
    order.sort_by_key(|&i| {
        let h = &input.horses[i];
        -(h.speed + h.stamina + h.pow + h.guts + h.wiz)
    });
    for (p, &i) in order.iter().enumerate() {
        out.horses[i].popularity = p as i32 + 1;
    }
    out
}

/// Horse `h`'s distance at time `t`, linear between frames.
pub(super) fn distance_at(frames: &[Frame], h: usize, t: f32) -> f32 {
    frame_at(frames, h, t, |f| f.distance)
}

/// Horse `h`'s `field` at time `t`, linear between frames.
fn frame_at(frames: &[Frame], h: usize, t: f32, field: impl Fn(&HorseFrame) -> f32) -> f32 {
    let k = frames.partition_point(|f| f.time <= t);
    match (k.checked_sub(1).map(|i| &frames[i]), frames.get(k)) {
        (Some(a), Some(b)) => {
            let (va, vb) = (field(&a.horses[h]), field(&b.horses[h]));
            va + (vb - va) * (t - a.time) / (b.time - a.time)
        }
        (Some(f), None) | (None, Some(f)) => field(&f.horses[h]),
        (None, None) => 0.0,
    }
}

/// Paired gain of run `a` over run `b` for horse `h`, in lengths (2.5 m): `h`'s distance gap at the moment the faster
/// of the two crosses the goal. Positive = `a` ahead. (`skill_gains`, `diff_runs`; `diff_batches` approximates it.)
pub(super) fn gap_lengths(fa: &[Frame], fb: &[Frame], h: usize, goal: f32) -> f32 {
    let t = cross_time(fa, h, goal).min(cross_time(fb, h, goal));
    (distance_at(fa, h, t) - distance_at(fb, h, t)) / 2.5
}

/// Time horse `h` first reached distance `d`, linear between frames (distances never decrease); ∞ if never.
fn cross_time(frames: &[Frame], h: usize, d: f32) -> f32 {
    let k = frames.partition_point(|f| f.horses[h].distance < d);
    match (k.checked_sub(1).map(|i| &frames[i]), frames.get(k)) {
        (Some(a), Some(b)) => {
            let (da, db) = (a.horses[h].distance, b.horses[h].distance);
            a.time + (b.time - a.time) * (d - da) / (db - da)
        }
        (None, Some(b)) => b.time,
        _ => f32::INFINITY,
    }
}

/// `None` = bit-exact match with the server replay; else a one-line description of the first divergence.
/// Custom races (no replay): `Some("no server replay")`.
pub fn verify_against_server(race: &Race, run: &Run) -> Option<String> {
    let Some(rep) = &race.replay else { return Some("no server replay".into()) };
    uma_race_sim::replay::diff(&run.out, rep).map(|d| d.to_string())
}
