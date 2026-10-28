//! Manufacturing request → u-schedule problem.
//!
//! The scheduling itself belongs to `u-schedule`; this module states a
//! [`ScheduleRequest`] in its terms — jobs as tasks, operations as
//! activities, equipment and workers as typed resources, calendars as working
//! windows, set-up matrices as transition matrices, an outsourced operation
//! as time of its own — and names what nothing can fill ([`Adapted::unfilled`]).
//! A request's pegging engine gives each operation the time its pegged
//! materials are available, as a release time like a material's arrival.

use std::collections::{BTreeSet, HashMap};

use u_schedule::dispatching::{rules, Decision, RuleEngine, RulePolicy};
use u_schedule::models::{
    Activity, ActivityDuration, Calendar as USCalendar, Overlap, Resource as USResource,
    ResourceRequirement as USRequirement, Split, Task, TransitionMatrix,
    TransitionMatrixCollection,
};
use u_schedule::validation::ValidationError;
use u_schedule::Problem;

use crate::scheduler::dispatching::{
    job_weight, DispatchingContext, DispatchingRuleName, MultiLayerDispatchingConfig,
};
use crate::{Calendar, ResourceKind, ResourceType, ScheduleRequest};

/// The units a worker's capacity is laid out in, so a load factor (a
/// fraction of the worker) is a whole usage: 0.5 takes 500.
const WORKER_UNITS: i32 = 1_000;

/// How far ahead a shift calendar is laid out as working windows.
const CALENDAR_HORIZON_MS: i64 = 366 * 24 * 3_600_000;

/// The request as a u-schedule problem.
pub struct Adapted {
    /// The tasks and resources, or why u-schedule refused them.
    pub problem: Result<Problem, Vec<ValidationError>>,
    pub start_time_ms: i64,
    pub transition_matrices: TransitionMatrixCollection,
    /// The rule the request dispatches by, if it names one.
    pub rule_engine: Option<RuleEngine>,
    /// The rules chosen at each decision, when the request switches rules
    /// (`DispatchingConfig.multi_layer`) — in place of `rule_engine`.
    pub rule_policy: Option<MultiLayerPolicy>,
    /// The requirements left out because nothing can fill them — a job's
    /// target site keeps none of the operation's equipment; u-aps schedules
    /// the operation without it — as (operation, why).
    pub unfilled: Vec<(String, String)>,
}

