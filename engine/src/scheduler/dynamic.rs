//! `SchedulingAlgorithm::Dynamic`'s settings.
//!
//! Dynamic runs the same search as the genetic algorithm within its own
//! budget. A pegging engine's material availability reaches every algorithm
//! as release times; rescheduling after an event is the `Rescheduler`'s
//! ([`crate::scheduler::reschedule`]).

use crate::scheduler::production::ProductionConfig;

/// Dynamic's settings: the search budget, as Production's.
#[derive(Debug, Clone, Default)]
pub struct DynamicSchedulerConfig {
    /// The search budget.
    pub production_config: ProductionConfig,
}
