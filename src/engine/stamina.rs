use uma_race_sim::api::RaceInput;

use super::{Activation, HorseRun, HorseStats, Race, Run, SpurtKind, Work, ability_type, distance_at, gap_lengths, par_map, run_once, summarize, target_type};

/// Raw stamina of "max stamina" (2000 after the sim halves the part over 1200, the game's cap).
pub const MAX_STAMINA: i32 = 2800;

/// One stamina debuff another horse in the field owns, measured on vs muted for the horse.
#[derive(Clone, Debug, PartialEq)]
pub struct DebuffStat {
    pub owner: usize,
    pub skill_id: i32,
    /// Fraction of seeds it activated.
    pub rate: f32,
    /// Fraction of seeds it hit the horse.
    pub hit_rate: f32,
    /// Mean HP it took when it hit.
    pub hp_lost: f32,
    /// Full-spurt rate over the seeds it hit: (on, muted).
    pub full_hit: (f32, f32),
    /// Full-spurt rate over all seeds: (on, muted on the seeds it hit).
    pub full: (f32, f32),
    /// Ran-out-of-HP rate over the seeds it hit: (on, muted).
    pub out_hit: (f32, f32),
    /// Ran-out-of-HP rate over all seeds: (on, muted on the seeds it hit).
    pub out: (f32, f32),
    /// The horse's distance when hit, per hit.
    pub hit_at: Vec<f32>,
}

/// The Stamina tab's measurement of one horse.
#[derive(Clone, Debug, PartialEq)]
pub struct Stamina {
    pub stats: HorseStats,
    /// Per seed: lengths vs the same horse at `MAX_STAMINA` (negative = lost) and its `spurt_kind`.
    pub loss: Vec<(f32, SpurtKind)>,
    /// Full-spurt rate at `MAX_STAMINA`.
    pub full_at_max: f32,
    /// Stamina debuffs of the rest of the field, most HP taken first.
    pub debuffs: Vec<DebuffStat>,
    /// Full-spurt and ran-out-of-HP rates with every debuff muted (None without debuffs).
    pub all_muted: Option<(f32, f32)>,
}

/// The skill drains other horses' HP: an HP ability with a negative value, not targeting the owner.
pub fn drains_hp(race: &Race, id: i32) -> bool {
    let a = race.data.skills.get(&id).into_iter().flat_map(|s| &s.details).flat_map(|d| &d.abilities);
    a.into_iter().any(|a| a.ability_type == ability_type::HP && a.float_ability_value < 0 && a.target_type != target_type::SELF)
}

