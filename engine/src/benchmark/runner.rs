//! Benchmark runner for evaluating scheduler performance

use crate::ga::{GaParams, GaScheduler, GeneticOperators};
use crate::models::{Job, Resource};
use std::time::Instant;

/// Benchmark result for a single instance
#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    /// Instance name
    pub instance_name: String,
    /// Number of jobs
    pub num_jobs: usize,
    /// Number of machines
    pub num_machines: usize,
    /// Achieved makespan (ms)
    pub makespan_ms: i64,
    /// Best known solution (ms)
    pub best_known_ms: Option<i64>,
    /// Gap from BKS (percentage)
    pub gap_percent: Option<f64>,
    /// Execution time (ms)
    pub execution_time_ms: u128,
    /// Number of generations
    pub generations: usize,
    /// Final fitness
    pub fitness: f64,
    /// Whether converged early
    pub converged: bool,
}

impl BenchmarkResult {
    /// Format result as a string
    pub fn format(&self) -> String {
        let gap_str = match self.gap_percent {
            Some(gap) => format!("{:.2}%", gap),
            None => "N/A".to_string(),
        };

        let bks_str = match self.best_known_ms {
            Some(bks) => format!("{}s", bks / 1000),
            None => "N/A".to_string(),
        };

        format!(
            "{}: {}x{} | Makespan: {}s | BKS: {} | Gap: {} | Time: {}ms | Gen: {}",
            self.instance_name,
            self.num_jobs,
            self.num_machines,
            self.makespan_ms / 1000,
            bks_str,
            gap_str,
            self.execution_time_ms,
            self.generations
        )
    }
}

/// Benchmark runner configuration
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    /// GA parameters
    pub ga_params: GaParams,
    /// Number of runs per instance (for statistical analysis)
    pub num_runs: usize,
    /// Verbose output
    pub verbose: bool,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            ga_params: GaParams {
                population_size: 100,
                max_generations: 500,
                elite_ratio: 0.05,
                tournament_size: 3,
                convergence_generations: 50,
                convergence_threshold: 0.001,
                time_limit_ms: None,
            },
            num_runs: 1,
            verbose: false,
        }
    }
}

/// Run benchmark on given jobs and resources
pub fn run_benchmark(
    name: &str,
    jobs: &[Job],
    resources: &[Resource],
    best_known_ms: Option<i64>,
    config: &BenchmarkConfig,
) -> BenchmarkResult {
    let start = Instant::now();

    let scheduler = GaScheduler::new(config.ga_params.clone(), GeneticOperators::default());
    let result = scheduler.schedule(jobs, resources, 0);

    let execution_time_ms = start.elapsed().as_millis();

    let gap_percent = best_known_ms
        .map(|bks| ((result.schedule.makespan_ms as f64 - bks as f64) / bks as f64) * 100.0);

    BenchmarkResult {
        instance_name: name.to_string(),
        num_jobs: jobs.len(),
        num_machines: resources.len(),
        makespan_ms: result.schedule.makespan_ms,
        best_known_ms,
        gap_percent,
        execution_time_ms,
        generations: result.generations,
        fitness: result.fitness,
        converged: result.converged,
    }
}

/// Benchmark instance tuple type
pub type BenchmarkInstance<'a> = (&'a str, Vec<Job>, Vec<Resource>, Option<i64>);

/// Run multiple benchmarks and return aggregated results
pub fn run_benchmark_suite(
    instances: Vec<BenchmarkInstance<'_>>,
    config: &BenchmarkConfig,
) -> Vec<BenchmarkResult> {
    let mut results = Vec::new();

    for (name, jobs, resources, bks) in instances {
        if config.verbose {
            println!("Running {}...", name);
        }

        let result = run_benchmark(name, &jobs, &resources, bks, config);

        if config.verbose {
            println!("  {}", result.format());
        }

        results.push(result);
    }

    results
}

