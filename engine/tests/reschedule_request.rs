//! Rescheduling with the request: the strategy keeps assignments as pins and
//! the dispatching scheduler places the rest, so the result keeps the
//! request's unavailable slots — which the request-less strategies, shifting
//! assignments on their own, do not know.

use uaps_engine::*;

const HOUR: i64 = 3_600_000;

fn op(id: &str, job: &str, machine: &str, hours: i64) -> Operation {
    Operation::new(id, job, 1)
        .with_time(0, hours * HOUR, 0)
        .with_equipment(vec![machine.to_string()])
}

/// Two lots on M1 (2 h each) and one on M2 (3 h).
fn request(m1: Resource) -> ScheduleRequest {
    ScheduleRequest::new(
        vec![
            Job::new("A").with_operation(op("A-1", "A", "M1", 2)),
            Job::new("B").with_operation(op("B-1", "B", "M1", 2)),
            Job::new("C").with_operation(op("C-1", "C", "M2", 3)),
        ],
        vec![m1, Resource::equipment("M2")],
    )
    .with_start_time(0)
}

fn on(schedule: &Schedule, op: &str) -> Vec<(String, i64, i64)> {
    let mut rows: Vec<(String, i64, i64)> = schedule
        .assignments
        .iter()
        .filter(|a| a.operation_id == op)
        .map(|a| (a.resource_id.clone(), a.start_ms, a.end_ms))
        .collect();
    rows.sort();
    rows
}

#[test]
fn a_breakdown_keeps_the_unaffected_and_places_the_rest_outside_the_slot() {
    let before = SimpleScheduler::new()
        .schedule(&request(Resource::equipment("M1")))
        .expect("schedules");
    // M1 breaks down 1 h in, for 5 h: the caller's request now says so.
    let after = request(Resource::equipment("M1").with_unavailable(TimeSlot::new(HOUR, 6 * HOUR)));
    let event = ScheduleEvent::MachineBreakdown {
        resource_id: "M1".into(),
        start_ms: HOUR,
        duration_ms: 5 * HOUR,
    };
    for strategy in [
        Some(RescheduleStrategy::RightShift),
        Some(RescheduleStrategy::AOR),
        Some(RescheduleStrategy::Partial),
        None,
    ] {
        let result = Rescheduler::new()
            .reschedule_request(&before, &event, strategy, &after)
            .expect("reschedules");
        // C on M2 is not affected: kept exactly.
        assert_eq!(
            on(&result.schedule, "C-1"),
            on(&before, "C-1"),
            "{strategy:?}"
        );
        assert!(!result.affected_operations.contains(&"C-1".to_string()));
        // Nothing on M1 works inside the breakdown.
        for a in result
            .schedule
            .assignments
            .iter()
            .filter(|a| a.resource_id == "M1")
        {
            for t in result
                .schedule
                .tasks
                .iter()
                .filter(|t| t.operation_id == a.operation_id)
            {
                assert!(
                    t.plan_end_ms <= HOUR || t.plan_begin_ms >= 6 * HOUR,
                    "{strategy:?}: {} works {}..{} in the breakdown",
                    a.operation_id,
                    t.plan_begin_ms,
                    t.plan_end_ms
                );
            }
        }
        assert!(
            result.schedule.violations.is_empty(),
            "{:?}",
            result.schedule.violations
        );
    }
}

#[test]
fn a_full_reschedule_keeps_nothing_and_matches_scheduling_the_request() {
    let before = SimpleScheduler::new()
        .schedule(&request(Resource::equipment("M1")))
        .expect("schedules");
    let after = request(Resource::equipment("M1").with_unavailable(TimeSlot::new(HOUR, 6 * HOUR)));
    let event = ScheduleEvent::MachineBreakdown {
        resource_id: "M1".into(),
        start_ms: HOUR,
        duration_ms: 5 * HOUR,
    };
    let result = Rescheduler::new()
        .reschedule_request(&before, &event, Some(RescheduleStrategy::Full), &after)
        .expect("reschedules");
    let fresh = SimpleScheduler::new().schedule(&after).expect("schedules");
    for id in ["A-1", "B-1", "C-1"] {
        assert_eq!(on(&result.schedule, id), on(&fresh, id), "{id}");
    }
    assert_eq!(result.strategy_used, RescheduleStrategy::Full);
}

