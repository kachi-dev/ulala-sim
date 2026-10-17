//! Run once's replay: the race seen from above on the sim's own course path, a frame every sim step. The map (click an
//! uma to follow it, wheel to zoom, drag to pan) and a time scrubber; the field and the race's events beside it.

use super::*;
use std::rc::Rc;
use dioxus::html::geometry::{ClientPoint, WheelDelta};
use std::time::Instant;
use uma_race_sim::api::{Span, span_kind};

/// One sim step (s): the frames' spacing, and what the arrow keys step.
const STEP: f32 = 1.0 / 15.0;
const SPEEDS: [f32; 5] = [0.25, 0.5, 1.0, 2.0, 4.0];
/// Follow camera width (m), and the zoom limits (m wide; the most is a multiple of the whole track's width).
const FOLLOW_M: f32 = 60.0;
const MIN_M: f32 = 15.0;
/// Blocks this short (s) or shorter are left out: brushes, not blocks.
const MIN_BLOCK: f32 = 0.2;
/// The track's drawing order follows the field in steps of this (m).
const FOCUS_M: f32 = 25.0;
/// Skill effect totals show under the umas only in views narrower than this (m): wider, they pile up.
const FX_M: f32 = 200.0;

#[derive(Clone, Copy, PartialEq)]
enum Cam {
    Track,
    Field,
}

/// The replay dialog, over the single run in Results.
#[component]
pub(crate) fn ReplayModal() -> Element {
    let s = use_context::<S>();
    let single = matches!(s.res.read().as_ref().map(|r| &r.kind), Some(Kind::Single { .. }));
    let open = *s.replay.read() && single && s.race.read().is_some();
    rsx! {
        if open { Replay {} }
    }
}

/// A state on the map's badges and in the feed: `EVENTS` (the results tables' states), the rest of `span_kind`, a
/// skill in effect. `icon` is a 24×24 stroke path.
#[derive(Clone, Copy, PartialEq)]
struct St {
    label: &'static str,
    color: &'static str,
    icon: &'static str,
}

const SKILL: St = St { label: "Skill", color: "#ff78ac", icon: "M12 4l8 8-8 8-8-8z" };

/// The legend's order: trouble first, then position keep and the rest.
const LEGEND: [u8; 11] = [
    span_kind::BLOCKED,
    span_kind::RUSHED,
    span_kind::SPOT_STRUGGLE,
    span_kind::DUEL,
    span_kind::PACE_UP,
    span_kind::PACE_UP_EX,
    span_kind::PACE_DOWN,
    span_kind::SPEED_UP,
    span_kind::OVERTAKE,
    span_kind::CONSERVE_POWER,
    span_kind::DOWNHILL,
];

/// The span is shown (`MIN_BLOCK`).
fn shown(s: &Span) -> bool {
    s.kind != span_kind::BLOCKED || s.end - s.start > MIN_BLOCK
}

fn state(kind: u8) -> St {
    if let Some(e) = EVENTS.iter().find(|e| e.kind == kind) {
        return St { label: e.label, color: e.color, icon: e.icon };
    }
    let (label, color, icon) = match kind {
        span_kind::DOWNHILL => ("Downhill", "#48c9b0", "M5 5l14 14M19 11v8h-8"),
        span_kind::SPEED_UP => ("Speed up", "#6fcbf2", "M5 6l6 6-6 6M13 6l6 6-6 6"),
        span_kind::OVERTAKE => ("Overtake", "#6fcbf2", "M5 19c0-8 5-11 13-11M14 4l4 4-4 4"),
        span_kind::PACE_UP => ("Pace up", "#6fcbf2", "M6 15l6-6 6 6"),
        span_kind::PACE_DOWN => ("Pace down", "#8fa6ff", "M6 9l6 6 6-6"),
        span_kind::PACE_UP_EX => ("Pace up EX", "#6fcbf2", "M6 13l6-6 6 6M6 19l6-6 6 6"),
        _ => ("State", "#a993a8", "M12 12h.01"),
    };
    St { label, color, icon }
}

/// `st`'s badge: its colour, its icon in the dark ink, `r` the radius, centred on (x, y).
fn badge(st: St, x: f32, y: f32, r: f32) -> Element {
    rsx! {
        g { transform: "translate({x} {y}) scale({r / 12.0})",
            circle { r: "12", fill: st.color }
            path {
                d: st.icon,
                transform: "translate(-8.4 -8.4) scale(.7)",
                fill: "none",
                stroke: "#1b1422",
                stroke_width: "2.8",
                stroke_linecap: "round",
                stroke_linejoin: "round",
            }
        }
    }
}

/// Each race phase's tint over the course surface (`PHASES` order; none for the early race): colour, opacity.
const PHASE_COLOR: [&str; 4] = ["var(--ph-early)", "var(--ph-mid)", "var(--ph-late)", "var(--ph-spurt)"];

/// Length of a track piece (m): the course is drawn in pieces so the stretch the field is on can go on top.
const PIECE_M: f32 = 50.0;

