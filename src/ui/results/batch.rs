//! Batch results: the per-uma stats table.

use super::*;
use super::detail::horse_detail;
use crate::charts::*;

/// Inline mini histogram of finishing places.
fn mini_hist(h: &[f32], color: &str) -> Element {
    let max = h.iter().cloned().fold(0.0001, f32::max);
    rsx! {
        div { class: "mini", style: "--c: {color}",
            for (k, v) in h.iter().enumerate() {
                div { title: "{k + 1}: {pct(*v)}", style: "height: {v / max * 100.0}%" }
            }
        }
    }
}

pub(super) fn batch_view(s: S, res: &Res, names: &[String], cur: &Arc<Batch>, base: Option<&Arc<Batch>>) -> Element {
    let mut focus = s.focus;
    let f = *focus.read();
    let diffs = base.map(|b| diff_batches(b, cur));
    let n = cur.horses.len();
    let ed = &res.ed_mask;
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| cur.horses[a].place.mean.total_cmp(&cur.horses[b].place.mean));
    let (lo, hi) = time_scale(cur.horses.iter().chain(base.map(|b| &b.horses[..]).unwrap_or(&[])).map(|h| &h.finish_time));
    let improved = diffs.as_ref().is_some_and(|d| d.iter().zip(ed).any(|(d, &m)| m && d.win_rate.1 > d.win_rate.0 + 0.005));
    rsx! {
        section { class: "card",
            div { class: "card-h",
                h3 { "{cur.seeds.len()} races" if base.is_some() { ": baseline → edited" } }
                span { class: "muted", "click an uma for details" }
                if improved { img { class: "great", src: "/a/great.webp", alt: "Great!" } }
            }
            div { class: "scroll-x",
                table { class: "grid results batch",
                    thead {
                        tr {
                            th { "Uma" }
                            th { class: "num", "Win" }
                            th { class: "num", "Avg place" }
                            th { class: "c-hist", "Places 1 to {n}" }
                            th { "Finish time (p10-p90)" }
                            th { class: "num", title: "Full spurt: on time from the final-leg start and at max spurt speed. HP out: ran out of stamina.", "Full spurt / HP out" }
                            if base.is_some() { th { class: "num", title: "Paired per seed: how far ahead (+) or behind (-) the edited setup finishes, in horse lengths (2.5 m)", "Δ lengths (mean)" } }
                        }
                    }
                    tbody {
                        for i in order {
                            {
                                let h = &cur.horses[i];
                                let d = diffs.as_ref().map(|d| &d[i]);
                                rsx! {
                                    tr {
                                        key: "{i}",
                                        class: if f == Some(i) { "sel" } else if ed[i] { "edited" },
                                        onclick: move |_| focus.set(if f == Some(i) { None } else { Some(i) }),
                                        td { class: "who", {who(res, names, i)} }
                                        td { class: "num",
                                            b { "{pct(h.win_rate)}" }
                                            if let Some(d) = d {
                                                if pct(d.win_rate.1) != pct(d.win_rate.0) {
                                                    div { class: "was", "{pct(d.win_rate.0)} " {delta(d.win_rate.1 - d.win_rate.0, false, format!("{:.1} pp", (d.win_rate.1 - d.win_rate.0).abs() * 100.0))} }
                                                }
                                            }
                                        }
                                        td { class: "num",
                                            "{h.place.mean:.2}"
                                            if let Some(d) = d {
                                                if format!("{:.2}", d.mean_place.1) != format!("{:.2}", d.mean_place.0) {
                                                    div { class: "was", "{d.mean_place.0:.2} " {delta(d.mean_place.1 - d.mean_place.0, true, format!("{:.2}", (d.mean_place.1 - d.mean_place.0).abs()))} }
                                                }
                                            }
                                        }
                                        td { class: "c-hist", {mini_hist(&h.place_hist, &res.colors[i])} }
                                        td { class: "boxcell",
                                            BoxRow {
                                                p10: h.finish_time.p10,
                                                p25: h.finish_time.p25,
                                                p50: h.finish_time.p50,
                                                p75: h.finish_time.p75,
                                                p90: h.finish_time.p90,
                                                lo,
                                                hi,
                                                color: res.colors[i].clone(),
                                            }
                                            span { class: "small muted", "{ft(h.finish_time.p50)}" }
                                            if h.finish_time.p50 > hi { span { class: "small off", title: "Far behind the field; the box is clipped at the right edge", " off scale" } }
                                        }
                                        td { class: "num",
                                            "{pct(h.full_spurt_rate)}"
                                            if let Some(d) = d {
                                                if pct(d.full_spurt_rate.1) != pct(d.full_spurt_rate.0) { div { class: "was", "was {pct(d.full_spurt_rate.0)}" } }
                                            }
                                            div { class: if h.hp_out_rate > 0.0 { "small no" } else { "small muted" }, title: "Ran out of HP", "{pct(h.hp_out_rate)} out" }
                                        }
                                        if let Some(d) = d {
                                            td { class: "num",
                                                {len_delta(d.lengths.mean)}
                                                if d.time_delta.p50.abs() >= 0.0005 {
                                                    div { class: "was", "median {d.time_delta.p50:+.3}s" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(i) = f.filter(|&i| i < n) {
            {horse_detail(s, res, names, i, cur, base, diffs.as_ref().map(|d| &d[i]))}
        }
    }
}

/// Shared finish-time axis for the box plots: robust to a straggler (a horse far behind would squeeze every other
/// row), so the range covers the field's p10-p90 within a few seconds of the median finisher.
fn time_scale<'a>(ds: impl Iterator<Item = &'a Dist>) -> (f32, f32) {
    let ds: Vec<&Dist> = ds.collect();
    let mut mids: Vec<f32> = ds.iter().map(|d| d.p50).collect();
    mids.sort_by(f32::total_cmp);
    let mid = mids.get(mids.len() / 2).copied().unwrap_or(0.0);
    let lo = ds.iter().map(|d| d.p10).fold(f32::MAX, f32::min).max(mid - 4.0);
    let hi = ds.iter().map(|d| d.p90).fold(f32::MIN, f32::max).min(mid + 4.0);
    (lo, hi.max(lo + 0.1))
}
