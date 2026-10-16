//! Setup pane (course and conditions, the field, run controls), the title-bar race context and the course pinned
//! above the analysis tabs.

mod course;
mod drawer;
mod field;
mod npcs;
mod run;

use super::*;
use crate::charts::{Band, Bins, CourseProfile, Region, SkillMark};
use course::CourseCard;
pub use drawer::{Drawer, Picker};
pub use npcs::NpcDrawer;
use field::FieldCard;
use run::RunBar;
pub(super) use run::filled;

#[component]
pub fn SetupTab() -> Element {
    rsx! {
        crate::ui::saves::RaceHeader {}
        CourseCard {}
        FieldCard {}
        RunBar {}
    }
}

/// Race context for the title bar: title, subtitle and status chips.
#[component]
pub fn RaceContext() -> Element {
    let s = use_context::<S>();
    let Some(race) = s.race.cloned() else { return rsx! {} };
    let input = s.input.read();
    let title = race_title(&race, &input);
    let subtitle = race_subtitle(&race, &input);
    let verify = s.verify.cloned();
    let edited = race.path.is_some() && differs(&race.input, &input);
    rsx! {
        div { class: "ctx",
            b { class: "tb-title", title: "{title}", "{title}" }
            span { class: "tb-sub", title: "{subtitle}", "{subtitle}" }
            if race.path.is_some() {
                span { class: "chip date", "Saved {date(&race.date)}" }
                if edited { span { class: "chip edited", "edited" } }
                match verify {
                    None => rsx! { span { class: "chip", "checking…" } },
                    Some(None) => rsx! {
                        span { class: "chip ok", title: "Our simulation of the saved seed reproduces the server's recorded race bit for bit, every frame.", "✓ server" }
                    },
                    Some(Some(m)) => rsx! { span { class: "chip no", title: "First divergence from the server replay: {m}", "✗ differs" } },
                }
            }
        }
    }
}

/// Band of activation distances, first to last (at least a sliver wide), and how often it activates along it: (from m,
/// to m, share of the busiest bin) in ~1/120-course bins, so it reads as where it starts, not how long it lasts. None
/// when it never activated.
fn span(distances: &[f32], course: f32) -> Option<(f32, f32, Bins)> {
    let mut d = distances.to_vec();
    d.sort_by(f32::total_cmp);
    let (&a, &last) = (d.first()?, d.last()?);
    let b = last.max(a + course * 0.008);
    let bw = course / 120.0;
    let nb = ((b - a) / bw).ceil().max(1.0) as usize;
    let mut counts = vec![0usize; nb];
    for &x in d.iter().filter(|&&x| x >= a && x <= b) {
        counts[(((x - a) / bw) as usize).min(nb - 1)] += 1;
    }
    let max = *counts.iter().max().unwrap_or(&1) as f32;
    let bins = counts.iter().enumerate().filter(|c| *c.1 > 0).map(|(i, &c)| (a + i as f32 * bw, (a + (i + 1) as f32 * bw).min(b), c as f32 / max)).collect();
    Some((a, b, bins))
}

/// The band of one skill (an open skill or unique), as the only region.
fn one_region(name: &str, distances: &[f32], course: f32) -> Vec<Region> {
    span(distances, course).map(|(a, b, bins)| (a, b, name.to_string(), "var(--accent)".to_string(), bins)).into_iter().collect()
}

/// A batch uma's median race: the seed where her finish time is the median of the batch's, run again in full.
#[derive(Clone)]
struct MedianRun {
    run: Arc<Run>,
    horse: usize,
    seed: i32,
    seeds: usize,
    time: f32,
    place: u8,
}

impl PartialEq for MedianRun {
    fn eq(&self, o: &Self) -> bool {
        Arc::ptr_eq(&self.run, &o.run)
    }
}

/// The focused uma's median race of the shown batch (None: a single run, no focus, or no batch).
fn median_run(s: S) -> Option<MedianRun> {
    let (res, race, horse) = (s.res.cloned()?, s.race.cloned()?, (*s.focus.read())?);
    let Kind::Batch { cur, .. } = &res.kind else { return None };
    let times = &cur.horses.get(horse)?.times;
    let mut order: Vec<usize> = (0..times.len()).collect();
    order.sort_by(|&a, &b| times[a].total_cmp(&times[b]));
    let k = *order.get(order.len() / 2)?;
    let seed = cur.seeds[k];
    let run = Arc::new(run_once(&race, &RaceInput { random_seed: seed, ..(*res.input).clone() }));
    Some(MedianRun { run, horse, seed, seeds: cur.seeds.len(), time: times[k], place: cur.horses[horse].places[k] })
}

