//! What lot splitting and overlap mean to every algorithm that places by the
//! default scheduler: an overlap stated on an operation lets the next one start
//! once it is that far along, never ending before it; a splittable operation
//! (or every operation of a splittable job) may run in parts on its
//! candidates, each assignment saying which part and how much of the work.

use uaps_engine::*;

const HOUR: i64 = 3_600_000;

fn op(id: &str, job: &str, sequence: i32, hours: i64, equipment: &[&str]) -> Operation {
    Operation::new(id, job, sequence)
        .with_time(0, hours * HOUR, 0)
        .with_equipment(equipment.iter().map(|e| e.to_string()).collect())
}

fn equipment() -> Vec<Resource> {
    vec![Resource::equipment("E1"), Resource::equipment("E2")]
}

fn span_of(schedule: &Schedule, op: &str) -> (i64, i64) {
    let mine: Vec<&Assignment> = schedule
        .assignments
        .iter()
        .filter(|a| a.operation_id == op)
        .collect();
    assert!(!mine.is_empty(), "{op} is assigned");
    (
        mine.iter().map(|a| a.start_ms).min().expect("assigned"),
        mine.iter().map(|a| a.end_ms).max().expect("assigned"),
    )
}

fn overlapping(next_hours: i64) -> Vec<Job> {
    vec![Job::new("J1")
        .with_operation(
            op("J1-1", "J1", 1, 10, &["E1"]).with_overlapping(OverlapConfig {
                min_predecessor_completion: 0.5,
                transfer_batch_size: 1,
            }),
        )
        .with_operation(op("J1-2", "J1", 2, next_hours, &["E2"]))]
}

fn splitting(job: Job) -> Vec<Job> {
    vec![job]
}

fn split_config() -> SplitConfig {
    SplitConfig {
        min_split_size_ms: HOUR,
        max_splits: 2,
        split_setup_overhead_ms: 0,
    }
}

const SEARCHES: [SchedulingAlgorithm; 4] = [
    SchedulingAlgorithm::Simple,
    SchedulingAlgorithm::GeneticAlgorithm,
    SchedulingAlgorithm::Production,
    SchedulingAlgorithm::Dynamic,
];

fn run(algorithm: SchedulingAlgorithm, jobs: &[Job]) -> Schedule {
    let mut engine = SchedulingEngine::new(EngineConfig {
        algorithm,
        ga_params: Some(GaParams {
            max_generations: 5,
            ..GaParams::default()
        }),
        ..EngineConfig::default()
    });
    let result = engine.schedule(jobs, &equipment(), 0).expect("schedules");
    assert!(
        !result
            .logs
            .iter()
            .any(|l| l.level == "WARN" && l.message.contains("genetic algorithm")),
        "{algorithm:?}: {:?}",
        result.logs
    );
    result.schedule
}

#[test]
fn the_next_operation_starts_once_the_overlapped_one_is_half_done() {
    for algorithm in SEARCHES {
        let schedule = run(algorithm, &overlapping(10));
        assert_eq!(span_of(&schedule, "J1-1"), (0, 10 * HOUR), "{algorithm:?}");
        assert_eq!(
            span_of(&schedule, "J1-2"),
            (5 * HOUR, 15 * HOUR),
            "{algorithm:?}"
        );
        assert!(
            schedule.violations.is_empty(),
            "{algorithm:?}: {:?}",
            schedule.violations
        );
    }
}

#[test]
fn an_overlapping_operation_never_ends_before_the_one_it_overlaps() {
    // Started at 5 h, one hour of work would end at 6 h — before J1-1.
    for algorithm in SEARCHES {
        let schedule = run(algorithm, &overlapping(1));
        let (_, end) = span_of(&schedule, "J1-2");
        assert!(end >= 10 * HOUR, "{algorithm:?}: ends at {end}");
        assert!(
            schedule.violations.is_empty(),
            "{algorithm:?}: {:?}",
            schedule.violations
        );
    }
}

fn parts(schedule: &Schedule, op: &str) -> Vec<(String, AssignmentPart, i64, i64)> {
    let mut out: Vec<(String, AssignmentPart, i64, i64)> = schedule
        .assignments
        .iter()
        .filter(|a| a.operation_id == op)
        .map(|a| {
            (
                a.resource_id.clone(),
                a.part.unwrap_or_else(|| panic!("{op} is split: {a:?}")),
                a.start_ms,
                a.end_ms,
            )
        })
        .collect();
    out.sort_by_key(|(_, p, _, _)| p.index);
    out
}

