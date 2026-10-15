use uma_race_sim::api::{HorseInput, RaceInput, SkillInput};

use super::{Dist, GRADE_A, MAX_STAMINA, Race, Run, STYLES, Work, ability_type, cards, course_info, custom_popularity, default_level, dist, distance_type, gap_lengths, names, new_horse, par_map, resume, run_forking, run_once, seeds, set_card, sim_input};

/// Default NPC field size (a Champions Meeting field): the measured uma and 8 NPCs.
pub const FIELD_MIN: usize = 9;

/// The styles `LOBBIES` counts, in order: Runaway, then the running styles.
pub const LOBBY_STYLES: [&str; 5] = ["Runaway", STYLES[0], STYLES[1], STYLES[2], STYLES[3]];

/// NPC lobbies, back-heavy to front-heavy: name and `LOBBY_STYLES` counts of the 8 NPCs.
pub const LOBBIES: [(&str, [usize; 5]); 8] = [
    ("Back-heavy", [0, 1, 1, 2, 4]),
    ("Leaning back", [0, 1, 2, 2, 3]),
    ("Balanced", [0, 2, 2, 2, 2]),
    ("Leaning front", [0, 3, 2, 2, 1]),
    ("Front-heavy", [0, 3, 3, 1, 1]),
    ("1 Runaway", [1, 3, 2, 1, 1]),
    ("2 Runaways", [2, 2, 2, 1, 1]),
    ("3 Runaways", [3, 2, 2, 1, 0]),
];

/// The `LOBBY_STYLES` index of each of `n` NPCs: `LOBBIES[lobby]`'s counts round-robin (a top-up takes the first few,
/// so it stays mixed), starting over past 8.
pub fn lobby_styles(lobby: usize, n: usize) -> Vec<usize> {
    let mix = LOBBIES[lobby.min(LOBBIES.len() - 1)].1;
    let (mut out, mut left) = (Vec::new(), mix);
    while out.len() < n {
        if left.iter().all(|&k| k == 0) {
            left = mix;
        }
        for (k, l) in left.iter_mut().enumerate() {
            if *l > 0 && out.len() < n {
                *l -= 1;
                out.push(k);
            }
        }
    }
    out
}

/// The field to measure in, and the measured uma's index in it. `alone`: `input.horses[horse]` at the middle gate
/// among `size - 1` NPCs; else `input`'s field with NPCs after it up to `size`. NPCs: `lobby_styles`, each a costume of that default style and an unused chara, `new_horse`
/// stats and unique, stamina at the cap (raw 2800, 2000 after the halving) so they keep a full spurt, A aptitude for
/// style, distance and surface. Popularity is re-ranked when NPCs were added.
pub fn npc_field(race: &Race, input: &RaceInput, horse: usize, alone: bool, lobby: usize, size: usize) -> (RaceInput, usize) {
    let mut out = input.clone();
    let field = if alone { vec![out.horses.swap_remove(horse)] } else { std::mem::take(&mut out.horses) };
    let n_npc = if alone { size.saturating_sub(1) } else { size.saturating_sub(field.len()) };
    let c = course_info(&race.data, input.course_set_id);
    let dt = distance_type(c.distance);
    let runaway = race.data.skills.keys().copied().filter(|&id| sets_style(race, id)).min();
    let mut used: Vec<i32> = field.iter().map(|h| h.chara_id).collect();
    let mut made = Vec::new();
    for k in lobby_styles(lobby, n_npc) {
        let style = k.max(1) as i32; // Runaway runs Front
        let card = cards().iter().find(|x| {
            names().card.get(&x.card_id).is_some_and(|n| n.running_style == style) && !used.contains(&x.chara_id) && race.data.skills.contains_key(&x.unique_skill)
        });
        let Some(card) = card else { continue };
        used.push(card.chara_id);
        let mut h = new_horse(race, card.card_id);
        h.running_style = style;
        h.proper_running_style[style as usize - 1] = GRADE_A;
        h.proper_distance[dt] = GRADE_A;
        h.proper_ground[(c.surface - 1).clamp(0, 1) as usize] = GRADE_A;
        h.stamina = MAX_STAMINA;
        if let (0, Some(id)) = (k, runaway) {
            h.skills.push(SkillInput { skill_id: id, level: 1 });
        }
        h.viewer_id = 0;
        made.push(h);
    }
    let i = if alone { made.len() / 2 } else { horse };
    let added = !made.is_empty();
    out.horses = if alone { [&made[..i], &field[..], &made[i..]].concat() } else { [field, made].concat() };
    (if added { custom_popularity(&out) } else { out }, i)
}

