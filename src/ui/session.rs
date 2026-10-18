//! The race being worked on: loading or creating it, and the edit helpers the setup goes through.

use super::*;

/// Everything a loaded or new race needs, computed off the UI thread.
type Loaded = (Arc<Race>, Arc<Vec<SkillInfo>>, Arc<Vec<CourseInfo>>, Arc<CourseView>);

fn prepare(race: Race) -> Loaded {
    let all = all_skills(&race);
    let courses = Arc::new(courses(&race));
    let cv = Arc::new(course_view(&race, race.input.course_set_id));
    (Arc::new(race), all, courses, cv)
}

fn install(mut s: S, (race, all, courses, cv): Loaded) {
    s.skills.set(all);
    s.courses.set(courses);
    s.cview.set(Some(cv));
    s.input.set(race.input.clone());
    s.trainers.set(race.trainers.clone());
    s.baseline.set(race.path.is_some().then(|| Arc::new(race.input.clone())));
    s.field_size.set(if race.path.is_some() { race.input.horses.len().clamp(FIELD_MIN, MAX_FIELD) } else { FIELD_MIN });
    *s.rev.write() += 1;
    s.verify.set(None);
    s.res.set(None);
    s.focus.set(None);
    s.skill_sel.set(None);
    s.editing.set(None);
    s.g_res.set(None);
    s.g_sel.set(None);
    s.sel.set(0);
    s.r_res.set(None);
    s.r_sel.set(None);
    s.slot_res.set(None);
    s.saved_race.set(None);
    s.race.set(Some(race));
}

pub(super) fn new_custom(s: S) {
    if !claim(s) {
        return;
    }
    bg(s, async move {
        let mut s = s;
        match job(s, "Warming up", 0, false, |_| prepare(new_race())).await {
            Ok((l, _)) => install(s, l),
            Err(e) => s.err.set(Some(e)),
        }
    });
}

/// Startup: the last session (`restore_session`) as it was left, else a new custom race.
pub(super) fn restore(s: S) {
    if !claim(s) {
        return;
    }
    bg(s, async move {
        let mut s = s;
        let r = job(s, "Warming up", 0, false, |_| {
            restore_session().map(|r| {
                let cv = Arc::new(course_view(&r.race, r.input.course_set_id));
                // A save's server check isn't stored: run its seed again, as opening it does.
                let v = r.race.path.is_some().then(|| verify_against_server(&r.race, &run_once(&r.race, &r.race.input)));
                (prepare(r.race), r.input, r.trainers, r.baseline, (r.field_size, r.setup), cv, v)
            })
        })
        .await;
        let Ok((Some((l, input, trainers, baseline, (field_size, setup), cv, v)), _)) = r else { return new_custom(s) };
        install(s, l);
        s.verify.set(v);
        let fs = if field_size > 0 { field_size } else { *s.field_size.peek() }.max(input.horses.len()).min(MAX_FIELD);
        s.field_size.set(fs);
        s.input.set(input);
        s.trainers.set(trainers);
        s.baseline.set(baseline.map(Arc::new));
        s.cview.set(Some(cv));
        s.saved_race.set(setup);
    });
}

/// Opens a setup saved by name: its race (or a custom one when that save is gone) with the setup, trainer names,
/// baseline, field size and NPCs as saved.
pub(super) fn open_setup(mut s: S, setup: SavedSetup) {
    if !claim(s) {
        return s.err.set(Some("Busy: wait for the current run to finish, or cancel it.".into()));
    }
    s.saves_open.set(false);
    s.left_open.set(true);
    bg(s, async move {
        let mut s = s;
        let r = job(s, "Loading setup", 0, false, move |_| {
            let race = setup_race(&setup);
            let cv = Arc::new(course_view(&race, setup.input.course_set_id));
            (prepare(race), setup, cv)
        })
        .await;
        let Ok(((l, setup, cv), _)) = r else { return };
        install(s, l);
        s.npcs.set(setup.npcs);
        s.field_size.set(setup.field_size.max(setup.input.horses.len()).clamp(FIELD_MIN, MAX_FIELD));
        s.input.set(setup.input);
        s.trainers.set(setup.trainers);
        s.baseline.set(setup.baseline.map(Arc::new));
        s.cview.set(Some(cv));
        s.saved_race.set(read_setup(&setup.path));
    });
}

