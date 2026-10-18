//! The tasks the default scheduler reports for an operation: one for each
//! stretch the operation actually works in, inside its assignment.

use uaps_engine::*;

const HOUR: i64 = 3_600_000;
/// 2024-01-22 (Monday) 00:00 UTC.
const MONDAY: i64 = 1_705_881_600_000;

#[test]
fn tasks_are_the_stretches_the_assignment_works_in() {
    // Day shift with lunch on the equipment; the operation's own hours stop
    // at 16:00, so its work pauses at lunch and at 16:00, not at 17:00.
    let job = Job::new("J1").with_operation(
        Operation::new("O1", "J1", 1)
            .with_time(0, 6 * HOUR, 0)
            .with_equipment(vec!["EQP".to_string()])
            .with_daily_work_window(DailyWorkWindow::from_hm(9, 0, 16, 0)),
    );
    let request = ScheduleRequest::new(
        vec![job],
        vec![Resource::equipment("EQP").with_calendar("CAL-DAY")],
    )
    .with_start_time(MONDAY)
    .with_calendar(Calendar::default_day());
    let schedule = SimpleScheduler::new()
        .schedule(&request)
        .expect("schedules");
    let a = &schedule.assignments[0];
    let spans: Vec<(i64, i64)> = schedule
        .tasks
        .iter()
        .map(|t| (t.plan_begin_ms - MONDAY, t.plan_end_ms - MONDAY))
        .collect();
    // 09–12, 13–16: six hours of work.
    assert_eq!(spans, [(9 * HOUR, 12 * HOUR), (13 * HOUR, 16 * HOUR)]);
    assert_eq!(
        (a.start_ms - MONDAY, a.end_ms - MONDAY),
        (9 * HOUR, 16 * HOUR)
    );
    let worked: i64 = schedule
        .tasks
        .iter()
        .map(|t| t.setup_ms + t.process_ms)
        .sum();
    assert_eq!(worked, 6 * HOUR);
    assert!(schedule.tasks.iter().all(|t| t.total_splits == 2));
    assert_eq!(schedule.tasks[0].split_reason, SplitReason::BreakStart);
}
