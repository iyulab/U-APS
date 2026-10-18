//! What an operation's `dependencies` mean to the default scheduler: it starts
//! once every operation it names has ended, in any job.

use uaps_engine::*;

const MINUTE: i64 = 60_000;

fn op(id: &str, job: &str, sequence: i32, minutes: i64, equipment: &str) -> Operation {
    Operation::new(id, job, sequence)
        .with_time(0, minutes * MINUTE, 0)
        .with_equipment(vec![equipment.to_string()])
}

fn equipment() -> Vec<Resource> {
    vec![Resource::equipment("E1"), Resource::equipment("E2")]
}

fn start_of(schedule: &Schedule, op: &str) -> i64 {
    schedule
        .assignments
        .iter()
        .find(|a| a.operation_id == op)
        .unwrap_or_else(|| panic!("{op} is assigned"))
        .start_ms
}

#[test]
fn an_operation_waits_for_an_operation_of_another_job() {
    // J2-1 runs on E2, which is free from 0, but waits for J1-2.
    let j1 = Job::new("J1")
        .with_operation(op("J1-1", "J1", 1, 30, "E1"))
        .with_operation(op("J1-2", "J1", 2, 30, "E1"));
    let j2 =
        Job::new("J2").with_operation(op("J2-1", "J2", 1, 10, "E2").with_dependencies(["J1-2"]));
    let schedule = SimpleScheduler::new()
        .schedule(&ScheduleRequest::new(vec![j2, j1], equipment()))
        .expect("schedules");
    assert_eq!(start_of(&schedule, "J1-2"), 30 * MINUTE);
    assert_eq!(start_of(&schedule, "J2-1"), 60 * MINUTE);
    assert!(schedule.violations.is_empty(), "{:?}", schedule.violations);
}

#[test]
fn a_dependency_on_no_operation_or_in_a_loop_is_refused() {
    let unknown =
        Job::new("J1").with_operation(op("J1-1", "J1", 1, 10, "E1").with_dependencies(["J9-1"]));
    let refused =
        SimpleScheduler::new().schedule(&ScheduleRequest::new(vec![unknown], equipment()));
    assert!(
        matches!(refused, Err(ScheduleError::Refused(_))),
        "{refused:?}"
    );

    // Each job's first operation waits for the other's second.
    let j1 = Job::new("J1")
        .with_operation(op("J1-1", "J1", 1, 10, "E1").with_dependencies(["J2-2"]))
        .with_operation(op("J1-2", "J1", 2, 10, "E1"));
    let j2 = Job::new("J2")
        .with_operation(op("J2-1", "J2", 1, 10, "E2").with_dependencies(["J1-2"]))
        .with_operation(op("J2-2", "J2", 2, 10, "E2"));
    let refused = SimpleScheduler::new().schedule(&ScheduleRequest::new(vec![j1, j2], equipment()));
    assert!(
        matches!(refused, Err(ScheduleError::Refused(_))),
        "{refused:?}"
    );
}

#[test]
fn every_algorithm_refuses_a_dependency_loop_and_reports_a_broken_dependency() {
    let looped = vec![
        Job::new("J1")
            .with_operation(op("J1-1", "J1", 1, 10, "E1").with_dependencies(["J2-2"]))
            .with_operation(op("J1-2", "J1", 2, 10, "E1")),
        Job::new("J2")
            .with_operation(op("J2-1", "J2", 1, 10, "E2").with_dependencies(["J1-2"]))
            .with_operation(op("J2-2", "J2", 2, 10, "E2")),
    ];
    let waiting = vec![
        Job::new("J1")
            .with_operation(op("J1-1", "J1", 1, 30, "E1"))
            .with_operation(op("J1-2", "J1", 2, 30, "E1")),
        Job::new("J2").with_operation(op("J2-1", "J2", 1, 10, "E2").with_dependencies(["J1-2"])),
    ];
    for algorithm in [
        SchedulingAlgorithm::Simple,
        SchedulingAlgorithm::Production,
        SchedulingAlgorithm::Dynamic,
    ] {
        let mut engine = SchedulingEngine::new(EngineConfig {
            algorithm,
            ..EngineConfig::default()
        });
        let refused = engine.schedule(&looped, &equipment(), 0);
        let error = refused.expect_err("a loop is refused");
        assert!(
            error.to_string().contains("wait on each other"),
            "{algorithm:?}: {error}"
        );

        // Kept, or said to be broken.
        let result = engine
            .schedule(&waiting, &equipment(), 0)
            .expect("schedules");
        let kept = start_of(&result.schedule, "J2-1") >= 60 * MINUTE;
        let reported = result.schedule.violations.iter().any(|v| {
            v.target_id == "J2-1" && matches!(v.constraint_type, ConstraintType::Precedence)
        });
        assert!(
            kept || reported,
            "{algorithm:?}: {:?}",
            result.schedule.assignments
        );
    }
}
