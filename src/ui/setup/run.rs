//! Run controls: one run or a batch, paired with the baseline when there are edits.

use super::*;

/// `inp` as it runs, and its rivals: when the setup fills empty gates, NPCs after the umas up to the field size, or
/// rivals from replays (`inp`'s seed's draw; each seed draws its own, `at_seed`). Err: the rival pool can't fill them.
pub(crate) fn filled(s: S, race: &Race, inp: &RaceInput) -> Result<(RaceInput, Option<Rivals>), String> {
    let npcs = s.npcs.peek().clone();
    let size = *s.field_size.peek();
    if !npcs.fill || size <= inp.horses.len() {
        return Ok((inp.clone(), None));
    }
    if !npcs.rivals.on {
        return Ok((npc_field(race, inp, 0, false, &npcs, size).0, None));
    }
    if let Some(e) = rivals_short(s, size - inp.horses.len()) {
        return Err(e);
    }
    let r = Rivals { pool: load_rivals(race, &rival_teams(s), &npcs.rivals)?, from: inp.horses.len() };
    let mut field = inp.clone();
    field.horses.resize(size, HorseInput::default());
    Ok((r.field(&field, inp.random_seed), Some(r)))
}

fn run(mut s: S, n: Option<usize>) {
    let (Some(race), Some(cv)) = (s.race.cloned(), s.cview.cloned()) else { return };
    // Empty gates up to the field size take NPCs or rivals (the same ones in the baseline run).
    let setup = s.input.cloned();
    let base_setup = s.baseline.cloned().filter(|b| has_edits(Some(b), &setup));
    let fields = filled(s, &race, &setup).and_then(|f| Ok((f, base_setup.as_deref().map(|b| filled(s, &race, b)).transpose()?)));
    let ((input, rivals), base) = match fields {
        Ok(f) => f,
        Err(e) => return s.err.set(Some(e)),
    };
    if !claim(s) {
        return;
    }
    let (base, base_rivals) = base.map_or((None, None), |(b, r)| (Some(Arc::new(b)), r));
    let input = Arc::new(input);
    let mut trainers = s.trainers.cloned();
    trainers.resize(input.horses.len(), String::new());
    if let Some(r) = &rivals {
        trainers = r.trainers(&trainers, input.random_seed, input.horses.len());
    }
    let trainers = Arc::new(trainers);
    // NPCs come after the umas: the player keeps her field index.
    let player = player_now(&race, &setup);
    let colors = Arc::new(uma_colors(&input, player));
    let mut ed_mask = edited_mask(base_setup.as_deref(), &setup);
    // Umas pair up with the baseline's (`pairing`); NPCs only when the umas are as many, so they keep their field index.
    let mut pairs = base_setup.as_deref().map_or(vec![], |b| pairing(b, &setup));
    if let (Some(b), Some(bf)) = (base_setup.as_deref(), base.as_deref()) {
        let same_gates = b.horses.len() == setup.horses.len();
        pairs.extend((setup.horses.len()..input.horses.len()).map(|k| (same_gates && k < bf.horses.len()).then_some(k)));
    }
    pairs.resize(input.horses.len(), None);
    ed_mask.resize(input.horses.len(), false);
    let base_input = base.clone();
    let rev = *s.rev.peek();
    let res_rivals = rivals.clone();
    bg(s, async move {
        let mut s = s;
        let (r, inp) = (race.clone(), input.clone());
        let pair = base.is_some();
        let out = match n {
            None => {
                let label = if pair { "Running baseline and edited" } else { "Running" };
                job(s, label, 0, false, move |_| {
                    // A frame every sim step: the replay and the per-progress curves.
                    let cur = Arc::new(run_once_dense(&r, &inp));
                    // Same seed as the edited run, so the pair differs only by the edits.
                    let base = base.map(|b| Arc::new(run_once_dense(&r, &at_seed(&b, inp.random_seed, base_rivals.as_ref()))));
                    Kind::Single { cur, base }
                })
                .await
            }
            Some(n) => {
                let total = n * if pair { 2 } else { 1 };
                job(s, &format!("Simulating {} races", thousands(total)), total, true, move |w| {
                    let seeds = seeds(inp.random_seed, n);
                    let base = base.map(|b| Arc::new(run_batch(&r, &b, base_rivals.as_ref(), &seeds, true, w)));
                    let cur = Arc::new(run_batch(&r, &inp, rivals.as_ref(), &seeds, true, w));
                    Kind::Batch { cur, base }
                })
                .await
            }
        };
        match out {
            // A cancelled batch covers only some seeds: drop it.
            Ok((_, true)) => {}
            Ok((kind, false)) => {
                s.res.set(Some(Res { kind, input, trainers, colors, player, ed_mask, pairs, base_input, rivals: res_rivals, cv, rev }));
                s.rival_open.set(None);
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
    let umas = input.horses.len();
    drop(input);
    // Runs leave gates empty unless the setup fills them, and then only when the rival pool can.
    let blocked = run_size(s, umas).err().filter(|_| s.npcs.read().fill);
    let running = use_busy(s);
    let n = *s.n.read();
    rsx! {
        section { class: "runbar sticky",
            button { class: "btn big", disabled: running() || blocked.is_some(), onclick: move |_| run(s, None),
                "Run once "
                small { "seed {seed}" }
            }
            div { class: "runn",
                button { class: "btn big alt", disabled: running() || blocked.is_some(), onclick: move |_| run(s, Some(n)), "Run ×{n}" }
                {seeds_input("runs", s.n, 1, 20000)}
            }
            span { class: "muted run-hint",
                if let Some(e) = &blocked {
                    span { class: "warn", "{e}" }
                } else if pair {
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
