//! Everything non-UI: loading saved races, running the sim (once or over many seeds), and the statistics/diffs the
//! UI shows. The UI only talks to this module; nothing here touches Dioxus.

mod batch;
mod checks;
mod cm;
mod conditions;
mod course;
mod data;
mod diff;
mod gains;
mod paths;
mod race;
mod restore;
mod rivals;
mod run;
mod saves;
mod skills;
mod stamina;
mod track;
mod update;
#[cfg(test)]
mod tests;
mod veterans;
mod wit;

pub use batch::*;
pub use checks::*;
pub use cm::*;
pub use conditions::*;
pub use course::*;
pub use data::*;
pub use diff::*;
pub use gains::*;
pub use paths::*;
pub use race::*;
pub use restore::*;
pub use rivals::*;
pub use run::*;
pub use saves::*;
pub use skills::*;
pub use stamina::*;
pub use track::*;
pub use update::*;
pub use veterans::*;
pub use wit::*;
pub use uma_race_sim::api::{HorseInput, RaceInput, SkillInput};
