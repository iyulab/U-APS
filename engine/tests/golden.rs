//! Golden responses of the FFI surface.
//!
//! Each scenario builds a request with the engine's own types, sends it
//! through `uaps_schedule` / `uaps_validate` / `uaps_reschedule` as JSON (the
//! path the C# SDK takes), and compares the response with
//! `tests/golden/<name>.json`. Timing (`metrics`) is dropped; everything else
//! must match exactly.
//!
//! The deterministic algorithm (`Simple`) is pinned byte for byte. The GA is
//! not seeded, so its scenario checks invariants only.
//!
//! Regenerate deliberately — when a change is meant to alter results — with
//! `UPDATE_GOLDEN=1 cargo test --test golden`, and review the diff.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;

use serde_json::{json, Value};
use uaps_engine::ffi::{uaps_free_string, uaps_reschedule, uaps_schedule, uaps_validate};
use uaps_engine::{
    Calendar, Crew, CrewManager, DispatchingConfig, DispatchingRuleName, Job, Material,
    MaterialManager, MaterialRequirement, Operation, Resource, SetupMatrix, SetupMatrixCollection,
    SiteTransitions, SkillMatrix, TeamRole,
};

type FfiFn = unsafe extern "C" fn(*const c_char, *mut *mut c_char) -> i32;

fn call(f: FfiFn, request: &Value) -> Value {
    let input = CString::new(request.to_string()).expect("request has no interior NUL");
    let mut out: *mut c_char = std::ptr::null_mut();
    // SAFETY: `input` is a valid NUL-terminated string that outlives the call,
    // `out` is a valid out-pointer, and the result is freed with the engine's
    // own `uaps_free_string`.
    let text = unsafe {
        f(input.as_ptr(), &mut out);
        assert!(!out.is_null(), "FFI wrote no response");
        let text = CStr::from_ptr(out)
            .to_str()
            .expect("UTF-8 response")
            .to_owned();
        uaps_free_string(out);
        text
    };
    let mut response: Value = serde_json::from_str(&text).expect("response is JSON");
    if let Some(object) = response.as_object_mut() {
        object.remove("metrics");
    }
    response
}

fn to_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("engine types serialize")
}

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(format!("{name}.json"))
}

fn assert_golden(name: &str, actual: &Value) {
    let path = golden_path(name);
    let rendered = serde_json::to_string_pretty(actual).expect("value renders") + "\n";
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().expect("golden dir")).expect("create golden dir");
        std::fs::write(&path, rendered).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing {} — run `UPDATE_GOLDEN=1 cargo test --test golden`",
            path.display()
        )
    });
    let expected: Value = serde_json::from_str(&expected).expect("golden is JSON");
    assert_eq!(
        &expected,
        actual,
        "{name}: response differs from {}",
        path.display()
    );
}

fn op(id: &str, job: &str, seq: i32, process_ms: i64, equipment: &[&str]) -> Operation {
    Operation::new(id, job, seq)
        .with_time(0, process_ms, 0)
        .with_equipment(equipment.iter().map(|e| e.to_string()).collect())
}

fn request(jobs: &[Job], resources: &[Resource]) -> Value {
    json!({ "jobs": to_json(&jobs), "resources": to_json(&resources) })
}

fn schedule_golden(name: &str, request: Value) -> Value {
    let response = call(uaps_schedule, &request);
    assert_eq!(response["success"], json!(true), "{name}: {response}");
    assert_golden(name, &response);
    response
}

/// 2024-01-22 (Monday) 00:00 UTC.
const MONDAY_MIDNIGHT: i64 = 1_705_881_600_000;
const HOUR: i64 = 3_600_000;

fn g01_single_job_chain_request() -> Value {
    let job = Job::new("J1")
        .with_operation(op("J1-1", "J1", 1, 30_000, &["M1"]))
        .with_operation(op("J1-2", "J1", 2, 45_000, &["M2"]))
        .with_operation(op("J1-3", "J1", 3, 15_000, &["M1"]));
    let resources = [Resource::equipment("M1"), Resource::equipment("M2")];
    request(&[job], &resources)
}

#[test]
fn g01_single_job_chain() {
    schedule_golden("g01_single_job_chain", g01_single_job_chain_request());
}

