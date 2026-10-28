//! [`SimpleScheduler`]'s placement on u-schedule.
//!
//! The request is stated as a u-schedule problem ([`crate::adapter`]),
//! scheduled by its `SimpleScheduler`, and read back into this engine's
//! [`Schedule`]: an assignment per resource with its site (a linked resource
//! u-schedule books as a companion is not reported, as before), the
//! operation's work as tasks — one for each stretch it actually runs in,
//! between the pauses where its resources (and its own hours) do not all
//! work (an operation without resources on `NONE`, in its own hours) — and
//! the material and due-date violations.

use std::collections::{BTreeSet, HashMap};

use u_metaheur::brkga::BrkgaConfig;
use u_schedule::models::joint_working_spans;
use u_schedule::scheduler::{DispatchSearch, SimpleScheduler as USScheduler};

use super::{
    Assignment, AssignmentPart, ConstraintType, Schedule, ScheduleError, SimpleScheduler, Violation,
};
use crate::adapter::{job_ready_ms, material_delays, outsourced, to_problem, MaterialDelay};
use crate::models::{SplitReason, Task, WorkingWindowEndReason};
use crate::{ResourceKind, ScheduleRequest};

/// Schedules `request` on u-schedule, or says why it cannot.
pub(super) fn schedule(request: &ScheduleRequest) -> Result<Schedule, ScheduleError> {
    place(request, |scheduler, problem, start| {
        Ok(scheduler.schedule(problem, start))
    })
}

/// Schedules `request` with the best dispatching order a random-key search
/// finds (u-schedule's `DispatchSearch`, `config`'s budget), weighing total
/// tardiness by `tardiness_weight` against makespan.
pub(super) fn search(
    request: &ScheduleRequest,
    config: BrkgaConfig,
    tardiness_weight: f64,
) -> Result<Schedule, ScheduleError> {
    place(request, |scheduler, problem, start| {
        DispatchSearch::new(scheduler, config)
            .with_tardiness_weight(tardiness_weight)
            .search(problem, start)
            .map(|found| found.schedule)
            .map_err(ScheduleError::Settings)
    })
}

/// Schedules `request` exactly: u-schedule's `ScheduleCpBuilder` model of the
/// problem the dispatching scheduler places (an operation choosing among the
/// machines that may run it), solved by u-metaheur's `BranchAndBoundSolver`
/// within `time_limit_ms`, from `start_from` when one is given. Returns the
/// schedule read back as the dispatching scheduler's is, and the solver's
/// solution (its status says how the search ended).
pub(crate) fn exact(
    request: &ScheduleRequest,
    time_limit_ms: i64,
    start_from: Option<&Schedule>,
) -> Result<(Schedule, u_metaheur::cp::CpSolution), ScheduleError> {
    use u_metaheur::cp::{BranchAndBoundSolver, CpSolution, SolverConfig, SolverStatus};
    use u_schedule::cp::ScheduleCpBuilder;
    let mut solution = CpSolution::empty(SolverStatus::Unknown);
    let schedule = place(request, |_, problem, start| {
        let builder = ScheduleCpBuilder::new(problem);
        // Room for every operation one after another on its slowest machine,
        // after the latest release.
        let work: i64 = problem
            .tasks()
            .iter()
            .flat_map(|t| &t.activities)
            .map(|a| {
                a.eligible_resources(problem.resources())
                    .into_iter()
                    .map(|r| a.duration_on([r]).total_ms())
                    .max()
                    .unwrap_or_else(|| a.duration.total_ms())
            })
            .sum();
        let released = problem
            .tasks()
            .iter()
            .flat_map(|t| {
                t.release_time
                    .into_iter()
                    .chain(t.activities.iter().filter_map(|a| a.release_time))
            })
            .fold(start, i64::max);
        let mut solver = BranchAndBoundSolver::new();
        if let Some(schedule) = start_from {
            let mut stated = u_schedule::models::Schedule::new();
            for a in &schedule.assignments {
                stated.add_assignment(u_schedule::models::Assignment::new(
                    &a.operation_id,
                    &a.job_id,
                    &a.resource_id,
                    a.start_ms,
                    a.end_ms,
                ));
            }
            solver = solver.with_incumbent(builder.incumbent(&stated));
        }
        let config = SolverConfig {
            time_limit_ms,
            ..SolverConfig::default()
        };
        let (placed, found) = builder.solve(&solver, &config, released + 2 * work);
        solution = found;
        Ok(placed)
    })?;
    Ok((schedule, solution))
}