/// The course, pinned above the analysis tabs. Regions follow the tab: Results = the focused uma's activations pinned
/// on the course (all in small fields; single runs; a batch shows the focused uma's median race), Skill gains = the
/// open skill, Uma ranking = the open unique.
#[component]
pub fn TopCourse() -> Element {
    let s = use_context::<S>();
    let median = use_memo(move || median_run(s));
    let Some(cv) = s.cview.cloned() else { return rsx! {} };
    let tab = *s.tab.read();
    let same = |c: &CourseView| c.info.id == cv.info.id;
    let mut hint = None;
    let mut median_label = None;
    let mut marks: Vec<SkillMark> = vec![];
    let mut speed = vec![];
    let mut bands: Vec<Band> = vec![];
    let regions: Vec<Region> = match tab {
        Tab::Results => match s.res.cloned().filter(|r| same(&r.cv)) {
            Some(r) => {
                match &r.kind {
                    Kind::Single { cur, .. } => {
                        let cs = activations(&r, cur, *s.focus.read());
                        (marks, speed) = (cs.marks, cs.speed);
                        bands = states(cur, *s.focus.read());
                        if cs.need_focus {
                            hint = Some("Select an uma to see where its skills activate.");
                        }
                    }
                    // A batch: the focused uma's median race, as a single run.
                    Kind::Batch { .. } => {
                        if let Some(m) = median() {
                            let cs = activations(&r, &m.run, Some(m.horse));
                            (marks, speed) = (cs.marks, cs.speed);
                            bands = states(&m.run, Some(m.horse));
                            let place = crate::ui::cm::ordinal(m.place as usize);
                            median_label = Some(format!("Median race of {} seeds · seed {} · {} · {place}", thousands(m.seeds), m.seed, ft(m.time)));
                        }
                    }
                }
                vec![]
            }
            None => vec![],
        },
        Tab::Gains => {
            let (g, id) = (s.g_res.cloned().filter(|g| same(&g.cv)), *s.g_sel.read());
            let it = g.as_ref().and_then(|g| g.items.iter().find(|x| Some(x.skill_id) == id));
            it.map_or(vec![], |it| one_region(&skill_info(it.skill_id).name, &it.distances, cv.distance))
        }
        Tab::Cm => vec![],
        Tab::Ranking => {
            let (r, sel) = (s.r_res.cloned().filter(|r| same(&r.cv)), *s.r_sel.read());
            let x = r.as_ref().zip(sel).and_then(|(r, (card, st))| r.items.iter().find(|x| x.card_id == card && x.style == st));
            let name = |card| card_info(card).map(|c| skill_info(c.unique_skill).name).unwrap_or_default();
            x.map_or(vec![], |x| one_region(&name(x.card_id), &x.distances, cv.distance))
        }
    };
    rsx! {
        div { class: "topchart", id: "topchart",
            CourseProfile {
                distance: cv.distance,
                elevation: cv.elevation.clone(),
                corners: cv.corner_spans(),
                straights: cv.straights.iter().map(|c| (c.start, c.end)).collect::<Vec<_>>(),
                slopes: cv.slopes.iter().map(|c| (c.start, c.end, c.grade)).collect::<Vec<_>>(),
                phases: cv.phases.to_vec(),
                final_leg_start: cv.final_leg_start,
                regions,
                marks,
                bands,
                speed,
            }
            if let Some(h) = hint { p { class: "muted small hint", "{h}" } }
            if let Some(l) = median_label { p { class: "small hint median", "{l}" } }
        }
    }
}

/// The focused uma's race states and position keep modes in run `cur`, a band row per one she was in.
fn states(cur: &Run, focus: Option<usize>) -> Vec<Band> {
    focus.and_then(|f| cur.horses.get(f)).map_or(vec![], course_bands)
}

/// At most this many skills in the pinned course when it shows several umas (the full list is in Skill activations).
const MAX_MARKS: usize = 8;

/// A skill's colour on the course, by what it does.
fn effect_color(id: i32) -> &'static str {
    match skill_info(id).effects.first().copied() {
        Some("Speed") => "var(--sk-speed)",
        Some("Acceleration") => "var(--sk-accel)",
        Some("Recovery") => "var(--good)",
        Some("Debuff" | "Stamina drain" | "Negative") => "var(--bad)",
        _ => "var(--muted)",
    }
}

/// Single-run skill effects for the course preview.
struct CourseSkills {
    marks: Vec<SkillMark>,
    /// The focused uma's speed, (m, m/s) to the goal.
    speed: Vec<(f32, f32)>,
    /// Too many umas to show: pick one.
    need_focus: bool,
}

/// Run `cur`'s skill effects: the focused uma's (coloured by what they do) with her speed, or everyone's in small
/// fields (in their colours, no speed). Passive skills (whole race) are left out.
fn activations(res: &Res, cur: &Run, focus: Option<usize>) -> CourseSkills {
    let mut cs = CourseSkills { marks: vec![], speed: vec![], need_focus: false };
    let n = res.input.horses.len();
    let who: Vec<usize> = match focus {
        Some(f) if f < n => vec![f],
        _ if n <= 6 => (0..n).collect(),
        _ => {
            cs.need_focus = true;
            return cs;
        }
    };
    let one = who.len() == 1;
    for a in cur.activations.iter().filter(|a| who.contains(&a.horse) && !a.passive) {
        let info = skill_info(a.skill_id);
        let finish = cur.out.results.get(a.horse).map_or(f32::MAX, |r| r.finish_time_raw);
        let what = skill_parts(a.skill_id).first().map(|p| p.effects.join(", ")).unwrap_or_default();
        let secs = a.end_time - a.time;
        let when = if secs < 0.01 {
            format!("at {:.0} m ({})", a.distance, ft(a.time))
        } else {
            format!("{:.0} to {:.0} m · {secs:.1} s ({} to {})", a.distance, a.end, ft(a.time), ft(a.end_time))
        };
        let cut = if secs >= 0.01 && a.end_time >= finish - 0.001 { " · cut short by the finish" } else { "" };
        let who_line = if one { String::new() } else { format!("{}\n", horse_name(&res.input.horses[a.horse])) };
        cs.marks.push(SkillMark {
            start: a.distance,
            end: a.end,
            color: if one { effect_color(a.skill_id).to_string() } else { res.colors[a.horse].clone() },
            tip: format!("{who_line}{}\n{what}\n{when}{cut}", info.name),
            name: info.name,
        });
    }
    if !one {
        cs.marks.truncate(MAX_MARKS);
        return cs;
    }
    let h = who[0];
    let mut speed = vec![];
    for f in cur.out.frames.iter() {
        let hf = &f.horses[h];
        speed.push((hf.distance.min(cur.goal), hf.speed));
        if hf.distance >= cur.goal {
            break;
        }
    }
    cs.speed = speed;
    cs
}
