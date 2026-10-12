//! Race-tab results: single run (finish table, positions, activations) and batch (per-uma stats + detail), each
//! optionally compared with the baseline.

mod batch;
mod detail;
mod single;

use super::*;
use batch::batch_view;
use single::single_view;
pub(super) use detail::{SPURT_KINDS, spurt_and_hp};

#[component]
pub fn ResultsTab() -> Element {
    let s = use_context::<S>();
    let has = s.res.read().is_some();
    rsx! {
        if has {
            Results {}
        } else {
            section { class: "card empty-res",
                h3 { "No run yet" }
                p { class: "muted", "Set up the race on the left, then run it once or many times." }
            }
        }
    }
}

#[component]
pub fn Results() -> Element {
    let s = use_context::<S>();
    let stale = use_stale(s, move || s.res.read().as_ref().map(|r| r.rev));
    let Some(res) = s.res.cloned() else { return rsx! {} };
    let names: Vec<String> = res.input.horses.iter().map(|h| chara_name(h.chara_id)).collect();
    let body = match &res.kind {
        Kind::Single { cur, base } => single_view(s, &res, &names, cur, base.as_deref()),
        Kind::Batch { cur, base } => batch_view(s, &res, &names, cur, base.as_ref()),
    };
    rsx! {
        div { id: "results", class: "anchor" }
        if stale() {
            div { class: "stale", "The setup changed since this run. Run again to update the results." }
        }
        {body}
    }
}

fn final_leg_marker(cv: &CourseView) -> Vec<(f32, String)> {
    vec![(cv.final_leg_start / cv.distance * 100.0, "Late race".into())]
}

fn who(res: &Res, names: &[String], i: usize) -> Element {
    let sub = subline(&res.input.horses, &res.trainers, i);
    rsx! {
        div { class: "whobox",
            {horse_icon(&res.input.horses[i], Some(&res.colors[i]), "sm")}
            div {
                b { "{names[i]}" }
                span { class: "gate", "#{i + 1}" }
                if res.ed_mask.get(i) == Some(&true) { span { class: "tag", "edited" } }
                // Tags share the second line with the trainer, keeping the column narrow.
                div { class: "muted small row2", {team_tags(&res.input, res.player, i)} if let Some(t) = sub { " {t}" } }
            }
        }
    }
}

/// Signed lengths, green ahead / red behind: "+1.23 L".
fn len_delta(v: f32) -> Element {
    let txt = format!("{v:+.2} L");
    let class = if txt.starts_with("+0.00") || txt.starts_with("-0.00") { "d flat" } else if v > 0.0 { "d good" } else { "d bad" };
    rsx! { span { class: "{class}", title: "Horse lengths (2.5 m) vs the baseline, + = ahead", "{txt}" } }
}
