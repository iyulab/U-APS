# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Maintained from 1.0.3 onward; earlier entries list release dates only (see git
history and the GitHub releases for those).

Versions here are the `UAPSVersion` shared by the `UAPS.SDK` and `UAPS.CLI`
NuGet packages. The Rust engine carries its own crate version and is not
published to a registry — it ships inside those packages.

## [Unreleased]

### Changed

- **The default scheduler (a request without `algorithm`) now places
  operations with the u-schedule dispatcher, so its schedules change.** The
  request is stated in u-schedule's terms — operations with their resource
  requirements, equipment efficiency and worker proficiency as speeds, crews,
  worker load factors as shares of a worker, linked resources, set-up
  matrices, site transitions as transport times between equipment, shift
  calendars, rule switching (`DispatchingConfig.MultiLayer`) decision by
  decision — scheduled there, and read back into the same response: one
  assignment per resource with its site, the operation split at its
  calendar's breaks into tasks, the material and due-date violations. What
  changes:
  - An operation takes the resource on which it can **start earliest**, using
    an idle gap the resource has before its last booking; at the same start,
    the one that processes it fastest, then the first listed. Before, it took
    the resource that became free soonest after its last booking (plus
    set-up), whether or not the job was ready then.
  - **A worker is never booked beyond what they can do at once.** A worker
    taken whole no longer overlaps work they hold at a load factor; the
    previous scheduler could book both, and on 300 random requests mixing
    alternatives, efficiencies, workers, set-ups, due dates, release times,
    rules, rule switching and calendars, 38 of its schedules held a resource
    beyond its capacity. None of the new ones does.
  - On a resource with a set-up matrix, the matrix's set-up replaces the
    operation's own set-up time instead of the larger of the two being taken.
  - On those 300 requests the makespan is shorter in 147, longer in 19 and
    the same in 134, and the total lateness against due dates is 17 % lower.
  A request using what the new path does not state yet — operation splitting
  or overlap, operation time windows or daily work windows, outsourcing,
  pegging, materials for an operation after the first, an operation without resources, a target site
  that keeps none of an operation's equipment — is scheduled as before.

## [1.3.1] - 2026-10-18

### Fixed

- **A request carrying a `SkillMatrix`, `CertificationMatrix`,
  `WorkerTimeOverride`, `CrewManager` or `MaterialManager` was refused by the
  engine** with `JSON parse error: invalid type: sequence, expected a map` (or
  `missing field`): the SDK sends these as lists of entries, the engine read
  them as maps keyed by a pair, which JSON cannot express. The engine now
  reads and writes the SDK's shapes, so these sections reach the scheduler.
- **Sequence-dependent set-up times never applied through the SDK**: `Job`
  did not send `ProductName`, so every set-up matrix lookup missed. It is now
  sent as `product_name`.
- The engine's certification vocabulary is the SDK's: levels `None`,
  `Basic`, `Advanced`, `Expert`, `Master` and targets `OperationType`,
  `Equipment`, `Product`. `Basic` scales processing time by 0.8 in the engine
  as it does in the SDK (was 0.7).

### Known limitations

These SDK fields are accepted and not yet applied by the engine:
`Operation.Dependencies` (operations of a job run in `Sequence` order only),
`Job.IsSplittable` (splitting is per operation) and `Crew.WithEquipment`.

## [1.3.0] - 2026-10-17

### Changed