/// Generate summary statistics for benchmark results
pub fn summarize_results(results: &[BenchmarkResult]) -> BenchmarkSummary {
    if results.is_empty() {
        return BenchmarkSummary::default();
    }

    let gaps: Vec<f64> = results.iter().filter_map(|r| r.gap_percent).collect();

    let avg_gap = if gaps.is_empty() {
        0.0
    } else {
        gaps.iter().sum::<f64>() / gaps.len() as f64
    };

    let max_gap = gaps.iter().cloned().fold(0.0, f64::max);
    let min_gap = gaps.iter().cloned().fold(f64::INFINITY, f64::min);

    let total_time: u128 = results.iter().map(|r| r.execution_time_ms).sum();
    let avg_time = total_time / results.len() as u128;

    let optimal_count = gaps.iter().filter(|&&g| g <= 0.0).count();

    BenchmarkSummary {
        total_instances: results.len(),
        avg_gap_percent: avg_gap,
        max_gap_percent: max_gap,
        min_gap_percent: if min_gap == f64::INFINITY {
            0.0
        } else {
            min_gap
        },
        total_time_ms: total_time,
        avg_time_ms: avg_time,
        optimal_count,
    }
}

/// Summary statistics for benchmark suite
#[derive(Debug, Clone, Default)]
pub struct BenchmarkSummary {
    /// Total number of instances
    pub total_instances: usize,
    /// Average gap from BKS
    pub avg_gap_percent: f64,
    /// Maximum gap
    pub max_gap_percent: f64,
    /// Minimum gap
    pub min_gap_percent: f64,
    /// Total execution time
    pub total_time_ms: u128,
    /// Average execution time per instance
    pub avg_time_ms: u128,
    /// Number of optimal solutions found
    pub optimal_count: usize,
}

impl BenchmarkSummary {
    /// Format summary as string
    pub fn format(&self) -> String {
        format!(
            "Summary: {} instances | Avg Gap: {:.2}% | Max Gap: {:.2}% | Min Gap: {:.2}% | Optimal: {} | Total Time: {}ms",
            self.total_instances,
            self.avg_gap_percent,
            self.max_gap_percent,
            self.min_gap_percent,
            self.optimal_count,
            self.total_time_ms
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::taillard::generate_test_instance;

    #[test]
    fn test_run_benchmark() {
        let instance = generate_test_instance(5, 3);

        let config = BenchmarkConfig {
            ga_params: GaParams {
                population_size: 20,
                max_generations: 10,
                ..Default::default()
            },
            num_runs: 1,
            verbose: false,
        };

        let result = run_benchmark(
            &instance.name,
            &instance.jobs,
            &instance.resources,
            None,
            &config,
        );

        assert_eq!(result.num_jobs, 5);
        assert_eq!(result.num_machines, 3);
        assert!(result.makespan_ms > 0);
    }

    #[test]
    fn test_gap_calculation() {
        let instance = generate_test_instance(3, 2);

        let config = BenchmarkConfig {
            ga_params: GaParams {
                population_size: 10,
                max_generations: 5,
                ..Default::default()
            },
            ..Default::default()
        };

        // Set a known BKS
        let result = run_benchmark(
            &instance.name,
            &instance.jobs,
            &instance.resources,
            Some(100_000), // 100 seconds
            &config,
        );

        // Gap should be calculated
        assert!(result.gap_percent.is_some());
    }

    #[test]
    fn test_summarize_results() {
        let results = vec![
            BenchmarkResult {
                instance_name: "test1".to_string(),
                num_jobs: 5,
                num_machines: 3,
                makespan_ms: 105_000,
                best_known_ms: Some(100_000),
                gap_percent: Some(5.0),
                execution_time_ms: 100,
                generations: 10,
                fitness: 105.0,
                converged: true,
            },
            BenchmarkResult {
                instance_name: "test2".to_string(),
                num_jobs: 10,
                num_machines: 5,
                makespan_ms: 200_000,
                best_known_ms: Some(200_000),
                gap_percent: Some(0.0),
                execution_time_ms: 200,
                generations: 20,
                fitness: 200.0,
                converged: true,
            },
        ];

        let summary = summarize_results(&results);

        assert_eq!(summary.total_instances, 2);
        assert_eq!(summary.avg_gap_percent, 2.5);
        assert_eq!(summary.max_gap_percent, 5.0);
        assert_eq!(summary.min_gap_percent, 0.0);
        assert_eq!(summary.optimal_count, 1);
        assert_eq!(summary.total_time_ms, 300);
    }
}
