//! Manufacturing request → u-schedule problem.
//!
//! The scheduling itself belongs to `u-schedule`; this module states a
//! [`ScheduleRequest`] in its terms — jobs as tasks, operations as
//! activities, equipment and workers as typed resources, calendars as working
//! windows, set-up matrices as transition matrices — and names every part of
//! the request it cannot state there yet ([`Unsupported`]), so a caller never
//! mistakes a partial translation for a complete one.

use std::collections::{BTreeSet, HashMap};

use u_schedule::dispatching::{rules, RuleEngine};
use u_schedule::models::{
    Activity, ActivityDuration, Calendar as USCalendar, Resource as USResource,
    ResourceRequirement as USRequirement, Task, TransitionMatrix, TransitionMatrixCollection,
};
use u_schedule::validation::ValidationError;
use u_schedule::Problem;

use crate::scheduler::dispatching::DispatchingRuleName;
use crate::{Calendar, ResourceKind, ResourceType, ScheduleRequest};

/// How far ahead a shift calendar is laid out as working windows.
const CALENDAR_HORIZON_MS: i64 = 366 * 24 * 3_600_000;

/// A part of a request the u-schedule problem cannot state yet, by the
/// general capability it needs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Unsupported {
    /// A processing time that depends on the resource chosen — resource
    /// efficiency, worker proficiency, a per-worker time override, a crew's
    /// efficiency.
    ResourceDependentDuration,
    /// A resource used at less than its whole capacity (a worker's load
    /// factor).
    FractionalResourceUse,
    /// Resources taken together: a robot that occupies its linked cell, a
    /// crew.
    ResourceBundle,
    /// A set-up charged before a resource's first activity.
    InitialSetup,
    /// A release for one operation rather than the whole job (a material
    /// arriving for a later operation).
    ActivityRelease,
    /// A time between operations that depends on where they run (site
    /// transitions).
    TransportTime,
    /// Operation-level splitting, overlap or time windows.
    ActivityShape,
    /// A dispatching rule that changes with the state of the shop.
    RuleSwitching,
    /// Outsourcing, materials beyond a release date, pegging.
    SupplyPlanning,
}

/// The request as a u-schedule problem.
pub struct Adapted {
    /// The tasks and resources, or why u-schedule refused them.
    pub problem: Result<Problem, Vec<ValidationError>>,
    pub start_time_ms: i64,
    pub transition_matrices: TransitionMatrixCollection,
    /// The rule the request dispatches by, if it names one.
    pub rule_engine: Option<RuleEngine>,
    /// What the problem leaves out, each capability once.
    pub unsupported: BTreeSet<Unsupported>,
}

