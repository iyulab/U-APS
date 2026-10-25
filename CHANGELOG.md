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

## [1.8.0] - 2026-10-25

### Added

- Engine calendars read shifts, breaks and holidays on a time zone's wall
  clock: `Calendar.utc_offsets` takes the zone as a schedule of offsets from
  UTC (a base offset and the instants it changes, for daylight saving). An
  empty schedule is UTC, so calendars that name no zone schedule as before.
  Holidays are dates — the calendar date of each value — compared with the
  local date. Crew shift assignments and outsourcing working days still read
  UTC.
- SDK: `Calendar.TimeZone` / `Calendar.WithTimeZone(TimeZoneInfo | string)`
  (an IANA name such as `Asia/Seoul`) — the request carries the zone to the
  engine as `UtcOffsetsDto`, laid out from a day before `StartTimeMs` to 367
  days after. `Calendar.IsHoliday` compares the date on the calendar's wall
  clock. `Resource.WithCalendar(id)`.
- Calendar date exceptions — overtime days, swapped holidays and closures:
  `Calendar.exceptions` (engine) lists dates that override the weekday rule
  and the holidays. A closed date starts no shift; a working date starts the
  shifts it names, or that weekday's shifts when it names none — even on a
  holiday. To work on a weekday that normally has no shift, name the shifts.
  Calendars without the list read as before. SDK: `CalendarException.Closed`
  / `CalendarException.WorkingDay(date, shifts…)` and
  `Calendar.WithException`; the request omits the list when it is empty.
- Breaks per shift — a day shift's lunch, a night shift's meal break:
  `Shift.breaks` (engine) holds breaks of that shift only, while
  `Calendar.breaks` still apply to every shift. A break after midnight in an
  overnight shift belongs to the shift that started the day before, and a
  shift's break does not stop work while another shift is on. Shifts without
  the list read as before. SDK: `Shift.Breaks` / `Shift.WithBreak`; the
  request omits the list when it is empty. `WorkingMinutesPerDay` subtracts
  them.

### Changed

- A calendar-wide break whose end is earlier than its start now spans
  midnight (23:50–00:10) instead of matching no minute.

### Deprecated

- `UAPS.SDK.Handlers` — `ICalendarHandler`, `RecurrenceHandler`, `ShiftHandler`,
  `CalendarPattern`, `BreakPeriod`, `ShiftDefinition`, `DateRange` and `TimeWindow`.
  Nothing in the SDK or the engine reads them: calendars reach the engine through
  `UAPS.SDK.Models.Calendar`, which now carries shifts, per-shift breaks, date
  exceptions and a time zone. The handlers expand dates on the machine's local time
  zone, so the same pattern gives different windows on differently configured
  machines. They are kept for compatibility only; move to
  `Calendar`.

### Fixed

- Build: the repository declares its own test runner (`global.json` — VSTest), so
  `dotnet test` on the SDK tests runs the same way from any checkout. Before, a
  `global.json` in a parent directory that chose Microsoft.Testing.Platform
  stopped the run with «All projects must use that test runner».
- SDK: `Resource.CalendarId` reaches the engine. It was accepted and never
  sent, so a resource with a calendar was scheduled as always available.
- The part of an overnight shift after midnight belongs to the day the shift
  started: a Friday 22:00–06:00 shift now covers Saturday 00:00–06:00, and
  does not cover Friday 00:00–06:00 unless a Thursday shift does. A shift that
  starts on a holiday contributes no hours after midnight either.
- A working window of an overnight shift ends at a break that starts after
  midnight, instead of running through it.
- The next working time is found across holidays of up to a month; a calendar
  closed for more than a week used to return the starting time unchanged.

## [1.7.0] - 2026-10-20

### Added

- `Job.IsSplittable` is applied: every operation of the job may be split, as
  with `Operation.IsSplittable` (it was accepted and ignored).
- Each assignment of a split operation carries `part` — which part
  (`index`, from 0) and how much of the processing it does (`work_ms`); the
  parts' `work_ms` add up to the operation's processing time. Assignments of
  an operation that runs whole carry none.

### Changed