/// States `request` as a u-schedule problem.
pub fn to_problem(request: &ScheduleRequest) -> Adapted {
    let mut unfilled = Vec::new();

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
            let units = match r.kind {
                ResourceKind::Equipment => 1,
                ResourceKind::Worker => WORKER_UNITS,
            };
            let mut out = USResource::new(&r.id, kind).with_capacity(r.capacity.max(1) * units);
            // A robot occupies the cell it is linked to.
            if let Some(linked) = &r.linked_resource_id {
                out = out.with_companion(linked);
            }
            if let Some(calendar) = r.calendar_id.as_deref().and_then(|id| calendars.get(id)) {
                out = out.with_calendar(windows(calendar, request.start_time_ms));
            }
            if r.kind == ResourceKind::Worker {
                for skill in worker_skills(request, &r.id) {
                    out = out.with_skill(skill);
                }
            }
            for (to, ms) in site_transport(request, r) {
                out = out.with_transport_to(to, ms);
            }
            out
        })
        .collect();

    // u-aps runs an operation without resources on the request's first
    // calendar.
    let default_calendar = request
        .calendars
        .first()
        .map(|c| windows(c, request.start_time_ms));

    let qualified_by_type =
        request.skill_matrix.is_some() || request.certification_matrix.is_some();

    let tasks: Vec<Task> = request
        .jobs
        .iter()
        // A job with no operations has nothing to schedule.
        .filter(|job| !job.operations.is_empty())
        .map(|job| {
            // u-aps: a lower priority value goes first; u-schedule: higher.
            // u-aps weighs a job by its priority (WSPT, ATC).
            let mut task = Task::new(&job.id)
                .with_priority(-job.priority)
                .with_weight(job_weight(job.priority));
            if let Some(product) = &job.product_name {
                task = task.with_category(product);
            }
            if let Some(due) = job.due_date {
                task = task.with_deadline(due.timestamp_millis());
            }
            let mut release = job.earliest_start.map(|t| t.timestamp_millis());
            for (i, op) in job.operations.iter().enumerate() {
                // u-aps checks an operation's materials against the material
                // manager when the job reaches it, and waits for the latest
                // to arrive. The manager does not draw stock down, so the
                // arrival is the same whenever it is asked.
                let pegged = request
                    .pegging_engine
                    .as_ref()
                    .map(|p| p.get_operation_material_availability(&op.id))
                    .filter(|&at| at > job_ready_ms(request, job));
                let arrival = material_delays(request, op, job_ready_ms(request, job))
                    .filter_map(|d| match d {
                        MaterialDelay::Until { at_ms, .. } => Some(at_ms),
                        MaterialDelay::Short { .. } => None,
                    })
                    .chain(pegged)
                    .max();
                // An operation's earliest start (its time window) holds it the same
                // way a late material does: the first operation's holds the job.
                let window = op.time_window.as_ref();
                let earliest = window.and_then(|w| w.earliest_start_ms);
                let mut op_release = None;
                for at in [arrival, earliest].into_iter().flatten() {
                    if i == 0 {
                        release = Some(release.map_or(at, |r| r.max(at)));
                    } else {
                        op_release = Some(op_release.map_or(at, |r: i64| r.max(at)));
                    }
                }
                // The other bounds of a time window are not applied — say so rather
                // than accept them in silence.
                if window.is_some_and(|w| w.latest_start_ms.is_some() || w.earliest_end_ms.is_some() || w.latest_end_ms.is_some()) {
                    unfilled.push((
                        op.id.clone(),
                        format!("Operation {} has a time window with a latest start or an end bound; only its earliest start is applied", op.id),
                    ));
                }
                // u-aps takes an operational crew whole, in place of the
                // operation's worker requirements.
                let mut crew = op
                    .crew_id
                    .as_deref()
                    .and_then(|id| request.crew_manager.as_ref()?.get_crew(id))
                    .filter(|c| c.is_operational());
                // A crew not assigned the operation's type does not take it:
                // the operation keeps its own worker requirements.
                if let (Some(c), Some(op_type)) = (crew, op.operation_type.as_deref()) {
                    if !c.handles_operation_type(op_type) {
                        unfilled.push((
                            op.id.clone(),
                            format!(
                                "Operation {} is of type {op_type}, which crew {} is not assigned; scheduled with the operation's own workers",
                                op.id, c.id
                            ),
                        ));
                        crew = None;
                    }
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
                if let Some(at) = op_release {
                    activity = activity.with_release_time(at);
                }
                // Operations of any job it waits for, past its own job's order.
                for dependency in &op.dependencies {
                    activity = activity.with_predecessor(dependency);
                }
                // u-aps states an overlap on the operation before: this one
                // may start once that one is that far along.
                let overlap = overlap_after(job, op);
                if let Some(overlap) = overlap {
                    activity = activity.with_overlap(overlap);
                }
                // An outsourced operation holds nothing here: it takes its
                // lead time from when the job reaches it, whatever the hour.
                if let Some((_, lead_ms)) = outsourced(request, op) {
                    let mut away = Activity::new(&op.id, &job.id, op.sequence)
                        .with_duration(ActivityDuration::fixed(lead_ms));
                    if let Some(at) = op_release {
                        away = away.with_release_time(at);
                    }
                    for dependency in &op.dependencies {
                        away = away.with_predecessor(dependency);
                    }
                    if let Some(overlap) = overlap {
                        away = away.with_overlap(overlap);
                    }
                    task = task.with_activity(away);
                    continue;
                }
                if let Some(split) = split_of(job, op) {
                    activity = activity.with_split(split);
                }
                for req in &op.required_resources {
                    if crew.is_some() && req.resource_type == ResourceType::Worker {
                        continue;
                    }
                    let kind = match req.resource_type {
                        ResourceType::Equipment => "Equipment",
                        ResourceType::Worker => "Worker",
                    };
                    let Some(candidates) = on_target_site(request, job, req) else {
                        unfilled.push((
                            op.id.clone(),
                            format!(
                                "Operation {} needs one of {} on target site {}, which keeps none of them; scheduled without it",
                                op.id,
                                req.candidates.join(", "),
                                job.target_site_id.as_deref().unwrap_or_default()
                            ),
                        ));
                        continue;
                    };
                    let Some(candidates) = crew_equipment(crew, req, candidates) else {
                        unfilled.push((
                            op.id.clone(),
                            format!(
                                "Operation {} needs one of {}, none of which its crew {} is assigned; scheduled without it",
                                op.id,
                                req.candidates.join(", "),
                                crew.map(|c| c.id.as_str()).unwrap_or_default()
                            ),
                        ));
                        continue;
                    };
                    let mut requirement = USRequirement::new(kind)
                        .with_quantity(req.quantity)
                        .with_usage(usage(req))
                        .with_candidates(candidates);
                    activity = with_timings(activity, request, op, req);
                    if req.resource_type == ResourceType::Worker && qualified_by_type {
                        if let Some(op_type) = &op.operation_type {
                            requirement = requirement.with_skill(op_type);
                        }
                    }
                    activity = activity.with_requirement(requirement);
                }
                // After the equipment, as u-aps lists the resources.
                if let Some(crew) = crew {
                    activity = with_crew(activity, request, op, crew);
                }
                // An operation left with no resources runs on the request's
                // first calendar; a daily work window holds any operation to
                // its hours, within that.
                let own = activity
                    .resource_requirements
                    .is_empty()
                    .then(|| default_calendar.clone())
                    .flatten();
                let daily = op
                    .daily_work_window
                    .map(|w| daily_windows(w, request.start_time_ms));
                let own = match (own, daily) {
                    (Some(a), Some(b)) => Some(intersection(&a, &b)),
                    (a, b) => a.or(b),
                };
                if let Some(calendar) = own {
                    activity = activity.with_calendar(calendar);
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
        // u-aps charges the default before a resource's first job too.
        let mut m = TransitionMatrix::new(&matrix.resource_id, &matrix.resource_id)
            .with_default(matrix.default_setup_ms)
            .with_initial(matrix.default_setup_ms);
        for (from, to, ms) in matrix.entries() {
            m.set_transition(from, to, ms);
        }
        transition_matrices.add(m);
    }

    let (rule_engine, rule_policy) = match &request.dispatching_config {
        None => (None, None),
        Some(config) => match &config.multi_layer {
            Some(multi) => (None, Some(MultiLayerPolicy(multi.clone()))),
            None => {
                let mut engine = add_rule(RuleEngine::new(), config.primary_rule, 1.0);
                if let Some(tie) = config.tie_breaker {
                    engine = add_rule(engine, tie, 0.0);
                }
                (Some(engine), None)
            }
        },
    };

    Adapted {
        problem: Problem::new(tasks, resources),
        start_time_ms: request.start_time_ms,
        transition_matrices,
        rule_engine,
        rule_policy,
        unfilled,
    }
}

/// How `op` may overlap the operation before it in `job` (by sequence): u-aps
/// states the overlap on that operation — `op` may start once it is
/// `min_predecessor_completion` of the way through — and `op` ends no earlier
/// than it. `transfer_batch_size` is not read: there is no quantity model.
fn overlap_after(job: &crate::Job, op: &crate::Operation) -> Option<Overlap> {
    let before = job
        .operations
        .iter()
        .filter(|o| o.sequence < op.sequence)
        .max_by_key(|o| o.sequence)?;
    let config = before.overlap_config.as_ref()?;
    Some(Overlap::new(config.min_predecessor_completion))
}

/// How `op` may be split into parts on its candidates: when it, or its whole
/// job, is splittable — by its `split_config`, else u-aps' default.
fn split_of(job: &crate::Job, op: &crate::Operation) -> Option<Split> {
    if !(op.is_splittable || job.is_splittable) {
        return None;
    }
    let config = op.split_config.clone().unwrap_or_default();
    Some(
        Split::new(i32::try_from(config.max_splits).unwrap_or(i32::MAX))
            .with_min_part(config.min_split_size_ms)
            .with_part_overhead(config.split_setup_overhead_ms),
    )
}

/// Who an outsourced operation goes to and for how long: its own lead time,
/// else its provider's default, else its processing time — as u-aps reads it.
pub(crate) fn outsourced<'r>(
    request: &'r ScheduleRequest,
    op: &'r crate::Operation,
) -> Option<(&'r str, i64)> {
    let config = op.outsourcing_config.as_ref()?;
    let lead_ms = config.lead_time_ms.unwrap_or_else(|| {
        request
            .outsourcing_manager
            .as_ref()
            .and_then(|m| m.get_provider(&config.provider_id))
            .map_or(op.time.process_ms, |p| p.default_lead_time_ms)
    });
    Some((config.provider_id.as_str(), lead_ms))
}

/// u-aps' multi-layer dispatching as a u-schedule [`RulePolicy`]: at each
/// decision, the switch rule whose condition holds on that decision's state
/// (the first by priority) or the default, or the weighted layers.
#[derive(Debug, Clone)]
pub struct MultiLayerPolicy(MultiLayerDispatchingConfig);

impl RulePolicy for MultiLayerPolicy {
    fn rules_for(&self, decision: &Decision<'_>) -> RuleEngine {
        let now = decision.context.current_time_ms;
        let waiting = || decision.waiting.iter().map(|&i| &decision.tasks[i]);
        let state = DispatchingContext {
            current_time_ms: now,
            delayed_job_count: waiting()
                .filter(|t| t.deadline.is_some_and(|due| due < now))
                .count(),
            resource_utilization: decision.context.resource_utilization.clone(),
            queue_length: decision.waiting.len(),
            // The priority was negated with the task.
            min_priority: waiting().map(|t| -t.priority).min(),
        };
        let multi = &self.0;
        if multi.layers.is_empty() {
            add_rule(RuleEngine::new(), multi.select_rule(&state), 1.0)
        } else {
            multi
                .layers
                .iter()
                .fold(RuleEngine::new(), |engine, layer| {
                    add_rule(engine, layer.rule, layer.weight)
                })
        }
    }
}

/// The time of `op` on each resource `req` may take: an equipment's
/// efficiency as a speed, a worker's as [`with_worker_time`] reads it.
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
    for r in pool {
        // The operation's own time on this resource states it outright.
        if let Some(entry) = op.resource_times.get(&r.id) {
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
        match kind {
            ResourceKind::Equipment => {
                if (r.efficiency - 1.0).abs() > f64::EPSILON {
                    activity = activity.with_speed_on(&r.id, r.efficiency);
                }
            }
            ResourceKind::Worker => {
                activity = with_worker_time(activity, request, op, &r.id, 1.0);
            }
        }
    }
    activity
}

/// A worker's time on `op`, its speed scaled by `team_speed`: a time
/// override for the operation type as a duration of its own (u-aps' order:
/// the override wins), otherwise proficiency × certification efficiency as a
/// speed. u-aps reads a worker's figures under the operation type, or the
/// operation id when it has none.
fn with_worker_time(
    activity: Activity,
    request: &ScheduleRequest,
    op: &crate::Operation,
    worker_id: &str,
    team_speed: f64,
) -> Activity {
    let op_type = op.operation_type.as_deref().unwrap_or(&op.id);
    let time_override = request
        .worker_time_override
        .as_ref()
        .and_then(|o| o.get_override(worker_id, op_type));
    if let Some(entry) = time_override {
        return activity.with_duration_on(
            worker_id,
            ActivityDuration::new(
                entry.setup_ms.unwrap_or(op.time.setup_ms),
                entry.process_ms.unwrap_or(op.time.process_ms),
                entry.wait_ms.unwrap_or(op.time.wait_ms),
            ),
        );
    }
    let proficiency = request
        .skill_matrix
        .as_ref()
        .map_or(1.0, |m| m.get_proficiency(worker_id, op_type));
    let certified = request.certification_matrix.as_ref().map_or(1.0, |m| {
        m.get_efficiency_modifier(worker_id, op_type, request.start_time_ms)
    });
    let speed = proficiency * certified * team_speed;
    if speed > 0.0 && (speed - 1.0).abs() > f64::EPSILON {
        activity.with_speed_on(worker_id, speed)
    } else {
        activity
    }
}

/// `crew` working `op`: every member held for the whole operation, the
/// crew's efficiency a speed of the team. u-aps reads the first member as
/// the operation's worker, so that member's own time carries the team's
/// speed — the members always go together, so which one holds it does not
/// change the time.
fn with_crew(
    mut activity: Activity,
    request: &ScheduleRequest,
    op: &crate::Operation,
    crew: &crate::Crew,
) -> Activity {
    let mut held = BTreeSet::new();
    for member in crew.members() {
        if held.insert(member.worker_id.as_str()) {
            activity = activity.with_requirement(
                USRequirement::new("Worker")
                    .with_candidates(vec![member.worker_id.clone()])
                    .with_usage(WORKER_UNITS),
            );
        }
    }
    match crew.members().first() {
        Some(first) => with_worker_time(
            activity,
            request,
            op,
            &first.worker_id,
            crew.efficiency_modifier,
        ),
        None => activity,
    }
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

/// When `job` can first start: the schedule start, or its earliest start if
/// later.
pub(crate) fn job_ready_ms(request: &ScheduleRequest, job: &crate::Job) -> i64 {
    job.earliest_start.map_or(request.start_time_ms, |t| {
        t.timestamp_millis().max(request.start_time_ms)
    })
}

/// What a material of an operation does to it at `at_ms`, by the material
/// manager.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MaterialDelay {
    /// It arrives later — the operation waits until then.
    Until { material_id: String, at_ms: i64 },
    /// There is not enough of it, with no arrival to wait for.
    Short {
        material_id: String,
        required: f64,
        available: f64,
    },
}

/// The materials `op` lacks at `at_ms` ([`MaterialDelay`]); none without a
/// material manager.
pub(crate) fn material_delays<'r>(
    request: &'r ScheduleRequest,
    op: &'r crate::Operation,
    at_ms: i64,
) -> impl Iterator<Item = MaterialDelay> + 'r {
    let check_time =
        chrono::DateTime::from_timestamp_millis(at_ms).unwrap_or_else(chrono::Utc::now);
    let result = request
        .material_manager
        .as_ref()
        .map(|m| m.check_availability(&op.material_requirements, check_time));
    result
        .into_iter()
        .filter(|r| !r.is_available)
        .flat_map(|r| r.shortages)
        .filter_map(move |s| match s.available_date {
            Some(date) => {
                let at = date.timestamp_millis();
                (at > at_ms).then_some(MaterialDelay::Until {
                    material_id: s.material_id,
                    at_ms: at,
                })
            }
            None => Some(MaterialDelay::Short {
                material_id: s.material_id,
                required: s.required,
                available: s.available,
            }),
        })
}

