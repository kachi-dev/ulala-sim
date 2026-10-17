//! Champions Meeting: the save owner's teams across a CM's saved races, and what the sim expects of each race.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{SaveMeta, TeamMember, Work, load_race, run_batch, seeds};

/// A lost race the team was this likely to win, or a won one it was at most `UPSET_WON` likely to win, is an upset.
pub const UPSET_LOST: f32 = 0.85;
pub const UPSET_WON: f32 = 0.30;

/// What the sim expects of one saved race: `n` runs of it (field, gates, mood, conditions) from fresh seeds.
#[derive(Clone, Debug, PartialEq)]
pub struct RaceSim {
    /// Share of runs a team member won.
    pub team_win: f32,
    /// Mean of the team's best place.
    pub best_place: f32,
    /// Per `SaveMeta::team` member, same order.
    pub members: Vec<MemberSim>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemberSim {
    pub win: f32,
    pub place: f32,
    /// place_hist[k] = share of runs finishing (k+1)th.
    pub place_hist: Vec<f32>,
    /// Mean place at 0..=100 % of the course.
    pub pos: Vec<f32>,
    pub full_spurt: f32,
    pub hp_out: f32,
}

/// Runs the saved race at `path` `n` times. `team`: the team's field indices. Partial when `w` is cancelled.
pub fn sim_race(path: &Path, team: &[usize], n: usize, w: &Work) -> Result<RaceSim, String> {
    let race = load_race(path)?;
    if let Some(&h) = team.iter().find(|&&h| h >= race.input.horses.len()) {
        return Err(format!("{}: no horse {h}", path.display()));
    }
    // Fresh seeds only: the save's own seed is the race that happened. The server's gates: this is that race again.
    let seeds = seeds(race.input.random_seed, n + 1).split_off(1);
    let b = run_batch(&race, &race.input, &seeds, false, w);
    let runs = b.horses.first().map_or(0, |h| h.places.len());
    let best: Vec<u8> = (0..runs).map(|r| team.iter().map(|&h| b.horses[h].places[r]).min().unwrap_or(0)).collect();
    let k = runs.max(1) as f32;
    Ok(RaceSim {
        team_win: best.iter().filter(|&&p| p == 1).count() as f32 / k,
        best_place: best.iter().map(|&p| p as f32).sum::<f32>() / k,
        members: team
            .iter()
            .map(|&h| {
                let x = &b.horses[h];
                MemberSim {
                    win: x.win_rate,
                    place: x.place.mean,
                    place_hist: x.place_hist.clone(),
                    pos: x.place_by_progress.iter().map(|d| d.mean).collect(),
                    full_spurt: x.full_spurt_rate,
                    hp_out: x.hp_out_rate,
                }
            })
            .collect(),
    })
}

/// `sim_race` for each (save, team), counting runs in `w.done`. Stops at a cancel (the race it was in is dropped).
/// The first error, if any, comes back with the rest.
pub fn sim_races(todo: &[(PathBuf, Vec<usize>)], n: usize, w: &Work) -> (Vec<(PathBuf, RaceSim)>, Option<String>) {
    let (mut out, mut err) = (vec![], None);
    for (path, team) in todo {
        let r = sim_race(path, team, n, w);
        if w.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        match r {
            Ok(r) => out.push((path.clone(), r)),
            Err(e) => {
                err.get_or_insert(e);
            }
        }
    }
    (out, err)
}

/// One saved CM race, with its sim when it has one.
#[derive(Clone, Copy)]
pub struct CmRace<'a> {
    pub path: &'a Path,
    /// "YYYY-MM-DD"
    pub date: &'a str,
    pub meta: &'a SaveMeta,
    pub sim: Option<&'a RaceSim>,
}

impl CmRace<'_> {
    /// The team's best finishing place (0 = unknown).
    pub fn best(&self) -> usize {
        self.meta.team.iter().map(|m| m.place).filter(|&p| p > 0).min().unwrap_or(0)
    }
    /// Lost at `UPSET_LOST`+ or won at `UPSET_WON` or less; None without a sim.
    pub fn upset(&self) -> Option<bool> {
        let p = self.sim?.team_win;
        Some(if self.meta.won { p <= UPSET_WON } else { p >= UPSET_LOST })
    }
}

