# Samples

Runnable inputs for the CLI. Each one is small enough to check by hand, which is
the point: if the scheduler's answer surprises you, the arithmetic is right here.

```powershell
dotnet tool install -g UAPS.CLI      # or build sdk/UAPS.CLI yourself
uaps samples/001-single-job.json out.json
```

| Sample | What it shows |
|---|---|
| `001-single-job.json` | The smallest input the scheduler accepts. One job, three operations in sequence, a dedicated machine for each — so the makespan is exactly the sum of the setup and process times, and any other answer is a bug. |
| `002-competing-jobs.json` | Three jobs contending for one lathe. The machine is the constraint, so every dispatching rule produces the same makespan; what changes is *which* job finishes late. Run it twice with `--rule SPT` and `--rule EDD` and compare the per-job tardiness in the output, not the makespan. |
| `003-equipment-and-worker.json` | An operation needing a machine *and* an operator at once, with two of each available. Both jobs should start at time zero: there is idle capacity on both sides. If they run one after the other, the scheduler reused one operator instead of taking the free one. |

Times are in milliseconds. `dueDateMs` is measured from the epoch, and
`options.startTimeMs` sets where the schedule begins — both are zero-based here,
so a due date of `3600000` means "one hour after the schedule starts".

## Input shape

```jsonc
{
  "jobs": [{
    "id": "JOB-1",
    "productName": "Bracket",     // optional
    "priority": 100,              // default 100
    "quantity": 1,                // default 1
    "dueDateMs": 3600000,         // or "dueDate": "1970-01-01T01:00:00Z"
    "operations": [{
      "id": "OP-1",
      "sequence": 1,              // operations run in this order within a job
      "setupMs": 300000,
      "processMs": 600000,
      "waitMs": 0,
      "requiredResources": [
        { "resourceType": "Equipment", "quantity": 1, "candidates": ["CUT-1"] }
      ]
    }]
  }],
  "resources": [
    { "id": "CUT-1", "type": "Equipment", "capability": "cutting", "efficiency": 1.0 }
  ],
  "options": { "startTimeMs": 0 }
}
```

`resourceType` and `type` are `Equipment` or `Worker`. An empty or absent
`candidates` list means any resource of that type will do.