pub(super) fn open(mut s: S, path: PathBuf) {
    // A setup saved by name opens as one (`Open a file…`, dropped on the window).
    if let Some(setup) = read_setup(&path) {
        return open_setup(s, setup);
    }
    if !claim(s) {
        return s.err.set(Some("Busy: wait for the current run to finish, or cancel it.".into()));
    }
    // Saved races land on Results, which shows the progress while the saved seed runs.
    s.saves_open.set(false);
    go(s, Tab::Results);
    s.left_open.set(false);
    bg(s, async move {
        let mut s = s;
        let loaded = job(s, "Loading race", 0, false, move |_| load_race(&path).map(prepare)).await.and_then(|r| r.0);
        let l = match loaded {
            Ok(l) => l,
            Err(e) => return s.err.set(Some(e)),
        };
        let race = l.0.clone();
        install(s, l);
        let rev = *s.rev.peek();
        let out = job(s, "Running the saved seed", 0, false, move |_| {
            let run = run_once(&race, &race.input);
            let v = verify_against_server(&race, &run);
            (run, v, race)
        })
        .await;
        match out {
            Ok(((run, v, race), _)) => {
                let Some(cv) = s.cview.cloned() else { return };
                s.verify.set(Some(v));
                let n = race.input.horses.len();
                s.res.set(Some(Res {
                    kind: Kind::Single { cur: Arc::new(run), base: None },
                    input: Arc::new(race.input.clone()),
                    trainers: Arc::new(race.trainers.clone()),
                    colors: Arc::new(uma_colors(&race.input, race.player)),
                    player: race.player,
                    ed_mask: vec![false; n],
                    pairs: vec![None; n],
                    base_input: None,
                    rivals: None,
                    cv,
                    rev,
                }));
            }
            Err(e) => s.err.set(Some(e)),
        }
    });
}

// ---------- edit helpers ----------

pub(super) fn horse_same(a: &HorseInput, b: &HorseInput) -> bool {
    (a.card_id, a.team_id) == (b.card_id, b.team_id)
        && (a.speed, a.stamina, a.pow, a.guts, a.wiz) == (b.speed, b.stamina, b.pow, b.guts, b.wiz)
        && (a.running_style, a.motivation) == (b.running_style, b.motivation)
        && (a.proper_distance, a.proper_running_style, a.proper_ground)
            == (b.proper_distance, b.proper_running_style, b.proper_ground)
        && a.skills == b.skills
}

pub(super) fn conditions_same(a: &RaceInput, b: &RaceInput) -> bool {
    (a.course_set_id, a.ground_condition, a.weather, a.season, a.time)
        == (b.course_set_id, b.ground_condition, b.weather, b.season, b.time)
}

