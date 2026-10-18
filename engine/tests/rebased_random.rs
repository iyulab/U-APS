//! The engine's dispatching schedule against the same request scheduled on
//! u-schedule ([`uaps_engine::rebased::schedule`]), on random requests that
//! mix what the golden scenarios test one at a time: alternative equipment
//! at different efficiencies, workers whole or at a load factor, set-up
//! matrices, priorities, due dates, release times, dispatching rules and
//! shift calendars.

use rand::rngs::SmallRng;
use rand::{RngExt, SeedableRng};
use serde_json::Value;
use uaps_engine::rebased::{schedule as rebased, NotRebased};
use uaps_engine::*;

const HOUR: i64 = 3_600_000;
/// 2024-01-22 (Monday) 00:00 UTC.
const MONDAY: i64 = 1_705_881_600_000;

/// Which features a random request may use.
#[derive(Debug, Clone, Copy, Default)]
struct Features {
    alternatives: bool,
    efficiency: bool,
    workers: bool,
    setups: bool,
    priority: bool,
    due: bool,
    release: bool,
    rule: bool,
    calendar: bool,
    switching: bool,
    materials: bool,
    time_only: bool,
    daily: bool,
}

impl Features {
    const ALL: Features = Features {
        alternatives: true,
        efficiency: true,
        workers: true,
        setups: true,
        priority: true,
        due: true,
        release: true,
        rule: true,
        calendar: true,
        switching: true,
        materials: true,
        time_only: true,
        daily: true,
    };
}