/// A stretch of the race's path or of the overrun loop: course distances, surface (the mid-lane line, stroked the
/// track's width) and rails, their colours (the ground; the rails in the race phase's), and its race phase (None for
/// the faint overrun loop).
#[derive(PartialEq)]
struct Piece {
    from: f32,
    to: f32,
    surface: String,
    rails: String,
    color: String,
    rail: &'static str,
    phase: Option<usize>,
}

/// `Geo::pieces`, compared by identity (a prop compared every frame).
#[derive(Clone)]
struct Pieces(Rc<Vec<Piece>>);

impl PartialEq for Pieces {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// The course: the race's path and the overrun loop in pieces, points along them for `Geo::covered`, and the extent.
/// A course can cross or run over itself (a start on the home straight, a 2nd lap), so what is on top depends on
/// where the field is.
struct Geo {
    track: Track,
    pieces: Pieces,
    /// (course distance, mid-lane point) every 10 m along every piece.
    samples: Vec<(f32, (f32, f32))>,
    /// min x, min y, max x, max y.
    bounds: [f32; 4],
}

fn geo(race: &Race, input: &RaceInput, phases: [f32; 4]) -> Geo {
    let track = track(race, input);
    let (w, goal) = (track.width, track.goal);
    // One colour for turf and dirt courses.
    let ground = "var(--track)";
    let end = track.loop_span.map_or(goal, |l| l.1);
    // Cut every `PIECE_M`, at each phase start and at the goal.
    let mut cuts: Vec<f32> = (0..=(end / PIECE_M) as usize).map(|k| k as f32 * PIECE_M).chain(phases).chain([goal, end]).collect();
    cuts.retain(|&d| (0.0..=end).contains(&d));
    cuts.sort_by(f32::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1.0);
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    // The loop only where the race's path isn't: next to it, it peeks out from under the path's rails.
    let near = w * 11.25;
    let path: Vec<(f32, f32)> = track.line(0.0, goal, w / 2.0, 10.0);
    let on_path = |from: f32, to: f32| {
        let pts = track.line(from, to, w / 2.0, 10.0);
        let hits = pts.iter().filter(|p| path.iter().any(|q| (p.0 - q.0).hypot(p.1 - q.1) < near)).count();
        hits * 2 >= pts.len()
    };
    let pieces = cuts
        .windows(2)
        .filter(|c| c[0] < goal || !on_path(c[0], c[1]))
        .map(|c| {
            let (from, to) = (c[0], c[1]);
            let rails = [0.0, w].map(|lane| track.line(from, to, lane, 2.0));
            for &(x, y) in rails.iter().flatten() {
                b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
            }
            let phase = (from < goal).then(|| phases.iter().rposition(|&p| from >= p).unwrap_or(0));
            let color = if phase.is_none() { format!("color-mix(in srgb, {ground} 45%, var(--surface-2))") } else { ground.to_string() };
            let rail = phase.map_or("var(--ink)", |k| PHASE_COLOR[k]);
            // 1 m over each neighbour: no anti-aliasing seam between pieces (the colours are opaque).
            let surface = path_d(&track.line((from - 1.0).max(0.0), (to + 1.0).min(end), w / 2.0, 2.0));
            Piece { from, to, surface, rails: rails.iter().map(|r| path_d(r)).collect(), color, rail, phase }
        })
        .collect();
    let samples = (0..=(end / 10.0) as usize).map(|k| k as f32 * 10.0).map(|d| (d, track.at(d, w / 2.0))).collect();
    Geo { track, pieces: Pieces(Rc::new(pieces)), samples, bounds: b }
}

impl Geo {
    /// The course at distance `d` lies under another stretch nearer to `focus` (m): a lap apart, on top of each other.
    fn covered(&self, d: f32, focus: f32) -> bool {
        let p = self.track.at(d, self.track.width / 2.0);
        let near = self.track.width * 11.25;
        self.samples.iter().any(|&(e, q)| (e - d).abs() > 150.0 && (e - focus).abs() < (d - focus).abs() && (q.0 - p.0).hypot(q.1 - p.1) < near)
    }
}

fn path_d(pts: &[(f32, f32)]) -> String {
    pts.iter().enumerate().map(|(i, (x, y))| format!("{}{x:.2} {y:.2}", if i == 0 { "M" } else { "L" })).collect()
}

/// Centre and width of the box around `pts` at `aspect` (width / height), padded by `pad`, at least `min` wide.
fn fit(pts: impl Iterator<Item = (f32, f32)>, aspect: f32, min: f32, pad: f32) -> (f32, f32, f32) {
    let b = pts.fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, (x, y)| [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)]);
    ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0, ((b[2] - b[0]).max((b[3] - b[1]) * aspect) + pad).max(min))
}

/// A feed entry: a skill activation (the names of an uma's skills at one moment together), or a state starting.
#[derive(Clone, PartialEq)]
struct Ev {
    t: f32,
    horse: usize,
    /// None = skills.
    kind: Option<u8>,
    text: String,
}