/// Where `schedule` — by any of this engine's algorithms — breaks `request`,
/// judged by u-schedule's check on the request as the default scheduler
/// states it: an operation without resources (`NONE`) or away at a provider
/// (`OUTSOURCE:…`) as time of its own. Lateness is not a break (it is
/// reported as such), nor is what the schedule already reports unfilled.
pub(crate) fn breaks(
    request: &ScheduleRequest,
    schedule: &Schedule,
) -> Vec<u_schedule::models::Violation> {
    use u_schedule::models::{Assignment as USAssignment, TimeOnlySpan};
    let Ok(problem) = to_problem(request).problem else {
        return Vec::new();
    };
    let mut us = u_schedule::models::Schedule::new();
    for a in &schedule.assignments {
        if a.resource_id == "NONE" || a.resource_id.starts_with("OUTSOURCE:") {
            us.time_only.push(TimeOnlySpan::new(
                &a.operation_id,
                &a.job_id,
                a.start_ms,
                a.end_ms,
            ));
        } else {
            let mut assignment = USAssignment::new(
                &a.operation_id,
                &a.job_id,
                &a.resource_id,
                a.start_ms,
                a.end_ms,
            )
            .with_setup(a.setup_ms);
            if let Some(part) = a.part {
                assignment = assignment.with_part(u_schedule::models::Part {
                    index: part.index,
                    work_ms: part.work_ms,
                });
            }
            us.add_assignment(assignment);
        }
    }
    let reported: BTreeSet<&str> = schedule
        .violations
        .iter()
        .filter(|v| matches!(v.constraint_type, ConstraintType::UnfilledRequirement))
        .map(|v| v.target_id.as_str())
        .collect();
    u_schedule::scheduler::check_schedule(
        &us,
        &u_schedule::scheduler::FeasibilityInput {
            tasks: problem.tasks(),
            resources: problem.resources(),
            constraints: &[],
        },
    )
    .into_iter()
    .filter(|v| v.violation_type != u_schedule::models::ViolationType::DeadlineMiss)
    .filter(|v| !reported.contains(v.entity_id.as_str()))
    .collect()
}

/// Reports in `schedule` where it breaks `request` ([`breaks`]), each as the
/// constraint it breaks.
pub(crate) fn report_breaks(request: &ScheduleRequest, schedule: &mut Schedule) {
    use u_schedule::models::ViolationType as V;
    for v in breaks(request, schedule) {
        let kind = match v.violation_type {
            V::RequirementUnfilled | V::SkillMismatch | V::UnknownActivity => {
                ConstraintType::UnfilledRequirement
            }
            V::EarlyStart => ConstraintType::EarliestStart,
            V::PrecedenceViolation | V::SynchronizeViolation => ConstraintType::Precedence,
            V::CapacityExceeded => ConstraintType::Capacity,
            V::ResourceUnavailable | V::TimeWindowViolation => ConstraintType::WorkingTime,
            V::DurationTooShort => ConstraintType::Duration,
            V::DeadlineMiss => continue,
            V::Custom(_) => ConstraintType::UnfilledRequirement,
        };
        schedule.add_violation(Violation::broken(kind, &v.entity_id, v.message));
    }
}