/// Each of `cur`'s umas' baseline uma (None: added since). Umas keep their order (adding appends, duplicating inserts
/// after the original, removing drops one), so this is the order-keeping alignment that pairs the most alike: the
/// same uma unchanged first, then the same costume, then any uma in its place (an uma changed to another costume).
pub(super) fn pairing(base: &RaceInput, cur: &RaceInput) -> Vec<Option<usize>> {
    let (b, c) = (&base.horses, &cur.horses);
    let score = |i: usize, j: usize| if horse_same(&b[i], &c[j]) { 3 } else if b[i].card_id == c[j].card_id { 2 } else { 1 };
    // best[i][j]: best total pairing b[i..] with c[j..].
    let mut best = vec![vec![0u32; c.len() + 1]; b.len() + 1];
    for i in (0..b.len()).rev() {
        for j in (0..c.len()).rev() {
            best[i][j] = (best[i + 1][j + 1] + score(i, j)).max(best[i + 1][j]).max(best[i][j + 1]);
        }
    }
    let (mut out, mut i, mut j) = (vec![None; c.len()], 0, 0);
    while i < b.len() && j < c.len() {
        if best[i][j] == best[i + 1][j + 1] + score(i, j) {
            out[j] = Some(i);
            (i, j) = (i + 1, j + 1);
        } else if best[i][j] == best[i + 1][j] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// Some uma is in both the baseline and the setup, so runs can be compared.
pub(super) fn comparable(base: &RaceInput, cur: &RaceInput) -> bool {
    pairing(base, cur).iter().any(Option::is_some)
}

/// Uma `i` of `cur` as it is on the baseline (None: no baseline, or added since).
pub(super) fn base_horse<'b>(base: Option<&'b RaceInput>, cur: &RaceInput, i: usize) -> Option<&'b HorseInput> {
    let b = base?;
    pairing(b, cur).get(i).copied().flatten().map(|j| &b.horses[j])
}

/// Per horse: differs from the baseline, or added since (all false when there is nothing to compare with).
pub(super) fn edited_mask(base: Option<&RaceInput>, cur: &RaceInput) -> Vec<bool> {
    match base.filter(|b| comparable(b, cur)) {
        Some(b) => pairing(b, cur).iter().zip(&cur.horses).map(|(p, c)| p.is_none_or(|j| !horse_same(&b.horses[j], c))).collect(),
        None => vec![false; cur.horses.len()],
    }
}

/// Anything to diff against the baseline? Conditions, umas edited, added or removed, with some uma in both.
pub(super) fn has_edits(base: Option<&RaceInput>, cur: &RaceInput) -> bool {
    base.is_some_and(|b| {
        let p = pairing(b, cur);
        p.iter().any(Option::is_some)
            && (!conditions_same(b, cur) || b.horses.len() != cur.horses.len() || p.iter().zip(&cur.horses).any(|(p, c)| p.is_none_or(|j| !horse_same(&b.horses[j], c))))
    })
}

/// Setup differs from the baseline in any way: offer a reset.
pub(super) fn differs(base: &RaceInput, cur: &RaceInput) -> bool {
    !comparable(base, cur) || has_edits(Some(base), cur) || base.random_seed != cur.random_seed
}

/// Back to the baseline: input, trainer names and the course view.
pub(super) fn reset_to_baseline(mut s: S) {
    let (Some(b), Some(race)) = (s.baseline.cloned(), s.race.cloned()) else { return };
    if b.course_set_id != s.input.peek().course_set_id {
        s.cview.set(Some(Arc::new(course_view(&race, b.course_set_id))));
    }
    let n = b.horses.len();
    let saved = race.path.is_some() && race.trainers.len() == n;
    s.trainers.set(if saved { race.trainers.clone() } else { vec![String::new(); n] });
    s.input.set((*b).clone());
    s.focus.set(None);
    s.editing.set(None);
    *s.rev.write() += 1;
}

/// A fresh race seed from the clock (non-negative, like the game's).
pub(super) fn random_seed() -> i32 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    (t.as_nanos() % 2_000_000_000) as i32
}

/// Gate `i` of `field` if it is costume `card`, else the first uma of that costume: the same uma across a past run
/// and the current field.
pub(super) fn same_uma(field: &[HorseInput], i: usize, card: i32) -> Option<usize> {
    field.get(i).is_some_and(|h| h.card_id == card).then_some(i).or_else(|| field.iter().position(|h| h.card_id == card))
}

/// Selects field uma `i`; the results focus follows to that uma in the last run (none if it isn't in it).
pub(super) fn select(mut s: S, i: usize) {
    s.sel.set(i);
    let card = s.input.peek().horses.get(i).map(|h| h.card_id);
    let at = s.res.peek().as_ref().zip(card).and_then(|(r, c)| same_uma(&r.input.horses, i, c));
    s.focus.set(at);
}