/// Skill gain (umalator-style) of one candidate skill for one horse.
#[derive(Clone, Debug, PartialEq)]
pub struct SkillGain {
    pub skill_id: i32,
    /// Horse lengths (2.5 m) gained per seed; positive = faster.
    pub lengths: Dist,
    pub samples: Vec<f32>,
    /// Fraction of seeds it activated.
    pub rate: f32,
    /// Activation distances.
    pub distances: Vec<f32>,
    /// (first activation distance, gain) of each seed it activated on.
    pub by_distance: Vec<(f32, f32)>,
    /// The owned skill of the same group (white ↔ gold, ○ ↔ ◎) it was measured in place of.
    pub replaces: Option<i32>,
    /// With `replaces`: the candidate is the better version (higher rarity, then group rate: ◎ over ○ over ×).
    pub upgrade: bool,
}

/// For each candidate: `horse` with the skill (added if not owned) vs the skill muted, paired on `n` seeds, in
/// `input`'s field (see `npc_field`). `blank`: the horse's other skills removed in both runs (the skill alone vs no
/// skills). A candidate the horse doesn't own but owns another version of (same `group_id`) is measured in place of
/// it: the gain is the new version's over the old one's. Sorted by mean gain, best first. Candidates without all `n` samples (cancelled)
/// are dropped. Also each horse's full-spurt rate over all runs.
pub fn skill_gains(race: &Race, input: &RaceInput, horse: usize, candidates: &[i32], blank: bool, n: usize, w: &Work) -> (Vec<SkillGain>, Vec<f32>) {
    let seeds = seeds(input.random_seed, n);
    let owned = &input.horses[horse].skills;
    let group = |id: i32| race.data.skills.get(&id).map_or(0, |s| s.group_id);
    let rank = |id: i32| race.data.skills.get(&id).map(|s| (s.rarity, s.group_rate));
    let replaces: Vec<Option<i32>> = candidates
        .iter()
        .map(|&id| {
            let g = group(id);
            let other = owned.iter().find(|s| g != 0 && s.skill_id != id && group(s.skill_id) == g);
            other.filter(|_| !owned.iter().any(|s| s.skill_id == id)).map(|s| s.skill_id)
        })
        .collect();
    // Owned skills measured on the horse as is share their skill-on run per seed (`owned_gains`): those seeds go first
    // (the longest items), each counting as one item per skill in `w.done`.
    let (shared, single): (Vec<usize>, Vec<usize>) = (0..candidates.len()).partition(|&k| {
        let id = candidates[k];
        !blank && replaces[k].is_none() && owned.iter().any(|s| s.skill_id == id) && !sets_style(race, id)
    });
    let ids: Vec<i32> = shared.iter().map(|&k| candidates[k]).collect();
    let first = if shared.is_empty() { 0 } else { n };
    let res = par_map(first + single.len() * n, w, |j| {
        if j < first {
            w.done.fetch_add(shared.len() - 1, std::sync::atomic::Ordering::Relaxed);
            return shared.iter().zip(owned_gains(race, input, horse, seeds[j], &ids)).map(|(&k, p)| (k * n + j, p)).collect();
        }
        let (k, s) = (single[(j - first) / n], (j - first) % n);
        let id = candidates[k];
        let p = paired_gain(race, input, horse, seeds[s], id, replaces[k], |h| {
            if blank {
                h.skills.retain(|x| x.skill_id == id);
            }
        });
        vec![(k * n + s, p)]
    });
    let mut res: Vec<(usize, Pair)> = res.into_iter().flat_map(|x| x.1).collect();
    res.sort_by_key(|x| x.0);
    let (v, spurt) = tally(n, res);
    let v = v.into_iter().map(|(k, g)| SkillGain { skill_id: candidates[k], lengths: g.lengths, samples: g.samples, rate: g.rate, distances: g.distances, by_distance: g.by_distance, replaces: replaces[k], upgrade: replaces[k].is_some_and(|old| rank(candidates[k]) > rank(old)) });
    (v.collect(), spurt)
}

/// `measure`'s result for one item.
struct Gain {
    lengths: Dist,
    samples: Vec<f32>,
    rate: f32,
    distances: Vec<f32>,
    by_distance: Vec<(f32, f32)>,
}

/// One skill on vs muted on one seed (`paired_gain`).
struct Pair {
    /// Lengths gained.
    gain: f32,
    /// Where it activated (skill on).
    distances: Vec<f32>,
    /// Per horse: full spurts over the two races (0..=2).
    full_spurts: Vec<u8>,
}

