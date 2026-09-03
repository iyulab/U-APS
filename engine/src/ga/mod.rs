//! Genetic Algorithm Module for FJSSP
//!
//! Manufacturing domain-specific GA scheduling.
//!
//! Uses `u-schedule`'s `ScheduleChromosome` and `GeneticOperators` for
//! GA infrastructure. Manufacturing-specific types (`OperationInfo`,
//! `GaParams`, `Population`, `PopulationStats`) are defined locally
//! because they carry domain semantics not appropriate for the lower layers.

use rand::prelude::*;

// Re-export scheduling GA types from u-schedule
pub use u_schedule::ga::operators::{CrossoverType, GeneticOperators, MutationType};
pub use u_schedule::ga::ActivityInfo;
pub use u_schedule::ga::ScheduleChromosome;

// Manufacturing domain-specific modules
pub mod auto_tuner;
pub mod hybrid_sa;
pub mod nsga2;
mod scheduler;

pub use auto_tuner::*;
pub use hybrid_sa::*;
pub use nsga2::*;
pub use scheduler::*;

/// Manufacturing operation info for GA chromosome construction.
///
/// Extends scheduling `ActivityInfo` with an `activity_id` field
/// that maps back to the engine's `Operation.id`. This is a
/// manufacturing-domain concept not present in the generic
/// scheduling framework.
#[derive(Debug, Clone)]
pub struct OperationInfo {
    /// Parent job (task) ID.
    pub task_id: String,
    /// Operation ID within the engine (e.g., "OP-1-1").
    pub activity_id: String,
    /// Sequence within job (1-based).
    pub sequence: i32,
    /// Candidate resource IDs.
    pub candidates: Vec<String>,
    /// Processing time in milliseconds.
    pub process_time_ms: i64,
}

impl OperationInfo {
    /// Converts to `ActivityInfo` for use with `ScheduleChromosome`.
    pub fn to_activity_info(&self) -> ActivityInfo {
        ActivityInfo {
            task_id: self.task_id.clone(),
            sequence: self.sequence,
            process_ms: self.process_time_ms,
            candidates: self.candidates.clone(),
        }
    }
}

/// Converts a slice of `OperationInfo` to `Vec<ActivityInfo>`.
pub fn to_activity_infos(operations: &[OperationInfo]) -> Vec<ActivityInfo> {
    operations.iter().map(|op| op.to_activity_info()).collect()
}

/// GA parameters for manufacturing scheduling.
///
/// This is a domain-specific parameter set that maps to
/// the manufacturing scheduler's needs. It carries fields
/// like `time_limit_ms` that are specific to production
/// scheduling constraints.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GaParams {
    /// Population size.
    pub population_size: usize,
    /// Maximum generations.
    pub max_generations: usize,
    /// Elite ratio (0.0 ~ 1.0).
    pub elite_ratio: f64,
    /// Tournament size for selection.
    pub tournament_size: usize,
    /// Convergence check: stagnant generation count.
    pub convergence_generations: usize,
    /// Convergence threshold.
    pub convergence_threshold: f64,
    /// Time limit in milliseconds (None = no limit).
    pub time_limit_ms: Option<i64>,
}

impl Default for GaParams {
    fn default() -> Self {
        Self {
            population_size: 100,
            max_generations: 500,
            elite_ratio: 0.1,
            tournament_size: 3,
            convergence_generations: 30,
            convergence_threshold: 0.001,
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
            tournament_size: 4,
            convergence_generations: 30,
            convergence_threshold: 0.002,
            time_limit_ms: Some(30_000),
        }
    }
}

/// Per-generation statistics for manufacturing GA.
#[derive(Debug, Clone)]
pub struct PopulationStats {
    pub generation: usize,
    pub best_fitness: f64,
    pub worst_fitness: f64,
    pub mean_fitness: f64,
    pub std_dev: f64,
}

/// Manual population management for manufacturing GA.
///
/// Unlike `u-metaheur`'s `GaRunner` (which encapsulates the loop),
/// the manufacturing scheduler needs explicit control over evaluation
/// (decode → fitness) because fitness depends on manufacturing-specific
/// logic (setup matrices, overlapping, splitting, efficiency).
pub struct Population {
    /// Current individuals.
    pub individuals: Vec<ScheduleChromosome>,
    /// GA parameters.
    pub params: GaParams,
    /// Genetic operators.
    pub operators: GeneticOperators,
    /// Current generation counter.
    pub generation: usize,
    /// Best individual seen so far.
    pub best: Option<ScheduleChromosome>,
    /// Per-generation best fitness history.
    pub fitness_history: Vec<f64>,
}