/// Focuses gate `i` of the last run (uma `card`); the selection follows when that uma is still in the field.
pub(super) fn focus_gate(mut s: S, i: usize, card: i32) {
    s.focus.set(Some(i));
    let at = same_uma(&s.input.peek().horses, i, card);
    if let Some(j) = at {
        s.sel.set(j);
    }
}

pub(super) fn edit(mut s: S, i: usize, f: impl FnOnce(&mut HorseInput)) {
    f(&mut s.input.write().horses[i]);
    *s.rev.write() += 1;
}

/// Gives uma `i` skill `k`, unless she already has it.
pub(super) fn add_skill(s: S, i: usize, k: SkillInput) {
    edit(s, i, |h| {
        if !h.skills.iter().any(|x| x.skill_id == k.skill_id) {
            h.skills.push(k);
        }
    });
}

/// `k` as newly learned: uniques at level 4, the rest at 1.
pub(super) fn learned(k: &SkillInfo) -> SkillInput {
    SkillInput { skill_id: k.id, level: if k.is_unique() { 4 } else { 1 } }
}

pub(super) fn edit_race(mut s: S, f: impl FnOnce(&mut RaceInput)) {
    f(&mut s.input.write());
    *s.rev.write() += 1;
}

pub(super) fn stat_mut(h: &mut HorseInput, k: usize) -> &mut i32 {
    match k {
        0 => &mut h.speed,
        1 => &mut h.stamina,
        2 => &mut h.pow,
        3 => &mut h.guts,
        _ => &mut h.wiz,
    }
}

pub(super) fn stats(h: &HorseInput) -> [i32; 5] {
    [h.speed, h.stamina, h.pow, h.guts, h.wiz]
}

/// Puts `v` back into input `id` (after a rejected or clamped entry, the box would otherwise keep the typed text).
pub(super) fn resync(id: &str, v: impl std::fmt::Display) {
    document::eval(&format!("{{ const e = document.getElementById('{id}'); if (e) e.value = '{v}'; }}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A field of umas by costume, `speed` marking an edit.
    fn field(umas: &[(i32, i32)]) -> RaceInput {
        RaceInput { horses: umas.iter().map(|&(card_id, speed)| HorseInput { card_id, speed, ..Default::default() }).collect(), ..Default::default() }
    }

    #[test]
    fn pairing_follows_umas() {
        let base = field(&[(1, 0), (2, 0), (3, 0)]);
        // Added at the end, removed from the middle, duplicated, costume changed in place.
        assert_eq!(pairing(&base, &field(&[(1, 0), (2, 0), (3, 0), (4, 0)])), [Some(0), Some(1), Some(2), None]);
        assert_eq!(pairing(&base, &field(&[(1, 0), (3, 0)])), [Some(0), Some(2)]);
        assert_eq!(pairing(&base, &field(&[(1, 0), (2, 0), (2, 0), (3, 0)])), [Some(0), Some(1), None, Some(2)]);
        assert_eq!(pairing(&base, &field(&[(1, 0), (5, 0), (3, 0)])), [Some(0), Some(1), Some(2)]);
        // Removed one of two of the same costume: the other keeps its pair (unchanged beats same costume).
        let twins = field(&[(1, 10), (1, 20)]);
        assert_eq!(pairing(&twins, &field(&[(1, 20)])), [Some(1)]);
        // Removed and added: the rest pair, the new one doesn't take the removed one's place.
        assert_eq!(pairing(&base, &field(&[(1, 0), (3, 0), (4, 0)])), [Some(0), Some(2), None]);
        assert!(has_edits(Some(&base), &field(&[(1, 0), (2, 0), (3, 0), (4, 0)])));
        assert!(!has_edits(Some(&base), &base));
    }
}
