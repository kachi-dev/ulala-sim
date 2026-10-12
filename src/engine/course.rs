use uma_race_sim::api::RaceInput;
use uma_race_sim::data::course::lane_path_name;
use uma_race_sim::data::GameData;
use uma_race_sim::sim::race as sim_race;

use super::{Race, SURFACES, data_dir, names, new_horse, pick, set_course};

#[derive(Clone, Debug, PartialEq)]
pub struct CourseInfo {
    /// race_course_set id.
    pub id: i32,
    pub track: String,
    pub distance: i32,
    /// 1 turf, 2 dirt.
    pub surface: i32,
    pub inout: i32,
    /// 1 Right, 2 Left, 3 StraightRight, 4 StraightLeft.
    pub turn: i32,
}

/// Courses the sim can run, sorted by track, surface, distance.
pub fn courses(race: &Race) -> Vec<CourseInfo> {
    // File checks, not JsonFiles::get: that would parse ~26 MB of course data just to list it. Pre-07 eras overlay
    // these same current files, so they are the full set.
    let dir = data_dir();
    let has = |sub: &str, stem: String| dir.join(sub).join(stem + ".json").is_file();
    let mut v: Vec<CourseInfo> = race
        .data
        .master
        .race_course_set
        .values()
        .filter(|c| has("courseeventparam", c.id.to_string()) && has("coursepath", lane_path_name(c.race_track_id, c.distance, c.ground, c.inout, 0)))
        .map(|c| course_info(&race.data, c.id))
        .collect();
    v.sort_by(|a, b| (&a.track, a.surface, a.distance, a.inout, a.id).cmp(&(&b.track, b.surface, b.distance, b.inout, b.id)));
    v
}

pub(super) fn course_info(data: &GameData, id: i32) -> CourseInfo {
    let c = data.master.race_course_set.get(&id).cloned().unwrap_or_default();
    let track = names().track.get(&c.race_track_id).cloned().unwrap_or_else(|| format!("Track {}", c.race_track_id));
    CourseInfo { id, track, distance: c.distance, surface: c.ground, inout: c.inout, turn: c.turn }
}

impl CourseInfo {
    /// e.g. "Tokyo Dirt 1600m".
    pub(super) fn label(&self) -> String {
        format!("{} {} {}m", self.track, pick(&SURFACES, self.surface).unwrap_or("?"), self.distance)
    }
}

/// RaceInfo.course_distance_type, 0-based: sprint, mile, medium, long.
pub fn distance_type(distance: i32) -> usize {
    match distance {
        ..=1400 => 0,
        1401..=1800 => 1,
        1801..=2400 => 2,
        _ => 3,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub start: f32,
    pub end: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Slope {
    pub start: f32,
    pub end: f32,
    /// Percent, + = uphill.
    pub grade: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CourseView {
    pub info: CourseInfo,
    pub distance: f32,
    /// Start distance of phases 0..=3 (opening, middle, final, last spurt).
    pub phases: [f32; 4],
    /// Distance where the final leg (last spurt phase) starts.
    pub final_leg_start: f32,
    pub corners: Vec<Section>,
    pub straights: Vec<Section>,
    pub slopes: Vec<Slope>,
    /// Course height in metres from start to goal at even ~5 m spacing, first point = 0, integrated from `slopes`.
    pub elevation: Vec<f32>,
}

impl CourseView {
    /// `corners` as (start, end) pairs.
    pub fn corner_spans(&self) -> Vec<(f32, f32)> {
        self.corners.iter().map(|c| (c.start, c.end)).collect()
    }
}

/// Course geometry exactly as the sim uses it.
pub fn course_view(race: &Race, course_set_id: i32) -> CourseView {
    // A throwaway sim init on that course: exactly the sim's corners, straights, phases and lane path.
    let mut input = RaceInput { horses: vec![new_horse(race, 105201)], ..race.input.clone() };
    set_course(race, &mut input, course_set_id);
    let r = sim_race::init(&input, &race.data);
    let pc = r.info.phase_distances;
    let d = pc.course_distance;
    let sec = |a: f32, b: f32| Section { start: a, end: b.min(d) };
    let n = (d / 5.0).round().max(1.0) as usize;
    let slopes: Vec<Slope> = r
        .course
        .event_param
        .course_event_slope
        .iter()
        .filter(|e| e.param._values.len() == 2)
        .map(|e| {
            let (v, a) = (&e.param._values, e.param._distance);
            Slope { start: a, end: (a + v[1] as f32).min(d), grade: v[0] as f32 / 10000.0 }
        })
        .collect();
    // Height the sim's slopes imply (integrated grade). The coursepath's visual height doesn't match the slopes the
    // sim runs on (flatter, and hills on courses with no slope events), so it isn't used.
    let h = |x: f32| slopes.iter().map(|s| (x.min(s.end) - s.start).max(0.0) * s.grade / 100.0).sum::<f32>();
    let elevation = (0..=n).map(|k| h(d * k as f32 / n as f32)).collect();
    CourseView {
        info: course_info(&race.data, course_set_id),
        distance: d,
        phases: [0.0, pc.phase_middle_start_distance, pc.phase_end_start_distance, pc.phase_last_start_distance],
        final_leg_start: pc.phase_end_start_distance,
        corners: r.course.corners.iter().map(|c| sec(c.start_distance, c.end_distance)).collect(),
        straights: r.course.straights.iter().map(|c| sec(c.start_distance, c.end_distance)).collect(),
        // course::calc_slope_list, keeping the grade (values[0] / 10000 = SlopePer, percent).
        slopes,
        elevation,
    }
}
