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
                (prepare(r.race), r.input, r.trainers, r.baseline, cv, v)
            })
        })
        .await;
        let Ok((Some((l, input, trainers, baseline, cv, v)), _)) = r else { return new_custom(s) };
        install(s, l);
        s.verify.set(v);
        s.input.set(input);
        s.trainers.set(trainers);
        s.baseline.set(baseline.map(Arc::new));
        s.cview.set(Some(cv));
    });
}

pub(super) fn open(mut s: S, path: PathBuf) {
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

/// Same field size as the baseline, so horses pair up by gate.
pub(super) fn comparable(base: &RaceInput, cur: &RaceInput) -> bool {
    base.horses.len() == cur.horses.len()
}

/// Per horse: differs from the baseline (all false when there is nothing to compare with).
pub(super) fn edited_mask(base: Option<&RaceInput>, cur: &RaceInput) -> Vec<bool> {
    match base.filter(|b| comparable(b, cur)) {
        Some(b) => b.horses.iter().zip(&cur.horses).map(|(a, c)| !horse_same(a, c)).collect(),
        None => vec![false; cur.horses.len()],
    }
}

/// Anything to diff against the baseline?
pub(super) fn has_edits(base: Option<&RaceInput>, cur: &RaceInput) -> bool {
    base.is_some_and(|b| comparable(b, cur) && (!conditions_same(b, cur) || b.horses.iter().zip(&cur.horses).any(|(a, c)| !horse_same(a, c))))
}

/// Setup differs from the baseline in any way (incl. field size): offer a reset.
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
