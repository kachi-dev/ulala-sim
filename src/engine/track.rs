//! Top-down course geometry for the replayer: the sim's own lane path (`sim::course::lanes`), seen from above.

use uma_race_sim::api::{Frame, HorseFrame, RaceInput};
use uma_race_sim::sim::course::lanes::{self, CourseLane};
use uma_race_sim::sim::race as sim_race;

use super::Race;

/// A course's path as the sim builds it. Points are world x / z with z flipped (SVG y points down).
pub struct Track {
    lane: CourseLane,
    /// Course width in lanes (`lane_position` units; the sim's lane 1 is 11.25 world units out).
    pub width: f32,
    /// Course distance (m).
    pub goal: f32,
    /// World units per course metre on lane 0.
    pub metre: f32,
    /// 1 turf, 2 dirt.
    pub ground: i32,
    /// The track's overrun loop as (from, to) course distances: the path past the main one. None without a loop.
    pub loop_span: Option<(f32, f32)>,
}

/// The track `input` runs on (its course set), from a sim race set up but not run.
pub fn track(race: &Race, input: &RaceInput) -> Track {
    let r = sim_race::init(input, &race.data);
    let main = r.course_lane.course_lane_anim.Distance as f32;
    Track {
        loop_span: r.course_lane.course_over_run_lane_anim.as_ref().map(|o| (main, main + o.Distance as f32)),
        width: r.info.lane_distance_max,
        goal: r.info.race_course_set.distance as f32,
        metre: r.world_per_metre,
        ground: r.info.race_course_set.ground,
        lane: r.course_lane,
    }
}

impl Track {
    /// Point at course distance `d` on lane `lane`.
    pub fn at(&self, d: f32, lane: f32) -> (f32, f32) {
        let p = lanes::calc_world_transform(&self.lane, d, lane).pos_world;
        (p.x, -p.z)
    }

    /// Lane `lane` from `from` to `to` m, a point every `step` m (and one at `to`).
    pub fn line(&self, from: f32, to: f32, lane: f32, step: f32) -> Vec<(f32, f32)> {
        let n = ((to - from) / step).ceil().max(1.0) as usize;
        (0..=n).map(|i| self.at((from + step * i as f32).min(to), lane)).collect()
    }
}

/// Every horse at time `t`: distance, lane, speed and HP linear between the recorded frames, the flags from the frame
/// before. Clamped to the first / last frame.
pub fn horses_at(frames: &[Frame], t: f32) -> Vec<HorseFrame> {
    let k = frames.partition_point(|f| f.time <= t);
    match (k.checked_sub(1).map(|i| &frames[i]), frames.get(k)) {
        (Some(a), Some(b)) => {
            let w = (t - a.time) / (b.time - a.time);
            let lerp = |x: f32, y: f32| x + (y - x) * w;
            a.horses
                .iter()
                .zip(&b.horses)
                .map(|(x, y)| HorseFrame {
                    distance: lerp(x.distance, y.distance),
                    lane_position: lerp(x.lane_position, y.lane_position),
                    speed: lerp(x.speed, y.speed),
                    hp: lerp(x.hp, y.hp),
                    ..*x
                })
                .collect()
        }
        (Some(a), None) => a.horses.clone(),
        (None, Some(b)) => b.horses.clone(),
        (None, None) => vec![],
    }
}
