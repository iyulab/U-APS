# UAPS.SDK

.NET client for the U-APS production scheduling engine.

The native engine is bundled in this package under `runtimes/{rid}/native/` for
every supported platform, so referencing the package is enough — nothing is
downloaded at run time.

## Install

```bash
dotnet add package UAPS.SDK
```

## Quick start

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

var client = new SchedulerClient();
var result = client.Schedule(new ScheduleRequest
{
    Jobs = [job],
    Resources = [Resource.CreateEquipment("M1", "Machine 1")],
    DispatchingConfig = new DispatchingConfig
    {
        PrimaryRule = "EDD",
        TieBreaker = "SPT",
    },
});

Console.WriteLine($"Makespan: {result.Schedule.MakespanMs}ms");
```

## Algorithms

| Algorithm | Type | Best for |
|---|---|---|
| `Simple` | Dispatching rule | Fast baseline, small problems |
| `GeneticAlgorithm` | Search over the dispatching order | Better than `Simple` when there is time to search |
| `Production` | Search (as `GeneticAlgorithm`) | Its own search budget |
| `Dynamic` | Search (as `GeneticAlgorithm`) | Its own search budget |
| `CpSat` | Constraint programming | Exact solutions for small problems |
| `Hybrid` | `CpSat`, then `GeneticAlgorithm` | Keeps the schedule that breaks the request less |
| `Auto` | Adaptive | By size: `CpSat` only for a plain job shop, else the search |

Dispatching rules: `FIFO`, `SPT`, `LPT`, `EDD`, `SLACK`, `CR`, `PRIORITY`,
`LWKR`, `MWKR`, `MOPNR`, `RANDOM`.

## Requirements

- .NET 10.0 or later
- Windows x64, Linux x64, macOS x64, or macOS arm64

## Links

- [Repository and documentation](https://github.com/iyulab/U-APS)
- [Changelog](https://github.com/iyulab/U-APS/blob/main/CHANGELOG.md)
- Command line tool: `UAPS.CLI`

Licensed under MIT.
