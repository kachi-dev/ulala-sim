//! Everything non-UI: loading saved races, running the sim (once or over many seeds), and the statistics/diffs the
//! UI shows. The UI only talks to this module; nothing here touches Dioxus.

mod batch;
mod checks;
mod conditions;
mod course;
mod data;
mod diff;
mod gains;
mod paths;
mod race;
mod restore;
mod run;
mod saves;
mod skills;
#[cfg(test)]
mod tests;
mod veterans;

pub use batch::*;
pub use checks::*;
pub use conditions::*;
pub use course::*;
pub use data::*;
pub use diff::*;
pub use gains::*;
pub use paths::*;
pub use race::*;
pub use restore::*;
pub use run::*;
pub use saves::*;
pub use skills::*;
pub use veterans::*;
pub use uma_race_sim::api::{HorseInput, RaceInput, SkillInput};
