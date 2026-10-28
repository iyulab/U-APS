//! Exact scheduler over a CP model.
//!
//! The request is stated as a u-schedule problem the way the dispatching
//! scheduler states it ([`crate::adapter`]), modelled by u-schedule's
//! `ScheduleCpBuilder` -- an operation that one of several machines may run
//! chooses among them, each for its own time -- and solved by u-metaheur's
//! `BranchAndBoundSolver`: branch and bound over active schedules, not
//! OR-Tools CP-SAT. The result is `Optimal` only when the search finishes
//! within the time limit; a search cut off by the limit reports `Feasible`
//! (best found) or `Timeout` (none found), and a model outside the solver's
//! class (an overlap between steps, among others) `Unknown`.

use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::cp::{CpSolution, SolverStatus};
use crate::scheduler::{Schedule, ScheduleError};
use crate::ScheduleRequest;

pub struct CpSatScheduler {
    config: CpSatConfig,
}

/// Settings of [`CpSatScheduler`]. Only `time_limit_ms` changes the search:
/// the branching (Giffler-Thompson), the bounds and the sequential search are
/// fixed, so `search_strategy`, `propagation_strength`, `restart_policy` and
/// `num_workers` are accepted and not read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpSatConfig {
    pub time_limit_ms: i64,
    pub search_strategy: SearchStrategy,
    pub propagation_strength: PropagationStrength,
    pub restart_policy: RestartPolicy,
    pub num_workers: usize,
}