impl Population {
    /// Creates a new population with random individuals.
    pub fn new(
        operations: &[OperationInfo],
        _resources: &[crate::models::Resource],
        params: GaParams,
        operators: GeneticOperators,
        rng: &mut impl Rng,
    ) -> Self {
        let activities = to_activity_infos(operations);
        let individuals: Vec<ScheduleChromosome> = (0..params.population_size)
            .map(|_| ScheduleChromosome::random(&activities, rng))
            .collect();

        Self {
            individuals,
            params,
            operators,
            generation: 0,
            best: None,
            fitness_history: Vec::new(),
        }
    }

    /// Evolves the population by one generation.
    pub fn evolve(&mut self, operations: &[OperationInfo], rng: &mut impl Rng) {
        let activities = to_activity_infos(operations);
        self.generation += 1;

        // Sort by fitness (ascending = better first)
        self.individuals.sort_by(|a, b| {
            a.fitness
                .partial_cmp(&b.fitness)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Track best
        if let Some(best_ind) = self.individuals.first() {
            match &self.best {
                None => self.best = Some(best_ind.clone()),
                Some(current_best) if best_ind.fitness < current_best.fitness => {
                    self.best = Some(best_ind.clone());
                }
                _ => {}
            }
            self.fitness_history.push(best_ind.fitness);
        }

        let mut new_population = Vec::with_capacity(self.params.population_size);

        // Elite preservation
        let elite_count = (self.params.population_size as f64 * self.params.elite_ratio) as usize;
        for ind in self.individuals.iter().take(elite_count) {
            new_population.push(ind.clone());
        }

        // Generate offspring
        while new_population.len() < self.params.population_size {
            let p1 = self.tournament_select(rng);
            let p2 = self.tournament_select(rng);

            let (mut c1, mut c2) = self.operators.crossover(&p1, &p2, &activities, rng);
            self.operators.mutate(&mut c1, &activities, rng);
            self.operators.mutate(&mut c2, &activities, rng);

            new_population.push(c1);
            if new_population.len() < self.params.population_size {
                new_population.push(c2);
            }
        }

        self.individuals = new_population;
    }

    /// Tournament selection.
    fn tournament_select(&self, rng: &mut impl Rng) -> ScheduleChromosome {
        let mut best_idx = rng.random_range(0..self.individuals.len());
        for _ in 1..self.params.tournament_size {
            let idx = rng.random_range(0..self.individuals.len());
            if self.individuals[idx].fitness < self.individuals[best_idx].fitness {
                best_idx = idx;
            }
        }
        self.individuals[best_idx].clone()
    }

    /// Returns the best individual.
    pub fn get_best(&self) -> Option<&ScheduleChromosome> {
        self.best.as_ref()
    }

    /// Checks convergence based on fitness history stagnation.
    pub fn is_converged(&self) -> bool {
        let n = self.params.convergence_generations;
        if self.fitness_history.len() < n {
            return false;
        }
        let recent = &self.fitness_history[self.fitness_history.len() - n..];
        if let (Some(&first), Some(&last)) = (recent.first(), recent.last()) {
            if first == 0.0 {
                return true;
            }
            let improvement = (first - last).abs() / first.abs();
            improvement < self.params.convergence_threshold
        } else {
            false
        }
    }

    /// Computes per-generation statistics.
    pub fn statistics(&self) -> PopulationStats {
        let fitnesses: Vec<f64> = self.individuals.iter().map(|i| i.fitness).collect();
        let n = fitnesses.len() as f64;

        let best = fitnesses.iter().cloned().fold(f64::INFINITY, f64::min);
        let worst = fitnesses.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let mean = if n > 0.0 {
            fitnesses.iter().sum::<f64>() / n
        } else {
            0.0
        };
        let variance = if n > 1.0 {
            fitnesses.iter().map(|&f| (f - mean).powi(2)).sum::<f64>() / n
        } else {
            0.0
        };

        PopulationStats {
            generation: self.generation,
            best_fitness: best,
            worst_fitness: worst,
            mean_fitness: mean,
            std_dev: variance.sqrt(),
        }
    }
}