/// States `request` as a u-schedule problem, has `run` place it with the
/// request's scheduler, and reads the placement back.
fn place(
    request: &ScheduleRequest,
    run: impl FnOnce(
        USScheduler,
        &u_schedule::Problem,
        i64,
    ) -> Result<u_schedule::models::Schedule, ScheduleError>,
) -> Result<Schedule, ScheduleError> {
    let adapted = to_problem(request);
    let problem = adapted.problem.map_err(ScheduleError::Refused)?;
    let mut scheduler = USScheduler::new().with_transition_matrices(adapted.transition_matrices);
    if let Some(engine) = adapted.rule_engine {
        scheduler = scheduler.with_rule_engine(engine);
    }
    if let Some(policy) = adapted.rule_policy {
        scheduler = scheduler.with_rule_policy(policy);
    }
    let placed = run(scheduler, &problem, adapted.start_time_ms)?;
    // An operation u-schedule could not place at all — no time in which its
    // resources (and its own hours) work together — is reported, and its job
    // goes on without it. A late job is reported below, as this engine words
    // it; anything else breaks the problem.
    let unplaced: BTreeSet<&str> = problem
        .tasks()
        .iter()
        .flat_map(|t| &t.activities)
        .map(|a| a.id.as_str())
        .filter(|id| {
            placed.assignment_for_activity(id).is_none()
                && placed.time_only_for_activity(id).is_none()
        })
        .collect();
    let broken: Vec<u_schedule::models::Violation> = placed
        .violations
        .iter()
        .filter(|v| v.violation_type != u_schedule::models::ViolationType::DeadlineMiss)
        .filter(|v| !unplaced.contains(v.entity_id.as_str()))
        .cloned()
        .collect();
    if !broken.is_empty() {
        return Err(ScheduleError::Broken(broken));
    }

    let resources: HashMap<&str, &crate::Resource> = request
        .resources
        .iter()
        .map(|r| (r.id.as_str(), r))
        .collect();
    let calendars: HashMap<&str, &crate::Calendar> = request
        .calendars
        .iter()
        .map(|c| (c.id.as_str(), c))
        .collect();
    let operations: HashMap<&str, (&crate::Job, &crate::Operation)> = request
        .jobs
        .iter()
        .flat_map(|job| {
            job.operations
                .iter()
                .map(move |op| (op.id.as_str(), (job, op)))
        })
        .collect();
    let companions_of: HashMap<&str, &[String]> = problem
        .resources()
        .iter()
        .map(|r| (r.id.as_str(), r.companions.as_slice()))
        .collect();
    // The working time u-schedule placed each operation in.
    let us_calendars: HashMap<&str, &u_schedule::models::Calendar> = problem
        .resources()
        .iter()
        .filter_map(|r| Some((r.id.as_str(), r.calendar.as_ref()?)))
        .collect();

    let mut schedule = Schedule::new();

    // Placements in the order u-schedule made them, one operation at a time
    // — by start, since it decides at the earliest start each time. An
    // operation without resources competes for nothing; it goes where its
    // start falls, as this engine records it, after the placements starting
    // no later (after the operation before it, too), jobs in input order.
    let job_index: HashMap<&str, usize> = request
        .jobs
        .iter()
        .enumerate()
        .map(|(i, j)| (j.id.as_str(), i))
        .collect();
    let mut order: Vec<(&str, (i64, usize))> = Vec::new();
    for a in &placed.assignments {
        if !order.iter().any(|(id, _)| *id == a.activity_id) {
            order.push((&a.activity_id, (a.start_ms, job_index[a.task_id.as_str()])));
        }
    }
    let mut time_only: Vec<(&str, (i64, usize))> = placed
        .time_only
        .iter()
        .map(|s| {
            (
                s.activity_id.as_str(),
                (s.start_ms, job_index[s.task_id.as_str()]),
            )
        })
        .collect();
    time_only.sort_by_key(|&(_, key)| key);
    for (id, key) in time_only {
        // At a common start the decisions went by rule; jobs in input order
        // is how this engine breaks ties.
        let at = order
            .iter()
            .position(|&(_, k)| k > key)
            .unwrap_or(order.len());
        order.insert(at, (id, key));
    }
    let order: Vec<&str> = order.into_iter().map(|(id, _)| id).collect();
    let activities: HashMap<&str, &u_schedule::models::Activity> = problem
        .tasks()
        .iter()
        .flat_map(|t| &t.activities)
        .map(|a| (a.id.as_str(), a))
        .collect();
    // When each job reaches its next operation: its ready time, then the end
    // of the operation before.
    let mut reached: HashMap<&str, i64> = request
        .jobs
        .iter()
        .map(|j| (j.id.as_str(), job_ready_ms(request, j)))
        .collect();
    for op_id in order {
        let (job, op) = operations[op_id];
        // An operation's materials, checked when the job reaches it: waiting
        // for one that arrives later, or short of one.
        let mut at = reached[job.id.as_str()];
        for delay in material_delays(request, op, at) {
            match delay {
                MaterialDelay::Until { material_id, at_ms } if at_ms > at => {
                    schedule.add_violation(Violation::material_availability_violation(
                        &op.id,
                        &material_id,
                        at_ms - at,
                    ));
                    at = at_ms;
                }
                MaterialDelay::Until { .. } => {}
                MaterialDelay::Short {
                    material_id,
                    required,
                    available,
                } => schedule.add_violation(Violation::material_shortage_violation(
                    &op.id,
                    &material_id,
                    required,
                    available,
                )),
            }
        }
        if let Some(span) = placed.time_only_for_activity(op_id) {
            reached.insert(job.id.as_str(), span.end_ms);
            // Away at its provider: no tasks of this plant's own.
            if let Some((provider, _)) = outsourced(request, op) {
                schedule.add_assignment(Assignment::new(
                    op_id,
                    &job.id,
                    format!("OUTSOURCE:{provider}"),
                    span.start_ms,
                    span.end_ms,
                ));
                continue;
            }
            schedule.add_assignment(Assignment::new(
                op_id,
                &job.id,
                "NONE",
                span.start_ms,
                span.end_ms,
            ));
            let own: Vec<&u_schedule::models::Calendar> =
                activities[op_id].calendar.iter().collect();
            for task in tasks_of(
                job,
                op,
                joint_working_spans(&own, span.start_ms, span.end_ms),
                op.time.setup_ms,
                request.calendars.first(),
            ) {
                schedule.add_task(task);
            }
            continue;
        }
        // The operation whole, or each part of it a split placed — a part
        // holds its own resources over its own span.
        let mut parts: Vec<(
            Option<u_schedule::models::Part>,
            Vec<&u_schedule::models::Assignment>,
        )> = Vec::new();
        for a in placed.assignments.iter().filter(|a| a.activity_id == op_id) {
            match parts.iter_mut().find(|(p, _)| *p == a.part) {
                Some((_, group)) => group.push(a),
                None => parts.push((a.part, vec![a])),
            }
        }
        parts.sort_by_key(|(p, _)| p.map(|p| p.index));
        let mut pieces = Vec::new();
        let mut op_end = i64::MIN;
        for (part, all) in &parts {
            // A resource held as a companion of another the part runs on.
            let held: Vec<&u_schedule::models::Assignment> = all
                .iter()
                .copied()
                .filter(|a| {
                    !all.iter().any(|p| {
                        companions_of
                            .get(p.resource_id.as_str())
                            .is_some_and(|c| c.contains(&a.resource_id))
                    })
                })
                .collect();
            let (start, end) = (held[0].start_ms, held[0].end_ms);
            op_end = op_end.max(end);
            let setup = held.iter().map(|a| a.setup_ms).max().unwrap_or(0);
            for a in &held {
                let mut assignment =
                    Assignment::with_setup(op_id, &job.id, &a.resource_id, start, end, setup);
                assignment.site_id = resources
                    .get(a.resource_id.as_str())
                    .and_then(|r| r.site_id.clone());
                assignment.part = part.map(|p| AssignmentPart {
                    index: p.index,
                    work_ms: p.work_ms,
                });
                schedule.add_assignment(assignment);
            }

            // The work in the stretches it runs in: while every resource
            // held and the operation's own hours work together, as u-schedule
            // placed it.
            let primary = held[0].resource_id.as_str();
            let calendar = resources
                .get(primary)
                .and_then(|r| r.calendar_id.as_deref())
                .and_then(|id| calendars.get(id).copied());
            let joint: Vec<&u_schedule::models::Calendar> = all
                .iter()
                .filter_map(|a| us_calendars.get(a.resource_id.as_str()).copied())
                .chain(activities[op_id].calendar.as_ref())
                .collect();
            // u-aps names the workers it picked by requirement, not a crew's.
            let workers: Vec<String> = if op.crew_id.is_some() && is_crewed(request, op) {
                Vec::new()
            } else {
                held.iter()
                    .filter(|a| {
                        resources
                            .get(a.resource_id.as_str())
                            .is_some_and(|r| r.kind == ResourceKind::Worker)
                    })
                    .map(|a| a.resource_id.clone())
                    .collect()
            };
            for (span, setup, reason) in
                stretches(joint_working_spans(&joint, start, end), setup, calendar)
            {
                pieces.push((span, setup, reason, primary, workers.clone()));
            }
        }
        reached.insert(job.id.as_str(), op_end);
        let required = op
            .required_resources
            .iter()
            .find(|r| r.resource_type == crate::ResourceType::Worker)
            .map_or(0, |r| r.quantity as u32);
        let total = pieces.len() as u32;
        for (k, (span, setup, reason, primary, workers)) in pieces.into_iter().enumerate() {
            let mut task =
                task_of(job, op, k as u32, total, span, setup, reason).with_equipment(primary);
            if !workers.is_empty() {
                task = task.with_workers(workers, required);
            }
            schedule.add_task(task);
        }
    }

    // What could not be filled, in the order of the request.
    for (op_id, why) in &adapted.unfilled {
        schedule.add_violation(Violation::unfilled_requirement_violation(
            op_id,
            why.clone(),
        ));
    }
    for job in &request.jobs {
        for op in job
            .operations
            .iter()
            .filter(|o| unplaced.contains(o.id.as_str()))
        {
            let why = placed
                .violations
                .iter()
                .find(|v| v.entity_id == op.id)
                .map_or_else(String::new, |v| format!(": {}", v.message));
            schedule.add_violation(Violation::unfilled_requirement_violation(
                &op.id,
                format!("Operation {} could not be scheduled{why}", op.id),
            ));
        }
    }
    SimpleScheduler::check_due_dates(&mut schedule, &request.jobs);
    Ok(schedule)
}

