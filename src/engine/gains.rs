use uma_race_sim::api::{HorseInput, RaceInput, SkillInput};

use super::{Dist, GRADE_A, MAX_STAMINA, Race, STYLES, Work, ability_type, cards, course_info, custom_popularity, default_level, dist, distance_type, gap_lengths, names, new_horse, par_map, run_once, seeds, set_card};

/// Umas in an NPC field: the measured uma and 8 NPCs; smaller fields are topped up to it.
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

/// The field to measure in, and the measured uma's index in it. `alone`: `input.horses[horse]` at gate 5 among 8 NPCs;
/// else `input`'s field with NPCs after it up to `FIELD_MIN`. NPCs: `LOBBIES[lobby]`'s styles round-robin (a top-up
/// takes the first few, so it stays mixed), each a costume of that default style and an unused chara, `new_horse`
/// stats and unique, stamina at the cap (raw 2800, 2000 after the halving) so they keep a full spurt, A aptitude for
/// style, distance and surface. Popularity is re-ranked when NPCs were added.
pub fn npc_field(race: &Race, input: &RaceInput, horse: usize, alone: bool, lobby: usize) -> (RaceInput, usize) {
    let mut out = input.clone();
    let field = if alone { vec![out.horses.swap_remove(horse)] } else { std::mem::take(&mut out.horses) };
    let n_npc = if alone { FIELD_MIN - 1 } else { FIELD_MIN.saturating_sub(field.len()) };
    let c = course_info(&race.data, input.course_set_id);
    let dt = distance_type(c.distance);
    let runaway = race.data.skills.keys().copied().filter(|&id| sets_style(race, id)).min();
    let mut left = LOBBIES[lobby.min(LOBBIES.len() - 1)].1;
    let mut used: Vec<i32> = field.iter().map(|h| h.chara_id).collect();
    let mut made = Vec::new();
    while made.len() < n_npc && left.iter().any(|&k| k > 0) {
        for (k, n) in left.iter_mut().enumerate() {
            if *n == 0 || made.len() == n_npc {
                continue;
            }
            *n -= 1;
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
    }
    let i = if alone { made.len().min(4) } else { horse };
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
}

/// For each candidate: `horse` with the skill (added if not owned) vs the skill muted, paired on `n` seeds, in
/// `input`'s field (see `npc_field`). Sorted by mean gain, best first. Candidates without all `n` samples (cancelled)
/// are dropped. Also each horse's full-spurt rate over all runs.
pub fn skill_gains(race: &Race, input: &RaceInput, horse: usize, candidates: &[i32], n: usize, w: &Work) -> (Vec<SkillGain>, Vec<f32>) {
    let seeds = seeds(input.random_seed, n);
    let (v, spurt) = measure(candidates.len(), n, w, |k, s| paired_gain(race, input, horse, seeds[s], candidates[k], |_| {}));
    let v = v.into_iter().map(|(k, g)| SkillGain { skill_id: candidates[k], lengths: g.lengths, samples: g.samples, rate: g.rate, distances: g.distances, by_distance: g.by_distance });
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
    let res = par_map(items * n, w, |i| paired(i / n, i % n));
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
/// and part of it). A skill the horse doesn't own is added at `default_level`.
fn paired_gain(race: &Race, input: &RaceInput, i: usize, seed: i32, id: i32, setup: impl Fn(&mut HorseInput)) -> Pair {
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
        }
        run_once(race, &inp)
    };
    let (a, b) = (run(false), run(true));
    let gain = gap_lengths(&a.out.frames, &b.out.frames, i, a.goal);
    let full_spurts = a.horses.iter().zip(&b.horses).map(|(x, y)| x.full_spurt as u8 + y.full_spurt as u8).collect();
    Pair { gain, distances: a.activations.iter().filter(|x| x.horse == i && x.skill_id == id).map(|x| x.distance).collect(), full_spurts }
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
        paired_gain(race, input, horse, seeds[s], unique, |h| {
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
