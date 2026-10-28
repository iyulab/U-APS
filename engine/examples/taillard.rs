//! Measures `SimpleScheduler`'s dispatching rules on Taillard's job-shop
//! instances, through the request a consumer sends (`ScheduleRequest` with a
//! dispatching rule), counting the violations each schedule reports.
//!
//! The instances are not in this repository. Download them in the standard
//! job-shop format (first line `jobs machines`, then one line per job of
//! `machine time` pairs, machines counted from 0 -- e.g. the `ta01` ... files
//! of the JSPLIB collection) into one directory:
//!
//! ```text
//! cargo run --release --example taillard -- <dir> [bks.csv] [rules]
//! ```
//!
//! `bks.csv` (optional) has lines `instance,makespan` -- the optimum or best
//! known upper bound; the gap column is against it. `rules` is a comma list of
//! `SPT,LPT,LWKR,MWKR,FIFO,WINQ,LPUL,EDD,PRIORITY` (default `MWKR,SPT,FIFO`),
//! `GA@<ms>` runs the genetic search over dispatching orders with that time
//! limit, and `CP@<ms>` the exact search (`CpSat`).
//! A flexible job shop (`.fjs`, Brandimarte's format) whose operations take
//! the same time on each of their machines is read too: an operation's
//! machines are its requirement's candidates.
//! One machine is one equipment resource, one operation one equipment
//! requirement; no worker, calendar or set-up is involved, so the numbers are
//! the dispatching alone.

use std::collections::HashMap;
use std::time::Instant;

use uaps_engine::{
    CpSatConfig, CpSatScheduler, DispatchingRuleName, GaParams, Job, Operation, Resource,
    ScheduleRequest, SimpleScheduler,
};

/// Reads one instance: jobs whose operations each need one machine -- in a
/// flexible one (`.fjs`), any of several, all for the operation's one time.
fn read(path: &std::path::Path) -> (Vec<Job>, Vec<Resource>) {
    let flexible = path.extension().is_some_and(|e| e == "fjs");
    let text = std::fs::read_to_string(path).expect("instance file is readable");
    let mut numbers = text
        .split_whitespace()
        .filter_map(|t| t.parse::<i64>().ok());
    let mut next = || numbers.next().expect("instance is complete");
    let (jobs, machines) = (next() as usize, next() as usize);
    let jobs = (0..jobs)
        .map(|j| {
            let id = format!("J{j}");
            let steps = if flexible { next() as usize } else { machines };
            (0..steps).fold(Job::new(&id), |job, k| {
                let choices = if flexible { next() as usize } else { 1 };
                let options: Vec<(i64, i64)> = (0..choices).map(|_| (next(), next())).collect();
                let time = options[0].1;
                assert!(
                    options.iter().all(|&(_, t)| t == time),
                    "{}: an operation's machines differ in time, which an operation states once",
                    path.display()
                );
                job.with_operation(
                    Operation::new(format!("{id}-O{k}"), &id, k as i32 + 1)
                        .with_time(0, time, 0)
                        .with_equipment(options.iter().map(|&(m, _)| format!("M{m}")).collect()),
                )
            })
        })
        .collect();
    let resources = (0..machines)
        .map(|m| Resource::equipment(format!("M{m}")))
        .collect();
    (jobs, resources)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::path::Path::new(
        args.first()
            .expect("usage: taillard <dir> [bks.csv] [rules]"),
    );
    let best: HashMap<String, f64> = args
        .get(1)
        .filter(|p| !p.is_empty())
        .map(|p| {
            std::fs::read_to_string(p)
                .expect("bks.csv is readable")
                .lines()
                .filter_map(|l| {
                    let (name, value) = l.split_once(',')?;
                    Some((name.trim().to_string(), value.trim().parse().ok()?))
                })
                .collect()
        })
        .unwrap_or_default();
    let chosen: Vec<String> = args.get(2).map_or_else(
        || vec!["MWKR".into(), "SPT".into(), "FIFO".into()],
        |list| list.split(',').map(str::to_string).collect(),
    );

    let mut names: Vec<_> = std::fs::read_dir(dir)
        .expect("instance directory is readable")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| (n.starts_with("ta") && !n.contains('.')) || n.ends_with(".fjs"))
        })
        .collect();
    names.sort();

    println!("instance,size,rule,makespan,best,gap_pct,violations,ms");
    let mut gaps: HashMap<&str, Vec<f64>> = HashMap::new();
    for path in &names {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("a file name");
        let (jobs, resources) = read(path);
        let size = format!("{}x{}", jobs.len(), resources.len());
        for rule in &chosen {
            let mut request = ScheduleRequest::new(jobs.clone(), resources.clone());
            let ga_limit = rule.strip_prefix("GA@").map(|ms| {
                ms.parse::<i64>()
                    .unwrap_or_else(|_| panic!("GA@<ms> takes milliseconds, got {rule:?}"))
            });
            let cp_limit = rule.strip_prefix("CP@").map(|ms| {
                ms.parse::<i64>()
                    .unwrap_or_else(|_| panic!("CP@<ms> takes milliseconds, got {rule:?}"))
            });
            if ga_limit.is_none() && cp_limit.is_none() && rule != "PRIORITY" {
                request = request.with_dispatching_rule(
                    DispatchingRuleName::parse(rule)
                        .unwrap_or_else(|| panic!("unknown rule {rule:?}")),
                );
            }
            let started = Instant::now();
            let schedule = match (ga_limit, cp_limit) {
                (Some(ms), _) => SimpleScheduler::new().search(
                    &request,
                    &GaParams {
                        time_limit_ms: Some(ms),
                        ..Default::default()
                    },
                ),
                (_, Some(ms)) => CpSatScheduler::new(CpSatConfig {
                    time_limit_ms: ms,
                    ..Default::default()
                })
                .schedule(&request)
                .map(|result| result.schedule),
                _ => SimpleScheduler::new().schedule(&request),
            }
            .expect("schedules");
            let ms = started.elapsed().as_millis();
            let makespan = schedule.makespan_ms as f64;
            let reference = best.get(name).copied();
            let gap = reference.map(|b| 100.0 * (makespan - b) / b);
            if let Some(g) = gap {
                gaps.entry(rule.as_str()).or_default().push(g);
            }
            println!(
                "{name},{size},{rule},{makespan},{},{},{},{ms}",
                reference.map_or(String::new(), |b| b.to_string()),
                gap.map_or(String::new(), |g| format!("{g:.1}")),
                schedule.violations.len(),
            );
        }
    }
    for rule in &chosen {
        if let Some(g) = gaps.get(rule.as_str()) {
            let mean = g.iter().sum::<f64>() / g.len() as f64;
            eprintln!("{rule}: mean gap {mean:.1} % over {} instances", g.len());
        }
    }
}