fn g02_competing_jobs_priority_request() -> Value {
    let jobs: Vec<Job> = (1..=3)
        .map(|i| {
            let id = format!("J{i}");
            Job::new(&id).with_priority(i).with_operation(op(
                &format!("{id}-1"),
                &id,
                1,
                20_000 * i64::from(i),
                &["LATHE"],
            ))
        })
        .collect();
    request(&jobs, &[Resource::equipment("LATHE")])
}

#[test]
fn g02_competing_jobs_priority() {
    schedule_golden(
        "g02_competing_jobs_priority",
        g02_competing_jobs_priority_request(),
    );
}

fn g03_equipment_and_worker_request() -> Value {
    let jobs: Vec<Job> = ["A", "B", "C"]
        .iter()
        .map(|j| {
            Job::new(*j).with_operation(
                op(&format!("{j}-1"), j, 1, 600_000, &["PRESS-1", "PRESS-2"]).with_workers(1),
            )
        })
        .collect();
    let resources = [
        Resource::equipment("PRESS-1"),
        Resource::equipment("PRESS-2"),
        Resource::worker("ALICE"),
        Resource::worker("BOB"),
    ];
    request(&jobs, &resources)
}

#[test]
fn g03_equipment_and_worker() {
    schedule_golden(
        "g03_equipment_and_worker",
        g03_equipment_and_worker_request(),
    );
}

fn g04_setup_matrix_request() -> Value {
    let products = ["A", "B", "A", "B"];
    let jobs: Vec<Job> = products
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let id = format!("J{i}");
            Job::new(&id)
                .with_product(format!("Product{p}"))
                .with_operation(op(&format!("{id}-1"), &id, 1, 30_000, &["EQP"]))
        })
        .collect();
    let matrices = SetupMatrixCollection::new().with_matrix(
        SetupMatrix::new("EQP")
            .with_default_setup(10_000)
            .with_setup("ProductA", "ProductB", 20_000)
            .with_setup("ProductB", "ProductA", 15_000),
    );
    let mut req = request(&jobs, &[Resource::equipment("EQP")]);
    req["setup_matrices"] = to_json(&matrices);
    req
}

#[test]
fn g04_setup_matrix() {
    schedule_golden("g04_setup_matrix", g04_setup_matrix_request());
}

fn g05_alternative_efficiency_request() -> Value {
    let jobs: Vec<Job> = (1..=4)
        .map(|i| {
            let id = format!("J{i}");
            Job::new(&id).with_operation(op(&format!("{id}-1"), &id, 1, 60_000, &["FAST", "SLOW"]))
        })
        .collect();
    let resources = [
        Resource::equipment("FAST").with_efficiency(2.0),
        Resource::equipment("SLOW").with_efficiency(0.5),
    ];
    request(&jobs, &resources)
}

#[test]
fn g05_alternative_efficiency() {
    schedule_golden(
        "g05_alternative_efficiency",
        g05_alternative_efficiency_request(),
    );
}

fn g06_worker_load_factor_request() -> Value {
    let jobs: Vec<Job> = (1..=3)
        .map(|i| {
            let id = format!("J{i}");
            Job::new(&id).with_priority(i).with_operation(
                op(&format!("{id}-1"), &id, 1, 30_000, &[&format!("EQP-{i}")])
                    .with_workers_load(1, 0.5),
            )
        })
        .collect();
    let resources = [
        Resource::equipment("EQP-1"),
        Resource::equipment("EQP-2"),
        Resource::equipment("EQP-3"),
        Resource::worker("W1"),
    ];
    request(&jobs, &resources)
}

#[test]
fn g06_worker_load_factor() {
    schedule_golden("g06_worker_load_factor", g06_worker_load_factor_request());
}

fn g07_linked_robot_request() -> Value {
    let jobs = [
        Job::new("J1")
            .with_priority(1)
            .with_operation(op("J1-1", "J1", 1, 30_000, &["ROBOT"])),
        Job::new("J2")
            .with_priority(2)
            .with_operation(op("J2-1", "J2", 1, 20_000, &["CELL"])),
    ];
    let resources = [
        Resource::equipment("ROBOT")
            .as_robot()
            .with_linked_resource("CELL"),
        Resource::equipment("CELL"),
    ];
    request(&jobs, &resources)
}

#[test]
fn g07_linked_robot() {
    schedule_golden("g07_linked_robot", g07_linked_robot_request());
}

