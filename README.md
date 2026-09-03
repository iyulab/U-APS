# U-APS

**Advanced Production Scheduling** — Hybrid Rust/C# scheduling system

[![Release](https://img.shields.io/github/v/release/iyulab/U-APS)](https://github.com/iyulab/U-APS/releases)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Overview

U-APS is a production scheduling system that combines a Rust optimization engine with a C# SDK.

```
Input (JSON/Excel) → Scheduling Engine → Output (JSON/Excel)
```

## Architecture

```
┌──────────────────────────────────────────────┐
│                   U-APS                       │
├──────────────────────────────────────────────┤
│  APS Engine (Rust)                            │
│  ├── 7 scheduling algorithms                 │
│  ├── FFI interface (5 C-ABI functions)       │
│  └── Built on u-schedule + u-metaheur        │
├──────────────────────────────────────────────┤
│  UAPS.SDK (C#)          │  UAPS.CLI (C#)     │
│  ├── Fluent API          │  ├── JSON I/O      │
│  ├── SchedulerClient     │  └── Excel I/O     │
│  └── NativeLoader        │                    │
└──────────────────────────┴────────────────────┘
```

## Installation

### CLI (Recommended)

```bash
# Install as dotnet tool
dotnet tool install -g UAPS.CLI

# Usage
uaps input.json output.json
uaps input.xlsx output.xlsx

# With dispatching strategy
uaps input.json output.json --strategy SPT
uaps input.json output.json --strategy EDD --tie-breaker FIFO
```

Or download standalone binaries from [GitHub Releases](https://github.com/iyulab/U-APS/releases).

### SDK

```bash
dotnet add package UAPS.SDK
```

```csharp
using UAPS.SDK.Client;
using UAPS.SDK.Interop;
using UAPS.SDK.Models;

await NativeLoader.EnsureLoadedAsync();

var job = Job.Create("J1")
    .WithPriority(1)
    .WithDueDate(DateTime.Now.AddHours(8));

job.Operations.Add(
    Operation.Create("O1", "J1", 1)
        .WithTime(0, 60_000, 0)
        .WithEquipment("M1")
        .WithMaterial("MAT-001", 10.0)
);

var resource = Resource.CreateEquipment("M1", "Machine 1");

var client = new SchedulerClient();
var request = new ScheduleRequest
{
    Jobs = [job],
    Resources = [resource],
    DispatchingConfig = new DispatchingConfig
    {
        PrimaryRule = "EDD",
        TieBreaker = "SPT"
    }
};
var result = client.Schedule(request);

Console.WriteLine($"Makespan: {result.Schedule.MakespanMs}ms");
```

## Features

### Scheduling Algorithms

| Algorithm | Type | Best For |
|-----------|------|----------|
| **Simple** | Dispatching rule | Fast baseline, small problems |
| **GeneticAlgorithm** | Metaheuristic | Medium problems (30-200 ops) |
| **Production** | GA + extensions | Real manufacturing with setup/split/overlap |
| **Dynamic** | Time-fence | Rescheduling with frozen horizons |
| **CpSat** | Constraint Programming | Exact solutions for small problems |
| **Hybrid** | GA + SA | Large problems needing escape from local optima |
| **Auto** | Adaptive | Selects algorithm based on problem characteristics |

### Dispatching Rules

| Rule | Description | Best For |
|------|-------------|----------|
| FIFO | First In First Out | Fair scheduling |
| SPT | Shortest Processing Time | Minimize avg flow time |
| LPT | Longest Processing Time | Load balancing |
| EDD | Earliest Due Date | Minimize tardiness |
| SLACK | Minimum Slack Time | Urgent jobs |
| CR | Critical Ratio | Balance due dates |
| PRIORITY | Job Priority | Explicit prioritization |
| LWKR | Least Work Remaining | Quick completions |
| MWKR | Most Work Remaining | Complex jobs first |
| MOPNR | Most Operations Remaining | Complex jobs first |
| RANDOM | Random | Baseline comparison |

### Constraint Management

- **Resource Constraints**: Equipment, workers, calendars, shifts
- **Material Constraints**: BOM, stock levels, safety stock, lead times
- **Time Constraints**: Due dates, earliest start, time windows
- **Setup Optimization**: Setup matrices, campaign scheduling
- **Multi-Site Scheduling**: Site transitions with inter-site transit times

### Advanced Features

- **Skill Matrix**: Worker proficiency tracking with learning curves
- **Certification Matrix**: Worker certifications with expiry management
- **Crew Management**: Team assignments with shift schedules
- **Rescheduling Events**: 18 event types for dynamic adjustments
  - Equipment: Breakdown, Recovery, Maintenance, Efficiency change
  - Material: Shortage, Delay, Arrival
  - Worker: Unavailable, Available, Skill change
  - Quality: Defect, Inspection delay
  - Order: Delay, Urgent order, Cancellation, Due date/Quantity/Priority change

### Analytics

- **KPIs**: Makespan, utilization, tardiness, MCE
- **CTP (Capable-To-Promise)**: Delivery date promises
- **What-If Analysis**: Scenario comparison
- **Bottleneck Detection**: Resource utilization analysis

### FFI Interface

Five C-ABI functions for external consumption:

| Function | Purpose |
|----------|---------|
| `uaps_schedule()` | Schedule with algorithm selection + KPI + metrics |
| `uaps_validate()` | Pre-flight input validation |
| `uaps_reschedule()` | Event-driven rescheduling |
| `uaps_version()` | Engine version string |
| `uaps_free_string()` | Free allocated string memory |

## Building

### Requirements

- Rust 1.70+
- .NET 9.0+ SDK

### Commands

```powershell
# Full build
.\scripts\build.ps1 -Configuration Release

# Run tests
.\scripts\test.ps1

# Run samples
.\scripts\run-samples.ps1 -Report
```

### Individual Components

```powershell
# Rust Engine only
cd engine
cargo build --release
cargo test

# C# SDK only
cd sdk
dotnet build -c Release
dotnet test
```

## Project Structure

```
U-APS/
├── engine/                 # APS engine (Rust)
│   └── src/
│       ├── models/         # Job, Operation, Resource, Material, etc.
│       ├── scheduler/      # Scheduling algorithms, KPI, rescheduling
│       ├── ga/             # Genetic algorithm with dual-vector encoding
│       ├── cp/             # Constraint programming solver
│       ├── benchmark/      # Standard JSP/FJSSP instances
│       ├── validation/     # Input validation
│       └── ffi.rs          # C-ABI FFI interface
├── sdk/
│   ├── UAPS.SDK/           # C# SDK
│   │   ├── Models/         # Domain models
│   │   ├── Client/         # SchedulerClient
│   │   ├── Simulation/     # SimulationSession
│   │   └── Analytics/      # WhatIfSimulator
│   ├── UAPS.CLI/           # Command-line tool
│   └── UAPS.SDK.Tests/     # C# tests
├── samples/                # Example scenarios
└── docs/                   # Documentation
```

## Samples

| Sample | Description |
|--------|-------------|
| `001-simple` | Basic single job scheduling |
| `002-multi-job` | Multiple jobs with priorities |
| `003-due-dates` | Due date constraints and violations |
| `004-setup-matrix` | Setup time optimization |
| `005-multi-resource` | Multi-resource operations |
| `006-parallel-machines` | Parallel machine scheduling |
| `007-complex-flow` | Complex multi-job flow |

## Test Results

- **Engine Tests**: 373 passed (Rust)
- **FFI Tests**: 12 integration tests
- **SDK Tests**: 78 passed (C#)

## License

MIT License — see [LICENSE](LICENSE).

## Dependencies

- [u-schedule](https://github.com/iyulab/u-schedule) — Scheduling framework
- [u-metaheur](https://github.com/iyulab/u-metaheur) — Metaheuristic optimization
- [u-numflow](https://github.com/iyulab/u-numflow) — Mathematical primitives