fn random_request(rng: &mut SmallRng, f: Features) -> ScheduleRequest {
    let n_equipment = rng.random_range(1..=4);
    let n_workers = if f.workers {
        rng.random_range(0..=3)
    } else {
        0
    };
    let products = ["P1", "P2", "P3"];
    let calendared = f.calendar && rng.random_bool(0.6);

    let mut resources: Vec<Resource> = (0..n_equipment)
        .map(|e| {
            let mut r = Resource::equipment(format!("E{e}"));
            if f.efficiency && rng.random_bool(0.4) {
                r = r.with_efficiency([0.5, 0.8, 1.25, 2.0][rng.random_range(0..4)]);
            }
            if calendared && rng.random_bool(0.6) {
                r = r.with_calendar("DAY");
            }
            r
        })
        .collect();
    resources.extend((0..n_workers).map(|w| Resource::worker(format!("W{w}"))));

    let jobs: Vec<Job> = (0..rng.random_range(1..=6))
        .map(|j| {
            let id = format!("J{j}");
            let mut job = Job::new(&id);
            if f.priority {
                job = job.with_priority(rng.random_range(0..4));
            }
            if rng.random_bool(0.5) {
                job = job.with_product(products[rng.random_range(0..products.len())]);
            }
            if f.due && rng.random_bool(0.4) {
                let due = MONDAY + rng.random_range(1..12) * HOUR;
                job =
                    job.with_due_date(chrono::DateTime::from_timestamp_millis(due).expect("valid"));
            }
            if f.release && rng.random_bool(0.3) {
                job.earliest_start =
                    chrono::DateTime::from_timestamp_millis(MONDAY + rng.random_range(0..4) * HOUR);
            }
            for k in 0..rng.random_range(1..=3) {
                let mut candidates: Vec<String> =
                    (0..n_equipment).map(|e| format!("E{e}")).collect();
                let keep = if f.alternatives {
                    rng.random_range(1..=candidates.len())
                } else {
                    1
                };
                while candidates.len() > keep {
                    let i = rng.random_range(0..candidates.len());
                    candidates.remove(i);
                }
                let mut op = Operation::new(format!("{id}-{k}"), &id, k + 1).with_time(
                    rng.random_range(0..3) * 60_000,
                    rng.random_range(1..12) * 10 * 60_000,
                    0,
                );
                // A step that holds nothing (a cure, a wait).
                let time_only = f.time_only && rng.random_bool(0.25);
                if !time_only {
                    op = op.with_equipment(candidates);
                }
                if f.materials && rng.random_bool(0.4) {
                    let mut need = MaterialRequirement::new(
                        ["STEEL", "BOLT"][rng.random_range(0..2)],
                        f64::from(rng.random_range(1..6)),
                    );
                    if rng.random_bool(0.5) {
                        need = need.with_availability(
                            chrono::DateTime::from_timestamp_millis(
                                MONDAY + rng.random_range(0..6) * HOUR,
                            )
                            .expect("valid"),
                        );
                    }
                    op = op.with_material(need);
                }
                if f.daily && rng.random_bool(0.3) {
                    // Hours of the day it may run in — some past midnight —
                    // and whether its work may be split at their ends.
                    let window = match rng.random_range(0..3) {
                        0 => DailyWorkWindow::from_hm(9, 0, 15, 0),
                        1 => DailyWorkWindow::from_hm(6, 30, 11, 0),
                        _ => DailyWorkWindow::from_hm(22, 0, 6, 0),
                    };
                    op = op.with_daily_work_window(window);
                    op.is_splittable = rng.random_bool(0.5);
                }
                if !time_only && n_workers > 0 && rng.random_bool(0.5) {
                    op = if rng.random_bool(0.5) {
                        op.with_workers_load(1, [0.25, 0.5][rng.random_range(0..2)])
                    } else {
                        op.with_workers(1)
                    };
                }
                job = job.with_operation(op);
            }
            job
        })
        .collect();

    let mut request = ScheduleRequest::new(jobs, resources).with_start_time(MONDAY);
    if f.setups && rng.random_bool(0.6) {
        let mut matrices = SetupMatrixCollection::new();
        for e in 0..n_equipment {
            if rng.random_bool(0.6) {
                matrices = matrices.with_matrix(
                    SetupMatrix::new(format!("E{e}"))
                        .with_default_setup(rng.random_range(0..3) * 60_000)
                        .with_setup("P1", "P2", 5 * 60_000)
                        .with_setup("P2", "P1", 7 * 60_000),
                );
            }
        }
        request = request.with_setup_matrices(matrices);
    }
    if f.rule && rng.random_bool(0.7) {
        use DispatchingRuleName as R;
        let rules = [
            R::Spt,
            R::Lpt,
            R::Edd,
            R::Mwkr,
            R::Lwkr,
            R::Fifo,
            R::Priority,
            R::Cr,
            R::Wspt,
            R::Atc,
            R::Mst,
            R::Sro,
            R::Winq,
            R::Lpul,
        ];
        request = request.with_dispatching_rule(rules[rng.random_range(0..rules.len())]);
    }
    if f.materials {
        let mut materials = MaterialManager::new();
        materials.add_material(Material::new("STEEL", "Steel").with_stock(10.0));
        materials.add_material(Material::new("BOLT", "Bolt").with_stock(2.0));
        request = request.with_material_manager(materials);
    }
    if f.switching && rng.random_bool(0.5) {
        use DispatchingRuleName as R;
        let condition = match rng.random_range(0..3) {
            0 => RuleSwitchCondition::TimeAfter {
                threshold_ms: MONDAY + rng.random_range(1..4) * HOUR,
            },
            1 => RuleSwitchCondition::DelayedJobsExceed {
                count: rng.random_range(1..3),
            },
            _ => RuleSwitchCondition::QueueLengthAbove {
                threshold: rng.random_range(2..5),
            },
        };
        let multi = MultiLayerDispatchingConfig::new(R::Spt)
            .with_switch_rule(RuleSwitchRule::new(condition, R::Edd).with_priority(1));
        request = request.with_dispatching_config(DispatchingConfig::multi_layer(multi));
    }
    if calendared {
        let weekdays = vec![
            DayOfWeek::Monday,
            DayOfWeek::Tuesday,
            DayOfWeek::Wednesday,
            DayOfWeek::Thursday,
            DayOfWeek::Friday,
        ];
        request = request.with_calendar(
            Calendar::new("DAY", "Day shift")
                .with_shift(Shift::new(
                    "day",
                    Time::new(8, 0),
                    Time::new(17, 0),
                    weekdays,
                ))
                .with_break(BreakTime::new(Time::new(12, 0), Time::new(13, 0))),
        );
    }
    request
}

