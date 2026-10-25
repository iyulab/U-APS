//! An operation's time window as the default scheduler reads it: the earliest
//! start holds the operation (the first one holds its job), and a window that
//! also carries a latest start or an end bound says those are not applied.

use uaps_engine::*;

const MINUTE: i64 = 60_000;

fn op(id: &str, sequence: i32, minutes: i64) -> Operation {
    Operation::new(id, "J1", sequence)
        .with_time(0, minutes * MINUTE, 0)
        .with_equipment(vec!["E1".to_string()])
}

fn start_of(schedule: &Schedule, op: &str) -> i64 {
    schedule
        .assignments
        .iter()
        .find(|a| a.operation_id == op)
        .unwrap_or_else(|| panic!("{op} is assigned"))
        .start_ms
}

fn unfilled(schedule: &Schedule) -> Vec<&str> {
    schedule
        .violations
        .iter()
        .filter(|v| matches!(v.constraint_type, ConstraintType::UnfilledRequirement))
        .map(|v| v.target_id.as_str())
        .collect()
}

#[test]
fn an_earliest_start_holds_the_first_and_a_later_operation() {
    let job = Job::new("J1")
        .with_operation(op("O1", 1, 10).with_earliest_start(30 * MINUTE))
        .with_operation(op("O2", 2, 10).with_earliest_start(90 * MINUTE));
    let schedule = SimpleScheduler::new()
        .schedule(&ScheduleRequest::new(
            vec![job],
            vec![Resource::equipment("E1")],
        ))
        .expect("schedules");
    assert_eq!(start_of(&schedule, "O1"), 30 * MINUTE);
    assert_eq!(start_of(&schedule, "O2"), 90 * MINUTE);
    assert!(unfilled(&schedule).is_empty(), "{:?}", schedule.violations);
}

#[test]
fn a_latest_bound_is_reported_as_not_applied() {
    let job = Job::new("J1").with_operation(
        op("O1", 1, 10).with_time_window(OperationTimeWindow::bounded(0, 5 * MINUTE)),
    );
    let schedule = SimpleScheduler::new()
        .schedule(&ScheduleRequest::new(
            vec![job],
            vec![Resource::equipment("E1")],
        ))
        .expect("schedules");
    assert_eq!(unfilled(&schedule), ["O1"]);
}
