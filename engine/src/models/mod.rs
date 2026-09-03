//! Manufacturing domain data models.
//!
//! - [`Job`] / [`Operation`] — Work to be scheduled (jobs contain ordered operations)
//! - [`Resource`] — Machines, workers, tools with calendars and capabilities
//! - [`SetupMatrixCollection`] — Sequence-dependent setup times between product types
//! - [`Calendar`] / [`Shift`] — Resource availability windows and shift patterns
//! - [`SkillLevel`] / [`SkillMatrix`] — Worker skill levels and certification
//! - [`Material`] / [`MaterialManager`] — Material availability constraints
//! - [`Crew`] / [`CrewManager`] — Crew assignment and management
//! - [`SiteTransitions`] — Multi-site inter-site transition times
//! - [`OutsourcingManager`] — External vendor capacity management

mod calendar;
mod crew;
mod job;
mod material;
mod operation;
mod outsourcing;
mod resource;
mod setup;
mod site;
mod skill;
mod task;

pub use calendar::*;
pub use crew::*;
pub use job::*;
pub use material::*;
pub use operation::*;
pub use outsourcing::*;
pub use resource::*;
pub use setup::*;
pub use site::*;
pub use skill::*;
pub use task::*;