/// The first path at which `a` and `b` differ, if any.
fn first_difference(path: &str, a: &Value, b: &Value) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: std::collections::BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            keys.into_iter().find_map(|k| {
                first_difference(
                    &format!("{path}.{k}"),
                    x.get(k).unwrap_or(&Value::Null),
                    y.get(k).unwrap_or(&Value::Null),
                )
            })
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x
            .iter()
            .zip(y)
            .enumerate()
            .find_map(|(i, (p, q))| first_difference(&format!("{path}[{i}]"), p, q)),
        _ if a == b => None,
        _ => Some(format!("{path}: {a} vs {b}")),
    }
}

/// The engine's schedule in u-schedule's terms, for its feasibility check:
/// an assignment per resource, an operation without resources (`NONE`) as a
/// time-only span.
fn as_u_schedule(s: &Schedule) -> u_schedule::models::Schedule {
    use u_schedule::models::{Assignment as USAssignment, TimeOnlySpan};
    let mut us = u_schedule::models::Schedule::new();
    for a in &s.assignments {
        if a.resource_id == "NONE" {
            us.time_only.push(TimeOnlySpan::new(
                &a.operation_id,
                &a.job_id,
                a.start_ms,
                a.end_ms,
            ));
        } else {
            us.add_assignment(
                USAssignment::new(
                    &a.operation_id,
                    &a.job_id,
                    &a.resource_id,
                    a.start_ms,
                    a.end_ms,
                )
                .with_setup(a.setup_ms),
            );
        }
    }
    us
}

/// Requests with `f`'s features: how many schedule the same, and the first
/// differences.
fn compare(f: Features, cases: usize) -> (usize, Vec<(usize, String)>, usize) {
    let mut rng = SmallRng::seed_from_u64(20261018);
    let (mut same, mut not_rebased) = (0, 0);
    let mut differing: Vec<(usize, String)> = Vec::new();
    for case in 0..cases {
        let request = random_request(&mut rng, f);
        let legacy =
            serde_json::to_value(SimpleScheduler::new().schedule(&request)).expect("serializes");
        match rebased(&request) {
            Ok(schedule) => {
                let schedule = serde_json::to_value(schedule).expect("serializes");
                match first_difference("schedule", &legacy, &schedule) {
                    None => same += 1,
                    Some(at) => differing.push((case, at)),
                }
            }
            Err(NotRebased::Unsupported(_)) | Err(_) => not_rebased += 1,
        }
    }
    (same, differing, not_rebased)
}

/// Each feature alone over plain requests, then all together.
#[test]
#[ignore = "measurement: run with --ignored --nocapture"]
fn differences_by_feature() {
    let plain = Features::default();
    let alone: [(&str, Features); 15] = [
        ("plain", plain),
        (
            "alternatives",
            Features {
                alternatives: true,
                ..plain
            },
        ),
        (
            "efficiency",
            Features {
                alternatives: true,
                efficiency: true,
                ..plain
            },
        ),
        (
            "workers",
            Features {
                workers: true,
                ..plain
            },
        ),
        (
            "setups",
            Features {
                setups: true,
                ..plain
            },
        ),
        (
            "priority",
            Features {
                priority: true,
                ..plain
            },
        ),
        ("due", Features { due: true, ..plain }),
        (
            "release",
            Features {
                release: true,
                ..plain
            },
        ),
        (
            "rule",
            Features {
                rule: true,
                ..plain
            },
        ),
        (
            "calendar",
            Features {
                calendar: true,
                ..plain
            },
        ),
        (
            "switching",
            Features {
                switching: true,
                ..plain
            },
        ),
        (
            "materials",
            Features {
                materials: true,
                ..plain
            },
        ),
        (
            "daily",
            Features {
                daily: true,
                ..plain
            },
        ),
        (
            "daily+calendar",
            Features {
                daily: true,
                calendar: true,
                time_only: true,
                ..plain
            },
        ),
        (
            "time-only",
            Features {
                time_only: true,
                calendar: true,
                ..plain
            },
        ),
    ];
    for (name, f) in alone.into_iter().chain([("all", Features::ALL)]) {
        let started = std::time::Instant::now();
        let (same, differing, not_rebased) = compare(f, 300);
        eprintln!(
            "{name:>14}: same {same:>3} · differing {:>3} · not rebased {not_rebased} · {:.1} s",
            differing.len(),
            started.elapsed().as_secs_f64()
        );
        for (case, at) in differing.iter().take(3) {
            eprintln!("               case {case}: {at}");
        }
    }
}