- The engine builds on u-schedule 0.30, and every algorithm that places by
  the default scheduler (`Simple`, `GeneticAlgorithm`, `Production`,
  `Dynamic`) now applies lot splitting and overlap with everything else it
  keeps — workers, calendars, release times, set-ups. A request that splits
  or overlaps operations no longer goes to the previous genetic algorithm
  (one machine per operation, no workers, calendars or release times), and
  its warning is gone.
- Lot splitting chooses how many parts (1 to `SplitConfig.max_splits`, each
  at least `min_split_size_ms` of work) by when the operation ends: the work
  is shared so the parts end together, and an operation runs whole when that
  ends no later. It was always split as many ways as allowed, evenly, even
  when that ended later.
- An overlap (`OverlapConfig.min_predecessor_completion` on an operation)
  lets the next operation start once this one is that far along, by the time
  it actually takes, and the next operation no longer ends before this one.
- Tasks of a split operation are numbered across its parts, each on its
  part's equipment. A part was reported as an assignment of an operation
  `<id>_split_<n>` that the request does not have.

### Fixed

Rescheduling and what-if scenarios handle a split operation (one assignment
per part, same `operation_id`) as one operation: a delay shifts every part
rather than the first, an operation is listed and counted once, a part on a
frozen stretch of the time fence keeps the whole operation in place, the
reported delay change compares each part with itself, and a processing-time
change scales each part's `work_ms` along with its duration.

### Removed

The previous genetic scheduler (`GaScheduler`, its population and operator
types, `Operation::create_splits`) is gone — no algorithm reaches it any more.
`GaParams` stays as the parameters of the search over dispatching orders.
`tournament_size` and `convergence_threshold` (never read by that search) are
no longer fields of it; a request that still sends them is accepted. The
benchmark runner measures `SimpleScheduler::search`, and its result no longer
reports `generations`, `fitness` or `converged`.

### Known limitations

`OverlapConfig.transfer_batch_size` is not read (there is no quantity model).
`CpSat` states neither splitting nor overlap and says so in its warning.
Scheduling an operation that may be split tries several part counts, each
with the parts placed one after another: with splitting allowed on every
operation of a request of hundreds of jobs, one schedule takes tens of times
as long, and `GeneticAlgorithm`, `Production` and `Dynamic` — which consider
many schedules within their time limit — get through fewer generations. Allow
splitting only where it pays, or lower `ga_params.max_generations`.

## [1.6.0] - 2026-10-18

### Fixed

- **`Operation.Dependencies` (`WithDependencies`) is applied.** An operation
  starts once every operation it names has ended, in any job, with the
  default scheduler and the genetic algorithm; the other algorithms — and the
  genetic algorithm on a request with splitting or overlap, which still goes
  to the previous one — report an operation that starts too early as a
  `Precedence` violation. A
  dependency on an operation the request does not have, or a loop of
  dependencies and job order that would keep operations from ever starting,
  is refused for every algorithm (`success: false`).
- **A crew works only the equipment it is assigned (`Crew.WithEquipment`).**
  An operation done by that crew runs on the equipment both the operation
  and the crew name; with no equipment of its own named, on the crew's. If
  the crew is assigned none of the operation's equipment, the operation is
  scheduled without equipment and reported (`UnfilledRequirement`), as for a
  target site that keeps none of it.
- A crew assigned operation types (`AssignedOperationTypes`) no longer takes
  an operation of another type: the operation keeps its own worker
  requirements, and the mismatch is reported (`UnfilledRequirement`).

### Changed

- **`Production` and `Dynamic` now search the default scheduler's order, as
  `GeneticAlgorithm` does** (each within its own budget:
  `ProductionConfig.ga_params`, `DynamicSchedulerConfig`'s), so their
  schedules keep worker requirements, calendars, release times and
  everything else the default scheduler keeps. Both ran the previous genetic
  algorithm, which holds one machine per operation: on 100 random requests
  78 of their schedules broke the request (reported since 1.5.0); on 30 now,
  none. Production's campaign grouping never reached the schedule (the
  genetic algorithm re-sorted the jobs) and still does not; Dynamic's shift
  of each assignment to its pegged material's arrival moved assignments past
  each other and their jobs' order, and is replaced by the next item. A
  request that splits or overlaps operations still goes to the previous
  genetic algorithm with any of the three, with the same warning.