/// How much of a resource `req` takes: a worker's load factor in
/// [`WORKER_UNITS`] (at least one unit), a piece of equipment whole.
fn usage(req: &crate::ResourceRequirement) -> i32 {
    match req.resource_type {
        ResourceType::Equipment => 1,
        ResourceType::Worker => {
            ((req.load_factor * f64::from(WORKER_UNITS)).round() as i32).clamp(1, WORKER_UNITS)
        }
    }
}

/// The time work takes to move from equipment `from` to each other piece of
/// equipment on another site, by the request's site transitions — u-aps
/// moves a job between the sites of the equipment its operations run on.
fn site_transport<'r>(
    request: &'r ScheduleRequest,
    from: &'r crate::Resource,
) -> impl Iterator<Item = (&'r str, i64)> {
    let transitions = request.site_transitions.as_ref();
    let from_site = from
        .site_id
        .as_deref()
        .filter(|_| from.kind == ResourceKind::Equipment);
    request
        .resources
        .iter()
        .filter(move |_| transitions.is_some() && from_site.is_some())
        .filter(|to| to.kind == ResourceKind::Equipment)
        .filter_map(move |to| {
            let ms = transitions?.get_transition_time(from_site, to.site_id.as_deref());
            (ms > 0).then_some((to.id.as_str(), ms))
        })
}

