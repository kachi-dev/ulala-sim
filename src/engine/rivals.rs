//! Replay rivals: teams from saved replays filling a field's empty gates, drawn anew from each seed.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use uma_race_sim::api::{HorseInput, RaceInput};
use uma_race_sim::data::corpus;

use super::{Batch, Race, SaveEntry, SaveMeta, Work, custom_popularity, par_map, splitmix};

/// Where the rival pool comes from (`rival_pool`); part of `NpcSetup`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RivalSetup {
    /// Empty gates take rivals instead of NPCs (with `NpcSetup::fill`).
    pub on: bool,
    /// The races in `picked`, instead of the filter.
    pub pick: bool,
    /// Filter: horseACT folders, a course (race_course_set id, 0 = any), ground, weather, season and time of day
    /// (`RaceInput`'s values, 0 = any), dates "YYYY-MM-DD" ("" = open).
    pub cats: Vec<String>,
    pub course: i32,
    pub conds: [i32; 4],
    pub from: String,
    pub to: String,
    pub picked: Vec<PathBuf>,
    /// Draw single umas instead of whole teams.
    pub singles: bool,
    /// Keep the umas of the user's trainers (the save owners), skipped by default.
    pub keep_mine: bool,
    /// Pool teams left out (`PoolTeam::key`).
    pub dropped: Vec<String>,
}

