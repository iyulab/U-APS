//! The dispatching scheduler on random requests that mix what the golden
//! scenarios test one at a time: alternative equipment at different
//! efficiencies, workers whole or at a load factor, set-up matrices,
//! priorities, due dates, release times, dispatching rules and rule
//! switching, materials, shift calendars and daily work windows, operations
//! without resources, target sites and outsourcing. Each schedule is judged
//! by u-schedule's own feasibility check.

use rand::rngs::SmallRng;
use rand::{RngExt, SeedableRng};
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
    sites: bool,
    outsourcing: bool,
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
        sites: true,
        outsourcing: true,
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
            if f.sites && rng.random_bool(0.8) {
                r = r.with_site(["S1", "S2"][rng.random_range(0..2)]);
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
            if f.sites && rng.random_bool(0.4) {
                // S3 keeps no equipment.
                job = job.with_site(["S1", "S2", "S3"][rng.random_range(0..3)]);
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
                if f.outsourcing && rng.random_bool(0.2) {
                    // Sent to a provider: its own lead time, the provider's, or
                    // (no provider listed) the processing time.
                    let mut away = OutsourcingConfig::new(["P1", "P2"][rng.random_range(0..2)]);
                    if rng.random_bool(0.5) {
                        away = away.with_lead_time_ms(rng.random_range(1..48) * HOUR);
                    }
                    op = op.with_outsourcing(away);
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
    if f.outsourcing && rng.random_bool(0.6) {
        let mut providers = OutsourcingManager::new();
        providers
            .add_provider(OutsourcingProvider::new("P1", "Plating").with_lead_time_ms(20 * HOUR));
        request = request.with_outsourcing_manager(providers);
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

/// The engine's schedule in u-schedule's terms, for its feasibility check:
/// an assignment per resource, an operation without resources (`NONE`) or
/// away at a provider (`OUTSOURCE:…`) as a time-only span.
fn as_u_schedule(s: &Schedule) -> u_schedule::models::Schedule {
    use u_schedule::models::{Assignment as USAssignment, TimeOnlySpan};
    let mut us = u_schedule::models::Schedule::new();
    for a in &s.assignments {
        if a.resource_id == "NONE" || a.resource_id.starts_with("OUTSOURCE:") {
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

/// The operations a schedule itself reports it could not fill or place
/// (`UnfilledRequirement`) — the previous scheduler reports none.
fn reported_unfilled(s: &Schedule) -> std::collections::BTreeSet<String> {
    s.violations
        .iter()
        .filter(|v| matches!(v.constraint_type, ConstraintType::UnfilledRequirement))
        .map(|v| v.target_id.clone())
        .collect()
}

/// Every random request is scheduled, and the schedule keeps it: no
/// resource beyond its capacity, every operation after the one before it and
/// in working time — judged by u-schedule's own check, as the engine's
/// schedule is read back — apart from the operations it reports it could
/// not fill or place.
#[test]
fn schedules_keep_random_requests() {
    use u_schedule::scheduler::{check_schedule, FeasibilityInput};

    let mut rng = SmallRng::seed_from_u64(7);
    for case in 0..200 {
        let request = random_request(&mut rng, Features::ALL);
        let schedule = SimpleScheduler::new()
            .schedule(&request)
            .unwrap_or_else(|e| panic!("case {case}: {e}"));
        let problem = uaps_engine::adapter::to_problem(&request)
            .problem
            .expect("adapted");
        let reported = reported_unfilled(&schedule);
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
        .filter(|v| !reported.contains(&v.entity_id))
        .collect();
        assert!(broken.is_empty(), "case {case}: {broken:?}");
    }
}