/// `paired(item, seed)` for `items` × `n` seeds on `worker_threads()` → per item (index, `Gain`), best mean first, and
/// each horse's full-spurt rate over every finished pair. Items without all `n` seeds (cancelled) are dropped: a
/// ranking on a few samples would mislead.
fn measure(items: usize, n: usize, w: &Work, paired: impl Fn(usize, usize) -> Pair + Sync) -> (Vec<(usize, Gain)>, Vec<f32>) {
    tally(n, par_map(items * n, w, |i| paired(i / n, i % n)))
}

/// `measure` of its pairs: (item * n + seed, pair) in index order.
fn tally(n: usize, res: Vec<(usize, Pair)>) -> (Vec<(usize, Gain)>, Vec<f32>) {
    let horses = res.first().map_or(0, |r| r.1.full_spurts.len());
    let spurt = (0..horses).map(|h| res.iter().map(|r| r.1.full_spurts[h] as f32).sum::<f32>() / (2 * res.len()) as f32).collect();
    // `res` is in index order, so each item's seeds are adjacent.
    let mut v: Vec<(usize, Gain)> = res
        .chunk_by(|a, b| a.0 / n == b.0 / n)
        .filter(|c| c.len() == n)
        .map(|c| {
            let samples: Vec<f32> = c.iter().map(|r| r.1.gain).collect();
            let rate = c.iter().filter(|r| !r.1.distances.is_empty()).count() as f32 / n as f32;
            let distances = c.iter().flat_map(|r| r.1.distances.iter().copied()).collect();
            let by_distance = c.iter().filter_map(|r| Some((*r.1.distances.first()?, r.1.gain))).collect();
            (c[0].0 / n, Gain { lengths: dist(samples.clone()), samples, rate, distances, by_distance })
        })
        .collect();
    v.sort_by(|a, b| b.1.lengths.mean.total_cmp(&a.1.lengths.mean));
    (v, spurt)
}

/// Gain in lengths of skill `id` for horse `i` (after `setup`) on one seed: skill on vs muted in `input`'s field, plus
/// the skill's activation distances and each horse's full spurts over the two runs. Gain = the horse's
/// distance gap at the moment the faster of the two runs crosses the goal (knock-on effects on the others are real
/// and part of it). A skill the horse doesn't own is added at `default_level`. `swap`: an owned skill muted in the
/// skill-on run instead, so both runs keep both skills' activations and the gain is `id`'s effect over `swap`'s.
///
/// The two runs are identical until `id` or `swap` first activates (only their effects differ), so the skill-on run
/// resumes from the muted one's last snapshot before that (`run_forking`); no activation = no skill-on run (gain 0).
/// Bit-identical to two full runs (`gains_fingerprint`).
fn paired_gain(race: &Race, input: &RaceInput, i: usize, seed: i32, id: i32, swap: Option<i32>, setup: impl Fn(&mut HorseInput)) -> Pair {
    let run = |muted: bool| {
        let mut inp = RaceInput { random_seed: seed, ..input.clone() };
        let h = &mut inp.horses[i];
        setup(h);
        h.viewer_id = 1; // dense frames near the goal (recording only)
        if !h.skills.iter().any(|s| s.skill_id == id) {
            h.skills.push(SkillInput { skill_id: id, level: default_level(race, id) });
        }
        if muted && sets_style(race, id) {
            // Runaway: the sim makes the horse Oonige from its skill list at the start, which muting doesn't change.
            h.skills.retain(|s| s.skill_id != id);
        } else if muted {
            // Activates with identical RNG draws and events, but applies no effect: a clean pair.
            h.muted_skills.push(id);
        } else if let Some(old) = swap {
            h.muted_skills.push(old);
        }
        inp
    };
    if sets_style(race, id) {
        // Runaway: the muted run drops the skill, so the runs differ from the start.
        return pair(&run_once(race, &run(false)), &run_once(race, &run(true)), i, id);
    }
    let (on, muted) = (run(false), run(true));
    let (on, muted) = (sim_input(race, &on), sim_input(race, &muted));
    let (b, forks) = run_forking(race, &muted, &[[(i, id)].into_iter().chain(swap.map(|s| (i, s))).collect()]);
    match forks.into_iter().next().flatten() {
        Some(f) => pair(&resume(f, &on), &b, i, id),
        None => unfired(&b),
    }
}