fn g08_calendar_spans_lunch_and_night_request() -> Value {
    let jobs = [
        Job::new("J1").with_operation(op("J1-1", "J1", 1, 3 * HOUR, &["EQP"])),
        Job::new("J2").with_operation(op("J2-1", "J2", 1, 6 * HOUR, &["EQP"])),
    ];
    let mut req = request(
        &jobs,
        &[Resource::equipment("EQP").with_calendar("CAL-DAY")],
    );
    req["start_time_ms"] = json!(MONDAY_MIDNIGHT + 11 * HOUR);
    req["calendars"] = to_json(&[Calendar::default_day()]);
    req
}

#[test]
fn g08_calendar_spans_lunch_and_night() {
    schedule_golden(
        "g08_calendar_spans_lunch_and_night",
        g08_calendar_spans_lunch_and_night_request(),
    );
}

fn g09_skill_proficiency_request() -> Value {
    let jobs: Vec<Job> = (1..=2)
        .map(|i| {
            let id = format!("J{i}");
            Job::new(&id).with_operation(
                op(&format!("{id}-1"), &id, 1, 60_000, &[&format!("EQP-{i}")])
                    .with_operation_type("welding")
                    .with_workers(1),
            )
        })
        .collect();
    let resources = [
        Resource::equipment("EQP-1"),
        Resource::equipment("EQP-2"),
        Resource::worker("EXPERT"),
        Resource::worker("NOVICE"),
    ];
    let skills = SkillMatrix::new()
        .with_skill("EXPERT", "welding", 2.0)
        .with_skill("NOVICE", "welding", 0.5);
    let mut req = request(&jobs, &resources);
    req["skill_matrix"] = to_json(&skills);
    req
}

#[test]
fn g09_skill_proficiency() {
    schedule_golden("g09_skill_proficiency", g09_skill_proficiency_request());
}

fn g10_material_availability_request() -> Value {
    let start = MONDAY_MIDNIGHT + 8 * HOUR;
    let arrives = chrono::DateTime::from_timestamp_millis(start + 4 * HOUR).expect("valid time");
    let jobs = [
        Job::new("J1").with_operation(
            op("J1-1", "J1", 1, 60_000, &["EQP"])
                .with_material(MaterialRequirement::new("STEEL", 10.0).with_availability(arrives)),
        ),
        Job::new("J2").with_operation(op("J2-1", "J2", 1, 60_000, &["EQP"])),
    ];
    let mut materials = MaterialManager::new();
    materials.add_material(Material::new("STEEL", "Steel plate").with_stock(100.0));
    let mut req = request(&jobs, &[Resource::equipment("EQP")]);
    req["start_time_ms"] = json!(start);
    req["material_manager"] = to_json(&materials);
    req
}

#[test]
fn g10_material_availability() {
    schedule_golden(
        "g10_material_availability",
        g10_material_availability_request(),
    );
}

fn g11_crew_request() -> Value {
    let crews = CrewManager::new().with_crew(
        Crew::new("TEAM-A", "Assembly A")
            .with_efficiency(1.5)
            .with_capacity(2, 5)
            .with_worker("W1", TeamRole::Leader)
            .with_worker("W2", TeamRole::Member),
    );
    let job =
        Job::new("J1").with_operation(op("J1-1", "J1", 1, 60_000, &["EQP"]).with_crew("TEAM-A"));
    let resources = [
        Resource::equipment("EQP"),
        Resource::worker("W1"),
        Resource::worker("W2"),
    ];
    let mut req = request(&[job], &resources);
    req["crew_manager"] = to_json(&crews);
    req
}

#[test]
fn g11_crew() {
    schedule_golden("g11_crew", g11_crew_request());
}

fn g12_multi_site_request() -> Value {
    let job = Job::new("J1")
        .with_operation(op("J1-1", "J1", 1, 30_000, &["EQP-A"]))
        .with_operation(op("J1-2", "J1", 2, 30_000, &["EQP-B"]));
    let resources = [
        Resource::equipment("EQP-A").with_site("SITE-A"),
        Resource::equipment("EQP-B").with_site("SITE-B"),
    ];
    let mut transitions = SiteTransitions::new();
    transitions.add_bidirectional("SITE-A", "SITE-B", HOUR);
    let mut req = request(&[job], &resources);
    req["site_transitions"] = to_json(&transitions);
    req
}

#[test]
fn g12_multi_site() {
    schedule_golden("g12_multi_site", g12_multi_site_request());
}

