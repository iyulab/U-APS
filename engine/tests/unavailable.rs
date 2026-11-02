//! A resource's unavailable slots and an operation that cannot pause: the
//! scheduler keeps every operation's work out of the slots, and runs an
//! uninterruptible one whole before or after them.

use uaps_engine::*;

const HOUR: i64 = 3_600_000;

/// An oven down for maintenance 08:00–16:00; lot A (6 h, from 06:00) and
/// lot B (8 h, from 12:00).
fn oven_request(a: Operation, b: Operation) -> ScheduleRequest {
    let lot = |id: &str, op: Operation| Job::new(id).with_operation(op);
    ScheduleRequest::new(
        vec![lot("A", a), lot("B", b)],
        vec![Resource::equipment("OVEN").with_unavailable(TimeSlot::new(8 * HOUR, 16 * HOUR))],
    )
    .with_start_time(0)
}

fn lot_op(job: &str, from: i64, hours: i64) -> Operation {
    Operation::new(format!("{job}-1"), job, 1)
        .with_time(0, hours * HOUR, 0)
        .with_equipment(vec!["OVEN".to_string()])
        .with_earliest_start(from * HOUR)
}

fn worked_spans(schedule: &Schedule, op: &str) -> Vec<(i64, i64)> {
    let mut spans: Vec<(i64, i64)> = schedule
        .tasks
        .iter()
        .filter(|t| t.operation_id == op)
        .map(|t| (t.plan_begin_ms, t.plan_end_ms))
        .collect();
    spans.sort_unstable();
    spans
}

#[test]
fn no_work_falls_in_a_resources_unavailable_slot() {
    let request = oven_request(lot_op("A", 6, 6), lot_op("B", 12, 8));
    let schedule = SimpleScheduler::new()
        .schedule(&request)
        .expect("schedules");
    for op in ["A-1", "B-1"] {
        let spans = worked_spans(&schedule, op);
        assert!(!spans.is_empty(), "{op} is scheduled");
        for (begin, end) in spans {
            assert!(
                end <= 8 * HOUR || begin >= 16 * HOUR,
                "{op} works {begin}..{end}, inside the maintenance"
            );
        }
    }
    // A pauses at the maintenance: 06–08 and then 16–20.
    assert_eq!(
        worked_spans(&schedule, "A-1"),
        [(6 * HOUR, 8 * HOUR), (16 * HOUR, 20 * HOUR)]
    );
}

#[test]
fn an_uninterruptible_operation_runs_whole_after_the_slot() {
    let request = oven_request(lot_op("A", 6, 6).uninterruptible(), lot_op("B", 12, 8));
    let schedule = SimpleScheduler::new()
        .schedule(&request)
        .expect("schedules");
    let a = worked_spans(&schedule, "A-1");
    // 06–08 is two hours, too short for six: A runs whole from 16:00.
    assert_eq!(a.len(), 1, "one stretch, got {a:?}");
    let (begin, end) = a[0];
    assert!(
        begin >= 16 * HOUR,
        "A starts after the maintenance, at {begin}"
    );
    assert_eq!(end - begin, 6 * HOUR);
}

#[test]
fn interruptible_is_the_default_and_round_trips() {
    let op = Operation::new("O", "J", 1);
    assert!(op.interruptible);
    let json = serde_json::to_value(&op).expect("serializes");
    let back: Operation = serde_json::from_value(json.clone()).expect("reads back");
    assert!(back.interruptible);
    // An older request without the field reads as interruptible.
    let mut older = json;
    older
        .as_object_mut()
        .expect("an object")
        .remove("interruptible");
    let read: Operation = serde_json::from_value(older).expect("reads");
    assert!(read.interruptible);
}