/// Horse `i` of `input` on `seeds`: its batch stats; the same seeds with its stamina at `MAX_STAMINA` (lengths lost
/// to stamina); and each HP-draining skill another horse owns muted, then all of them (what they cost it). Muting
/// keeps the RNG draws and events, so a pair differs only by the effect. `with_debuffs` false skips the debuffs.
pub fn stamina(race: &Race, input: &RaceInput, i: usize, seeds: &[i32], with_debuffs: bool, w: &Work) -> Stamina {
    let debuffs: Vec<(usize, i32)> = input
        .horses
        .iter()
        .enumerate()
        .filter(|&(o, _)| with_debuffs && o != i)
        .flat_map(|(o, h)| h.skills.iter().filter(|k| drains_hp(race, k.skill_id)).map(move |k| (o, k.skill_id)))
        .collect();
    let run = |seed: i32, edit: &dyn Fn(&mut RaceInput)| {
        let mut inp = RaceInput { random_seed: seed, ..input.clone() };
        inp.horses[i].viewer_id = 1; // dense frames near the goal (recording only), for `gap_lengths`
        edit(&mut inp);
        run_once(race, &inp)
    };
    let mute = |inp: &mut RaceInput, &(o, id): &(usize, i32)| inp.horses[o].muted_skills.push(id);
    // Debuff (o, id) in `a`: whether it activated, and when it first hit horse i.
    let hit = |a: &Run, &(o, id): &(usize, i32)| {
        let mut acts = a.activations.iter().filter(|x| x.horse == o && x.skill_id == id);
        let fired = acts.clone().next().is_some();
        (fired, acts.find(|x| x.targets >> i & 1 == 1).map(|x| x.time))
    };
    let outcome = |r: &Run| Outcome { full: r.horses[i].full_spurt, out: r.horses[i].hp_out };
    let any = !debuffs.is_empty();
    let res: Vec<SeedResult> = par_map(seeds.len(), w, |s| {
        let a = run(seeds[s], &|_| {});
        let max = run(seeds[s], &|inp| inp.horses[i].stamina = MAX_STAMINA);
        let per: Vec<DebuffSeed> = debuffs
            .iter()
            .map(|d| {
                let (fired, t) = hit(&a, d);
                let m = run(seeds[s], &|inp| mute(inp, d));
                // Both runs are identical up to the hit: the HP gap at the next frame is what it took.
                let f = t.and_then(|t| a.out.frames.iter().position(|f| f.time > t));
                let hp_lost = f.and_then(|k| Some(m.out.frames.get(k)?.horses[i].hp - a.out.frames[k].horses[i].hp)).unwrap_or(0.0);
                DebuffSeed { fired, hit_at: t.map(|t| distance_at(&a.out.frames, i, t)), hp_lost, muted: outcome(&m) }
            })
            .collect();
        let all_muted = any.then(|| outcome(&run(seeds[s], &|inp| debuffs.iter().for_each(|d| mute(inp, d)))));
        SeedResult {
            loss: (gap_lengths(&a.out.frames, &max.out.frames, i, a.goal), a.horses[i].spurt_kind),
            as_is: outcome(&a),
            full_at_max: max.horses[i].full_spurt,
            debuffs: per,
            all_muted,
            runs: (a.horses, a.activations),
        }
    })
    .into_iter()
    .map(|x| x.1)
    .collect();
    let n = res.len().max(1) as f32;
    let runs: Vec<_> = res.iter().map(|r| r.runs.clone()).collect();
    let stats = summarize(input, &seeds[..res.len()], &runs).horses.swap_remove(i);
    let full = |s: usize| res[s].as_is.full;
    let out = |s: usize| res[s].as_is.out;
    let rate = |f: &dyn Fn(usize) -> bool| (0..res.len()).filter(|&s| f(s)).count() as f32 / n;
    let mut debuffs: Vec<DebuffStat> = debuffs
        .iter()
        .enumerate()
        .map(|(k, &(owner, skill_id))| {
            let p = |s: usize| &res[s].debuffs[k];
            let hits: Vec<usize> = (0..res.len()).filter(|&s| p(s).hit_at.is_some()).collect();
            let over_hits = |f: &dyn Fn(usize) -> bool| hits.iter().filter(|&&s| f(s)).count() as f32 / hits.len().max(1) as f32;
            DebuffStat {
                owner,
                skill_id,
                rate: rate(&|s| p(s).fired),
                hit_rate: hits.len() as f32 / n,
                hp_lost: hits.iter().map(|&s| p(s).hp_lost).sum::<f32>() / hits.len().max(1) as f32,
                full_hit: (over_hits(&full), over_hits(&|s| p(s).muted.full)),
                // Muted, on the seeds it hit: elsewhere it can't have taken the horse's HP, and the difference there is
                // knock-on noise (its effect on the others, then diverging RNG).
                full: (rate(&full), rate(&|s| if p(s).hit_at.is_some() { p(s).muted.full } else { full(s) })),
                out_hit: (over_hits(&out), over_hits(&|s| p(s).muted.out)),
                out: (rate(&out), rate(&|s| if p(s).hit_at.is_some() { p(s).muted.out } else { out(s) })),
                hit_at: hits.iter().filter_map(|&s| p(s).hit_at).collect(),
            }
        })
        .collect();
    debuffs.sort_by(|a, b| (b.hp_lost * b.hit_rate).total_cmp(&(a.hp_lost * a.hit_rate)));
    Stamina {
        stats,
        loss: res.iter().map(|r| r.loss).collect(),
        full_at_max: rate(&|s| res[s].full_at_max),
        debuffs,
        // Likewise only on the seeds some debuff hit.
        all_muted: any.then(|| {
            let hit_any = |s: usize| res[s].debuffs.iter().any(|p| p.hit_at.is_some());
            let m = |s: usize| res[s].all_muted.filter(|_| hit_any(s)).unwrap_or(res[s].as_is);
            (rate(&|s| m(s).full), rate(&|s| m(s).out))
        }),
    }
}

/// The measured horse's full spurt and ran out of HP in one race.
#[derive(Clone, Copy)]
struct Outcome {
    full: bool,
    out: bool,
}

/// One seed of `stamina`.
struct SeedResult {
    /// Every horse's summary and the activations, as is.
    runs: (Vec<HorseRun>, Vec<Activation>),
    as_is: Outcome,
    /// Lengths vs the same horse at `MAX_STAMINA` (negative = lost) and its spurt.
    loss: (f32, SpurtKind),
    full_at_max: bool,
    debuffs: Vec<DebuffSeed>,
    /// With every debuff muted (None without debuffs).
    all_muted: Option<Outcome>,
}

/// One debuff on one seed.
struct DebuffSeed {
    fired: bool,
    /// The horse's distance when it first hit it.
    hit_at: Option<f32>,
    /// HP it took then.
    hp_lost: f32,
    /// With it muted.
    muted: Outcome,
}