fn feed(run: &Run, names: &[String]) -> Vec<Ev> {
    let mut evs: Vec<Ev> = vec![];
    for a in run.activations.iter().filter(|a| !a.passive) {
        let name = skill_info(a.skill_id).name;
        match evs.last_mut() {
            Some(e) if e.horse == a.horse && e.t == a.time => e.text = format!("{}, {name}", e.text),
            _ => evs.push(Ev { t: a.time, horse: a.horse, kind: None, text: name }),
        }
    }
    for s in run.out.spans.iter().filter(|s| shown(s)) {
        let by = (s.kind == span_kind::BLOCKED)
            .then(|| horses_at(&run.out.frames, s.start).get(s.horse).and_then(|h| usize::try_from(h.block_front_horse_index).ok()))
            .flatten()
            .and_then(|j| names.get(j))
            .map_or(String::new(), |n| format!(" by {n}"));
        evs.push(Ev { t: s.start, horse: s.horse, kind: Some(s.kind), text: format!("{}{by} for {:.1} s", state(s.kind).label, s.end - s.start) });
    }
    evs.sort_by(|a, b| a.t.total_cmp(&b.t));
    evs
}

#[component]
fn Replay() -> Element {
    let mut s = use_context::<S>();
    let (res, race) = use_hook(|| (Rc::new(s.res.peek().clone().unwrap()), s.race.peek().clone().unwrap()));
    let Kind::Single { cur: run, .. } = &res.kind else { return rsx! {} };
    let names = use_hook(|| Rc::new(res.input.horses.iter().map(horse_name).collect::<Vec<_>>()));
    let geo = use_hook(|| Rc::new(geo(&race, &res.input, res.cv.phases)));
    let evs = use_hook(|| Rc::new(feed(run, &names)));
    // When the leader reached each race phase (s): the timeline's phase bar.
    let phase_t = use_hook(|| {
        let reach = |d: f32| run.out.frames.iter().find(|f| f.horses.iter().any(|h| h.distance >= d)).map_or(f32::MAX, |f| f.time);
        res.cv.phases.map(reach)
    });
    let end = run.out.frames.last().map_or(0.0, |f| f.time);
    let mut t = use_signal(|| 0.0f32);
    let mut playing = use_signal(|| true);
    let mut speed = use_signal(|| 1.0f32);
    let mut cam = use_signal(|| Cam::Field);
    let mut follow = use_signal(|| *s.focus.peek());
    // Wheel zoom (× the camera's width) and drag pan (world offset), reset by any camera change.
    let mut zoom = use_signal(|| 1.0f32);
    let mut pan = use_signal(|| (0.0f32, 0.0f32));
    // A drag on the map: where it started (client px), the pan then, and whether it moved (else it is a click).
    let mut drag = use_signal(|| None::<(f64, f64, (f32, f32), bool)>);
    let mut size = use_signal(|| (0.0f32, 0.0f32));
    // The stage element and its top-left in client px: the cursor's map point from client coordinates, whatever
    // element (an overlay, a marker) is under it.
    let mut stage = use_signal(|| None::<Rc<MountedData>>);
    let mut origin = use_signal(|| (0.0f64, 0.0f64));
    let locate = move || {
        spawn(async move {
            if let Some(el) = stage.peek().clone()
                && let Ok(r) = el.get_client_rect().await
            {
                origin.set((r.origin.x, r.origin.y));
            }
        });
    };
    use_future(move || async move {
        let mut last = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_millis(16)).await;
            let dt = last.elapsed().as_secs_f32();
            last = Instant::now();
            if *playing.peek() {
                let next = *t.peek() + dt * *speed.peek();
                t.set(next.min(end));
                if next >= end {
                    playing.set(false);
                }
            }
        }
    });
    let mut toggle = move || {
        if !playing() && t() >= end {
            t.set(0.0);
        }
        playing.toggle();
    };
    let mut seek = move |to: f32| {
        playing.set(false);
        t.set(to.clamp(0.0, end));
    };
    let mut set_cam = move |c: Cam, f: Option<usize>| {
        cam.set(c);
        follow.set(f);
        zoom.set(1.0);
        pan.set((0.0, 0.0));
    };

    let now = t();
    let hs = horses_at(&run.out.frames, now);
    let n = hs.len();
    let tr = &geo.track;
    let m = tr.metre;
    let pos: Vec<(f32, f32)> = hs.iter().map(|h| tr.at(h.distance, h.lane_position)).collect();
    let fol = follow().filter(|&f| f < n);

    // The camera: its own box at the stage's measured aspect, then the zoom and pan.
    let (sw, sh) = size();
    let aspect = if sw > 0.0 && sh > 0.0 { sw / sh } else { 1.6 };
    let b = geo.bounds;
    let extent = (b[2] - b[0]).max((b[3] - b[1]) * aspect);
    let (cx0, cy0, base) = match (fol, cam()) {
        (Some(i), _) => (pos[i].0, pos[i].1, FOLLOW_M * m),
        (None, Cam::Track) => fit([(b[0], b[1]), (b[2], b[3])].into_iter(), aspect, 0.0, extent * 0.12),
        // On the middle uma by distance, wide enough for everyone: the box around the whole field can sit on empty
        // track between two groups once it strings out, and a zoom or pan kept relative to it drifts off the racers.
        // The middle distance at the field's mean lane, not the middle uma's point: that snaps a lane over when two
        // umas swap the middle place.
        (None, Cam::Field) => {
            let mut ds: Vec<f32> = hs.iter().map(|h| h.distance).collect();
            ds.sort_by(f32::total_cmp);
            let lane = hs.iter().map(|h| h.lane_position).sum::<f32>() / n as f32;
            let c = tr.at(ds[n / 2], lane);
            let (hx, hy) = pos.iter().fold((0f32, 0f32), |(hx, hy), p| (hx.max((p.0 - c.0).abs()), hy.max((p.1 - c.1).abs())));
            (c.0, c.1, (2.0 * hx).max(2.0 * hy * aspect).max(40.0 * m) + 24.0 * m)
        }
    };
    let (zmin, zmax) = (MIN_M * m / base, extent * 1.5 / base);
    let z = zoom().clamp(zmin, zmax.max(zmin));
    let vw = base * z;
    let vh = vw / aspect;
    let (cx, cy) = (cx0 + pan().0, cy0 + pan().1);
    // World units per screen px: markers and labels keep their screen size at any zoom.
    let ppx = if sw > 0.0 { vw / sw } else { vw / 900.0 };
    let (ox, oy) = origin();
    let to_world = move |p: ClientPoint| (cx - vw / 2.0 + (p.x - ox) as f32 * ppx, cy - vh / 2.0 + (p.y - oy) as f32 * ppx);
    let r = (0.9 * m).clamp(9.0 * ppx, 24.0 * ppx);
    let font = 12.0 * ppx;

    // Finished umas by finish place, then the rest by distance.
    let key = |i: usize| if hs[i].distance >= run.goal { (0, run.horses[i].place as f32) } else { (1, -hs[i].distance) };
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| key(a).partial_cmp(&key(b)).unwrap_or(std::cmp::Ordering::Equal));
    let lead = order.first().map_or(0.0, |&i| hs[i].distance.min(run.goal));
    let hp0: Vec<f32> = run.out.frames.first().map_or(vec![], |f| f.horses.iter().map(|h| h.hp).collect());
    // The states in effect at the playhead, in `LEGEND` order.
    let kinds = |i: usize| -> Vec<u8> {
        let mut k: Vec<u8> = run.out.spans.iter().filter(|x| x.horse == i && x.start <= now && now < x.end && shown(x)).map(|x| x.kind).collect();
        k.sort_by_key(|&k| LEGEND.iter().position(|&l| l == k));
        k.dedup();
        k
    };

    let w = tr.width;
    // Where the field is: what it is on goes on top (in `FOCUS_M` steps, so the track redraws only that often).
    let focus = fol.map_or_else(|| (hs.iter().map(|h| h.distance).fold(f32::MAX, f32::min) + lead) / 2.0, |i| hs[i].distance);
    let focus = (focus / FOCUS_M).round() * FOCUS_M;
    let focus_phase = res.cv.phases.iter().rposition(|&p| focus >= p).unwrap_or(0);
    // Marks every 200 m (but next to a phase start), the phase starts, the start and the goal: (label, accent, rail
    // point, outer rail point, label point just outside). Not where the course lies under the stretch the field is on.
    let ph = res.cv.phases;
    let near_phase = |d: f32| ph[1..].iter().any(|&p| (p - d).abs() < 30.0);
    let marks: Vec<(String, bool, (f32, f32), (f32, f32), (f32, f32))> = (0..=(run.goal / 200.0) as usize)
        .map(|k| (k as f32 * 200.0, None))
        .filter(|&(d, _)| d < run.goal && (d == 0.0 || !near_phase(d)))
        .chain((1..4).filter(|&k| ph[k] > 0.0 && ph[k] < run.goal).map(|k| (ph[k], Some(k))))
        .chain([(run.goal, None)])
        .filter(|&(d, _)| !geo.covered(d, focus))
        .map(|(d, phase)| {
            let label = match phase {
                Some(k) => format!("{} {d:.0}m", PHASES[k]),
                None if d == 0.0 => "Start".into(),
                None if d == run.goal => "Goal".into(),
                None => format!("{d:.0}m"),
            };
            let (a, b) = (tr.at(d, 0.0), tr.at(d, w));
            let k = font * 1.2 / (b.0 - a.0).hypot(b.1 - a.1);
            (label, d == run.goal || phase.is_some_and(|k| k >= 2), a, b, (b.0 + (b.0 - a.0) * k, b.1 + (b.1 - a.1) * k))
        })
        .collect();
    let lead_phase = PHASES[ph.iter().rposition(|&p| lead >= p).unwrap_or(0)];
    let draw: Vec<usize> = (0..n).filter(|&i| Some(i) != fol).chain(fol).collect();
    // Under each uma: a strip of her states' icons, then the skill effects on her now, summed (speed, then
    // acceleration; a line only when non-zero). Per uma: the strip (x, y, width, height; empty without states), its
    // icons (state, x), and (text, class, y) per line: speed and acceleration in their skill colours, debuffs red.
    let ir = (r * 0.36).max(6.5 * ppx);
    let below: Vec<([f32; 4], Vec<(St, f32)>, Vec<(String, &str, f32)>)> = (0..n)
        .map(|i| {
            if vw / m >= FX_M {
                return Default::default();
            }
            let (x, y) = pos[i];
            let ks = kinds(i);
            let (step, ph) = (ir * 2.5, ir * 2.6);
            let sw = step * ks.len() as f32 + ir * 0.5;
            let strip = if ks.is_empty() { [0.0; 4] } else { [x - sw / 2.0, y + r + ir * 0.4, sw, ph] };
            let icons = ks.into_iter().enumerate().map(|(k, kind)| (state(kind), x - sw / 2.0 + ir * 0.25 + step * (k as f32 + 0.5))).collect();
            let top = if strip[3] > 0.0 { strip[1] + strip[3] } else { y + r };
            let h = &hs[i];
            let lines = [(h.skill_speed, "m/s", "rp-skill speed"), (h.skill_accel, "m/s²", "rp-skill accel")]
                .into_iter()
                .filter(|(v, ..)| v.abs() >= 0.005)
                .enumerate()
                .map(|(k, (v, unit, class))| (format!("{v:+.2} {unit}"), if v < 0.0 { "rp-skill neg" } else { class }, top + font * (0.3 + 1.15 * k as f32)))
                .collect();
            (strip, icons, lines)
        })
        .collect();
    let past = evs.partition_point(|e| e.t <= now);
    let ticks: Vec<f32> = (0..=(end / 10.0) as usize).map(|k| k as f32 * 10.0).collect();

    rsx! {
        div { class: "scrim", onclick: move |_| s.replay.set(false) }
        div {
            class: "modal replay",
            tabindex: "0",
            onmounted: move |e| async move {
                let _ = e.set_focus(true).await;
            },
            onkeydown: move |e| match e.key() {
                Key::Character(c) if c == " " => {
                    e.prevent_default();
                    toggle();
                }
                Key::ArrowLeft | Key::ArrowRight => {
                    e.prevent_default();
                    let d = if e.modifiers().shift() { 1.0 } else { STEP };
                    seek(t() + if e.key() == Key::ArrowLeft { -d } else { d });
                }
                Key::Escape => s.replay.set(false),
                _ => {}
            },
            div { class: "drawer-h",
                h3 { "Replay" }
                span { class: "muted small", "seed {run.seed} · Space play / pause · ← → one frame (Shift: 1 s)" }
                div { class: "grow" }
                div { class: "seg small",
                    for (c , label) in [(Cam::Track, "Whole track"), (Cam::Field, "Field")] {
                        button { class: if fol.is_none() && cam() == c { "on" }, onclick: move |_| set_cam(c, None), "{label}" }
                    }
                }
                button { class: "btn icon", title: "Close", onclick: move |_| s.replay.set(false), {icon("x")} }
            }
            div { class: "rp-body",
                div { class: "rp-left",
                    div {
                        class: if drag().is_some_and(|d| d.3) { "rp-stage grabbing" } else { "rp-stage" },
                        onmounted: move |e| {
                            stage.set(Some(e.data()));
                            locate();
                        },
                        onresize: move |e| {
                            if let Ok(b) = e.get_content_box_size() {
                                size.set((b.width as f32, b.height as f32));
                            }
                            locate();
                        },
                        onwheel: move |e| {
                            e.prevent_default();
                            let dy = match e.delta() {
                                WheelDelta::Pixels(v) => v.y,
                                WheelDelta::Lines(v) => v.y * 40.0,
                                WheelDelta::Pages(v) => v.y * 400.0,
                            } as f32;
                            // The point under the cursor stays put.
                            let nz = (z * 1.0015f32.powf(dy)).clamp(zmin, zmax.max(zmin));
                            let k = nz / z;
                            let (wx, wy) = to_world(e.client_coordinates());
                            let (px, py) = pan();
                            zoom.set(nz);
                            pan.set((px + (wx - cx) * (1.0 - k), py + (wy - cy) * (1.0 - k)));
                        },
                        onmousedown: move |e| {
                            let p = e.client_coordinates();
                            drag.set(Some((p.x, p.y, pan(), false)));
                        },
                        onmousemove: move |e| {
                            if let Some((x0, y0, p0, moved)) = drag() {
                                let p = e.client_coordinates();
                                let (dx, dy) = ((p.x - x0) as f32, (p.y - y0) as f32);
                                if moved || dx.hypot(dy) > 4.0 {
                                    drag.set(Some((x0, y0, p0, true)));
                                    pan.set((p0.0 - dx * ppx, p0.1 - dy * ppx));
                                }
                            }
                        },
                        onmouseup: move |e| {
                            // A click (no drag) on an uma follows her.
                            if let Some((.., false)) = drag() {
                                let (wx, wy) = to_world(e.client_coordinates());
                                let hit = (0..pos.len())
                                    .map(|i| (i, (pos[i].0 - wx).hypot(pos[i].1 - wy)))
                                    .filter(|&(_, d)| d < r * 1.4)
                                    .min_by(|a, b| a.1.total_cmp(&b.1));
                                if let Some((i, _)) = hit {
                                    set_cam(cam(), Some(i));
                                }
                            }
                            drag.set(None);
                        },
                        onmouseleave: move |_| drag.set(None),
                        svg { view_box: "{cx - vw / 2.0} {cy - vh / 2.0} {vw} {vh}",
                            TrackBase { pieces: geo.pieces.clone(), focus, phase: focus_phase, width: w * 11.25 }
                            for (label , goal , a , b , l) in marks {
                                {
                                    rsx! {
                                        line {
                                            x1: "{a.0}",
                                            y1: "{a.1}",
                                            x2: "{b.0}",
                                            y2: "{b.1}",
                                            stroke: if goal { "var(--accent)" } else { "var(--ink)" },
                                            stroke_opacity: if goal { "1" } else { ".45" },
                                            stroke_width: if goal { "3" } else { "1" },
                                            vector_effect: "non-scaling-stroke",
                                        }
                                        text {
                                            x: "{l.0}",
                                            y: "{l.1}",
                                            font_size: "{font}",
                                            fill: if goal { "var(--accent)" } else { "var(--muted)" },
                                            font_weight: "800",
                                            text_anchor: "middle",
                                            dominant_baseline: "middle",
                                            "{label}"
                                        }
                                    }
                                }
                            }
                            // Blocked: a dashed line to the uma in front.
                            for i in 0..n {
                                if let Some(j) = usize::try_from(hs[i].block_front_horse_index).ok().filter(|&j| j < n && kinds(i).contains(&span_kind::BLOCKED)) {
                                    line {
                                        x1: "{pos[i].0}",
                                        y1: "{pos[i].1}",
                                        x2: "{pos[j].0}",
                                        y2: "{pos[j].1}",
                                        stroke: state(span_kind::BLOCKED).color,
                                        stroke_width: "2",
                                        stroke_dasharray: "6 4",
                                        vector_effect: "non-scaling-stroke",
                                    }
                                }
                            }
                            defs {
                                clipPath { id: "rp-clip",
                                    circle { r: "{r}" }
                                }
                            }
                            for i in draw {
                                {
                                    let (x, y) = pos[i];
                                    let me = Some(i) == fol;
                                    let src = portrait(&res.input.horses[i]);
                                    rsx! {
                                        g { key: "{i}",
                                            g { transform: "translate({x} {y})",
                                                if me {
                                                    circle { r: "{r * 1.3}", fill: "none", stroke: "var(--accent)", stroke_width: "2.5", vector_effect: "non-scaling-stroke" }
                                                }
                                                circle { r: "{r}", fill: "var(--surface)" }
                                                if let Some(src) = src {
                                                    image {
                                                        href: "/a/{src}",
                                                        x: "{-r}",
                                                        y: "{-r}",
                                                        width: "{2.0 * r}",
                                                        height: "{2.0 * r}",
                                                        clip_path: "url(#rp-clip)",
                                                    }
                                                }
                                                circle { r: "{r}", fill: "none", stroke: "{res.colors[i]}", stroke_width: "2.5", vector_effect: "non-scaling-stroke" }
                                                if me {
                                                    text {
                                                        class: "rp-name",
                                                        y: "{-r * 1.3 - font * 0.6}",
                                                        font_size: "{font * 1.1}",
                                                        text_anchor: "middle",
                                                        "{names[i]}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            // States and skill effects, over every marker.
                            for (i , (strip , icons , lines)) in below.into_iter().enumerate() {
                                if strip[3] > 0.0 {
                                    rect {
                                        x: "{strip[0]}",
                                        y: "{strip[1]}",
                                        width: "{strip[2]}",
                                        height: "{strip[3]}",
                                        rx: "{strip[3] / 2.0}",
                                        fill: "#1b1422",
                                        fill_opacity: ".85",
                                    }
                                    for (st , ix) in icons {
                                        {badge(st, ix, strip[1] + strip[3] / 2.0, ir)}
                                    }
                                }
                                for (name , class , ny) in lines {
                                    text {
                                        class,
                                        x: "{pos[i].0}",
                                        y: "{ny}",
                                        font_size: "{font}",
                                        text_anchor: "middle",
                                        dominant_baseline: "hanging",
                                        "{name}"
                                    }
                                }
                            }
                        }
                        if let Some(f) = fol {
                            div { class: "rp-follow", onmousedown: |e| e.stop_propagation(),
                                span { class: "muted", "Following" }
                                " {names[f]}"
                                button { class: "btn icon", title: "Stop following", onclick: move |_| set_cam(cam(), None), {icon("x")} }
                            }
                        }
                        if fol.is_some() || cam() != Cam::Track {
                            div { class: "rp-mini", onmousedown: |e| e.stop_propagation(),
                                div { class: "muted small", "Whole track · box = this view" }
                                svg { view_box: "{b[0] - extent * 0.04} {b[1] - extent * 0.04} {b[2] - b[0] + extent * 0.08} {b[3] - b[1] + extent * 0.08}",
                                    TrackBase { pieces: geo.pieces.clone(), focus, phase: focus_phase, width: w * 11.25 * 2.5 }
                                    for i in 0..n {
                                        circle { r: "{extent / 90.0}", cx: "{pos[i].0}", cy: "{pos[i].1}", fill: "{res.colors[i]}" }
                                    }
                                    rect {
                                        x: "{cx - vw / 2.0}",
                                        y: "{cy - vh / 2.0}",
                                        width: "{vw}",
                                        height: "{vh}",
                                        fill: "none",
                                        stroke: "var(--ink)",
                                        stroke_width: "1.5",
                                        vector_effect: "non-scaling-stroke",
                                    }
                                }
                            }
                        }
                        div { class: "rp-zoom", onmousedown: |e| e.stop_propagation(),
                            button { aria_label: "Zoom out", onclick: move |_| zoom.set((z * 1.25).min(zmax.max(zmin))), "−" }
                            span { "{vw / m:.0} m" }
                            button { aria_label: "Zoom in", onclick: move |_| zoom.set((z / 1.25).max(zmin)), "+" }
                        }
                    }
                    div { class: "rp-legend",
                        for k in LEGEND {
                            {
                                let st = state(k);
                                rsx! {
                                    span {
                                        svg { view_box: "-12 -12 24 24", {badge(st, 0.0, 0.0, 12.0)} }
                                        "{st.label}"
                                    }
                                }
                            }
                        }
                    }
                    div { class: "rp-bar",
                        button { class: "btn", title: if playing() { "Pause (Space)" } else { "Play (Space)" }, onclick: move |_| toggle(),
                            if playing() { {icon("pause")} } else { {icon("play")} }
                        }
                        span { class: "rp-time", "{ft(now)} / {ft(end)}" }
                        div { class: "rp-scrub",
                            div { class: "rp-ticks",
                                for k in ticks {
                                    span { style: "left: {k / end * 100.0}%", "{ft(k).trim_end_matches(\".00\")}" }
                                }
                            }
                            div { class: "rp-phases", title: "Race phases, as the leader reached them",
                                for (k , (a , b)) in phase_t.iter().zip(phase_t.iter().skip(1).chain([&end])).enumerate() {
                                    span { style: "left: {a / end * 100.0}%; width: {(b - a).max(0.0) / end * 100.0}%; background: {PHASE_COLOR[k]}",
                                        b { "{PHASES[k]}" }
                                    }
                                }
                            }
                            input {
                                r#type: "range",
                                min: "0",
                                max: "{end}",
                                step: "{STEP}",
                                value: "{now}",
                                // The dialog's arrow keys step instead, by sim frames.
                                onkeydown: move |e| e.prevent_default(),
                                oninput: move |e| seek(e.value().parse().unwrap_or(0.0)),
                            }
                        }
                        div { class: "seg small",
                            for v in SPEEDS {
                                button { class: if speed() == v { "on" }, onclick: move |_| speed.set(v), "{v}×" }
                            }
                        }
                    }
                }
                div { class: "rp-side",
                    div { class: "rp-stand",
                        div { class: "rp-lead muted small", "Leader at {lead:.0} m of {run.goal:.0} m · {lead_phase}" }
                        div { class: "rp-srow head",
                            span {}
                            span { "Uma" }
                            span { class: "num", "Behind" }
                            span { class: "num", "m/s" }
                            span { "HP" }
                        }
                        for (p , i) in order.into_iter().enumerate() {
                            {
                                let h = &hs[i];
                                let behind = if p == 0 {
                                    "Leader".to_string()
                                } else if h.distance >= run.goal {
                                    format!("+{:.1} L", run.horses[i].behind_lengths)
                                } else {
                                    format!("+{:.1} L", (lead - h.distance).max(0.0) / 2.5)
                                };
                                let hp_pct = hp0.get(i).map_or(0.0, |&h0| h.hp.max(0.0) / h0.max(1.0) * 100.0);
                                rsx! {
                                    div { key: "{i}", class: if Some(i) == fol { "rp-srow sel" } else { "rp-srow" },
                                        b { class: "num", "{p + 1}" }
                                        span { class: "rp-who",
                                            {horse_icon(&res.input.horses[i], Some(&res.colors[i]), "xs")}
                                            b { "{names[i]}" }
                                        }
                                        span { class: "num muted", "{behind}" }
                                        span { class: "num muted", "{h.speed:.2}" }
                                        span { class: "rp-hp", title: "HP {h.hp.max(0.0):.0}",
                                            span { style: "width: {hp_pct}%" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Feed { evs: evs.clone(), past, follow: fol, names: names.clone(), t, playing }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    All,
    Skills,
    States,
    Followed,
}

/// The race's events in time order, a line at the playhead (`past` events before it), later ones dimmed; clicking one
/// seeks there. Renders only when the playhead passes an event.
#[component]
fn Feed(evs: Rc<Vec<Ev>>, past: usize, follow: Option<usize>, names: Rc<Vec<String>>, t: Signal<f32>, playing: Signal<bool>) -> Element {
    let mut filter = use_signal(|| Filter::All);
    let f = if filter() == Filter::Followed && follow.is_none() { Filter::All } else { filter() };
    // Keep the playhead line in view as it moves.
    use_effect(use_reactive!(|past| {
        let _ = past;
        document::eval("document.getElementById('rp-now')?.scrollIntoView({block: 'center', behavior: 'smooth'})");
    }));
    let shown: Vec<(usize, &Ev)> = evs
        .iter()
        .enumerate()
        .filter(|(_, e)| match f {
            Filter::All => true,
            Filter::Skills => e.kind.is_none(),
            Filter::States => e.kind.is_some(),
            Filter::Followed => Some(e.horse) == follow,
        })
        .collect();
    let line_at = shown.iter().position(|&(k, _)| k >= past).unwrap_or(shown.len());
    let at_end = line_at == shown.len();
    rsx! {
        div { class: "rp-feed",
            div { class: "rp-feed-h",
                h4 { "Events" }
                div { class: "grow" }
                div { class: "seg small",
                    for (v , label) in [(Filter::All, "All"), (Filter::Skills, "Skills"), (Filter::States, "States"), (Filter::Followed, "Followed")] {
                        button {
                            class: if f == v { "on" },
                            disabled: v == Filter::Followed && follow.is_none(),
                            onclick: move |_| filter.set(v),
                            "{label}"
                        }
                    }
                }
            }
            div { class: "rp-feed-list",
                for (n , (k , e)) in shown.into_iter().enumerate() {
                    if n == line_at {
                        div { id: "rp-now", class: "rp-now" }
                    }
                    {
                        let st = e.kind.map_or(SKILL, state);
                        let te = e.t;
                        let cls = match (k < past, Some(e.horse) == follow) {
                            (true, true) => "rp-ev me",
                            (true, false) => "rp-ev",
                            (false, true) => "rp-ev me later",
                            (false, false) => "rp-ev later",
                        };
                        rsx! {
                            div {
                                key: "{k}",
                                class: cls,
                                onclick: move |_| {
                                    playing.set(false);
                                    t.set(te);
                                },
                                span { class: "muted num", "{ft(e.t)}" }
                                svg { view_box: "-12 -12 24 24", {badge(st, 0.0, 0.0, 12.0)} }
                                span {
                                    b { "{names[e.horse]}" }
                                    " "
                                    span { style: "color: {st.color}", "{e.text}" }
                                }
                            }
                        }
                    }
                }
                if at_end {
                    div { id: "rp-now", class: "rp-now" }
                }
            }
        }
    }
}

/// `Geo::pieces`, farthest from `focus` (m) first, so the stretch the field is on is drawn last, on top; the rails
/// bright in race phase `phase` (the field's), dim in the others. Redraws only when `focus` moves a step.
#[component]
fn TrackBase(pieces: Pieces, focus: f32, phase: usize, width: f32) -> Element {
    let gap = |p: &Piece| if (p.from..=p.to).contains(&focus) { 0.0 } else { (p.from - focus).abs().min((p.to - focus).abs()) };
    let mut order: Vec<&Piece> = pieces.0.iter().collect();
    order.sort_by(|a, b| gap(b).total_cmp(&gap(a)));
    rsx! {
        g {
            for p in order {
                g { key: "{p.from}",
                    path {
                        d: "{p.surface}",
                        fill: "none",
                        style: "stroke: {p.color}",
                        stroke_width: "{width}",
                        stroke_linejoin: "round",
                    }
                    path {
                        d: "{p.rails}",
                        fill: "none",
                        stroke: p.rail,
                        stroke_opacity: match p.phase {
                            None => ".25",
                            Some(k) if k == phase => "1",
                            Some(_) => ".3",
                        },
                        stroke_width: if p.phase.is_some() { "3" } else { "1" },
                        // Round ends overlap the next piece's: no notch at the joins.
                        stroke_linecap: if p.phase.is_some() { "round" } else { "butt" },
                        vector_effect: "non-scaling-stroke",
                    }
                }
            }
        }
    }
}
