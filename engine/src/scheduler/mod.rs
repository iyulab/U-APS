//! Production scheduling algorithms and supporting infrastructure.
//!
//! ## Core Schedulers
//!
//! - [`SimpleScheduler`] — Dispatching-rule-based greedy scheduler
//! - [`ProductionScheduler`] — GA-based with setup matrices, split operations, overlap
//! - [`DynamicScheduler`] — Time-fence-aware rescheduling
//! - [`SchedulingEngine`] — Unified entry point for all 7 algorithms
//!
//! ## Features
//!
//! - [`kpi`] — KPI computation (makespan, utilization, tardiness, flow time)
//! - [`reschedule`] — Event-driven rescheduling (16 event types, 5 strategies)
//! - [`ctp`] — Capable-to-Promise / Available-to-Promise queries
//! - [`whatif`] — What-if scenario analysis
//! - [`analytics`] — Schedule analytics and bottleneck detection
//! - [`campaign`] — Campaign scheduling (batch grouping)
//! - [`pegging`] — Order pegging engine
//! - [`dispatching`] — Dispatching rule engine (SPT, EDD, etc.)
//! - [`time_fence`] — Frozen/slushy/free time fence management
//! - [`task_splitter`] — Operation splitting across parallel machines

pub mod analytics;
pub mod campaign;
mod constraint;
pub mod ctp;
pub mod dispatching;
pub mod dynamic;
pub mod engine;
pub mod kpi;
pub mod pegging;
pub mod production;
pub mod reschedule;
mod schedule;
mod simple;
pub mod task_splitter;
pub mod time_fence;
pub mod whatif;

pub use analytics::*;
pub use campaign::*;
pub use constraint::*;
pub use ctp::*;
pub use dispatching::*;
pub use dynamic::*;
pub use engine::*;
pub use kpi::*;
pub use pegging::*;
pub use production::*;
pub use reschedule::*;
pub use schedule::*;
pub use simple::*;
pub use task_splitter::*;
pub use time_fence::*;
pub use whatif::*;