/// States `request` as a u-schedule problem.
pub fn to_problem(request: &ScheduleRequest) -> Adapted {
    let mut unsupported = BTreeSet::new();

    let calendars: HashMap<&str, &Calendar> = request
        .calendars
        .iter()
        .map(|c| (c.id.as_str(), c))
        .collect();

    let resources: Vec<USResource> = request
        .resources
        .iter()
        .map(|r| {
            let kind = match r.kind {
                ResourceKind::Equipment => "Equipment",
                ResourceKind::Worker => "Worker",
            };
            let mut out = USResource::new(&r.id, kind).with_capacity(r.capacity.max(1));
            if r.linked_resource_id.is_some() {
                unsupported.insert(Unsupported::ResourceBundle);
            }
            if let Some(calendar) = r.calendar_id.as_deref().and_then(|id| calendars.get(id)) {
                out = out.with_calendar(windows(calendar, request.start_time_ms));
            }
            if r.kind == ResourceKind::Worker {
                for skill in worker_skills(request, &r.id) {
                    out = out.with_skill(skill);
                }
            }
            out
        })
        .collect();

    let qualified_by_type =
        request.skill_matrix.is_some() || request.certification_matrix.is_some();
    if request.site_transitions.is_some() {
        unsupported.insert(Unsupported::TransportTime);
    }
    if request.pegging_engine.is_some() || request.outsourcing_manager.is_some() {
        unsupported.insert(Unsupported::SupplyPlanning);
    }

    let tasks: Vec<Task> = request
        .jobs
        .iter()
        .map(|job| {
            // u-aps: a lower priority value goes first; u-schedule: higher.
            let mut task = Task::new(&job.id).with_priority(-job.priority);
            if let Some(product) = &job.product_name {
                task = task.with_category(product);
            }
            if let Some(due) = job.due_date {
                task = task.with_deadline(due.timestamp_millis());
            }
            let mut release = job.earliest_start.map(|t| t.timestamp_millis());
            for (i, op) in job.operations.iter().enumerate() {
                for m in &op.material_requirements {
                    if let Some(at) = m.availability_date {
                        if i == 0 {
                            let at = at.timestamp_millis();
                            release = Some(release.map_or(at, |r| r.max(at)));
                        } else {
                            unsupported.insert(Unsupported::ActivityRelease);
                        }
                    }
                }
                if op.crew_id.is_some() {
                    // A crew's efficiency scales the whole team's time.
                    unsupported.insert(Unsupported::ResourceBundle);
                    unsupported.insert(Unsupported::ResourceDependentDuration);
                }
                if op.outsourcing_config.is_some() {
                    unsupported.insert(Unsupported::SupplyPlanning);
                }
                if op.is_splittable
                    || op.split_config.is_some()
                    || op.overlap_config.is_some()
                    || op.time_window.is_some()
                    || op.daily_work_window.is_some()
                {
                    unsupported.insert(Unsupported::ActivityShape);
                }
                let mut activity = Activity::new(&op.id, &job.id, op.sequence)
                    .with_duration(
                        // A u-aps wait holds the resource after processing.
                        ActivityDuration::new(
                            op.time.setup_ms,
                            op.time.process_ms,
                            op.time.wait_ms,
                        ),
                    )
                    // u-aps splits every operation at its calendar's breaks.
                    .resumable();
                for req in &op.required_resources {
                    if req.load_factor < 1.0 {
                        unsupported.insert(Unsupported::FractionalResourceUse);
                    }
                    let kind = match req.resource_type {
                        ResourceType::Equipment => "Equipment",
                        ResourceType::Worker => "Worker",
                    };
                    let mut requirement = USRequirement::new(kind)
                        .with_quantity(req.quantity)
                        .with_candidates(req.candidates.clone());
                    activity = with_timings(activity, request, op, req);
                    if req.resource_type == ResourceType::Worker && qualified_by_type {
                        if let Some(op_type) = &op.operation_type {
                            requirement = requirement.with_skill(op_type);
                        }
                    }
                    activity = activity.with_requirement(requirement);
                }
                task = task.with_activity(activity);
            }
            if let Some(release) = release {
                task = task.with_release_time(release);
            }
            task
        })
        .collect();

    let mut transition_matrices = TransitionMatrixCollection::new();
    for matrix in request.setup_matrices.matrices() {
        unsupported.insert(Unsupported::InitialSetup);
        let mut m = TransitionMatrix::new(&matrix.resource_id, &matrix.resource_id)
            .with_default(matrix.default_setup_ms);
        for (from, to, ms) in matrix.entries() {
            m.set_transition(from, to, ms);
        }
        transition_matrices.add(m);
    }

    let rule_engine = request.dispatching_config.as_ref().map(|config| {
        if config.multi_layer.is_some() {
            unsupported.insert(Unsupported::RuleSwitching);
        }
        let mut engine = add_rule(RuleEngine::new(), config.primary_rule, 1.0);
        if let Some(tie) = config.tie_breaker {
            engine = add_rule(engine, tie, 0.0);
        }
        engine
    });

    Adapted {
        problem: Problem::new(tasks, resources),
        start_time_ms: request.start_time_ms,
        transition_matrices,
        rule_engine,
        unsupported,
    }
}