- **The default (`Simple`) scheduler decides one operation at a time instead
  of one job at a time, so its schedules change.** Every unfinished job offers
  its next operation, placed at its earliest start; the earliest start is the
  decision time and the offers that can start then compete. The dispatching
  rule (or, with none, the job priority — lower value first) picks one on a
  context filled for that decision: the time, each job's work left, when it
  joined the queue, how many other waiting jobs want its next machine, the
  competitors' average operation and each resource's utilization. Before, jobs
  were sorted once on an empty context and each was booked whole: LWKR behaved
  as SPT, MWKR as LPT, WINQ and LPUL as input order, CR and ATC could not see
  the clock, and a machine idled for a job that was not ready while another
  waited. On Taillard's 40 job-shop instances (`engine/examples/taillard.rs`:
  one equipment per machine, no workers, calendars or set-ups) the mean
  makespan gap to the best known bound was 805–809 % whichever rule ran — a
  job was booked from its first operation to its last before the next was
  looked at, so with differing routings the schedule came out almost serial
  and the rules could not help — and is now 21.9 % with MWKR, 28.4 % FIFO,
  30.3 % SPT, 47.9 % LWKR and 42.0 % with no rule (every job had the same
  priority). A schedule you rely on may now differ in the order operations
  take a shared resource, their start times and the makespan; the placement
  of one operation — resource choice, efficiency, skills, calendars,
  set-ups, materials, crews, sites — is the same code as before.
- A job's `earliest_start` is kept: none of its operations starts before it.
  It used to be ignored.
- Violations for due dates are reported in input order of the jobs (they were
  in the order the rule sorted them).

### Fixed

- `DispatchingConfig.MultiLayer`, which the SDK sends, was dropped without a
  word. It is now used: at each decision the strategy picks the rule —
  `default_rule` or the first (highest `priority`) switch rule whose condition
  holds on the decision's state (time, delayed waiting jobs, queue length,
  most urgent priority, a resource's utilization), or the weighted `layers`.
  A switch condition is read as the SDK sends it (`{ "type": "TimeAfter",
  "threshold": 3600000 }`, `{ "type": "UtilizationAbove", "resource_id":
  "EQP-1", "threshold": 0.8 }`, `{ "type": "Always" }`); a name it does not
  know, a missing `resource_id` or a fractional count is refused.

## [1.2.1] - 2026-10-14

### Fixed

- The `CpSat` scheduler reported `Optimal` for any schedule it found and
  `Infeasible` whenever it found none, whether or not its search had finished.
  It now reports `Optimal` / `Infeasible` only when the branch-and-bound search
  ran to completion within the time limit, and `Feasible` (best schedule found)
  or `Timeout` (none found) when the limit cut it off. The documentation no
  longer calls it CP-SAT or "exact": it is this engine's own branch and bound.

### Changed

- The engine depends on `u-schedule` 0.16 and `u-metaheur` 0.8 (`u-schedule` 0.7
  and `u-metaheur` 0.4 before; their configuration builders now refuse an
  out-of-range value instead of clamping it, and the engine passes none). `u-schedule` 0.11's WSPT and ATC read a task's weight instead of
  deriving it from its priority; the engine now hands them the weight its
  priority convention (lower is more urgent) always meant, `1000 / (p + 1)`,
  so the dispatching order does not change. A negative priority now counts as
  0 for that weight; it used to produce an infinite or negative one.

## [1.2.0] - 2026-09-15

### Added

- The four what-if scenario types that were declared but refused since 1.1.0
  now do what their names say. `WhatIfScenario.AddJob` appends a copy of the
  job to the scenario request (a duplicate id is refused), `RemoveJob` drops
  the job and every dependency other operations had on its operations,
  `ChangeDueDate` sets the job's due date, and `ChangeSetupTime` scales the
  target resource's sequence-dependent setup matrix — default and entries —
  by a multiplier, and is refused for a resource without a matrix, since a
  scenario that changes nothing would report the baseline's numbers as its
  own. The scenario request now carries its own copy of the setup matrices so
  that scaling one never touches the baseline.
- `Operation.WithDependencies(params string[] operationIds)` — the builder
  the `Dependencies` list never had.

## [1.1.0] - 2026-09-10

### Added

- `samples/` — runnable CLI inputs, each small enough to check by hand, with the
  input schema documented alongside them. The README described a samples
  directory and three build scripts that did not exist; the scripts are gone
  from it and the samples now do.
