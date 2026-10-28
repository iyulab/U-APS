//! Scale scenarios: synthetic shop floors of 20–500 jobs scheduled through
//! the public engine, each with a wall-clock budget.
//!
//! The two default-scale scenarios that finish in seconds run with the suite.
//! The rest spend a search budget or minutes of wall clock, so they run only
//! when asked, alone and in release:
//!
//! ```text
//! cargo test --release --test scale -- --ignored --test-threads=1
//! ```
//!
//! Under a parallel suite a budget measures the load of the other tests.

use std::time::Instant;

use uaps_engine::{
    CpSatConfig, GaParams, Job, Operation, ProductionConfig, Resource, SchedulingEngine,
};

/// `job_count` jobs of `ops_per_job` operations over `resource_count`
/// machines; each operation may run on about half of the machines, and the
/// machines differ in efficiency.
fn scenario(
    job_count: usize,
    ops_per_job: usize,
    resource_count: usize,
) -> (Vec<Job>, Vec<Resource>) {
    let jobs = (0..job_count)
        .map(|j| {
            let job_id = format!("J{j:04}");
            (0..ops_per_job).fold(
                Job::new(&job_id)
                    .with_product(format!("PRODUCT-{}", j % 10))
                    .with_priority((j % 5) as i32 + 1),
                |job, o| {
                    let process_time = 5_000 + (j * 100 + o * 50) as i64 % 30_000;
                    let machines: Vec<String> = (0..resource_count)
                        .filter(|r| (j + o + r) % 3 != 0)
                        .take(resource_count / 2 + 1)
                        .map(|r| format!("M{r:02}"))
                        .collect();
                    job.with_operation(
                        Operation::new(format!("{job_id}-O{}", o + 1), &job_id, (o + 1) as i32)
                            .with_time(0, process_time, 0)
                            .with_equipment(machines),
                    )
                },
            )
        })
        .collect();
    let resources = (0..resource_count)
        .map(|r| {
            Resource::equipment(format!("M{r:02}")).with_efficiency(0.8 + (r as f64 * 0.02) % 0.2)
        })
        .collect();
    (jobs, resources)
}

fn ga(population_size: usize, max_generations: usize, time_limit_ms: Option<i64>) -> GaParams {
    GaParams {
        population_size,
        max_generations,
        time_limit_ms,
        ..Default::default()
    }
}

/// Schedules the scenario and returns the wall-clock milliseconds it took,
/// after checking that every job came back scheduled.
fn run(
    mut engine: SchedulingEngine,
    (job_count, ops_per_job, resource_count): (usize, usize, usize),
) -> u128 {
    let (jobs, resources) = scenario(job_count, ops_per_job, resource_count);
    let start = Instant::now();
    let result = engine
        .schedule(&jobs, &resources, 0)
        .expect("a generated scenario is a valid request");
    let elapsed = start.elapsed().as_millis();
    assert!(result.schedule.makespan_ms > 0);
    println!(
        "{job_count}J/{}O/{resource_count}R: {elapsed} ms, makespan {} ms",
        job_count * ops_per_job,
        result.schedule.makespan_ms
    );
    elapsed
}

fn within(budget_ms: u128, engine: SchedulingEngine, shape: (usize, usize, usize)) {
    let elapsed = run(engine, shape);
    assert!(
        elapsed < budget_ms,
        "{shape:?} took {elapsed} ms, budget {budget_ms} ms"
    );
}

#[test]
fn scenario_has_the_requested_shape() {
    let (jobs, resources) = scenario(10, 3, 5);
    assert_eq!(jobs.len(), 10);
    assert_eq!(resources.len(), 5);
    assert_eq!(jobs.iter().map(|j| j.operations.len()).sum::<usize>(), 30);
}

#[test]
fn production_schedules_100_jobs() {
    run(
        SchedulingEngine::production(ProductionConfig::default()),
        (100, 3, 10),
    );
}

#[test]
fn simple_schedules_500_jobs() {
    run(SchedulingEngine::simple(), (500, 2, 20));
}

#[test]
#[ignore = "wall-clock budget: run alone with --release --ignored"]
fn simple_500_jobs_within_10_s() {
    within(10_000, SchedulingEngine::simple(), (500, 2, 20));
}

#[test]
#[ignore = "wall-clock budget: run alone with --release --ignored"]
fn production_within_budget() {
    for (shape, budget_ms) in [
        ((100, 3, 10), 90_000),
        ((200, 3, 15), 180_000),
        ((300, 3, 20), 300_000),
        ((500, 3, 25), 600_000),
    ] {
        within(
            budget_ms,
            SchedulingEngine::production(ProductionConfig::default()),
            shape,
        );
    }
}

#[test]
#[ignore = "wall-clock budget: run alone with --release --ignored"]
fn genetic_within_budget() {
    within(
        90_000,
        SchedulingEngine::genetic(GaParams {
            elite_ratio: 0.1,
            convergence_generations: 20,
            ..ga(50, 100, None)
        }),
        (50, 3, 8),
    );
    within(
        120_000,
        SchedulingEngine::genetic(ga(50, 100, Some(120_000))),
        (100, 3, 10),
    );
    within(
        300_000,
        SchedulingEngine::genetic(ga(50, 50, Some(300_000))),
        (200, 3, 15),
    );
}

#[test]
#[ignore = "wall-clock budget: run alone with --release --ignored"]
fn cp_and_auto_within_budget() {
    within(
        120_000,
        SchedulingEngine::cpsat(CpSatConfig::default()),
        (20, 2, 5),
    );
    within(90_000, SchedulingEngine::auto(), (100, 3, 10));
}
