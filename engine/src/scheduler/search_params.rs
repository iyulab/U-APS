//! Parameters of the random-key genetic search over dispatching orders
//! ([`SimpleScheduler::search`](super::SimpleScheduler::search)).

/// Genetic search parameters for manufacturing scheduling.
///
/// A domain-specific parameter set that maps to the manufacturing scheduler's
/// needs, with fields like `time_limit_ms` that are specific to production
/// scheduling constraints.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GaParams {
    /// Population size.
    pub population_size: usize,
    /// Maximum generations.
    pub max_generations: usize,
    /// Elite ratio (0.0 ~ 1.0).
    pub elite_ratio: f64,
    /// Convergence check: stagnant generation count.
    pub convergence_generations: usize,
    /// Time limit in milliseconds (None = no limit).
    pub time_limit_ms: Option<i64>,
}

impl Default for GaParams {
    fn default() -> Self {
        Self {
            population_size: 100,
            max_generations: 500,
            elite_ratio: 0.1,
            convergence_generations: 30,
            time_limit_ms: None,
        }
    }
}

impl GaParams {
    /// Balanced preset for medium-sized problems (30-200 ops).
    pub fn balanced() -> Self {
        Self {
            population_size: 100,
            max_generations: 300,
            elite_ratio: 0.1,
            convergence_generations: 30,
            time_limit_ms: Some(30_000),
        }
    }
}