fn g13_dispatching_edd_with_lateness_request() -> Value {
    let dues = [9, 3, 6, 1];
    let jobs: Vec<Job> = dues
        .iter()
        .enumerate()
        .map(|(i, due_min)| {
            let id = format!("J{i}");
            let due =
                chrono::DateTime::from_timestamp_millis(due_min * 60_000).expect("valid time");
            Job::new(&id).with_due_date(due).with_operation(op(
                &format!("{id}-1"),
                &id,
                1,
                120_000,
                &["EQP"],
            ))
        })
        .collect();
    let mut req = request(&jobs, &[Resource::equipment("EQP")]);
    req["dispatching_config"] = to_json(&DispatchingConfig::new(DispatchingRuleName::Edd));
    req
}

#[test]
fn g13_dispatching_edd_with_lateness() {
    schedule_golden(
        "g13_dispatching_edd_with_lateness",
        g13_dispatching_edd_with_lateness_request(),
    );
}

#[test]
fn g14_validate_rejects_duplicates() {
    let jobs = [
        Job::new("J1").with_operation(op("J1-1", "J1", 1, 1_000, &["EQP"])),
        Job::new("J1").with_operation(op("J1-2", "J1", 1, 1_000, &["MISSING"])),
    ];
    let response = call(
        uaps_validate,
        &request(&jobs, &[Resource::equipment("EQP")]),
    );
    assert_eq!(response["valid"], json!(false), "{response}");
    assert_golden("g14_validate_rejects_duplicates", &response);
}

#[test]
fn g15_reschedule_machine_breakdown() {
    let jobs: Vec<Job> = (1..=3)
        .map(|i| {
            let id = format!("J{i}");
            Job::new(&id)
                .with_operation(op(&format!("{id}-1"), &id, 1, 60_000, &["M1"]))
                .with_operation(op(&format!("{id}-2"), &id, 2, 30_000, &["M2"]))
        })
        .collect();
    let first = call(
        uaps_schedule,
        &request(
            &jobs,
            &[Resource::equipment("M1"), Resource::equipment("M2")],
        ),
    );
    let response = call(
        uaps_reschedule,
        &json!({
            "schedule": first["schedule"],
            "event": { "MachineBreakdown": { "resource_id": "M2", "start_ms": 60_000, "duration_ms": 45_000 } },
            "strategy": "RightShift",
        }),
    );
    assert_eq!(response["success"], json!(true), "{response}");
    assert_golden("g15_reschedule_machine_breakdown", &response);
}

/// SDK fields the engine reads past today (CHANGELOG «Known limitations»),
/// each with what the scheduler lacks. Removing an entry makes the guard count
/// that field again. Matched as a path suffix.
const KNOWN_GAPS: &[(&str, &str)] = &[
    (
        ".dependencies",
        "cross-operation predecessors: the engine orders a job's operations by sequence only",
    ),
    (
        "].is_splittable",
        "job-level splitting: the engine splits operations, not jobs",
    ),
    (
        ".assigned_equipment_ids",
        "crew-to-equipment assignment: the engine's crews carry operation types only",
    ),
];