/// `op`'s work as tasks, one per stretch of `spans` it runs in, in order:
/// `<operation>-T001`, `-T002`, … The set-up comes first, in as many
/// stretches as it takes; the rest of each stretch is processing. A stretch
/// that ends before the work does says why from `calendar` (the first
/// resource's) when the pause is that calendar's, else as the end of a shift.
fn tasks_of(
    job: &crate::Job,
    op: &crate::Operation,
    spans: Vec<(i64, i64)>,
    setup_ms: i64,
    calendar: Option<&crate::Calendar>,
) -> Vec<Task> {
    let stretches = stretches(spans, setup_ms, calendar);
    let total = stretches.len() as u32;
    stretches
        .into_iter()
        .enumerate()
        .map(|(k, (span, setup, reason))| task_of(job, op, k as u32, total, span, setup, reason))
        .collect()
}

/// Each of `spans` with the set-up it carries (the set-up first, in as many
/// stretches as it takes) and why it ends — as [`tasks_of`] reads them.
fn stretches(
    spans: Vec<(i64, i64)>,
    setup_ms: i64,
    calendar: Option<&crate::Calendar>,
) -> Vec<((i64, i64), i64, SplitReason)> {
    let last = spans.len().saturating_sub(1);
    let mut setup_left = setup_ms;
    spans
        .into_iter()
        .enumerate()
        .map(|(k, (begin, end))| {
            let setup = setup_left.min(end - begin);
            setup_left -= setup;
            let reason = if k == last {
                SplitReason::None
            } else {
                match calendar.and_then(|c| c.find_working_window_end(begin)) {
                    Some((at, WorkingWindowEndReason::BreakStart)) if at == end => {
                        SplitReason::BreakStart
                    }
                    Some((at, WorkingWindowEndReason::Holiday)) if at == end => {
                        SplitReason::Holiday
                    }
                    _ => SplitReason::ShiftEnd,
                }
            };
            ((begin, end), setup, reason)
        })
        .collect()
}

/// The `k`-th of `total` tasks of `op`: `<operation>-T00k+1` over `span`.
fn task_of(
    job: &crate::Job,
    op: &crate::Operation,
    k: u32,
    total: u32,
    (begin, end): (i64, i64),
    setup: i64,
    reason: SplitReason,
) -> Task {
    let mut task = Task::new(
        format!("{}-T{:03}", op.id, k + 1),
        &op.id,
        &job.id,
        begin,
        end,
    )
    .with_setup(setup)
    .with_process(end - begin - setup)
    .with_splittable(op.is_splittable || job.is_splittable)
    .with_split_info(k, total, reason);
    if let Some(name) = &job.product_name {
        task = task.with_product_name(name);
    }
    task
}

/// Whether `op`'s crew takes the operation (operational, as u-aps reads it).
fn is_crewed(request: &ScheduleRequest, op: &crate::Operation) -> bool {
    op.crew_id
        .as_deref()
        .and_then(|id| request.crew_manager.as_ref()?.get_crew(id))
        .is_some_and(|c| c.is_operational())
}