- **`Auto` picks `CpSat` only for a request its model states whole** — a
  plain job shop: one machine per operation, processing times only, no
  workers, calendars, release times, set-ups or other extensions. A small
  request with any of them goes to `Production` (the search). `Auto` used to
  pick `CpSat` for every request of up to 50 operations × resources, and its
  schedule broke whatever else the request asked. `Hybrid` likewise runs its
  `CpSat` phase only for such a request, and is then the genetic algorithm
  alone (no longer spending its time limit on a phase whose schedule could
  not be kept). `CpSat` chosen by name says in a warning which of the
  request's features its schedule does not keep.
- **A pegging engine's material availability is a release time for every
  algorithm**: an operation waits for its pegged materials, as for a
  material manager's arrival. Only `Dynamic` read it before, and only after
  placing.
- The genetic algorithm, `Production` and `Dynamic` no longer log that they
  ignore calendars, the skill matrix, materials, crews and the other
  extension fields: they have read them since they searched the default
  scheduler's order.
- **A schedule's tasks are now the stretches each operation actually works
  in.** An operation pauses wherever any resource it holds, or its daily
  work window, does not work, and its assignment runs from its start to its
  end over those pauses; its tasks used to be worked out again from the
  first resource's calendar alone, so they could disagree with the
  assignment — a later start, hours past the end, or work through another
  resource's pause, and an operation with `IsSplittable` false was reported
  as one task in a later unbroken stretch while its assignment paused over
  a break. The set-up is in the first tasks, as before.
- `Operation.IsSplittable` is documented as what it does: it allows lot
  splitting with `SplitConfig` (applied by the previous genetic algorithm).
  Every operation pauses outside working time whatever its value, as every
  assignment already did.

### Removed

- The engine's `scheduler::task_splitter` module, which nothing used any
  more.
