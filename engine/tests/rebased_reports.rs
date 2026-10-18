//! What the u-schedule path reports about operations it cannot fill or place
//! ([`uaps_engine::rebased::schedule`]).

use uaps_engine::rebased::schedule as rebased;
use uaps_engine::*;

/// 2024-01-22 (Monday) 00:00 UTC.
const MONDAY: i64 = 1_705_881_600_000;

fn unfilled(schedule: &Schedule) -> Vec<&str> {
    schedule
        .violations
        .iter()
        .filter(|v| matches!(v.constraint_type, ConstraintType::UnfilledRequirement))
        .map(|v| v.target_id.as_str())
        .collect()
}

#[test]
fn a_target_site_without_the_equipment_is_scheduled_without_it_and_reported() {
    let job = Job::new("J1").with_site("S2").with_operation(
        Operation::new("O1", "J1", 1)
            .with_time(0, 60_000, 0)
            .with_equipment(vec!["E1".to_string()]),
    );
    let request = ScheduleRequest::new(vec![job], vec![Resource::equipment("E1").with_site("S1")])
        .with_start_time(MONDAY);
    let schedule = rebased(&request).expect("on the new path");
    // As the previous scheduler did: without the equipment, in time only.
    assert_eq!(schedule.assignments.len(), 1);
    assert_eq!(schedule.assignments[0].resource_id, "NONE");
    assert_eq!(unfilled(&schedule), ["O1"]);
    let message = &schedule.violations[0].message;
    assert!(
        message.contains("E1") && message.contains("S2"),
        "{message}"
    );
}

#[test]
fn an_operation_that_can_never_run_is_left_out_and_reported() {
    // Night hours on day-shift equipment: never working together.
    let night = Operation::new("O1", "J1", 1)
        .with_time(0, 60 * 60_000, 0)
        .with_equipment(vec!["E1".to_string()])
        .with_daily_work_window(DailyWorkWindow::from_hm(22, 0, 6, 0));
    let next = Operation::new("O2", "J1", 2)
        .with_time(0, 30 * 60_000, 0)
        .with_equipment(vec!["E2".to_string()]);
    let job = Job::new("J1").with_operation(night).with_operation(next);
    let weekdays = vec![
        DayOfWeek::Monday,
        DayOfWeek::Tuesday,
        DayOfWeek::Wednesday,
        DayOfWeek::Thursday,
        DayOfWeek::Friday,
    ];
    let request = ScheduleRequest::new(
        vec![job],
        vec![
            Resource::equipment("E1").with_calendar("DAY"),
            Resource::equipment("E2"),
        ],
    )
    .with_start_time(MONDAY)
    .with_calendar(Calendar::new("DAY", "Day shift").with_shift(Shift::new(
        "day",
        Time::new(8, 0),
        Time::new(17, 0),
        weekdays,
    )));
    let schedule = rebased(&request).expect("answered on the new path, not handed back");
    assert!(schedule.assignment_for_operation("O1").is_none());
    assert!(schedule.assignment_for_operation("O2").is_some());
    assert_eq!(unfilled(&schedule), ["O1"]);
}
