//! Run controls: one run or a batch, paired with the baseline when there are edits.

use super::*;

fn run(s: S, n: Option<usize>) {
    let (Some(race), Some(cv)) = (s.race.cloned(), s.cview.cloned()) else { return };
    if !claim(s) {
        return;
    }
    // Empty gates up to the field size take NPCs (the same ones in the baseline run).
    let (npcs, size) = (s.npcs.cloned(), *s.field_size.peek());
    let fill = |inp: &RaceInput| if npcs.fill { npc_field(&race, inp, 0, false, &npcs, size).0 } else { inp.clone() };
    let setup = s.input.cloned();
    let base = s.baseline.cloned().filter(|b| has_edits(Some(b), &setup)).map(|b| Arc::new(fill(&b)));
    let input = Arc::new(fill(&setup));
    let mut trainers = s.trainers.cloned();
    trainers.resize(input.horses.len(), String::new());
    let trainers = Arc::new(trainers);
    // NPCs come after the umas: the player keeps her gate.
    let player = player_now(&race, &setup);
    let colors = Arc::new(uma_colors(&input, player));
    let ed_mask = edited_mask(base.as_deref(), &input);
    let rev = *s.rev.peek();
    bg(s, async move {
        let mut s = s;
        let (r, inp) = (race.clone(), input.clone());
        let pair = base.is_some();
        let out = match n {
            None => {
                let label = if pair { "Running baseline and edited" } else { "Running" };
                job(s, label, 0, false, move |_| {
                    let cur = Arc::new(run_once(&r, &inp));
                    // Same seed as the edited run, so the pair differs only by the edits.
                    let base = base.map(|b| Arc::new(run_once(&r, &RaceInput { random_seed: inp.random_seed, ..(*b).clone() })));
                    Kind::Single { cur, base }
                })
                .await
            }
            Some(n) => {
                let total = n * if pair { 2 } else { 1 };
                job(s, &format!("Simulating {} races", thousands(total)), total, true, move |w| {
                    let seeds = seeds(inp.random_seed, n);
                    let base = base.map(|b| Arc::new(run_batch(&r, &b, &seeds, w)));
                    let cur = Arc::new(run_batch(&r, &inp, &seeds, w));
                    Kind::Batch { cur, base }
                })
                .await
            }
        };
        match out {
            // A cancelled batch covers only some seeds: drop it.
            Ok((_, true)) => {}
            Ok((kind, false)) => {
                s.res.set(Some(Res { kind, input, trainers, colors, player, ed_mask, cv, rev }));
                go(s, Tab::Results);
                reveal("pane-results");
            }
            Err(e) => s.err.set(Some(e)),
        }
    });
}

#[component]
pub(super) fn RunBar() -> Element {
    let s = use_context::<S>();
    let input = s.input.read();
    let base = s.baseline.cloned();
    let pair = has_edits(base.as_deref(), &input);
    let seed = input.random_seed;
    drop(input);
    let running = use_busy(s);
    let n = *s.n.read();
    rsx! {
        section { class: "runbar sticky",
            button { class: "btn big", disabled: running(), onclick: move |_| run(s, None),
                "Run once "
                small { "seed {seed}" }
            }
            div { class: "runn",
                button { class: "btn big alt", disabled: running(), onclick: move |_| run(s, Some(n)), "Run ×{n}" }
                {seeds_input("runs", s.n, 1, 20000)}
            }
            span { class: "muted run-hint",
                if pair {
                    "Runs compare the baseline with your edits on the same seeds."
                } else if base.is_some() {
                    "No edits yet: change an uma or the conditions to compare with the baseline."
                } else {
                    "Tip: pin a baseline, then edit, to see what each change does."
                }
            }
            Progress {}
        }
    }
}
