//! `SchedulingAlgorithm::Production`'s settings.
//!
//! Production runs the same search as the genetic algorithm — over the order
//! the default scheduler takes the jobs in — within its own budget.

use crate::GaParams;

/// Production's settings: the search budget.
#[derive(Debug, Clone, Default)]
pub struct ProductionConfig {
    /// The search's population, generations, stall generations and time
    /// limit; without a time limit the engine's `timeout_ms` bounds it.
    pub ga_params: GaParams,
}