/// One team of the rival pool, from the saves index.
#[derive(Clone, Debug, PartialEq)]
pub struct PoolTeam {
    /// Trainer name and the team's trained uma ids: the same team in many replays is one entry.
    pub key: String,
    pub trainer: String,
    /// The newest replay it raced in, and its umas there.
    pub path: PathBuf,
    pub umas: Vec<PoolUma>,
    /// Replays it raced in, and won (one of its umas first).
    pub seen: usize,
    pub won: usize,
    /// The newest one's date and folder.
    pub last: String,
    pub category: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PoolUma {
    /// Field index in `PoolTeam::path`.
    pub horse: usize,
    pub card: i32,
    pub chara: i32,
    pub style: i32,
    /// `uma_id`.
    pub id: i32,
}

/// The teams in the replays `setup` picks or filters, newest first (`saves` is). A team is a replay's umas of one
/// `team_id` (team 0: each uma alone); NPCs (no costume: mobs, career rivals), and unless `keep_mine` the umas of any save owner's trainer, are left
/// out. Drawing whole teams, only full ones (`TEAM_SIZE`) count: a lobby of real CM teams. `dropped` teams stay in: the
/// dialog lists them unticked. Also how many replays matched.
pub fn rival_pool(saves: &[SaveEntry], meta: &HashMap<PathBuf, SaveMeta>, setup: &RivalSetup) -> (Vec<PoolTeam>, usize) {
    let mine: HashSet<&str> = meta.values().map(|m| m.player_trainer.as_str()).filter(|t| !t.is_empty()).collect();
    let picked: HashSet<&PathBuf> = setup.picked.iter().collect();
    let (mut out, mut at, mut replays) = (Vec::<PoolTeam>::new(), HashMap::<String, usize>::new(), 0);
    for e in saves {
        let Some(m) = meta.get(&e.path) else { continue };
        let ok = if setup.pick {
            picked.contains(&e.path)
        } else {
            setup.cats.contains(&e.category)
                && (setup.course == 0 || m.course_id == setup.course)
                && (0..4).all(|k| setup.conds[k] == 0 || m.conds[k] == setup.conds[k])
                && (setup.from.is_empty() || e.date >= setup.from)
                && (setup.to.is_empty() || e.date <= setup.to)
        };
        if !ok {
            continue;
        }
        replays += 1;
        let mut groups: Vec<Vec<usize>> = vec![];
        for (i, u) in m.field.iter().enumerate() {
            if u.card == 0 || (!setup.keep_mine && mine.contains(u.trainer.as_str())) {
                continue;
            }
            match groups.iter_mut().find(|g| u.team != 0 && m.field[g[0]].team == u.team) {
                Some(g) => g.push(i),
                None => groups.push(vec![i]),
            }
        }
        for g in groups.into_iter().filter(|g| setup.singles || g.len() == TEAM_SIZE) {
            let trainer = m.field[g[0]].trainer.clone();
            let mut ids: Vec<i32> = g.iter().map(|&i| uma_id(&m.field[i])).collect();
            ids.sort();
            let key = format!("{trainer}|{ids:?}");
            let won = g.iter().any(|&i| m.field[i].place == 1) as usize;
            if let Some(&k) = at.get(&key) {
                out[k].seen += 1;
                out[k].won += won;
                continue;
            }
            at.insert(key.clone(), out.len());
            let umas = g.iter().map(|&i| PoolUma { horse: i, card: m.field[i].card, chara: m.field[i].chara, style: m.field[i].style, id: uma_id(&m.field[i]) }).collect();
            out.push(PoolTeam { key, trainer, path: e.path.clone(), umas, seen: 1, won, last: e.date.clone(), category: e.category.clone() });
        }
    }
    (out, replays)
}

/// Umas in a Champions Meeting team.
pub const TEAM_SIZE: usize = 3;

/// An uma's id within its trainer's: the trained uma, else (old saves) her costume.
fn uma_id(u: &super::FieldUma) -> i32 {
    if u.trained != 0 { u.trained } else { -u.card }
}

/// What the draw picks from, as (pool team, its umas): whole teams, or with `singles` each uma once (one in two teams,
/// a trainer's changed lineup, counts once).
pub fn pool_units(pool: &[PoolTeam], singles: bool) -> Vec<(usize, Vec<usize>)> {
    if !singles {
        return pool.iter().enumerate().map(|(t, team)| (t, (0..team.umas.len()).collect())).collect();
    }
    let mut seen = HashSet::new();
    let mut v = vec![];
    for (t, team) in pool.iter().enumerate() {
        for (m, u) in team.umas.iter().enumerate() {
            if seen.insert((team.trainer.as_str(), u.id)) {
                v.push((t, vec![m]));
            }
        }
    }
    v
}

/// Umas the pool holds: what can fill empty gates.
pub fn pool_umas(pool: &[PoolTeam], singles: bool) -> usize {
    pool_units(pool, singles).iter().map(|u| u.1.len()).sum()
}

/// A pool team as it runs: its umas from its newest replay.
#[derive(Clone, Debug)]
pub struct RivalTeam {
    pub trainer: String,
    pub horses: Vec<HorseInput>,
    /// `PoolTeam::category` and `seen`.
    pub category: String,
    pub seen: usize,
}

/// The rivals of one field: `pool` drawn into gates `from..` (the umas come before), a new draw per seed (`at_seed`).
#[derive(Clone, Debug)]
pub struct Rivals {
    pub pool: Arc<Vec<RivalTeam>>,
    pub from: usize,
}

/// The same loaded pool (`HorseInput` has no `PartialEq`).
impl PartialEq for Rivals {
    fn eq(&self, o: &Self) -> bool {
        Arc::ptr_eq(&self.pool, &o.pool) && self.from == o.from
    }
}

/// Parsed saves' horses, by path (a replay never changes).
fn parsed() -> &'static Mutex<HashMap<PathBuf, Arc<Vec<HorseInput>>>> {
    static P: OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<HorseInput>>>>> = OnceLock::new();
    P.get_or_init(Default::default)
}

