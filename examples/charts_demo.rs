//! Visual preview of every chart with fake data: `cargo run --example charts_demo`.
#![allow(dead_code)]
#[path = "../src/charts/mod.rs"]
mod charts;

use charts::*;
use dioxus::prelude::*;

const CSS: &str = r#"
:root, .light { --bg:#fff7f2; --surface:#fffdfa; --ink:#3b2f36; --muted:#8a7b83; --grid:#efe3e0;
        --accent:#f27aa5; --accent-2:#6cb8e6; --good:#2e9e5b; --bad:#d64545; }
.dark { --bg:#16131f; --surface:#1e1a2e; --ink:#f1e9f0; --muted:#a99bb0; --grid:#342d48;
        --accent:#f58bb3; --accent-2:#7cc4ef; --good:#4cc27a; --bad:#ef6b6b; }
body { margin:0; font-family:system-ui,"Segoe UI",sans-serif; }
main { display:grid; grid-template-columns:1fr 1fr; }
.theme { display:flex; flex-direction:column; gap:12px; padding:12px; background:var(--bg); color:var(--ink); }
.card { background:var(--surface); border-radius:12px; padding:10px 14px; box-shadow:0 1px 3px #f2c6d433; }
h3 { margin:0 0 6px; font-size:13px; font-weight:600; }
td { font-size:12px; padding:2px 8px 2px 0; }
"#;

const NAMES: [&str; 12] = [
    "Haru Urara", "Special Week", "Silence Suzuka", "Tokai Teio", "Oguri Cap", "Gold Ship",
    "Mejiro McQueen", "Rice Shower", "King Halo", "Agnes Tachyon", "Daiwa Scarlet", "Vodka",
];

/// Tiny deterministic noise so the demo needs no rand crate.
fn noise(seed: u32) -> f32 {
    let mut x = seed.wrapping_mul(747796405).wrapping_add(2891336453);
    x = ((x >> ((x >> 28) + 4)) ^ x).wrapping_mul(277803737);
    ((x >> 22) ^ x) as f32 / u32::MAX as f32
}

/// Single run: each horse gets a smooth "progress" curve; placement = rank at each %.
fn single_run() -> Vec<Vec<f32>> {
    let n = NAMES.len();
    let score = |h: usize, x: f32| {
        let (a, b, c) = (noise(h as u32 * 3), noise(h as u32 * 3 + 1), noise(h as u32 * 3 + 2));
        a * (1.0 - x) * 3.0 + b * x * x * 4.0 + (x * 6.0 + c * 6.0).sin() * 0.4 + if h == 0 { x * 2.5 } else { 0.0 }
    };
    let mut lines = vec![vec![0.0; 101]; n];
    #[allow(clippy::needless_range_loop)] // fills lines[h][p] column by column
    for p in 0..=100 {
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&i, &j| score(j, p as f32 / 100.0).total_cmp(&score(i, p as f32 / 100.0)));
        for (rank, &h) in order.iter().enumerate() {
            lines[h][p] = rank as f32 + 1.0;
        }
    }
    lines
}

fn batch(label: &str, color: &str, shift: f32, dashed: bool, highlight: bool) -> PlaceSeries {
    let med = |p: usize| {
        let x = p as f32 / 100.0;
        (9.0 - 5.0 * x * x - shift * x + (x * 9.0).sin() * 0.6).clamp(1.0, 12.0)
    };
    let off = |p: usize, k: f32| (0..=100).map(|q| (med(q) + k * (1.0 + q as f32 / 60.0)).clamp(1.0, 12.0)).nth(p).unwrap();
    let band = |k: f32| Some(((0..=100).map(|p| off(p, -k)).collect(), (0..=100).map(|p| off(p, k)).collect()));
    PlaceSeries {
        label: label.into(), color: color.into(), line: (0..=100).map(med).collect(),
        inner: band(1.0), outer: band(2.2), dashed, highlight,
    }
}

fn main() {
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
    dioxus::LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(
            WindowBuilder::new().with_title("charts demo").with_always_on_top(true).with_inner_size(LogicalSize::new(1500.0, 1020.0)),
        ))
        .launch(App);
}

#[component]
fn App() -> Element {
    let single: Vec<PlaceSeries> = single_run().into_iter().enumerate().map(|(i, line)| PlaceSeries {
        label: NAMES[i].into(), color: horse_color(i).into(), line, inner: None, outer: None,
        dashed: false, highlight: i == 0,
    }).collect();
    let compare = vec![
        batch("Haru Urara (original)", "var(--muted)", 0.0, true, false),
        batch("Haru Urara (edited)", "var(--accent)", 3.5, false, true),
    ];
    let markers = vec![(66.7, "Final leg".to_string()), (87.5, "Last spurt".to_string())];
    let places: Vec<(String, f32)> = [0.08, 0.12, 0.15, 0.14, 0.11, 0.1, 0.08, 0.07, 0.06, 0.04, 0.03, 0.02]
        .iter().enumerate().map(|(i, &v)| (format!("{}", i + 1), v)).collect();
    let before: Vec<f32> = [0.04, 0.07, 0.1, 0.12, 0.12, 0.12, 0.11, 0.1, 0.08, 0.07, 0.04, 0.03].to_vec();
    let deltas: Vec<f32> = (0..400).map(|i| (noise(i) + noise(i + 999) + noise(i + 5000) - 1.5) * 1.2 - 0.25).collect();
    let skills = vec![
        ("Corner Recovery ○".to_string(), horse_color(0).to_string(), vec![820.0, 1310.0]),
        ("Straightaway Adept".into(), horse_color(1).into(), vec![1650.0]),
        ("Gourmand of victory".into(), horse_color(2).into(), (0..300).map(|i| 1650.0 + noise(i) * 400.0 + noise(i + 77) * 300.0).collect()),
        ("Unused skill".into(), horse_color(3).into(), vec![]),
        ("Uma Stan".into(), horse_color(4).into(), vec![300.0, 2050.0, 2310.0]),
    ];

    // 2400 m oval: home straight, C1-C2, back straight, C3-C4, final straight; a dip and the final-straight hill.
    let corners = vec![(350.0, 600.0), (600.0, 850.0), (1250.0, 1550.0), (1550.0, 1850.0)];
    let straights = vec![(0.0, 350.0), (850.0, 1250.0), (1850.0, 2400.0)];
    let slopes = vec![(400.0, 620.0, -1.5), (1000.0, 1150.0, 0.8), (1950.0, 2080.0, 2.2), (2250.0, 2330.0, -0.6)];
    let regions = vec![
        (420.0, 470.0, "Escape Artist".to_string(), horse_color(0).to_string()),
        (1300.0, 1360.0, "Corner Recovery".into(), horse_color(1).into()),
        (1330.0, 1450.0, "Swinging Maestro".into(), horse_color(6).into()),
        (1620.0, 1800.0, "Gourmand of victory".into(), horse_color(2).into()),
        (1860.0, 1960.0, "Straightaway Adept".into(), horse_color(3).into()),
        (2000.0, 2400.0, "Uma Stan (last spurt)".into(), horse_color(4).into()),
    ];
    let gains: Vec<(String, String, [f32; 6], usize)> = [
        "Swinging Maestro", "Corner Recovery ○", "Gourmand of victory", "Straightaway Adept", "Escape Artist",
        "Uma Stan", "Shrewd Step", "Tail Held High", "Unrestrained", "Productive Plan", "Concentration", "Calm in a Crowd",
    ].iter().enumerate().map(|(i, n)| {
        let (mu, sd) = (1.6 - i as f32 * 0.2, 0.3 + noise(i as u32 * 7) * 0.5);
        let q = [-1.28, -0.67, 0.0, 0.67, 1.28].map(|z| mu + z * sd); // normal quantiles
        (n.to_string(), "var(--accent)".to_string(), [q[0], q[1], q[2], q[3], q[4], mu], 500)
    }).collect();

    // CHARTS_DEMO_PAGE=1|2|3 shows a third of the charts (fits one screenshot); unset = everything, scroll.
    let page = std::env::var("CHARTS_DEMO_PAGE").unwrap_or_default();
    let (p1, p2, p3) = (page.is_empty() || page == "1", page.is_empty() || page == "2", page.is_empty() || page == "3");
    rsx! {
        style { {CSS} }
        main {
            for theme in ["dark", "light"] {
                div { class: "theme {theme}",
                    if p3 {
                        div { class: "card", h3 { "Course · Turf 2400 m" }
                            CourseProfile {
                                distance: 2400.0, elevation: vec![], corners: corners.clone(), straights: straights.clone(),
                                slopes: slopes.clone(), phases: vec![0.0, 400.0, 1600.0, 2000.0], final_leg_start: 1600.0, regions: regions.clone()
                            }
                        }
                        div { class: "card", h3 { "Skill length gains · 500 seeds" }
                            GainChart { rows: gains.clone() }
                        }
                    }
                    if p1 {
                        div { class: "card", h3 { "Placement over course · single run (12)" }
                            PlaceChart { series: single.clone(), field_size: 12, markers: markers.clone() }
                        }
                        div { class: "card", h3 { "Placement over course · 1000 seeds, edited vs original" }
                            PlaceChart { series: compare.clone(), field_size: 12, markers: markers.clone() }
                        }
                    }
                    if p2 {
                        div { class: "card", h3 { "Finish position (edited vs original outline)" }
                            BarChart { bars: places.clone(), compare: Some(before.clone()), color: "var(--accent)" }
                        }
                        div { class: "card", h3 { "Finish time delta per seed" }
                            Histogram { values: deltas.clone(), color: "var(--accent-2)", unit: "s" }
                        }
                        div { class: "card", h3 { "Skill activations (2400 m)" }
                            CourseStrip { rows: skills.clone(), course_distance: 2400.0, final_leg_start: 1600.0 }
                        }
                        div { class: "card", h3 { "BoxRow in a table" }
                            table {
                                for (i, n) in NAMES.iter().take(4).enumerate() {
                                    tr {
                                        td { "{n}" }
                                        td { style: "width:140px",
                                            BoxRow { p10: -1.0 + i as f32 * 0.3, p25: -0.5 + i as f32 * 0.3, p50: -0.1 + i as f32 * 0.3, p75: 0.3 + i as f32 * 0.3, p90: 0.8 + i as f32 * 0.3, lo: -1.5, hi: 2.0, color: horse_color(i) }
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