- `Operation.WithWorkerCandidates(count, ids)` in the SDK, and
  `Operation::with_worker_candidates` in the engine — the worker counterpart of
  `WithEquipment`. Worker candidates could previously be set through JSON and
  FFI input but not through either builder, which is why the defect below was
  invisible to every test and visible only to callers outside the API.

### Fixed

- An operation that names worker candidates got them in the order they were
  written, not by when they were free. With two operators available, a second
  operation would take the one already busy and wait for them, serialising work
  that had idle capacity on both sides — a two-machine, two-operator cell ran at
  half its throughput. Equipment selection already chose the earliest-available
  candidate, and so did worker selection when no candidate list was given; only
  the listed-candidates path differed. Reachable through JSON and FFI input
  only: the builder API could not express worker candidates, which is why no
  test had ever covered that branch.
- A what-if scenario no longer starts from a different problem than its
  baseline. Building the scenario's copy of a request rebuilt each resource
  requirement through the builders, and the worker branch carried only the
  count across — so a worker-candidate restriction vanished and the load factor
  reset to 1.0. The comparison the whole feature exists to make was against a
  problem the caller never described. Requirements are now copied whole, with
  their own candidate lists rather than shared ones.
- The same clone dropped most of every object it copied, not only the worker
  requirements: it rebuilt each job, operation and resource through the
  builders, so only what a builder happens to carry survived — 6 of a job's 24
  fields, 5 of an operation's 26, 3 of a resource's 18. Dependencies, materials,
  time windows, transit times, capabilities, capacity and unavailable periods
  were all absent from the scenario, and each of them changes a schedule. The
  clone copies now instead of rebuilding, so a field added to a model cannot
  quietly fall out of it again.
- Four of the nine declared what-if scenario types — `AddJob`, `RemoveJob`,
  `ChangeDueDate`, `ChangeSetupTime` — had no implementation and fell straight
  through, so the scenario returned the baseline unchanged and was reported as
  a success. That is exactly what a caller sees when a real change turns out
  not to matter, and a what-if that cannot separate those two readings is worse
  than one that refuses. They now fail with a message naming the supported
  types. Implementing them is a separate piece of work; `ChangeSetupTime` in
  particular needs the setup matrices to be copied rather than shared.

## [1.0.3] - 2026-09-07

### Added

- `UAPS.CLI`'s first run downloads the native engine for the current platform,
  and that was written down nowhere. A .NET tool package is runtime-identifier
  neutral, so unlike the SDK package it cannot carry the engine binaries
  itself. The README now states this, names the cache directory, and gives two
  ways to avoid the download.

### Fixed

- The offline first-run path reported `A task was canceled` instead of the
  guidance written for it. `HttpClient` surfaces its own timeout as a
  cancellation rather than an `HttpRequestException`, so that case escaped the
  only `catch`. It is now caught and guarded on the caller's token, so a
  genuine caller-initiated cancellation still propagates unchanged.

### Changed

- The engine binary in these packages is rebuilt against `rand` 0.10 and now
  declares `rust-version = "1.87"`, verified by building on that toolchain. No
  public signature is added or removed, which is why this is a patch release.
- Assertions in the test suite move from `FluentAssertions` 7.0.0 to
  `AwesomeAssertions` 9.6.0. Version 8 of the former changed from Apache 2.0 to
  a proprietary licence; the community fork keeps an OSI licence and a path to
  current releases. Test-only — no shipped code is affected.
- Test tooling: `coverlet.collector` 10.0.1, `Microsoft.NET.Test.Sdk` 18.9.0,
  `xunit` 2.9.3, `xunit.runner.visualstudio` 4.0.0.
- CI installs the .NET SDK version the projects actually target. It had pinned
  9.0.x against `net10.0` projects and passed only because the runner image
  happens to also ship .NET 10.

## [1.0.2] - 2026-09-03

`UAPS.SDK` bundles the native engine binaries in the package under
`runtimes/{rid}/native/` instead of downloading them on first use. Release
distribution consolidated into this repository.

## [1.0.1] - 2026-09-03

## [1.0.0] - 2026-09-03