/// `req`'s candidates, kept to the job's target site when it names one (a
/// piece of equipment with no site stays — u-aps' rule). `None` when the
/// site keeps none of them.
fn on_target_site(
    request: &ScheduleRequest,
    job: &crate::Job,
    req: &crate::ResourceRequirement,
) -> Option<Vec<String>> {
    let (Some(site), ResourceType::Equipment) = (&job.target_site_id, &req.resource_type) else {
        return Some(req.candidates.clone());
    };
    let kept: Vec<String> = req
        .candidates
        .iter()
        .filter(|c| {
            request
                .resources
                .iter()
                .find(|r| &r.id == *c)
                .and_then(|r| r.site_id.as_ref())
                .is_none_or(|s| s == site)
        })
        .cloned()
        .collect();
    // An empty list would read as «any equipment».
    (kept.is_empty() == req.candidates.is_empty()).then_some(kept)
}

/// The equipment `candidates` (already kept to the job's site) that the
/// operation's crew is assigned, when the crew is assigned any: a crew works
/// its own equipment. No candidates read as «any equipment», so they become
/// the crew's. `None` when the crew is assigned none of them.
fn crew_equipment(
    crew: Option<&crate::Crew>,
    req: &crate::ResourceRequirement,
    candidates: Vec<String>,
) -> Option<Vec<String>> {
    let assigned = match crew {
        Some(c) if req.resource_type == ResourceType::Equipment && !c.equipment_ids.is_empty() => {
            &c.equipment_ids
        }
        _ => return Some(candidates),
    };
    if candidates.is_empty() {
        return Some(assigned.clone());
    }
    let kept: Vec<String> = candidates
        .into_iter()
        .filter(|e| assigned.contains(e))
        .collect();
    (!kept.is_empty()).then_some(kept)
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

/// A daily work window's hours (UTC minute of day, as u-aps reads them — an
/// end before the begin runs past midnight) from the day before `start_ms`
/// over [`CALENDAR_HORIZON_MS`]. Begin and end the same: the whole day.
fn daily_windows(window: crate::DailyWorkWindow, start_ms: i64) -> USCalendar {
    const MINUTE: i64 = 60_000;
    const DAY: i64 = 24 * 60 * MINUTE;
    let mut out = USCalendar::new("daily_work_window");
    if window.begin_minute == window.end_minute {
        return out;
    }
    let begin = i64::from(window.begin_minute) * MINUTE;
    let end = i64::from(window.end_minute) * MINUTE;
    let length = if end > begin {
        end - begin
    } else {
        DAY - begin + end
    };
    let first = start_ms.div_euclid(DAY) * DAY - DAY;
    let mut day = first;
    while day < start_ms + CALENDAR_HORIZON_MS {
        out = out.with_window(day + begin, day + begin + length);
        day += DAY;
    }
    out
}

/// The working time of both `a` and `b`, as windows — each laid out as
/// [`windows`] and [`daily_windows`] make them (windows only, ordered,
/// disjoint).
fn intersection(a: &USCalendar, b: &USCalendar) -> USCalendar {
    // No windows: always working.
    if a.time_windows.is_empty() {
        return b.clone();
    }
    if b.time_windows.is_empty() {
        return a.clone();
    }
    let mut out = USCalendar::new(format!("{}&{}", a.id, b.id));
    let (mut i, mut j) = (0, 0);
    let (x, y) = (&a.time_windows, &b.time_windows);
    while i < x.len() && j < y.len() {
        let start = x[i].start_ms.max(y[j].start_ms);
        let end = x[i].end_ms.min(y[j].end_ms);
        if start < end {
            out = out.with_window(start, end);
        }
        if x[i].end_ms < y[j].end_ms {
            i += 1;
        } else {
            j += 1;
        }
    }
    // Nothing in common: never working, not «always» (no windows).
    if out.time_windows.is_empty() {
        out = out.with_blocked(i64::MIN, i64::MAX);
    }
    out
}

/// `calendar`'s working windows from `start_ms` over [`CALENDAR_HORIZON_MS`],
/// as u-aps itself reads them (shifts, breaks, holidays).
fn windows(calendar: &Calendar, start_ms: i64) -> USCalendar {
    let mut out = USCalendar::new(&calendar.id);
    if calendar.shifts.is_empty() && calendar.holidays.is_empty() && calendar.exceptions.is_empty()
    {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Crew, CrewManager, Job, Operation, Resource, SkillMatrix, TeamRole};

    fn crew_request(crew: Crew) -> ScheduleRequest {
        let op = Operation::new("O1", "J1", 1)
            .with_time(0, 60_000, 0)
            .with_operation_type("assembly")
            .with_equipment(vec!["EQP".to_string()])
            .with_workers(1)
            .with_crew("TEAM");
        ScheduleRequest::new(
            vec![Job::new("J1").with_operation(op)],
            vec![
                Resource::equipment("EQP"),
                Resource::worker("W1"),
                Resource::worker("W2"),
                Resource::worker("W3"),
            ],
        )
        .with_crew_manager(CrewManager::new().with_crew(crew))
    }

    fn only_activity(adapted: &Adapted) -> &Activity {
        let problem = adapted.problem.as_ref().expect("problem is valid");
        &problem.tasks()[0].activities[0]
    }

    fn worker_candidates(activity: &Activity) -> Vec<Vec<String>> {
        activity
            .resource_requirements
            .iter()
            .filter(|r| r.resource_type == "Worker")
            .map(|r| r.candidates.clone())
            .collect()
    }

    #[test]
    fn an_operations_time_on_a_resource_states_it_outright() {
        // M2's 90 % efficiency would make 60 s take 66.7 s; the operation's own
        // time there (30 s process, 5 s set-up) is what it takes instead. M1
        // keeps the operation's time.
        let op = Operation::new("O1", "J1", 1)
            .with_time(0, 60_000, 0)
            .with_equipment(vec!["M1".to_string(), "M2".to_string()])
            .with_time_on(
                "M2",
                crate::TimeOverrideEntry::new()
                    .with_process(30_000)
                    .with_setup(5_000),
            );
        let request = ScheduleRequest::new(
            vec![Job::new("J1").with_operation(op)],
            vec![
                Resource::equipment("M1"),
                Resource::equipment("M2").with_efficiency(0.9),
            ],
        );
        let adapted = to_problem(&request);
        let activity = only_activity(&adapted);
        let on_m2 = activity.duration_on(["M2"]);
        assert_eq!((on_m2.setup_ms, on_m2.process_ms), (5_000, 30_000));
        assert_eq!(activity.duration_on(["M1"]).process_ms, 60_000);
    }

    #[test]
    fn an_operational_crew_replaces_the_worker_requirement() {
        let crew = Crew::new("TEAM", "Team")
            .with_efficiency(1.5)
            .with_worker("W1", TeamRole::Leader)
            .with_worker("W2", TeamRole::Member);
        let adapted = to_problem(&crew_request(crew));
        let activity = only_activity(&adapted);
        assert_eq!(
            worker_candidates(activity),
            vec![vec!["W1".to_string()], vec!["W2".to_string()]]
        );
        assert_eq!(activity.duration_on(["EQP", "W1", "W2"]).process_ms, 40_000);
        assert!(adapted.unfilled.is_empty());
    }

    #[test]
    fn the_first_members_proficiency_and_the_crews_efficiency_multiply() {
        let crew = Crew::new("TEAM", "Team")
            .with_efficiency(1.5)
            .with_worker("W1", TeamRole::Leader)
            .with_worker("W2", TeamRole::Member);
        let mut request = crew_request(crew);
        request = request.with_skill_matrix(SkillMatrix::new().with_skill("W1", "assembly", 2.0));
        let adapted = to_problem(&request);
        assert_eq!(
            only_activity(&adapted)
                .duration_on(["EQP", "W1", "W2"])
                .process_ms,
            20_000
        );
    }

    #[test]
    fn a_crew_without_a_leader_leaves_the_worker_requirement() {
        // u-aps: not operational, so the operation falls back to its workers.
        let crew = Crew::new("TEAM", "Team")
            .with_efficiency(1.5)
            .with_worker("W1", TeamRole::Member)
            .with_worker("W2", TeamRole::Member);
        let adapted = to_problem(&crew_request(crew));
        let activity = only_activity(&adapted);
        assert_eq!(worker_candidates(activity), vec![Vec::<String>::new()]);
        assert_eq!(activity.duration_on(["EQP", "W1"]).process_ms, 60_000);
    }

    fn equipment_lists(activity: &Activity) -> Vec<Vec<String>> {
        activity
            .resource_requirements
            .iter()
            .filter(|r| r.resource_type == "Equipment")
            .map(|r| r.candidates.clone())
            .collect()
    }

    fn led_crew() -> Crew {
        Crew::new("TEAM", "Team").with_worker("W1", TeamRole::Leader)
    }

    #[test]
    fn a_crew_works_only_the_equipment_it_is_assigned() {
        let mut request = crew_request(led_crew().with_equipment("EQP2"));
        request.resources.push(Resource::equipment("EQP2"));
        request.jobs[0].operations[0].required_resources[0].candidates =
            vec!["EQP".to_string(), "EQP2".to_string()];
        let adapted = to_problem(&request);
        assert_eq!(
            equipment_lists(only_activity(&adapted)),
            vec![vec!["EQP2".to_string()]]
        );
        assert!(adapted.unfilled.is_empty());

        // «Any equipment» becomes the crew's.
        request.jobs[0].operations[0].required_resources[0]
            .candidates
            .clear();
        let adapted = to_problem(&request);
        assert_eq!(
            equipment_lists(only_activity(&adapted)),
            vec![vec!["EQP2".to_string()]]
        );

        // None of the operation's equipment is the crew's: reported, and
        // scheduled without it.
        let adapted = to_problem(&crew_request(led_crew().with_equipment("EQP2")));
        assert!(equipment_lists(only_activity(&adapted)).is_empty());
        assert_eq!(adapted.unfilled.len(), 1);
        assert!(
            adapted.unfilled[0].1.contains("crew TEAM"),
            "{}",
            adapted.unfilled[0].1
        );
    }

    #[test]
    fn a_crew_not_assigned_the_operations_type_does_not_take_it() {
        let adapted = to_problem(&crew_request(led_crew().with_operation_type("painting")));
        // The operation's own worker requirement, not the crew's members.
        assert_eq!(
            worker_candidates(only_activity(&adapted)),
            vec![Vec::<String>::new()]
        );
        assert_eq!(adapted.unfilled.len(), 1);
        assert!(
            adapted.unfilled[0].1.contains("assembly"),
            "{}",
            adapted.unfilled[0].1
        );

        let adapted = to_problem(&crew_request(led_crew().with_operation_type("assembly")));
        assert_eq!(
            worker_candidates(only_activity(&adapted)),
            vec![vec!["W1".to_string()]]
        );
        assert!(adapted.unfilled.is_empty());
    }

    fn sited_request(target: &str) -> ScheduleRequest {
        let op = Operation::new("O1", "J1", 1)
            .with_time(0, 60_000, 0)
            .with_equipment(vec!["EQP-A".to_string(), "EQP-B".to_string()]);
        ScheduleRequest::new(
            vec![Job::new("J1").with_site(target).with_operation(op)],
            vec![
                Resource::equipment("EQP-A").with_site("SITE-A"),
                Resource::equipment("EQP-B").with_site("SITE-B"),
            ],
        )
    }

    fn equipment_candidates(adapted: &Adapted) -> Vec<String> {
        only_activity(adapted).resource_requirements[0]
            .candidates
            .clone()
    }

    #[test]
    fn a_target_site_keeps_the_equipment_on_it() {
        let adapted = to_problem(&sited_request("SITE-B"));
        assert_eq!(equipment_candidates(&adapted), ["EQP-B"]);
        assert!(adapted.unfilled.is_empty());
    }

    #[test]
    fn a_target_site_with_none_of_the_equipment_is_named_and_left_out() {
        let adapted = to_problem(&sited_request("SITE-C"));
        assert_eq!(adapted.unfilled.len(), 1);
        assert_eq!(adapted.unfilled[0].0, "O1");
        // Nothing left to hold: it runs in time only.
        assert!(only_activity(&adapted).resource_requirements.is_empty());
    }

    #[test]
    fn a_calendar_on_a_time_zone_lays_out_the_zones_shifts() {
        const HOUR: i64 = 3_600_000;
        // 2027-02-08 00:00 UTC, a Monday; Monday 00:00 in Seoul is nine hours earlier.
        let monday_utc = 1_802_044_800_000;
        let seoul = Calendar::default_day().with_utc_offsets(crate::UtcOffsets::fixed(9 * 60));
        let laid = windows(&seoul, monday_utc - 9 * HOUR);
        let first = laid.time_windows.first().cloned().expect("a window");
        // Monday 08:00–12:00 Seoul = Sunday 23:00 – Monday 03:00 UTC
        assert_eq!(
            (first.start_ms, first.end_ms),
            (monday_utc - HOUR, monday_utc + 3 * HOUR)
        );
    }
}