- From the engine's Rust API, what no algorithm used any more: the
  `ProductionScheduler` and `DynamicScheduler` (with their result types),
  the campaign planner (`scheduler::campaign`, and `ProductionConfig`'s
  `campaign_config`, `enable_parallel` and `verbose`, which did not change a
  schedule), `DynamicSchedulerConfig`'s `fence_config`,
  `enable_material_constraints` and `default_reschedule_strategy` (read only
  by methods no algorithm called; rescheduling is the `Rescheduler`'s), the
  `ga::hybrid_sa`, `ga::nsga2` and `ga::auto_tuner` modules (used by no
  algorithm, only by benchmarks), and `cp::ga_solution_to_cp_initial`. The
  SDK and CLI did not reach any of them.

### Known limitations

`Job.IsSplittable` is still accepted and not applied (operations are split
per operation, `Operation.IsSplittable`).

## [1.5.0] - 2026-10-18

### Changed

- **`GeneticAlgorithm` now searches the order the default scheduler takes
  the jobs in** (a biased random-key genetic algorithm whose every candidate
  is a default-scheduler run), so its schedules keep everything the default
  scheduler keeps — worker requirements, calendars, release times, set-up
  matrices, linked resources, sites, materials, outsourcing — and are never
  worse than the default scheduler's. The cost is makespan plus a tenth of
  the total tardiness. `GaParams` maps to the search as population size,
  generations, elite ratio, stall generations (`convergence_generations`)
  and time limit; `tournament_size` and `convergence_threshold` are not
  used. Each candidate is a whole scheduling run, so a search without a time
  limit stops after 30 seconds (`DEFAULT_SEARCH_TIME_LIMIT_MS`). A request
  with operation splitting (`IsSplittable`) or overlap still goes to the
  previous genetic algorithm, which logs a warning that it does not apply
  worker requirements, calendars or release times.

- **The previous dispatching scheduler is gone.** Every request the default
  scheduler takes is placed on u-schedule; nothing falls back any more. In
  the engine's Rust API `SimpleScheduler::schedule` returns
  `Result<Schedule, ScheduleError>`: `Refused` when the request is malformed
  (the FFI answers `success: false`, as for other scheduling errors),
  `Broken` for a defect in the engine. A job without operations is skipped,
  as before.

- The default scheduler's new path now takes requests with operations
  without resources, daily work windows, `IsSplittable`, `SplitConfig`,
  `OverlapConfig` or `TimeWindow` (they used to go to the previous
  scheduler whole). An operation without resources still runs on the
  request's first calendar and is reported on `NONE`. A daily work window
  holds an operation to its hours — past midnight when it ends before it
  begins — and its work pauses outside them and resumes, as it does over a
  calendar's breaks. `SplitConfig`, `OverlapConfig` and an operation's
  `TimeWindow` are, as before, not applied by the default scheduler
  (the genetic algorithm applies splitting and overlap).
- Outsourced operations and requests with an outsourcing manager or a
  pegging engine are scheduled on the new path too, so every request the
  default scheduler takes now goes there. An outsourced operation is booked,
  as before, on `OUTSOURCE:<provider>` for its lead time (its own, else the
  provider's default, else its processing time) from when its job reaches
  it, without calendars and without tasks. The pegging engine is, as before,
  not read by the default scheduler.
- A job whose target site keeps none of an operation's equipment is
  scheduled on the new path too. The operation still runs without that
  equipment, as before — and the schedule now says so: a violation of the
  new type `UnfilledRequirement` names the operation, the equipment and the
  site.
- An operation that cannot be scheduled at all on the new path — its
  resources and its own hours never work together (a night work window on
  day-shift equipment) — is left out of the schedule and reported as an
  `UnfilledRequirement` violation; the rest of its job is scheduled without
  it. Before, the request went to the previous scheduler, which booked such
  an operation for no time or across a day it could not work in.

### Fixed

- The other algorithms (`Production`, `Dynamic`, `CpSat`, `Hybrid`, and
  `GeneticAlgorithm` on a request that splits or overlaps operations) do not
  read the whole request, and their schedules said nothing about what they
  left out. Each schedule is now judged by the default scheduler's check and
  says where it breaks the request: a violation per break, as
  `UnfilledRequirement` (a worker or other requirement not booked, or an
  operation not scheduled), `EarliestStart`, `Precedence`, `Capacity`, or the
  new `WorkingTime` (outside a calendar or work window) and `Duration`
  (shorter than its time). On 100 random requests, 78 of the production and
  dynamic schedulers' schedules break the request, all of them now saying so.
- `Hybrid` could return an empty schedule: when CP-SAT found nothing in its
  time limit, its makespan of 0 beat the GA's. It now takes the schedule
  that breaks the request less, then the shorter, and its GA phase is the
  `GeneticAlgorithm` search.
- `GeneticAlgorithm` no longer leaves workers out and starts jobs before
  their earliest start: on 100 random requests mixing alternatives,
  efficiencies, workers, set-ups, priorities, due dates and release times,
  79 of its schedules did one or both (no violation was reported). None of
  the new ones does, and their makespan is 6 % shorter than the default
  scheduler's on average.
- An operation without resources that runs into a break no longer starts
  over after it: it pauses and resumes, as operations with resources always
  have. Before, the work done before the break was thrown away while the
  reported start stayed before it, so the operation and its job ended up to
  the length of that work later.
- A request whose operation has a daily work window that never meets its
  equipment's calendar (night hours on day-shift equipment) no longer hangs
  the engine: splitting its work into tasks looked for working time without
  end.
- An operation with a daily work window past midnight that cannot finish
  before the window closes resumes in the next window. Before, the previous
  scheduler could book its equipment from its start until a window more than
  a day later.

## [1.4.0] - 2026-10-18

### Changed

- **The default scheduler (a request without `algorithm`, or with
  `"Simple"`) now places operations with the u-schedule dispatcher, so its
  schedules change.** The
  request is stated in u-schedule's terms — operations with their resource
  requirements, equipment efficiency and worker proficiency as speeds, crews,
  worker load factors as shares of a worker, linked resources, set-up
  matrices, site transitions as transport times between equipment, shift
  calendars, rule switching (`DispatchingConfig.MultiLayer`) decision by
  decision, each operation's materials — scheduled there, and read back into the same response: one
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
    rules, rule switching, materials and calendars, 42 of its schedules held a resource
    beyond its capacity. None of the new ones does.
  - On a resource with a set-up matrix, the matrix's set-up replaces the
    operation's own set-up time instead of the larger of the two being taken.
  - On those 300 requests the makespan is shorter in 135, longer in 23 and
    the same in 142, and the total lateness against due dates is 14 % lower.
  A request using what the new path does not state yet — operation splitting
  or overlap, operation time windows or daily work windows, outsourcing,
  pegging, an operation without resources, a target site
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