/// `paired_gain` of each of `ids` on one seed, all skills horse `i` owns (no swap, no Runaway, no `setup`): the
/// skill-on run of every one is `input` as is, so it runs once, and each muted run resumes from it before that skill
/// first activated. Same pairs as `paired_gain`.
fn owned_gains(race: &Race, input: &RaceInput, i: usize, seed: i32, ids: &[i32]) -> Vec<Pair> {
    let mut on = RaceInput { random_seed: seed, ..input.clone() };
    on.horses[i].viewer_id = 1; // dense frames near the goal (recording only)
    let sim_on = sim_input(race, &on);
    let (a, forks) = run_forking(race, &sim_on, &ids.iter().map(|&id| vec![(i, id)]).collect::<Vec<_>>());
    ids.iter()
        .zip(forks)
        .map(|(&id, f)| {
            let Some(f) = f else { return unfired(&a) };
            let mut muted = on.clone();
            muted.horses[i].muted_skills.push(id);
            pair(&a, &resume(f, &sim_input(race, &muted)), i, id)
        })
        .collect()
}

/// The `Pair` of skill `id` from its skill-on run `a` and muted run `b`.
fn pair(a: &Run, b: &Run, i: usize, id: i32) -> Pair {
    let gain = gap_lengths(&a.out.frames, &b.out.frames, i, a.goal);
    let full_spurts = a.horses.iter().zip(&b.horses).map(|(x, y)| x.full_spurt as u8 + y.full_spurt as u8).collect();
    Pair { gain, distances: a.activations.iter().filter(|x| x.horse == i && x.skill_id == id).map(|x| x.distance).collect(), full_spurts }
}

/// The skill never activated: the other run would be identical to `r` (gain 0).
fn unfired(r: &Run) -> Pair {
    Pair { gain: 0.0, distances: Vec::new(), full_spurts: r.horses.iter().map(|x| 2 * x.full_spurt as u8).collect() }
}

/// The skill has a Runaway ability: its effect is the running style, set at the start.
fn sets_style(race: &Race, id: i32) -> bool {
    race.data.skills.get(&id).is_some_and(|s| s.details.iter().flat_map(|d| &d.abilities).any(|a| a.ability_type == ability_type::RUNAWAY))
}

/// One (card, running style) of `uma_ranking`.
#[derive(Clone, Debug, PartialEq)]
pub struct UmaRank {
    pub card_id: i32,
    /// 1 Front .. 4 End.
    pub style: i32,
    /// Unique skill gain in lengths vs the same horse with its unique muted.
    pub lengths: Dist,
    /// Activation rate of the unique.
    pub rate: f32,
    pub samples: Vec<f32>,
    /// Unique skill activation distances over all seeds.
    pub distances: Vec<f32>,
    /// (first activation distance, gain) of each seed it activated on.
    pub by_distance: Vec<(f32, f32)>,
}

/// Best uma on this course by unique skill: for each (card, style), `input.horses[horse]` wearing that card (stats,
/// skills, mood kept; `set_card` swaps the unique), running that style with A aptitude for it and for the course's
/// distance and surface (so only the unique differs between cards), unique on vs muted on `n` seeds (`skill_gains`'
/// measure). Sorted by mean gain, best first; cancelled items are dropped. Also each horse's full-spurt rate.
pub fn uma_ranking(race: &Race, input: &RaceInput, horse: usize, cards: &[i32], styles: &[i32], n: usize, w: &Work) -> (Vec<UmaRank>, Vec<f32>) {
    let seeds = seeds(input.random_seed, n);
    let c = course_info(&race.data, input.course_set_id);
    let dt = distance_type(c.distance);
    // (card, style, unique)
    let items: Vec<(i32, i32, i32)> = cards
        .iter()
        .flat_map(|&k| {
            let unique = names().card.get(&k).map_or(0, |c| c.unique_skill);
            styles.iter().map(move |&s| (k, s, unique))
        })
        .collect();
    let (v, spurt) = measure(items.len(), n, w, |k, s| {
        let (card, style, unique) = items[k];
        paired_gain(race, input, horse, seeds[s], unique, None, |h| {
            set_card(h, card);
            h.running_style = style;
            h.proper_running_style[(style - 1).clamp(0, 3) as usize] = 7;
            h.proper_distance[dt] = 7;
            h.proper_ground[(c.surface - 1).clamp(0, 1) as usize] = 7;
        })
    });
    let v = v.into_iter().map(|(k, g)| UmaRank { card_id: items[k].0, style: items[k].1, lengths: g.lengths, samples: g.samples, rate: g.rate, distances: g.distances, by_distance: g.by_distance });
    (v.collect(), spurt)
}