/// Over random requests with every feature: both schedules judged by one
/// checker (u-schedule's feasibility check on the adapted problem), and the
/// makespan and total lateness of each.
#[test]
#[ignore = "measurement: run with --ignored --nocapture"]
fn quality_of_both_schedules() {
    use u_schedule::scheduler::{check_schedule, FeasibilityInput};

    let mut rng = SmallRng::seed_from_u64(20261018);
    let (mut legacy_broken, mut rebased_broken) = (0, 0);
    let (mut better, mut worse, mut equal) = (0, 0, 0);
    let (mut legacy_late, mut rebased_late) = (0i64, 0i64);
    let mut kinds: std::collections::BTreeMap<String, usize> = Default::default();
    for _ in 0..300 {
        let request = random_request(&mut rng, Features::ALL);
        let legacy = SimpleScheduler::new().schedule(&request);
        let Ok(rebased) = rebased(&request) else {
            continue;
        };
        let adapted = uaps_engine::adapter::to_problem(&request);
        let problem = adapted.problem.expect("adapted");
        let judge = |s: &Schedule| {
            check_schedule(
                &as_u_schedule(s),
                &FeasibilityInput {
                    tasks: problem.tasks(),
                    resources: problem.resources(),
                    constraints: &[],
                },
            )
            .into_iter()
            .filter(|v| v.violation_type != u_schedule::models::ViolationType::DeadlineMiss)
            .collect::<Vec<_>>()
        };
        let lv = judge(&legacy);
        for v in &lv {
            *kinds.entry(format!("{:?}", v.violation_type)).or_default() += 1;
        }
        legacy_broken += usize::from(!lv.is_empty());
        rebased_broken += usize::from(!judge(&rebased).is_empty());
        match rebased.makespan_ms.cmp(&legacy.makespan_ms) {
            std::cmp::Ordering::Less => better += 1,
            std::cmp::Ordering::Greater => worse += 1,
            std::cmp::Ordering::Equal => equal += 1,
        }
        let late = |s: &Schedule| -> i64 {
            request
                .jobs
                .iter()
                .filter_map(|j| {
                    let due = j.due_date?.timestamp_millis();
                    let end = j
                        .operations
                        .iter()
                        .filter_map(|o| s.assignment_for_operation(&o.id).map(|a| a.end_ms))
                        .max()?;
                    Some((end - due).max(0))
                })
                .sum()
        };
        legacy_late += late(&legacy);
        rebased_late += late(&rebased);
    }
    eprintln!("broken schedules: legacy {legacy_broken} · rebased {rebased_broken}");
    eprintln!("legacy violation kinds: {kinds:?}");
    eprintln!("rebased makespan: shorter {better} · longer {worse} · equal {equal}");
    eprintln!(
        "total lateness (h): legacy {:.1} · rebased {:.1}",
        legacy_late as f64 / 3.6e6,
        rebased_late as f64 / 3.6e6
    );
}

/// Every schedule u-schedule places for a random request keeps the
/// request: no resource beyond its capacity, every operation after the one
/// before it and in working time — judged by u-schedule's own check, as the
/// engine's schedule is read back.
#[test]
fn rebased_schedules_keep_random_requests() {
    use u_schedule::scheduler::{check_schedule, FeasibilityInput};

    let mut rng = SmallRng::seed_from_u64(7);
    for case in 0..200 {
        let request = random_request(&mut rng, Features::ALL);
        let Ok(schedule) = rebased(&request) else {
            continue;
        };
        let problem = uaps_engine::adapter::to_problem(&request)
            .problem
            .expect("adapted");
        let broken: Vec<_> = check_schedule(
            &as_u_schedule(&schedule),
            &FeasibilityInput {
                tasks: problem.tasks(),
                resources: problem.resources(),
                constraints: &[],
            },
        )
        .into_iter()
        .filter(|v| v.violation_type != u_schedule::models::ViolationType::DeadlineMiss)
        .collect();
        assert!(broken.is_empty(), "case {case}: {broken:?}");
    }
}