/// The request the C# SDK writes (pinned by the SDK's
/// `RequestWireContractTests`), read by the engine section by section so a
/// failure names every section the engine cannot read, then scheduled whole.
#[test]
fn sdk_request_fixture_is_accepted() {
    /// Keys present in `sent` and absent from `kept` — fields the engine read
    /// past without keeping. Arrays are compared element by element.
    fn dropped(path: &str, sent: &Value, kept: &Value, out: &mut Vec<String>) {
        match (sent, kept) {
            (Value::Object(s), Value::Object(k)) => {
                for (key, value) in s {
                    let empty = match value {
                        Value::Null => true,
                        Value::Array(a) => a.is_empty(),
                        Value::Object(o) => o.is_empty(),
                        _ => false,
                    };
                    if empty {
                        continue;
                    }
                    let here = format!("{path}.{key}");
                    match k.get(key) {
                        Some(kv) => dropped(&here, value, kv, out),
                        None => out.push(here),
                    }
                }
            }
            (Value::Array(s), Value::Array(k)) => {
                for (i, (sv, kv)) in s.iter().zip(k).enumerate() {
                    dropped(&format!("{path}[{i}]"), sv, kv, out);
                }
            }
            _ => {}
        }
    }
    fn section<T: serde::de::DeserializeOwned + serde::Serialize>(
        request: &Value,
        key: &str,
        errors: &mut Vec<String>,
    ) {
        if let Some(value) = request.get(key) {
            match serde_json::from_value::<T>(value.clone()) {
                Err(e) => errors.push(format!("{key}: {e}")),
                Ok(read) => {
                    let mut lost = Vec::new();
                    dropped(key, value, &to_json(&read), &mut lost);
                    errors.extend(
                        lost.into_iter()
                            .filter(|p| !KNOWN_GAPS.iter().any(|(suffix, _)| p.ends_with(suffix)))
                            .map(|p| format!("{p}: sent, not kept")),
                    );
                }
            }
        }
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("sdk-request-full.json");
    let text = std::fs::read_to_string(&path).expect("SDK request fixture");
    let request: Value = serde_json::from_str(&text).expect("fixture is JSON");

    let mut errors = Vec::new();
    section::<Vec<Job>>(&request, "jobs", &mut errors);
    section::<Vec<Resource>>(&request, "resources", &mut errors);
    section::<SetupMatrixCollection>(&request, "setup_matrices", &mut errors);
    section::<Vec<Calendar>>(&request, "calendars", &mut errors);
    section::<SkillMatrix>(&request, "skill_matrix", &mut errors);
    section::<uaps_engine::CertificationMatrix>(&request, "certification_matrix", &mut errors);
    section::<uaps_engine::WorkerTimeOverride>(&request, "worker_time_override", &mut errors);
    section::<CrewManager>(&request, "crew_manager", &mut errors);
    section::<DispatchingConfig>(&request, "dispatching_config", &mut errors);
    section::<MaterialManager>(&request, "material_manager", &mut errors);
    section::<uaps_engine::OutsourcingManager>(&request, "outsourcing_manager", &mut errors);
    assert!(
        errors.is_empty(),
        "sections the engine cannot read:\n{}",
        errors.join("\n")
    );

    let response = call(uaps_schedule, &request);
    assert_eq!(response["success"], json!(true), "{response}");
    assert_golden("sdk_request_full", &response);
}

/// Each scheduling scenario stated as a u-schedule problem
/// (`uaps_engine::adapter`) and scheduled by u-schedule, against the golden:
/// what the adapter leaves out and whether the schedules agree. Pinned in
/// `tests/golden/adapter_parity.json`, so moving a capability into u-schedule
/// shows as a diff here.
#[test]
fn adapter_parity() {
    use std::collections::BTreeSet;
    use uaps_engine::adapter::to_problem;
    use uaps_engine::ffi::schedule_request_from_json;

    fn placements(
        rows: impl Iterator<Item = (String, String, i64, i64)>,
    ) -> BTreeSet<(String, String, i64, i64)> {
        rows.collect()
    }

    let mut table = serde_json::Map::new();
    for (name, request) in schedule_scenarios() {
        let golden: Value = serde_json::from_str(
            &std::fs::read_to_string(golden_path(name)).expect("golden exists"),
        )
        .expect("golden is JSON");
        let uaps = placements(
            golden["schedule"]["assignments"]
                .as_array()
                .expect("assignments")
                .iter()
                .map(|a| {
                    (
                        a["operation_id"]
                            .as_str()
                            .expect("operation_id")
                            .to_string(),
                        a["resource_id"].as_str().expect("resource_id").to_string(),
                        a["start_ms"].as_i64().expect("start_ms"),
                        a["end_ms"].as_i64().expect("end_ms"),
                    )
                }),
        );

        let parsed =
            schedule_request_from_json(&request.to_string()).expect("scenario request parses");
        let adapted = to_problem(&parsed);
        let unsupported: Vec<String> = adapted
            .unsupported
            .iter()
            .map(|u| format!("{u:?}"))
            .collect();
        let row = match adapted.problem {
            Err(errors) => json!({
                "unsupported": unsupported,
                "refused": errors.iter().map(|e| e.to_string()).collect::<Vec<_>>(),
            }),
            Ok(problem) => {
                let mut scheduler = u_schedule::scheduler::SimpleScheduler::new()
                    .with_transition_matrices(adapted.transition_matrices);
                if let Some(engine) = adapted.rule_engine {
                    scheduler = scheduler.with_rule_engine(engine);
                }
                let schedule = scheduler.schedule(&problem, adapted.start_time_ms);
                // u-aps reports a linked resource it holds (a robot's cell)
                // without an assignment; u-schedule books it as a companion.
                // Compared as u-aps reports.
                let companion = |a: &u_schedule::models::Assignment| {
                    schedule.assignments.iter().any(|p| {
                        p.activity_id == a.activity_id
                            && problem.resources().iter().any(|r| {
                                r.id == p.resource_id && r.companions.contains(&a.resource_id)
                            })
                    })
                };
                let us = placements(schedule.assignments.iter().filter(|a| !companion(a)).map(
                    |a| {
                        (
                            a.activity_id.clone(),
                            a.resource_id.clone(),
                            a.start_ms,
                            a.end_ms,
                        )
                    },
                ));
                json!({
                    "unsupported": unsupported,
                    "same_schedule": us == uaps,
                    "makespan_ms": { "u_aps": golden["schedule"]["makespan_ms"], "u_schedule": schedule.makespan_ms() },
                    "placements_differing": uaps.symmetric_difference(&us).count(),
                    "u_schedule_violations": schedule.violations.len(),
                })
            }
        };
        // The adapter's own claim: a request it states completely is
        // scheduled exactly as u-aps schedules it.
        if row["unsupported"] == json!([]) {
            assert_eq!(
                row["same_schedule"],
                json!(true),
                "{name}: nothing reported unsupported, yet the schedules differ: {row}"
            );
        }
        table.insert(name.to_string(), row);
    }
    assert_golden("adapter_parity", &Value::Object(table));
}

/// The scheduling scenarios, by golden name.
fn schedule_scenarios() -> Vec<(&'static str, Value)> {
    vec![
        ("g01_single_job_chain", g01_single_job_chain_request()),
        (
            "g02_competing_jobs_priority",
            g02_competing_jobs_priority_request(),
        ),
        (
            "g03_equipment_and_worker",
            g03_equipment_and_worker_request(),
        ),
        ("g04_setup_matrix", g04_setup_matrix_request()),
        (
            "g05_alternative_efficiency",
            g05_alternative_efficiency_request(),
        ),
        ("g06_worker_load_factor", g06_worker_load_factor_request()),
        ("g07_linked_robot", g07_linked_robot_request()),
        (
            "g08_calendar_spans_lunch_and_night",
            g08_calendar_spans_lunch_and_night_request(),
        ),
        ("g09_skill_proficiency", g09_skill_proficiency_request()),
        (
            "g10_material_availability",
            g10_material_availability_request(),
        ),
        ("g11_crew", g11_crew_request()),
        ("g12_multi_site", g12_multi_site_request()),
        (
            "g13_dispatching_edd_with_lateness",
            g13_dispatching_edd_with_lateness_request(),
        ),
    ]
}

#[test]
fn ga_invariants() {
    // The GA is unseeded, so its schedule is not pinned — only that it is a
    // complete, violation-free schedule no worse than the dispatching one.
    let jobs: Vec<Job> = (1..=6)
        .map(|i| {
            let id = format!("J{i}");
            let (a, b) = if i % 2 == 0 {
                ("M1", "M2")
            } else {
                ("M2", "M1")
            };
            Job::new(&id)
                .with_operation(op(&format!("{id}-1"), &id, 1, 10_000 * i64::from(i), &[a]))
                .with_operation(op(
                    &format!("{id}-2"),
                    &id,
                    2,
                    7_000 * i64::from(7 - i),
                    &[b],
                ))
        })
        .collect();
    let resources = [Resource::equipment("M1"), Resource::equipment("M2")];
    let simple = call(uaps_schedule, &request(&jobs, &resources));
    let mut req = request(&jobs, &resources);
    req["algorithm"] = json!("GeneticAlgorithm");
    let ga = call(uaps_schedule, &req);
    assert_eq!(ga["success"], json!(true), "{ga}");
    let schedule = &ga["schedule"];
    assert_eq!(
        schedule["assignments"].as_array().map(Vec::len),
        Some(12),
        "every operation is scheduled: {schedule}"
    );
    assert_eq!(schedule["violations"], json!([]), "{schedule}");
    let makespan = |s: &Value| s["schedule"]["makespan_ms"].as_i64().expect("makespan");
    assert!(
        makespan(&ga) <= makespan(&simple),
        "GA {} > Simple {}",
        makespan(&ga),
        makespan(&simple)
    );
}