impl Default for CpSatConfig {
    fn default() -> Self {
        Self {
            time_limit_ms: 60_000,
            search_strategy: SearchStrategy::ActivityBased,
            propagation_strength: PropagationStrength::Full,
            restart_policy: RestartPolicy::Luby,
            num_workers: 4,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SearchStrategy {
    Chronological,
    ActivityBased,
    MinDomainFirst,
    MaxImpactFirst,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PropagationStrength {
    Basic,
    Full,
    Extended,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RestartPolicy {
    None,
    Geometric,
    Luby,
}

impl CpSatScheduler {
    pub fn new(config: CpSatConfig) -> Self {
        Self { config }
    }

    /// Schedules `request` exactly, or says why it cannot be stated.
    pub fn schedule(&self, request: &ScheduleRequest) -> Result<CpSatResult, ScheduleError> {
        self.run(request, None)
    }

    /// As [`schedule`](Self::schedule), starting the search from
    /// `start_from` -- a schedule of the same request, from another
    /// algorithm. Its makespan bounds the search from the start, and it is
    /// the answer unless a shorter one is found: a search that finishes proves
    /// it `Optimal`. A schedule the model does not admit (an operation on a
    /// machine it may not choose, a different length) is not used.
    pub fn schedule_from(
        &self,
        request: &ScheduleRequest,
        start_from: &Schedule,
    ) -> Result<CpSatResult, ScheduleError> {
        self.run(request, Some(start_from))
    }

    fn run(
        &self,
        request: &ScheduleRequest,
        start_from: Option<&Schedule>,
    ) -> Result<CpSatResult, ScheduleError> {
        let start_instant = Instant::now();
        let (schedule, solution) =
            crate::scheduler::placement::exact(request, self.config.time_limit_ms, start_from)?;
        // The search says how it ended; a solution found is not a proof.
        let status = match solution.status {
            SolverStatus::Optimal => CpSatStatus::Optimal,
            SolverStatus::Feasible => CpSatStatus::Feasible,
            SolverStatus::Timeout => CpSatStatus::Timeout,
            SolverStatus::Infeasible => CpSatStatus::Infeasible,
            SolverStatus::Unknown | SolverStatus::ModelInvalid => CpSatStatus::Unknown,
        };
        Ok(CpSatResult {
            schedule,
            solution,
            solve_time_ms: start_instant.elapsed().as_millis() as i64,
            status,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpSatResult {
    pub schedule: Schedule,
    pub solution: CpSolution,
    pub solve_time_ms: i64,
    pub status: CpSatStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CpSatStatus {
    Optimal,
    Feasible,
    Infeasible,
    Timeout,
    /// The request states something the exact solver does not search (an
    /// overlap between steps, among others): no schedule.
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Job, Operation, Resource};

    fn create_test_jobs() -> Vec<Job> {
        (0..5)
            .map(|i| {
                let job_id = format!("J{}", i);
                Job::new(&job_id).with_operation(
                    Operation::new(format!("{}-O1", job_id), &job_id, 1)
                        .with_time(0, 10_000 + (i * 1000) as i64, 0)
                        .with_equipment(vec!["M1".to_string()]),
                )
            })
            .collect()
    }

    fn create_test_resources() -> Vec<Resource> {
        vec![
            Resource::equipment("M1").with_efficiency(1.0),
            Resource::equipment("M2").with_efficiency(0.9),
        ]
    }

    #[test]
    fn test_cpsat_basic() {
        let jobs = create_test_jobs();
        let resources = create_test_resources();
        let scheduler = CpSatScheduler::new(CpSatConfig {
            time_limit_ms: 1000,
            ..Default::default()
        });
        let result = scheduler
            .schedule(&ScheduleRequest::new(jobs, resources))
            .expect("stated");
        // Five operations on one machine: the optimum is their total length.
        assert_eq!(result.status, CpSatStatus::Optimal);
        assert_eq!(result.schedule.makespan_ms, 60_000);
        assert_eq!(result.schedule.assignments.len(), 5);
    }

    #[test]
    fn test_cpsat_precedence() {
        let job = Job::new("J1")
            .with_operation(
                Operation::new("J1-O1", "J1", 1)
                    .with_time(0, 5000, 0)
                    .with_equipment(vec!["M1".to_string()]),
            )
            .with_operation(
                Operation::new("J1-O2", "J1", 2)
                    .with_time(0, 3000, 0)
                    .with_equipment(vec!["M1".to_string()]),
            );
        let jobs = vec![job];
        let resources = create_test_resources();
        let scheduler = CpSatScheduler::new(CpSatConfig {
            time_limit_ms: 1000,
            ..Default::default()
        });
        let result = scheduler
            .schedule(&ScheduleRequest::new(jobs, resources))
            .expect("stated");
        assert_eq!(result.status, CpSatStatus::Optimal);
        assert_eq!(result.schedule.makespan_ms, 8000);
        let o1 = result
            .schedule
            .assignments
            .iter()
            .find(|a| a.operation_id == "J1-O1")
            .unwrap();
        let o2 = result
            .schedule
            .assignments
            .iter()
            .find(|a| a.operation_id == "J1-O2")
            .unwrap();
        assert!(o1.end_ms <= o2.start_ms);
    }

    #[test]
    fn test_cpsat_two_machine_flow() {
        // Johnson's two-machine flow shop: jobs (a, b) on M1 then M2.
        let times = [
            (3000, 6000),
            (5000, 2000),
            (1000, 2000),
            (6000, 6000),
            (7000, 5000),
        ];
        let jobs: Vec<Job> = times
            .iter()
            .enumerate()
            .map(|(i, &(a, b))| {
                let id = format!("J{i}");
                Job::new(&id)
                    .with_operation(
                        Operation::new(format!("{id}-O1"), &id, 1)
                            .with_time(0, a, 0)
                            .with_equipment(vec!["M1".to_string()]),
                    )
                    .with_operation(
                        Operation::new(format!("{id}-O2"), &id, 2)
                            .with_time(0, b, 0)
                            .with_equipment(vec!["M2".to_string()]),
                    )
            })
            .collect();
        let scheduler = CpSatScheduler::new(CpSatConfig {
            time_limit_ms: 5000,
            ..Default::default()
        });
        let machines = vec![Resource::equipment("M1"), Resource::equipment("M2")];
        let result = scheduler
            .schedule(&ScheduleRequest::new(jobs.clone(), machines))
            .expect("stated");
        // Johnson's order J2 J0 J3 J4 J1 gives 24 000.
        assert_eq!(result.status, CpSatStatus::Optimal);
        assert_eq!(result.schedule.makespan_ms, 24_000);
        // M2 at 90 % efficiency takes each second operation 1/0.9 as long.
        let request = ScheduleRequest::new(jobs, create_test_resources());
        let slower = scheduler.schedule(&request).expect("stated");
        assert_eq!(slower.status, CpSatStatus::Optimal);
        assert!(slower.schedule.makespan_ms > 24_000);
        assert!(crate::scheduler::placement::breaks(&request, &slower.schedule).is_empty());
    }

    #[test]
    fn test_cpsat_from_a_schedule_proves_it() {
        let jobs = create_test_jobs();
        let resources = create_test_resources();
        let scheduler = CpSatScheduler::new(CpSatConfig {
            time_limit_ms: 1000,
            ..Default::default()
        });
        let request = ScheduleRequest::new(jobs, resources);
        let first = scheduler.schedule(&request).expect("stated");
        let again = scheduler
            .schedule_from(&request, &first.schedule)
            .expect("stated");
        assert_eq!(again.status, CpSatStatus::Optimal);
        assert_eq!(again.schedule.makespan_ms, 60_000);
        assert!(again.solution.num_nodes <= first.solution.num_nodes);
    }

    #[test]
    fn an_operation_takes_the_machine_that_finishes_first() {
        // "J0-O1" may run on M1 or M2, "J1-O1" on M1 alone: the exact
        // schedule runs them together, J0 on M2.
        let jobs = vec![
            Job::new("J0").with_operation(
                Operation::new("J0-O1", "J0", 1)
                    .with_time(0, 10_000, 0)
                    .with_equipment(vec!["M1".to_string(), "M2".to_string()]),
            ),
            Job::new("J1").with_operation(
                Operation::new("J1-O1", "J1", 1)
                    .with_time(0, 10_000, 0)
                    .with_equipment(vec!["M1".to_string()]),
            ),
        ];
        let resources = vec![Resource::equipment("M1"), Resource::equipment("M2")];
        let scheduler = CpSatScheduler::new(CpSatConfig {
            time_limit_ms: 1000,
            ..Default::default()
        });
        let result = scheduler
            .schedule(&ScheduleRequest::new(jobs, resources))
            .expect("stated");
        assert_eq!(result.status, CpSatStatus::Optimal);
        assert_eq!(result.schedule.makespan_ms, 10_000);
        let on = |op: &str| {
            result
                .schedule
                .assignment_for_operation(op)
                .expect("placed")
                .resource_id
                .clone()
        };
        assert_eq!(on("J0-O1"), "M2");
        assert_eq!(on("J1-O1"), "M1");
    }

    #[test]
    fn set_up_wait_release_and_dependencies_are_kept() {
        // J0's operation: 1 s set-up, 4 s processing, 2 s wait, released at
        // 3 s. J1's waits for it (a dependency across jobs) on the same
        // machine.
        let jobs = vec![
            Job::new("J0").with_operation(
                Operation::new("J0-O1", "J0", 1)
                    .with_time(1_000, 4_000, 2_000)
                    .with_equipment(vec!["M1".to_string()])
                    .with_time_window(crate::models::OperationTimeWindow {
                        earliest_start_ms: Some(3_000),
                        latest_start_ms: None,
                        earliest_end_ms: None,
                        latest_end_ms: None,
                        is_hard: true,
                        penalty_per_ms: 0.0,
                    }),
            ),
            Job::new("J1").with_operation(
                Operation::new("J1-O1", "J1", 1)
                    .with_time(0, 1_000, 0)
                    .with_equipment(vec!["M1".to_string()])
                    .with_dependencies(["J0-O1"]),
            ),
        ];
        let request = ScheduleRequest::new(jobs, vec![Resource::equipment("M1")]);
        let scheduler = CpSatScheduler::new(CpSatConfig {
            time_limit_ms: 1000,
            ..Default::default()
        });
        let result = scheduler.schedule(&request).expect("stated");
        assert_eq!(result.status, CpSatStatus::Optimal);
        let j0 = result
            .schedule
            .assignment_for_operation("J0-O1")
            .expect("placed");
        let j1 = result
            .schedule
            .assignment_for_operation("J1-O1")
            .expect("placed");
        assert_eq!((j0.start_ms, j0.end_ms), (3_000, 10_000));
        assert!(j1.start_ms >= j0.end_ms);
        assert!(crate::scheduler::placement::breaks(&request, &result.schedule).is_empty());
    }
}
