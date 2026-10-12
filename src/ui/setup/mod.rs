//! Setup pane (course and conditions, the field, run controls), the title-bar race context and the course pinned
//! above the analysis tabs.

mod course;
mod drawer;
mod field;
mod run;

use super::*;
use crate::charts::{Bins, CourseProfile, Region};
use course::CourseCard;
pub use drawer::{Drawer, Picker};
use field::FieldCard;
use run::RunBar;

#[component]
pub fn SetupTab() -> Element {
    rsx! {
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

/// p10-p90 band of activation distances (at least a sliver wide) and how often it activates along it: (from m, to m,
/// share of the busiest bin) in ~1/120-course bins, so it reads as where it starts, not how long it lasts. None when
/// it never activated.
fn span(distances: &[f32], course: f32) -> Option<(f32, f32, Bins)> {
    let mut d: Vec<f32> = distances.iter().copied().filter(|&x| x >= 1.0).collect();
    d.sort_by(f32::total_cmp);
    let q = |p: f32| d[((d.len() - 1) as f32 * p).round() as usize];
    if d.is_empty() {
        return None;
    }
    let (a, b) = (q(0.1), q(0.9).max(q(0.1) + course * 0.008));
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
    span(distances, course).map(|(a, b, bins)| (a, b, format!("{name} (p10-p90)"), "var(--accent)".to_string(), bins)).into_iter().collect()
}

/// The course, pinned above the analysis tabs. Regions follow the tab: Results = the focused uma's activations (all
/// in small fields), Skill gains = the open skill, Uma ranking = the open unique.
#[component]
pub fn TopCourse() -> Element {
    let s = use_context::<S>();
    let Some(cv) = s.cview.cloned() else { return rsx! {} };
    let tab = *s.tab.read();
    let same = |c: &CourseView| c.info.id == cv.info.id;
    let mut hint = None;
    let regions: Vec<Region> = match tab {
        Tab::Results => match s.res.cloned().filter(|r| same(&r.cv)) {
            // A skill open in the batch uma detail: just that skill.
            Some(Res { kind: Kind::Batch { cur, .. }, .. }) if s.skill_sel.read().is_some_and(|(h, _)| Some(h) == *s.focus.read()) => {
                let (h, id) = s.skill_sel.read().unwrap_or_default();
                let st = cur.horses.get(h).and_then(|hs| hs.skills.iter().find(|k| k.skill_id == id));
                st.map_or(vec![], |st| one_region(&skill_info(id).name, &st.distances, cv.distance))
            }
            Some(r) => {
                let (reg, need) = regions(&r, *s.focus.read());
                if need {
                    hint = Some("Select an uma to see where its skills activate.");
                }
                reg
            }
            None => vec![],
        },
        Tab::Gains => {
            let (g, id) = (s.g_res.cloned().filter(|g| same(&g.cv)), *s.g_sel.read());
            let it = g.as_ref().and_then(|g| g.items.iter().find(|x| Some(x.skill_id) == id));
            it.map_or(vec![], |it| one_region(&skill_info(it.skill_id).name, &it.distances, cv.distance))
        }
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
            }
            if let Some(h) = hint { p { class: "muted small hint", "{h}" } }
        }
    }
}

/// At most this many regions in the pinned course (each label takes a lane; the full list is in Skill activations).
const MAX_REGIONS: usize = 8;

/// Activation regions for the course preview: the focused uma, or everyone in small fields. True = too many umas to
/// show, pick one.
fn regions(res: &Res, focus: Option<usize>) -> (Vec<Region>, bool) {
    let n = res.input.horses.len();
    let who: Vec<usize> = match focus {
        Some(f) if f < n => vec![f],
        _ if n <= 6 => (0..n).collect(),
        _ => return (vec![], true),
    };
    let mut out = vec![];
    match &res.kind {
        Kind::Single { cur, .. } => {
            // Activations are points; draw them as short ticks.
            let len = res.cv.distance * 0.012;
            for a in cur.activations.iter().filter(|a| who.contains(&a.horse) && a.distance >= 1.0).take(MAX_REGIONS) {
                let end = (a.distance + len).min(res.cv.distance);
                out.push((a.distance, end, skill_info(a.skill_id).name, res.colors[a.horse].clone(), vec![]));
            }
        }
        Kind::Batch { cur, .. } => {
            let mut rated = vec![];
            for &h in &who {
                for st in &cur.horses[h].skills {
                    if let Some((a, b, bins)) = span(&st.distances, res.cv.distance) {
                        let label = format!("{} {:.0}%", skill_info(st.skill_id).name, st.rate * 100.0);
                        rated.push((st.rate, (a, b, label, res.colors[h].clone(), bins)));
                    }
                }
            }
            // The most frequent ones.
            rated.sort_by(|x, y| y.0.total_cmp(&x.0));
            out.extend(rated.into_iter().take(MAX_REGIONS).map(|x| x.1));
        }
    }
    (out, false)
}