/// The time of `op` on each resource `req` may take: an equipment's
/// efficiency as a speed; for a worker, a time override for the operation
/// type as a duration of its own, otherwise its proficiency × certification
/// efficiency as a speed (u-aps' order: the override wins).
fn with_timings(
    mut activity: Activity,
    request: &ScheduleRequest,
    op: &crate::Operation,
    req: &crate::ResourceRequirement,
) -> Activity {
    let kind = match req.resource_type {
        ResourceType::Equipment => ResourceKind::Equipment,
        ResourceType::Worker => ResourceKind::Worker,
    };
    let pool = request
        .resources
        .iter()
        .filter(|r| r.kind == kind)
        .filter(|r| req.candidates.is_empty() || req.candidates.contains(&r.id));
    // u-aps reads a worker's figures under the operation type, or the
    // operation id when it has none.
    let op_type = op.operation_type.as_deref().unwrap_or(&op.id);
    for r in pool {
        match kind {
            ResourceKind::Equipment => {
                if (r.efficiency - 1.0).abs() > f64::EPSILON {
                    activity = activity.with_speed_on(&r.id, r.efficiency);
                }
            }
            ResourceKind::Worker => {
                let time_override = request
                    .worker_time_override
                    .as_ref()
                    .and_then(|o| o.get_override(&r.id, op_type));
                if let Some(entry) = time_override {
                    activity = activity.with_duration_on(
                        &r.id,
                        ActivityDuration::new(
                            entry.setup_ms.unwrap_or(op.time.setup_ms),
                            entry.process_ms.unwrap_or(op.time.process_ms),
                            entry.wait_ms.unwrap_or(op.time.wait_ms),
                        ),
                    );
                    continue;
                }
                let proficiency = request
                    .skill_matrix
                    .as_ref()
                    .map_or(1.0, |m| m.get_proficiency(&r.id, op_type));
                let certified = request.certification_matrix.as_ref().map_or(1.0, |m| {
                    m.get_efficiency_modifier(&r.id, op_type, request.start_time_ms)
                });
                let speed = proficiency * certified;
                if speed > 0.0 && (speed - 1.0).abs() > f64::EPSILON {
                    activity = activity.with_speed_on(&r.id, speed);
                }
            }
        }
    }
    activity
}

fn add_rule(engine: RuleEngine, rule: DispatchingRuleName, weight: f64) -> RuleEngine {
    use DispatchingRuleName as R;
    match rule {
        R::Spt => engine.with_weighted_rule(rules::Spt, weight),
        R::Lpt => engine.with_weighted_rule(rules::Lpt, weight),
        R::Lwkr => engine.with_weighted_rule(rules::Lwkr, weight),
        R::Mwkr => engine.with_weighted_rule(rules::Mwkr, weight),
        R::Wspt => engine.with_weighted_rule(rules::Wspt, weight),
        R::Edd => engine.with_weighted_rule(rules::Edd, weight),
        R::Mst => engine.with_weighted_rule(rules::Mst, weight),
        R::Cr => engine.with_weighted_rule(rules::Cr, weight),
        R::Sro => engine.with_weighted_rule(rules::Sro, weight),
        R::Atc => engine.with_weighted_rule(rules::Atc::default(), weight),
        R::Fifo => engine.with_weighted_rule(rules::Fifo, weight),
        R::Winq => engine.with_weighted_rule(rules::Winq, weight),
        R::Lpul => engine.with_weighted_rule(rules::Lpul, weight),
        // The priority was negated with the task, so u-schedule's
        // higher-first rule keeps u-aps' lower-first order.
        R::Priority => engine.with_weighted_rule(rules::Priority, weight),
    }
}

/// The operation types a worker is qualified for, as skills: proficient (a
/// skill-matrix entry above zero) and, with a certification matrix,
/// certified at the schedule start.
fn worker_skills(request: &ScheduleRequest, worker_id: &str) -> Vec<String> {
    let mut types: BTreeSet<&str> = request
        .jobs
        .iter()
        .flat_map(|j| &j.operations)
        .filter_map(|op| op.operation_type.as_deref())
        .collect();
    if let Some(skills) = &request.skill_matrix {
        types.retain(|t| skills.can_perform(worker_id, t));
    }
    if let Some(certs) = &request.certification_matrix {
        types.retain(|t| certs.is_certified(worker_id, t, request.start_time_ms));
    }
    types.into_iter().map(str::to_string).collect()
}

/// `calendar`'s working windows from `start_ms` over [`CALENDAR_HORIZON_MS`],
/// as u-aps itself reads them (shifts, breaks, holidays).
fn windows(calendar: &Calendar, start_ms: i64) -> USCalendar {
    let mut out = USCalendar::new(&calendar.id);
    if calendar.shifts.is_empty() && calendar.holidays.is_empty() {
        return out;
    }
    let horizon = start_ms + CALENDAR_HORIZON_MS;
    let mut t = calendar.next_working_time(start_ms);
    while t < horizon {
        let end = match calendar.find_working_window_end(t) {
            Some((end, _)) if end > t => end,
            _ => break,
        };
        out = out.with_window(t, end);
        let next = calendar.next_working_time(end);
        if next <= t {
            break;
        }
        t = next;
    }
    out
}