#[test]
fn a_kept_assignment_the_request_now_forbids_is_placed_again() {
    // C would be kept, but the caller's request makes M2 unavailable where C
    // sits: C is released and placed after the slot.
    let before = SimpleScheduler::new()
        .schedule(&request(Resource::equipment("M1")))
        .expect("schedules");
    let mut after = request(Resource::equipment("M1"));
    after.resources[1] = Resource::equipment("M2").with_unavailable(TimeSlot::new(0, HOUR));
    let event = ScheduleEvent::OperationDelay {
        operation_id: "A-1".into(),
        delay_ms: HOUR,
    };
    let result = Rescheduler::new()
        .reschedule_request(&before, &event, Some(RescheduleStrategy::AOR), &after)
        .expect("reschedules");
    assert!(result.affected_operations.contains(&"C-1".to_string()));
    for t in result
        .schedule
        .tasks
        .iter()
        .filter(|t| t.operation_id == "C-1")
    {
        assert!(
            t.plan_begin_ms >= HOUR,
            "C-1 works {}..{}",
            t.plan_begin_ms,
            t.plan_end_ms
        );
    }
    assert!(
        result.schedule.violations.is_empty(),
        "{:?}",
        result.schedule.violations
    );
}

/// One order with two steps on one machine (step 2 depends on step 1), 30 minutes each.
fn two_step_request(m1: Resource, written_down: bool) -> ScheduleRequest {
    const MIN: i64 = 60_000;
    let step = |id: &str, seq: i32| {
        Operation::new(id, "P", seq)
            .with_time(0, 30 * MIN, 0)
            .with_equipment(vec!["M1".to_string()])
    };
    // The order inside a job holds without being written down; a dependency list says it too.
    let second = if written_down {
        step("P-2", 2).with_dependencies(vec!["P-1".to_string()])
    } else {
        step("P-2", 2)
    };
    ScheduleRequest::new(
        vec![Job::new("P")
            .with_operation(step("P-1", 1))
            .with_operation(second)],
        vec![m1],
    )
    .with_start_time(0)
}

#[test]
fn a_breakdown_inside_an_early_step_also_moves_the_later_steps_of_the_order() {
    for written_down in [true, false] {
        later_steps_move_with_the_affected_one(written_down);
    }
}

fn later_steps_move_with_the_affected_one(written_down: bool) {
    // Step 1 sits 0:00-0:30, step 2 sits 0:30-1:00. A breakdown 0:10-0:20 touches only step 1:
    // step 2 does not overlap it. Keeping step 2 where it was while step 1 is pushed past the
    // breakdown would put step 2 before its predecessor ends — the plan would break its own
    // precedence. The steps that follow an affected one move with it.
    const MIN: i64 = 60_000;
    let before = SimpleScheduler::new()
        .schedule(&two_step_request(Resource::equipment("M1"), written_down))
        .expect("schedules");
    let after = two_step_request(
        Resource::equipment("M1").with_unavailable(TimeSlot::new(10 * MIN, 20 * MIN)),
        written_down,
    );
    let event = ScheduleEvent::MachineBreakdown {
        resource_id: "M1".into(),
        start_ms: 10 * MIN,
        duration_ms: 10 * MIN,
    };
    for strategy in [
        Some(RescheduleStrategy::RightShift),
        Some(RescheduleStrategy::AOR),
        Some(RescheduleStrategy::Partial),
    ] {
        let result = Rescheduler::new()
            .reschedule_request(&before, &event, strategy, &after)
            .unwrap_or_else(|e| {
                panic!("{strategy:?} (dependencies written down: {written_down}): {e}")
            });
        let end_of = |op: &str| {
            result
                .schedule
                .assignments
                .iter()
                .filter(|a| a.operation_id == op)
                .map(|a| a.end_ms)
                .max()
                .unwrap()
        };
        let start_of = |op: &str| {
            result
                .schedule
                .assignments
                .iter()
                .filter(|a| a.operation_id == op)
                .map(|a| a.start_ms)
                .min()
                .unwrap()
        };
        assert!(
            start_of("P-2") >= end_of("P-1"),
            "{strategy:?}: step 2 starts {} before step 1 ends {}",
            start_of("P-2"),
            end_of("P-1")
        );
        assert!(
            result.affected_operations.contains(&"P-2".to_string()),
            "{strategy:?}: the later step is placed again with the affected one"
        );
        assert!(
            result.schedule.violations.is_empty(),
            "{strategy:?}: {:?}",
            result.schedule.violations
        );
    }
}