/// What a team member is known by across races: the trained uma, else the card.
pub fn member_id(m: &TeamMember) -> i32 {
    if m.trained != 0 { m.trained } else { m.card }
}

/// Real results of a set of races, plus the sim's when every one of them has a sim.
#[derive(Clone, Debug, PartialEq)]
pub struct Tally {
    pub races: usize,
    pub wins: usize,
    /// Mean of the team's best place.
    pub best_place: f32,
    /// The uma with the most wins (card, chara) and its wins.
    pub top: Option<((i32, i32), usize)>,
    pub sim: Option<SimTally>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SimTally {
    /// Mean team win chance.
    pub win: f32,
    /// Sum of the team win chances.
    pub expected: f32,
    pub best_place: f32,
    /// Upsets lost, won.
    pub upsets: (usize, usize),
}

pub fn tally(races: &[CmRace]) -> Tally {
    let k = races.len().max(1) as f32;
    let mut by: HashMap<(i32, i32), usize> = HashMap::new();
    for r in races.iter().filter(|r| r.meta.won) {
        *by.entry(r.meta.winner).or_default() += 1;
    }
    let top = by.into_iter().max_by_key(|&(w, n)| (n, std::cmp::Reverse(w)));
    let sims: Option<Vec<&RaceSim>> = races.iter().map(|r| r.sim).collect();
    let sim = sims.filter(|v| !v.is_empty()).map(|v| SimTally {
        win: v.iter().map(|s| s.team_win).sum::<f32>() / k,
        expected: v.iter().map(|s| s.team_win).sum(),
        best_place: v.iter().map(|s| s.best_place).sum::<f32>() / k,
        upsets: (
            races.iter().filter(|r| r.upset() == Some(true) && !r.meta.won).count(),
            races.iter().filter(|r| r.upset() == Some(true) && r.meta.won).count(),
        ),
    });
    Tally {
        races: races.len(),
        wins: races.iter().filter(|r| r.meta.won).count(),
        best_place: races.iter().map(|r| r.best() as f32).sum::<f32>() / k,
        top,
        sim,
    }
}

/// A team: the same three trained umas.
#[derive(Clone, Debug, PartialEq)]
pub struct CmTeam {
    /// Sorted `member_id`s.
    pub key: Vec<i32>,
    /// By running style, front first.
    pub members: Vec<TeamMember>,
    /// Indices into the races given to `cm_teams`.
    pub races: Vec<usize>,
}

/// The teams in `races`, the first one used first (`races` is newest first).
pub fn cm_teams(races: &[CmRace]) -> Vec<CmTeam> {
    let mut teams: Vec<CmTeam> = vec![];
    for (i, r) in races.iter().enumerate().rev() {
        let mut key: Vec<i32> = r.meta.team.iter().map(member_id).collect();
        key.sort();
        match teams.iter_mut().find(|t| t.key == key) {
            Some(t) => t.races.push(i),
            None => {
                let mut members = r.meta.team.clone();
                members.sort_by_key(|m| (m.style, member_id(m)));
                teams.push(CmTeam { key, members, races: vec![i] });
            }
        }
    }
    for t in &mut teams {
        t.races.reverse();
    }
    teams
}

/// One team member over the team's races: real results and the sim's mean (when every race has a sim).
#[derive(Clone, Debug, PartialEq)]
pub struct MemberTally {
    pub member: TeamMember,
    pub wins: usize,
    pub place: f32,
    /// Share of races per place.
    pub place_hist: Vec<f32>,
    pub sim: Option<MemberSim>,
}

pub fn member_tallies(team: &CmTeam, races: &[CmRace]) -> Vec<MemberTally> {
    let rs: Vec<&CmRace> = team.races.iter().filter_map(|&i| races.get(i)).collect();
    let k = rs.len().max(1) as f32;
    let all_sims = rs.iter().all(|r| r.sim.is_some()) && !rs.is_empty();
    team.members
        .iter()
        .map(|m| {
            let id = member_id(m);
            // This member's place and sim in each race (its slot in `team` differs between saves).
            let at: Vec<(usize, Option<&MemberSim>)> = rs
                .iter()
                .filter_map(|r| {
                    let j = r.meta.team.iter().position(|x| member_id(x) == id)?;
                    Some((r.meta.team[j].place, r.sim.and_then(|s| s.members.get(j))))
                })
                .collect();
            let field = at.iter().map(|x| x.0).chain(rs.iter().map(|r| r.meta.team.len() + r.meta.rivals.len())).max().unwrap_or(0);
            let mut place_hist = vec![0.0; field];
            for &(p, _) in &at {
                if let Some(h) = p.checked_sub(1).and_then(|p| place_hist.get_mut(p)) {
                    *h += 1.0 / k;
                }
            }
            let sims: Vec<&MemberSim> = at.iter().filter_map(|x| x.1).collect();
            let mean = |f: &dyn Fn(&MemberSim) -> f32| sims.iter().map(|s| f(s)).sum::<f32>() / sims.len().max(1) as f32;
            let mean_vec = |f: &dyn Fn(&MemberSim) -> &Vec<f32>| {
                let len = sims.iter().map(|s| f(s).len()).max().unwrap_or(0);
                (0..len).map(|i| sims.iter().map(|s| f(s).get(i).copied().unwrap_or(0.0)).sum::<f32>() / sims.len() as f32).collect()
            };
            let sim = (all_sims && sims.len() == at.len()).then(|| MemberSim {
                win: mean(&|s| s.win),
                place: mean(&|s| s.place),
                place_hist: mean_vec(&|s| &s.place_hist),
                pos: mean_vec(&|s| &s.pos),
                full_spurt: mean(&|s| s.full_spurt),
                hp_out: mean(&|s| s.hp_out),
            });
            MemberTally {
                member: m.clone(),
                wins: at.iter().filter(|x| x.0 == 1).count(),
                place: at.iter().map(|x| x.0 as f32).sum::<f32>() / at.len().max(1) as f32,
                place_hist,
                sim,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(trained: i32, style: i32, horse: usize, place: usize) -> TeamMember {
        TeamMember { card: 100000 + trained, chara: 1000 + trained, trained, style, horse, place }
    }

    #[test]
    fn teams_and_tallies() {
        // Newest first. Team {1,2,3} twice (slots shuffled), then {1,2,4}.
        let metas = [
            SaveMeta { team: vec![member(4, 4, 0, 9), member(1, 3, 1, 1), member(2, 2, 2, 3)], won: true, winner: (100001, 1001), ..Default::default() },
            SaveMeta { team: vec![member(2, 2, 0, 2), member(3, 4, 1, 8), member(1, 3, 2, 4)], won: false, winner: (5, 5), ..Default::default() },
            SaveMeta { team: vec![member(1, 3, 0, 1), member(2, 2, 1, 5), member(3, 4, 2, 9)], won: true, winner: (100001, 1001), ..Default::default() },
        ];
        let sim = |w: f32| RaceSim { team_win: w, best_place: 1.5, members: vec![MemberSim { win: w, ..Default::default() }; 3] };
        let sims = [sim(0.2), sim(0.9), sim(0.6)];
        let races: Vec<CmRace> = metas.iter().zip(&sims).map(|(meta, s)| CmRace { path: Path::new(""), date: "", meta, sim: Some(s) }).collect();

        let teams = cm_teams(&races);
        assert_eq!(teams.len(), 2);
        assert_eq!(teams[0].key, vec![1, 2, 3]);
        assert_eq!(teams[0].races, vec![1, 2]);
        assert_eq!(teams[0].members.iter().map(|m| m.trained).collect::<Vec<_>>(), vec![2, 1, 3]); // by style
        assert_eq!(teams[1].races, vec![0]);

        let t = tally(&races);
        assert_eq!((t.races, t.wins, t.top), (3, 2, Some(((100001, 1001), 2))));
        assert!((t.best_place - 4.0 / 3.0).abs() < 1e-6);
        let s = t.sim.unwrap();
        assert!((s.expected - 1.7).abs() < 1e-6);
        assert_eq!(s.upsets, (1, 1)); // lost at 90%, won at 20%

        let m = member_tallies(&teams[0], &races);
        let urara = m.iter().find(|x| x.member.trained == 1).unwrap();
        assert_eq!(urara.wins, 1);
        assert!((urara.place - 2.5).abs() < 1e-6);
        assert_eq!(urara.place_hist[0], 0.5);
        assert_eq!(urara.place_hist[3], 0.5);
        assert!(urara.sim.is_some());
    }
}