/// Parses the saves at `paths` not parsed yet (in parallel), so `load_rivals` finds them.
pub fn warm_rivals(paths: &[PathBuf]) -> Result<(), String> {
    let todo: Vec<PathBuf> = {
        let p = parsed().lock().unwrap_or_else(|e| e.into_inner());
        let mut v: Vec<PathBuf> = paths.iter().filter(|x| !p.contains_key(*x)).cloned().collect();
        v.sort();
        v.dedup();
        v
    };
    let done = par_map(todo.len(), &Work::default(), |i| {
        let s = std::fs::read_to_string(&todo[i]).map_err(|e| e.to_string())?;
        corpus::parse_race(&s).map(|r| Arc::new(r.0.horses)).map_err(|e| format!("{}: {e}", todo[i].display()))
    });
    let mut p = parsed().lock().unwrap_or_else(|e| e.into_inner());
    for (i, r) in done {
        p.insert(todo[i].clone(), r?);
    }
    Ok(())
}

/// The pool's teams as they run in `race` (the skills its game data has), or with `singles` each uma alone. Teams in
/// `dropped` are left out.
pub fn load_rivals(race: &Race, pool: &[PoolTeam], setup: &RivalSetup) -> Result<Arc<Vec<RivalTeam>>, String> {
    let pool: Vec<PoolTeam> = pool.iter().filter(|t| !setup.dropped.contains(&t.key)).cloned().collect();
    warm_rivals(&pool.iter().map(|t| t.path.clone()).collect::<Vec<_>>())?;
    let p = parsed().lock().unwrap_or_else(|e| e.into_inner());
    let horse = |t: &PoolTeam, m: usize| {
        let mut h = p.get(&t.path).and_then(|v| v.get(t.umas[m].horse)).cloned().ok_or_else(|| format!("{}: no horse {}", t.path.display(), t.umas[m].horse))?;
        h.skills.retain(|k| race.data.skills.contains_key(&k.skill_id));
        Ok::<_, String>(h)
    };
    let teams = pool_units(&pool, setup.singles)
        .into_iter()
        .map(|(t, ms)| {
            let team = &pool[t];
            let horses = ms.into_iter().map(|m| horse(team, m)).collect::<Result<_, String>>()?;
            Ok(RivalTeam { trainer: team.trainer.clone(), horses, category: team.category.clone(), seen: team.seen })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Arc::new(teams))
}

/// The (unit, uma) of each of `slots` rival gates on `seed`, and how many of them came as whole units: units in an order
/// drawn from the seed, whole while they fit; the slots left (fewer than a unit) each take one uma, drawn too, of a
/// different unit not in the race, running alone. Its own stream (salted), leaving the sim's and the gates' draws
/// alone. Fewer when the pool runs out.
pub fn draw(sizes: &[usize], seed: i32, slots: usize) -> (Vec<(usize, usize)>, usize) {
    let mut x = seed as u32 as u64 ^ 0xBB67_AE85_84CA_A73B;
    let n = sizes.len();
    let mut order: Vec<usize> = (0..n).collect();
    for i in 0..n {
        let j = i + (splitmix(&mut x) % (n - i) as u64) as usize;
        order.swap(i, j);
    }
    let mut rest = order.into_iter().filter(|&t| sizes[t] > 0).peekable();
    let mut out = vec![];
    while let Some(&t) = rest.peek().filter(|&&t| sizes[t] <= slots - out.len()) {
        out.extend((0..sizes[t]).map(|m| (t, m)));
        rest.next();
    }
    let whole = out.len();
    for t in rest.take(slots - whole) {
        out.push((t, (splitmix(&mut x) % sizes[t] as u64) as usize));
    }
    (out, whole)
}

/// The pool, units of `sizes`, fills `slots` empty gates (`draw`'s count is the same for every seed).
pub fn pool_fills(sizes: &[usize], slots: usize) -> bool {
    draw(sizes, 0, slots).0.len() == slots
}

impl Rivals {
    pub fn sizes(&self) -> Vec<usize> {
        self.pool.iter().map(|t| t.horses.len()).collect()
    }

    /// `input` on `seed` with that seed's draw in the rival gates: each drawn team, and each uma drawn alone, its own
    /// team id (after the umas'), popularity ranked again.
    pub fn field(&self, input: &RaceInput, seed: i32) -> RaceInput {
        let mut out = RaceInput { random_seed: seed, ..input.clone() };
        let slots = input.horses.len().saturating_sub(self.from);
        let base = input.horses[..self.from.min(input.horses.len())].iter().map(|h| h.team_id).max().unwrap_or(0) + 1;
        let (picks, whole) = draw(&self.sizes(), seed, slots);
        // Team per gate: the whole units' in order, then one each for the umas alone.
        let mut teams: Vec<Option<usize>> = vec![];
        for (j, (t, m)) in picks.into_iter().enumerate() {
            let k = match teams.iter().position(|&x| x == Some(t)).filter(|_| j < whole) {
                Some(k) => k,
                None => {
                    teams.push((j < whole).then_some(t));
                    teams.len() - 1
                }
            };
            let mut h = self.pool[t].horses[m].clone();
            h.team_id = base + k as i32;
            out.horses[self.from + j] = h;
        }
        custom_popularity(&out)
    }

    /// Trainer names of `seed`'s field: the rivals' after `trainers` (the umas').
    pub fn trainers(&self, trainers: &[String], seed: i32, size: usize) -> Vec<String> {
        let mut v: Vec<String> = trainers.iter().take(self.from).cloned().collect();
        v.resize(self.from, String::new());
        v.extend(draw(&self.sizes(), seed, size.saturating_sub(self.from)).0.into_iter().map(|(t, _)| self.pool[t].trainer.clone()));
        v
    }
}

/// `input` as it runs on `seed`: with rivals, that seed's draw in their gates.
pub fn at_seed(input: &RaceInput, seed: i32, rivals: Option<&Rivals>) -> RaceInput {
    match rivals {
        Some(r) => r.field(input, seed),
        None => RaceInput { random_seed: seed, ..input.clone() },
    }
}

/// One rival team's record over a batch.
#[derive(Clone, Debug, PartialEq)]
pub struct RivalStat {
    /// `Rivals::pool` index.
    pub team: usize,
    /// Races it was drawn in, won (its best place 1st), and finished ahead of the umas (its best place beat theirs).
    pub drawn: usize,
    pub wins: usize,
    pub ahead: usize,
    /// Per uma of the team: races won.
    pub uma_wins: Vec<usize>,
}

/// Each drawn team's record in batch `b` of a field with `r`, most wins first (a big pool draws each team a few times:
/// by rate, one lucky race would top it), then best win rate.
pub fn rival_stats(r: &Rivals, b: &Batch) -> Vec<RivalStat> {
    let sizes = r.sizes();
    let mut st: Vec<RivalStat> = sizes.iter().enumerate().map(|(t, &k)| RivalStat { team: t, drawn: 0, wins: 0, ahead: 0, uma_wins: vec![0; k] }).collect();
    let slots = b.horses.len().saturating_sub(r.from);
    for (s, &seed) in b.seeds.iter().enumerate() {
        let place = |h: usize| b.horses[h].places.get(s).copied().unwrap_or(u8::MAX);
        let mine = (0..r.from).map(place).min().unwrap_or(u8::MAX);
        let mut best: HashMap<usize, u8> = HashMap::new();
        // Umas drawn alone aren't their team's race: only the whole units count.
        let (picks, whole) = draw(&sizes, seed, slots);
        for (j, (t, m)) in picks.into_iter().enumerate().take(whole) {
            let p = place(r.from + j);
            let e = best.entry(t).or_insert(u8::MAX);
            *e = (*e).min(p);
            st[t].uma_wins[m] += (p == 1) as usize;
        }
        for (t, p) in best {
            st[t].drawn += 1;
            st[t].wins += (p == 1) as usize;
            st[t].ahead += (p < mine) as usize;
        }
    }
    st.retain(|x| x.drawn > 0);
    st.sort_by(|a, b| b.wins.cmp(&a.wins).then((b.wins * a.drawn).cmp(&(a.wins * b.drawn))));
    st
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::FieldUma;

    #[test]
    fn draws_vary_by_seed_and_fill_the_slots() {
        let sizes = [3, 3, 3, 3, 2];
        let (a, whole) = draw(&sizes, 1, 6);
        assert_eq!((a.clone(), whole), draw(&sizes, 1, 6), "same seed, same draw");
        assert_eq!(whole, 6);
        assert_eq!(a.len(), 6);
        // Whole teams: the first three gates are one team, the next three another.
        assert!(a[..3].iter().all(|x| x.0 == a[0].0) && a[3..].iter().all(|x| x.0 == a[3].0) && a[0].0 != a[3].0);
        let distinct: HashSet<Vec<(usize, usize)>> = (0..50).map(|s| draw(&sizes, s, 6).0).collect();
        assert!(distinct.len() > 5, "seeds draw different teams");
        assert_eq!(draw(&sizes, 7, 20).0.len(), 14, "a short pool fills what it can");
        // 7 slots, teams of 3: two whole teams, then one uma alone from a third.
        let teams = [3; 5];
        for s in 0..20 {
            let (p, whole) = draw(&teams, s, 7);
            assert_eq!((p.len(), whole), (7, 6));
            assert!(p[..6].iter().all(|x| x.0 != p[6].0), "the lone uma is from a team not in the race");
        }
        // 8 slots: two lone umas, from two more teams; 3 teams can't (only one left after the two whole ones).
        assert!(pool_fills(&teams, 8) && !pool_fills(&teams[..3], 8));
    }

    #[test]
    fn headless_saves_have_a_date() {
        let e = crate::engine::save_entry(std::path::Path::new("r/Room match (headless)/90497311-20261018.json"));
        assert_eq!((e.winner.as_str(), e.date.as_str(), e.category.as_str()), ("", "2026-10-18", "Room match (headless)"));
        assert_eq!(crate::engine::save_entry(std::path::Path::new("r/Room match/Oguri Cap-95.2s-20260803.json")).winner, "Oguri Cap");
    }

    #[test]
    fn pool_counts_a_team_once_and_skips_mine() {
        let u = |trained, team, trainer: &str, place| FieldUma { card: 100101, chara: 1001, trained, style: 1, team, trainer: trainer.into(), place };
        let race = |field| SaveMeta { player_trainer: "Me".into(), field, ..Default::default() };
        let e = |p: &str| SaveEntry { path: p.into(), category: "Room match".into(), winner: String::new(), time: String::new(), date: "2026-10-10".into() };
        let meta = HashMap::from([
            (PathBuf::from("a"), race(vec![u(1, 1, "Me", 2), u(5, 2, "Kaze", 1), u(6, 2, "Kaze", 3)])),
            // Kaze's same team again, and Kaze with another lineup.
            (PathBuf::from("b"), race(vec![u(6, 3, "Kaze", 4), u(5, 3, "Kaze", 5), u(7, 1, "Kaze", 1), u(2, 2, "Me", 2)])),
        ]);
        // Whole teams: only full ones, and Kaze's teams here have 2 umas and 1.
        let teams = RivalSetup { cats: vec!["Room match".into()], ..Default::default() };
        assert!(rival_pool(&[e("a"), e("b")], &meta, &teams).0.is_empty());
        let setup = RivalSetup { singles: true, ..teams };
        let (pool, replays) = rival_pool(&[e("a"), e("b")], &meta, &setup);
        assert_eq!(replays, 2);
        let keys: Vec<(&str, usize, usize)> = pool.iter().map(|t| (t.key.as_str(), t.seen, t.won)).collect();
        assert_eq!(keys, [("Kaze|[5, 6]", 2, 1), ("Kaze|[7]", 1, 1)]);
        assert_eq!(pool_umas(&pool, true), 3);
        let keep = RivalSetup { keep_mine: true, ..setup };
        assert_eq!(rival_pool(&[e("a"), e("b")], &meta, &keep).0.len(), 4);
    }
}
