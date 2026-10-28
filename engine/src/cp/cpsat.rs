//! Exact scheduler over a CP model.
//!
//! The request becomes a disjunctive model (each operation on its first
//! candidate resource, job order as precedence, makespan objective), solved
//! by u-metaheur's `BranchAndBoundSolver` -- branch and bound over active
//! schedules, not OR-Tools CP-SAT. The result is `Optimal` only when the
//! search finishes within the time limit; a search cut off by the limit
//! reports `Feasible` (best found) or `Timeout` (none found).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;
use u_metaheur::cp::BranchAndBoundSolver;

use crate::cp::{CpModel, CpSolution, CpSolver, SolverConfig, SolverStatus};
use crate::models::{Job, Resource};
use crate::scheduler::Schedule;

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

    pub fn schedule(
        &self,
        jobs: &[Job],
        resources: &[Resource],
        start_time_ms: i64,
    ) -> CpSatResult {
        let start_instant = Instant::now();
        let model = self.build_model(jobs, resources, start_time_ms);
        let solution = self.solve_internal(&model);
        let schedule = self.solution_to_schedule(&solution, jobs, resources, start_time_ms);
        let elapsed = start_instant.elapsed().as_millis() as i64;
        // The search says how it ended; a solution found is not a proof.
        let status = match solution.status {
            SolverStatus::Optimal => CpSatStatus::Optimal,
            SolverStatus::Feasible => CpSatStatus::Feasible,
            SolverStatus::Timeout => CpSatStatus::Timeout,
            _ => CpSatStatus::Infeasible,
        };
        CpSatResult {
            schedule,
            solution,
            solve_time_ms: elapsed,
            status,
        }
    }

    fn build_model(&self, jobs: &[Job], _resources: &[Resource], start_time_ms: i64) -> CpModel {
        use crate::cp::IntervalVar;
        let horizon = start_time_ms + 30 * 24 * 3600 * 1000;
        let mut model = CpModel::new("scheduling", horizon);
        let mut resource_intervals: HashMap<String, Vec<String>> = HashMap::new();
        for job in jobs {
            let mut prev_op: Option<String> = None;
            for op in &job.operations {
                let resource_id = op
                    .required_resources
                    .first()
                    .and_then(|req| req.candidates.first())
                    .cloned()
                    .unwrap_or_else(|| "default".to_string());
                let interval =
                    IntervalVar::new(&op.id, start_time_ms, horizon, op.time.process_ms, horizon);
                model.add_interval(interval);
                resource_intervals
                    .entry(resource_id)
                    .or_default()
                    .push(op.id.clone());
                if let Some(prev) = prev_op {
                    model.add_precedence(prev, op.id.clone(), 0);
                }
                prev_op = Some(op.id.clone());
            }
        }
        for (_res_id, intervals) in resource_intervals {
            if intervals.len() > 1 {
                model.add_no_overlap(intervals);
            }
        }
        model.set_objective(crate::cp::CpObjective::MinimizeMaxEnd);
        model
    }

    /// The model's exact solution by u-metaheur's branch and bound, within
    /// the time limit.
    fn solve_internal(&self, model: &CpModel) -> CpSolution {
        let config = SolverConfig {
            time_limit_ms: self.config.time_limit_ms,
            num_workers: self.config.num_workers,
            stop_after_first: false,
        };
        BranchAndBoundSolver::new().solve(model, &config)
    }

    fn solution_to_schedule(
        &self,
        solution: &CpSolution,
        jobs: &[Job],
        _resources: &[Resource],
        _start_time_ms: i64,
    ) -> Schedule {
        use crate::scheduler::{Assignment, Schedule};
        let mut assignments = Vec::new();
        for job in jobs {
            for op in &job.operations {
                if let Some(interval_sol) = solution.intervals.get(&op.id) {
                    let resource_id = op
                        .required_resources
                        .first()
                        .and_then(|req| req.candidates.first())
                        .cloned()
                        .unwrap_or_else(|| "default".to_string());
                    assignments.push(Assignment {
                        operation_id: op.id.clone(),
                        job_id: job.id.clone(),
                        resource_id,
                        start_ms: interval_sol.start,
                        end_ms: interval_sol.end,
                        setup_ms: 0,
                        site_id: None,
                        part: None,
                    });
                }
            }
        }
        Schedule {
            assignments,
            makespan_ms: solution.max_end(),
            violations: vec![],
            tasks: vec![],
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Operation;

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
        let result = scheduler.schedule(&jobs, &resources, 0);
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
        let result = scheduler.schedule(&jobs, &resources, 0);
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
        let result = scheduler.schedule(&jobs, &create_test_resources(), 0);
        // Johnson's order J2 J0 J3 J4 J1 gives 24 000.
        assert_eq!(result.status, CpSatStatus::Optimal);
        assert_eq!(result.schedule.makespan_ms, 24_000);
    }
}