fn assert_split_in_two(schedule: &Schedule, op: &str, algorithm: SchedulingAlgorithm) {
    let parts = parts(schedule, op);
    assert_eq!(parts.len(), 2, "{algorithm:?}: {parts:?}");
    let machines: Vec<&str> = parts.iter().map(|(r, ..)| r.as_str()).collect();
    assert!(
        machines.contains(&"E1") && machines.contains(&"E2"),
        "{algorithm:?}: {parts:?}"
    );
    assert_eq!(
        parts.iter().map(|(_, p, ..)| p.work_ms).sum::<i64>(),
        8 * HOUR,
        "{algorithm:?}"
    );
    assert_eq!(span_of(schedule, op), (0, 4 * HOUR), "{algorithm:?}");
    // The work as tasks, one per part, each on its own machine.
    let tasks: Vec<&Task> = schedule
        .tasks
        .iter()
        .filter(|t| t.operation_id == op)
        .collect();
    assert_eq!(tasks.len(), 2, "{algorithm:?}: {tasks:?}");
    assert!(tasks.iter().all(|t| t.is_splittable && t.total_splits == 2));
    let mut on: Vec<&str> = tasks
        .iter()
        .filter_map(|t| t.equipment_id.as_deref())
        .collect();
    on.sort_unstable();
    assert_eq!(on, ["E1", "E2"], "{algorithm:?}");
    assert!(
        schedule.violations.is_empty(),
        "{algorithm:?}: {:?}",
        schedule.violations
    );
}

#[test]
fn a_splittable_operation_runs_in_parts_on_its_candidates() {
    let jobs = splitting(
        Job::new("J1")
            .with_operation(op("J1-1", "J1", 1, 8, &["E1", "E2"]).with_splitting(split_config())),
    );
    for algorithm in SEARCHES {
        assert_split_in_two(&run(algorithm, &jobs), "J1-1", algorithm);
    }
}

#[test]
fn a_splittable_job_splits_its_operations() {
    let mut job = Job::new("J1").with_operation(op("J1-1", "J1", 1, 8, &["E1", "E2"]));
    job.operations[0].split_config = Some(split_config());
    let jobs = splitting(job.splittable());
    for algorithm in SEARCHES {
        assert_split_in_two(&run(algorithm, &jobs), "J1-1", algorithm);
    }
}

#[test]
fn split_and_overlap_settings_out_of_range_are_refused_and_a_zero_minimum_is_not() {
    let schedule = |jobs: Vec<Job>| {
        SchedulingEngine::new(EngineConfig::default()).schedule(&jobs, &equipment(), 0)
    };
    let with_split = |config: SplitConfig| {
        vec![Job::new("J1")
            .with_operation(op("J1-1", "J1", 1, 8, &["E1", "E2"]).with_splitting(config))]
    };
    for bad in [
        SplitConfig {
            max_splits: 0,
            ..split_config()
        },
        SplitConfig {
            min_split_size_ms: -1,
            ..split_config()
        },
        SplitConfig {
            split_setup_overhead_ms: -1,
            ..split_config()
        },
    ] {
        let refused = schedule(with_split(bad.clone()));
        assert!(refused.is_err(), "{bad:?}: {refused:?}");
    }
    // Parts of any size.
    let any_size = schedule(with_split(SplitConfig {
        min_split_size_ms: 0,
        ..split_config()
    }))
    .expect("a zero minimum part is accepted");
    assert!(any_size.schedule.violations.is_empty());

    let mut overlapped = overlapping(10);
    overlapped[0].operations[0]
        .overlap_config
        .as_mut()
        .expect("overlap")
        .min_predecessor_completion = 1.5;
    assert!(schedule(overlapped).is_err());
}

#[test]
fn an_operation_that_is_not_splittable_runs_whole() {
    let jobs = splitting(Job::new("J1").with_operation(op("J1-1", "J1", 1, 8, &["E1", "E2"])));
    let schedule = run(SchedulingAlgorithm::Simple, &jobs);
    assert_eq!(schedule.assignments.len(), 1);
    assert_eq!(schedule.assignments[0].part, None);
    assert_eq!(span_of(&schedule, "J1-1"), (0, 8 * HOUR));
}

#[test]
fn a_part_after_the_first_carries_the_split_overhead() {
    let config = SplitConfig {
        split_setup_overhead_ms: HOUR,
        ..split_config()
    };
    let jobs = splitting(
        Job::new("J1").with_operation(op("J1-1", "J1", 1, 8, &["E1", "E2"]).with_splitting(config)),
    );
    let schedule = run(SchedulingAlgorithm::Simple, &jobs);
    let parts = parts(&schedule, "J1-1");
    assert_eq!(parts.len(), 2, "{parts:?}");
    // The second part's hour of handling: the work is shared so both end
    // together, at 4.5 h.
    assert_eq!(span_of(&schedule, "J1-1").1, 9 * HOUR / 2, "{parts:?}");
    assert!(schedule.violations.is_empty(), "{:?}", schedule.violations);
}
