//! Other Mechanics subtab of the batch uma detail: what each mechanic not tied to one stat (`MECHANICS`) gains, the uma
//! as set up against the same with that mechanic muted, on the run's field, over their own seed count.

use super::*;
use crate::ui::wit::mechanic_results;
use uma_race_sim::api::span_kind;

/// Horse `h` of the batch, ▲/▼ vs `b` (the same uma on the baseline, when there is one): full spurt and the conserve
/// power boost, straight from the batch.
pub(super) fn mechanics_cards(h: &HorseStats, b: Option<&HorseStats>) -> Element {
    let cp = |x: &HorseStats| x.states[span_kind::CONSERVE_POWER as usize];
    let vs = |f: &dyn Fn(&HorseStats) -> f32, txt: &dyn Fn(f32) -> String| {
        b.map(|b| {
            let d = f(h) - f(b);
            delta(d, false, txt(d.abs()))
        })
    };
    let (rate, secs) = cp(h);
    rsx! {
        div { class: "kpis",
            div { b { "{pct(h.full_spurt_rate)}" } div { class: "kpi2", span { "full spurt" } {vs(&|x| x.full_spurt_rate, &pp)} } }
            div { b { "{pct(rate)}" } div { class: "kpi2", span { "power conserve" } {vs(&|x| cp(x).0, &pp)} } }
            div { b { "{secs:.2} s" } div { class: "kpi2", span { "boost" } {vs(&|x| cp(x).1, &|v| format!("{v:.2} s"))} } }
        }
    }
}

/// Horse `i` of the batch on screen: the length calculation's button (beside the detail's subtabs).
#[component]
pub(super) fn MechanicsRun(i: usize) -> Element {
    let s = use_context::<S>();
    let running = use_busy(s);
    let Some((input, rivals)) = setup_for(s, i) else { return rsx! {} };
    let pow = input.horses[i].pow;
    let n = *s.m_n.read();
    // Per seed: as set up, then each mechanic muted.
    let runs = n * (MECHANICS.len() + 1);
    let work = runs * input.horses.len();
    let eta = eta(s, work);
    let start = move |_| {
        let Some(race) = s.race.cloned() else { return };
        let (input, rivals) = (input.clone(), rivals.clone());
        let seeds = seeds(input.random_seed, n);
        let rev = *s.rev.peek();
        let label = format!("Simulating {} races", thousands(runs));
        let run = move |w: &Work| Arc::new(mechanics(&race, &input, rivals.as_ref(), i, &seeds, w));
        measure(s, label, n, work, run, move |mut s, rows| {
            s.m_sel.set(None);
            s.m_res.set(Some(MechRes { horse: i, rows, rev }));
        });
    };
    rsx! {
        div { class: "runn",
            button { class: "btn small", disabled: running(), onclick: start, "Calculate length differences" }
            {seeds_input("mn", s.m_n, 10, 20000)}
        }
        span { class: "chip", "Power {pow}" }
        span { class: "muted small", "{thousands(runs)} races, {eta}" }
        Progress {}
    }
}

/// The measurement of horse `i`, once simulated.
#[component]
pub(super) fn MechanicsOut(i: usize) -> Element {
    let s = use_context::<S>();
    let stale = use_stale(s, move || s.m_res.read().as_ref().map(|r| r.rev));
    let Some(r) = s.m_res.cloned().filter(|r| r.horse == i) else { return rsx! {} };
    let rows = &r.rows;
    // Per mechanic: what it changes besides lengths, as set up (muted it does nothing).
    let effect = |k: usize| -> Element {
        let v = &rows[k];
        let on: Vec<f32> = v.iter().map(|p| p.new.conserve).filter(|&t| t > 0.0).collect();
        if on.is_empty() {
            return rsx! { span { class: "muted", "-" } };
        }
        let secs = on.iter().sum::<f32>() / on.len() as f32;
        rsx! { "Released {pct(on.len() as f32 / v.len() as f32)} · {secs:.2} s boost" }
    };
    // ponytail: no Total row while there is one mechanic; mute them all at once (like Wit's ALL) once there are more.
    mechanic_results(&MECHANICS, rows, None, s.m_sel, stale(), effect)
}
