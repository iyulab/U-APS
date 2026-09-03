//! CP-SAT Enhanced Solver
//!
//! 향상된 제약 전파 및 탐색 기반 CP 솔버

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

use crate::cp::{CpModel, CpSolution, IntervalSolution, SolverStatus};
use crate::models::{Job, Resource};
use crate::scheduler::Schedule;

pub struct CpSatScheduler {
    config: CpSatConfig,
}

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

#[derive(Debug, Clone)]
struct Domain {
    min: i64,
    max: i64,
    fixed: Option<i64>,
}

impl Domain {
    fn new(min: i64, max: i64) -> Self {
        Self {
            min,
            max,
            fixed: None,
        }
    }
    fn size(&self) -> i64 {
        if self.fixed.is_some() {
            1
        } else {
            self.max - self.min + 1
        }
    }
    fn is_fixed(&self) -> bool {
        self.fixed.is_some() || self.min == self.max
    }
    fn value(&self) -> Option<i64> {
        if let Some(v) = self.fixed {
            Some(v)
        } else if self.min == self.max {
            Some(self.min)
        } else {
            None
        }
    }
    fn reduce_min(&mut self, new_min: i64) -> bool {
        if new_min > self.min {
            self.min = new_min;
            true
        } else {
            false
        }
    }
    fn reduce_max(&mut self, new_max: i64) -> bool {
        if new_max < self.max {
            self.max = new_max;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
struct TaskInfo {
    name: String,
    est: i64,
    ect: i64,
    lst: i64,
    lct: i64,
    duration: i64,
}

impl TaskInfo {
    fn from_domains(name: &str, start_domain: &Domain, end_domain: &Domain, duration: i64) -> Self {
        Self {
            name: name.to_string(),
            est: start_domain.min,
            ect: start_domain.min + duration,
            lst: start_domain.max,
            lct: end_domain.max,
            duration,
        }
    }
}

#[derive(Clone)]
struct SearchNode {
    start_domains: HashMap<String, Domain>,
    end_domains: HashMap<String, Domain>,
    #[allow(dead_code)]
    precedences: Vec<(String, String)>,
    lower_bound: i64,
    depth: usize,
}

impl SearchNode {
    fn is_complete(&self) -> bool {
        self.start_domains.values().all(|d| d.is_fixed())
    }
    fn makespan(&self) -> i64 {
        self.end_domains
            .values()
            .filter_map(|d| d.value())
            .max()
            .unwrap_or(0)
    }
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
        let status = if solution.is_solution_found() {
            CpSatStatus::Optimal
        } else {
            CpSatStatus::Infeasible
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

    fn solve_internal(&self, model: &CpModel) -> CpSolution {
        let start_time = Instant::now();
        let deadline =
            start_time + std::time::Duration::from_millis(self.config.time_limit_ms as u64);
        if model.validate().is_err() {
            return CpSolution::empty(SolverStatus::ModelInvalid);
        }
        let mut initial_node = SearchNode {
            start_domains: HashMap::new(),
            end_domains: HashMap::new(),
            precedences: Vec::new(),
            lower_bound: 0,
            depth: 0,
        };
        for (name, interval) in &model.intervals {
            let duration = interval.duration.fixed.unwrap_or(interval.duration.min);
            initial_node.start_domains.insert(
                name.clone(),
                Domain::new(interval.start.min, interval.start.max),
            );
            initial_node.end_domains.insert(
                name.clone(),
                Domain::new(interval.start.min + duration, interval.end.max),
            );
        }
        if !self.propagate(&mut initial_node, model) {
            return CpSolution::empty(SolverStatus::Infeasible);
        }
        let mut best_solution: Option<SearchNode> = None;
        let mut best_makespan = i64::MAX;
        let mut stack = vec![initial_node];
        let mut num_nodes = 0u64;
        while let Some(node) = stack.pop() {
            if Instant::now() > deadline {
                break;
            }
            num_nodes += 1;
            if node.lower_bound >= best_makespan {
                continue;
            }
            if node.is_complete() {
                let makespan = node.makespan();
                if makespan < best_makespan {
                    best_makespan = makespan;
                    best_solution = Some(node);
                }
                continue;
            }
            let children = self.branch(&node, model);
            for mut child in children {
                if self.propagate(&mut child, model) && child.lower_bound < best_makespan {
                    stack.push(child);
                }
            }
        }
        let elapsed = start_time.elapsed().as_millis() as i64;
        if let Some(node) = best_solution {
            let mut solution = CpSolution::empty(SolverStatus::Optimal);
            for (name, interval) in &model.intervals {
                let start = node.start_domains[name].value().unwrap_or(0);
                let duration = interval.duration.fixed.unwrap_or(interval.duration.min);
                solution.intervals.insert(
                    name.clone(),
                    IntervalSolution {
                        start,
                        end: start + duration,
                        duration,
                        is_present: true,
                    },
                );
            }
            solution.objective_value = Some(best_makespan as f64);
            solution.solve_time_ms = elapsed;
            solution.num_nodes = num_nodes;
            solution
        } else {
            let mut solution = CpSolution::empty(SolverStatus::Infeasible);
            solution.solve_time_ms = elapsed;
            solution.num_nodes = num_nodes;
            solution
        }
    }

    fn propagate(&self, node: &mut SearchNode, model: &CpModel) -> bool {
        let mut changed = true;
        let mut iterations = 0;
        while changed && iterations < 100 {
            changed = false;
            iterations += 1;
            for constraint in &model.constraints {
                if let crate::cp::CpConstraint::Precedence {
                    before,
                    after,
                    min_delay,
                } = constraint
                {
                    if let (Some(before_end), Some(after_start)) = (
                        node.end_domains.get(before),
                        node.start_domains.get_mut(after),
                    ) {
                        if after_start.reduce_min(before_end.min + min_delay) {
                            changed = true;
                        }
                    }
                    if let (Some(after_start), Some(before_end)) = (
                        node.start_domains.get(after),
                        node.end_domains.get_mut(before),
                    ) {
                        if before_end.reduce_max(after_start.max - min_delay) {
                            changed = true;
                        }
                    }
                }
            }
            for (name, interval) in &model.intervals {
                let duration = interval.duration.fixed.unwrap_or(interval.duration.min);
                if let Some(start_domain) = node.start_domains.get(name) {
                    if let Some(end_domain) = node.end_domains.get_mut(name) {
                        changed |= end_domain.reduce_min(start_domain.min + duration);
                        changed |= end_domain.reduce_max(start_domain.max + duration);
                    }
                }
                if let Some(end_domain) = node.end_domains.get(name) {
                    if let Some(start_domain) = node.start_domains.get_mut(name) {
                        changed |= start_domain.reduce_max(end_domain.max - duration);
                    }
                }
            }
            for constraint in &model.constraints {
                if let crate::cp::CpConstraint::NoOverlap { intervals, .. } = constraint {
                    let tasks: Vec<TaskInfo> = intervals
                        .iter()
                        .filter_map(|name| {
                            let start_domain = node.start_domains.get(name)?;
                            let end_domain = node.end_domains.get(name)?;
                            let duration = model
                                .intervals
                                .get(name)?
                                .duration
                                .fixed
                                .unwrap_or(model.intervals[name].duration.min);
                            Some(TaskInfo::from_domains(
                                name,
                                start_domain,
                                end_domain,
                                duration,
                            ))
                        })
                        .collect();
                    if tasks.len() < 2 {
                        continue;
                    }
                    if !self.check_overload(&tasks) {
                        return false;
                    }
                    // Edge finding disabled (needs refinement)
                    for (name, new_est) in self.edge_finding(&tasks) {
                        if let Some(start_domain) = node.start_domains.get_mut(&name) {
                            if start_domain.reduce_min(new_est) {
                                changed = true;
                            }
                        }
                    }
                    // Temporarily disabled not_first_not_last
                    for (name, new_est, new_lct) in self.not_first_not_last(&tasks) {
                        if let Some(start_domain) = node.start_domains.get_mut(&name) {
                            if start_domain.reduce_min(new_est) {
                                changed = true;
                            }
                        }
                        if let Some(end_domain) = node.end_domains.get_mut(&name) {
                            if end_domain.reduce_max(new_lct) {
                                changed = true;
                            }
                        }
                    }
                }
            }
            for domain in node.start_domains.values() {
                if domain.min > domain.max {
                    return false;
                }
            }
            for domain in node.end_domains.values() {
                if domain.min > domain.max {
                    return false;
                }
            }
        }
        node.lower_bound = node.end_domains.values().map(|d| d.min).max().unwrap_or(0);
        true
    }

    fn check_overload(&self, tasks: &[TaskInfo]) -> bool {
        if tasks.is_empty() {
            return true;
        }
        let mut indices: Vec<usize> = (0..tasks.len()).collect();
        indices.sort_by_key(|&i| tasks[i].lct);
        for &i in &indices {
            let lct = tasks[i].lct;
            let total_duration: i64 = tasks
                .iter()
                .filter(|t| t.lct <= lct)
                .map(|t| t.duration)
                .sum();
            let min_est = tasks
                .iter()
                .filter(|t| t.lct <= lct)
                .map(|t| t.est)
                .min()
                .unwrap_or(0);
            if total_duration > lct - min_est {
                return false;
            }
        }
        true
    }

    fn edge_finding(&self, tasks: &[TaskInfo]) -> Vec<(String, i64)> {
        let mut updates = Vec::new();
        if tasks.len() < 2 {
            return updates;
        }

        let mut sorted_tasks = tasks.to_vec();
        sorted_tasks.sort_by_key(|t| t.lct);
        let n = sorted_tasks.len();

        // For each task j, check if it must be scheduled after set Θ
        for j in 0..n {
            let task_j = &sorted_tasks[j];

            // Try different Θ sets (all tasks with LCT ≤ lct_i, excluding j)
            for i in 0..n {
                if i == j {
                    continue;
                }

                // Θ = tasks with index ≤ i, excluding j
                let theta: Vec<&TaskInfo> = sorted_tasks[0..=i]
                    .iter()
                    .filter(|t| t.name != task_j.name)
                    .collect();

                if theta.is_empty() {
                    continue;
                }

                let theta_est = theta.iter().map(|t| t.est).min().unwrap();
                let theta_duration: i64 = theta.iter().map(|t| t.duration).sum();
                let theta_lct = sorted_tasks[i].lct;

                // Check if j can fit before Θ completes
                // If adding j would exceed the capacity, j must come after Θ
                let total_with_j = theta_duration + task_j.duration;

                if total_with_j > theta_lct - theta_est {
                    // j cannot fit in Θ, so j must start after Θ completes
                    let new_est = theta_est + theta_duration;

                    // Only update if this actually improves the bound
                    // and doesn't make the domain empty
                    if new_est > task_j.est && new_est <= task_j.lct - task_j.duration {
                        updates.push((task_j.name.clone(), new_est));
                    }
                }
            }
        }

        updates
    }

    fn not_first_not_last(&self, tasks: &[TaskInfo]) -> Vec<(String, i64, i64)> {
        let mut updates = Vec::new();
        if tasks.len() < 2 {
            return updates;
        }
        for (i, task_i) in tasks.iter().enumerate() {
            let mut new_est = task_i.est;
            let mut new_lct = task_i.lct;
            for (j, task_j) in tasks.iter().enumerate() {
                if i == j {
                    continue;
                }
                if task_i.est < task_j.ect
                    && task_i.lct <= task_j.lct
                    && task_j.est + task_j.duration + task_i.duration > task_i.lct
                {
                    new_est = new_est.max(task_j.ect);
                }
                if task_i.lct > task_j.lst
                    && task_i.est >= task_j.est
                    && task_i.duration + task_j.duration > task_i.lct - task_j.est
                {
                    new_lct = new_lct.min(task_j.lst);
                }
            }
            // Safety check: only update if domain remains valid
            if new_est <= new_lct - task_i.duration
                && (new_est > task_i.est || new_lct < task_i.lct)
            {
                updates.push((task_i.name.clone(), new_est, new_lct));
            }
        }
        updates
    }

    fn branch(&self, node: &SearchNode, _model: &CpModel) -> Vec<SearchNode> {
        let mut best_var: Option<String> = None;
        let mut best_size = i64::MAX;
        for (name, domain) in &node.start_domains {
            if !domain.is_fixed() && domain.size() < best_size {
                best_size = domain.size();
                best_var = Some(name.clone());
            }
        }
        let var_name = match best_var {
            Some(name) => name,
            None => return vec![],
        };
        let domain = &node.start_domains[&var_name];
        let mid = (domain.min + domain.max) / 2;
        let mut left = node.clone();
        left.start_domains
            .get_mut(&var_name)
            .unwrap()
            .reduce_max(mid);
        left.depth += 1;
        let mut right = node.clone();
        right
            .start_domains
            .get_mut(&var_name)
            .unwrap()
            .reduce_min(mid + 1);
        right.depth += 1;
        vec![left, right]
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
        let scheduler = CpSatScheduler::new(CpSatConfig::default());
        let result = scheduler.schedule(&jobs, &resources, 0);
        assert_eq!(result.status, CpSatStatus::Optimal);
        assert!(result.schedule.makespan_ms > 0);
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
        let scheduler = CpSatScheduler::new(CpSatConfig::default());
        let result = scheduler.schedule(&jobs, &resources, 0);
        assert_eq!(result.status, CpSatStatus::Optimal);
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
    fn test_cpsat_edge_finding() {
        let jobs: Vec<Job> = vec![
            Job::new("J1").with_operation(
                Operation::new("J1-O1", "J1", 1)
                    .with_time(0, 10000, 0)
                    .with_equipment(vec!["M1".to_string()]),
            ),
            Job::new("J2").with_operation(
                Operation::new("J2-O1", "J2", 1)
                    .with_time(0, 10000, 0)
                    .with_equipment(vec!["M1".to_string()]),
            ),
            Job::new("J3").with_operation(
                Operation::new("J3-O1", "J3", 1)
                    .with_time(0, 10000, 0)
                    .with_equipment(vec!["M1".to_string()]),
            ),
        ];
        let resources = vec![Resource::equipment("M1").with_efficiency(1.0)];
        let scheduler = CpSatScheduler::new(CpSatConfig {
            time_limit_ms: 5000,
            propagation_strength: PropagationStrength::Extended,
            ..Default::default()
        });
        let result = scheduler.schedule(&jobs, &resources, 0);
        assert_eq!(result.status, CpSatStatus::Optimal);
        assert_eq!(result.schedule.assignments.len(), 3);
        assert!(result.schedule.makespan_ms >= 30000);
    }

    #[test]
    fn test_domain_operations() {
        let mut domain = Domain::new(0, 100);
        assert!(!domain.is_fixed());
        assert_eq!(domain.size(), 101);
        domain.reduce_min(50);
        assert_eq!(domain.min, 50);
        domain.reduce_max(60);
        assert_eq!(domain.max, 60);
        assert_eq!(domain.size(), 11);
    }

    #[test]
    fn test_task_info_creation() {
        let start_domain = Domain::new(0, 100);
        let end_domain = Domain::new(50, 150);
        let task = TaskInfo::from_domains("test", &start_domain, &end_domain, 50);
        assert_eq!(task.name, "test");
        assert_eq!(task.est, 0);
        assert_eq!(task.ect, 50);
        assert_eq!(task.lst, 100);
        assert_eq!(task.lct, 150);
        assert_eq!(task.duration, 50);
    }
}
