using System.Text.Json;
using UAPS.CLI.Excel;
using UAPS.CLI.Json;
using UAPS.SDK.Client;
using UAPS.SDK.Models;
using UAPS.SDK.Simulation;

namespace UAPS.CLI.Simulation;

/// <summary>
/// CLI 시뮬레이션 실행기
/// </summary>
public class SimulationRunner
{
    private SimulationSession? _session;
    private string _inputPath = string.Empty;

    /// <summary>
    /// 배치 시뮬레이션 실행
    /// </summary>
    public int RunBatch(string inputPath, string changesPath, string outputPath)
    {
        Console.WriteLine("=== U-APS Simulation (Batch Mode) ===");
        Console.WriteLine($"Input:   {inputPath}");
        Console.WriteLine($"Changes: {changesPath}");
        Console.WriteLine($"Output:  {outputPath}");
        Console.WriteLine();

        try
        {
            // 1. Load input
            var (jobs, resources) = LoadInput(inputPath);
            _session = SimulationSession.Create(jobs, resources);

            var asIs = _session.CurrentSchedule
                ?? throw new InvalidOperationException("Failed to create baseline schedule");

            Console.WriteLine("📊 AS-IS Schedule:");
            PrintScheduleSummary(asIs);
            Console.WriteLine();

            // 2. Apply changes
            var changes = LoadChanges(changesPath);
            ApplyChanges(changes);

            // 3. Reschedule
            Console.WriteLine("🔄 Rescheduling...");
            var toBe = _session.Reschedule();

            Console.WriteLine("📊 TO-BE Schedule:");
            PrintScheduleSummary(toBe);
            Console.WriteLine();

            // 4. Compare
            var comparison = _session.Compare(asIs, toBe);
            Console.WriteLine("📈 Comparison:");
            Console.WriteLine($"   {comparison.Summary}");
            Console.WriteLine();

            // 5. Export
            ExportComparison(outputPath, asIs, toBe, comparison, jobs);

            Console.WriteLine($"✅ Results saved to: {outputPath}");
            return 0;
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"❌ Error: {ex.Message}");
            return 1;
        }
    }

    /// <summary>
    /// 인터랙티브 시뮬레이션 실행
    /// </summary>
    public int RunInteractive(string inputPath)
    {
        Console.WriteLine("=== U-APS Simulation (Interactive Mode) ===");
        Console.WriteLine($"Input: {inputPath}");
        Console.WriteLine();

        try
        {
            _inputPath = inputPath;

            // Load input
            var (jobs, resources) = LoadInput(inputPath);
            _session = SimulationSession.Create(jobs, resources);

            var asIs = _session.CurrentSchedule;
            if (asIs != null)
            {
                Console.WriteLine("📊 AS-IS Schedule loaded:");
                PrintScheduleSummary(asIs);
            }

            Console.WriteLine();
            Console.WriteLine("Commands:");
            Console.WriteLine("  job <id> priority <n>     - Set job priority");
            Console.WriteLine("  job <id> due <datetime>   - Set job due date");
            Console.WriteLine("  op <id> process <mins>    - Set operation process time");
            Console.WriteLine("  op <id> setup <mins>      - Set operation setup time");
            Console.WriteLine("  res <id> disable          - Disable resource");
            Console.WriteLine("  res <id> enable           - Enable resource");
            Console.WriteLine("  schedule                  - Run rescheduling");
            Console.WriteLine("  compare                   - Compare AS-IS vs TO-BE");
            Console.WriteLine("  reset                     - Reset to AS-IS");
            Console.WriteLine("  export <path>             - Export results");
            Console.WriteLine("  list jobs                 - List all jobs");
            Console.WriteLine("  list ops                  - List all operations");
            Console.WriteLine("  list res                  - List all resources");
            Console.WriteLine("  quit                      - Exit");
            Console.WriteLine();

            while (true)
            {
                Console.Write("sim> ");
                var line = Console.ReadLine()?.Trim();

                if (string.IsNullOrEmpty(line))
                    continue;

                if (line.Equals("quit", StringComparison.OrdinalIgnoreCase) ||
                    line.Equals("exit", StringComparison.OrdinalIgnoreCase))
                {
                    Console.WriteLine("Goodbye!");
                    break;
                }

                try
                {
                    ProcessCommand(line);
                }
                catch (Exception ex)
                {
                    Console.WriteLine($"❌ {ex.Message}");
                }
            }

            return 0;
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"❌ Error: {ex.Message}");
            return 1;
        }
    }

    private void ProcessCommand(string line)
    {
        if (_session == null)
            throw new InvalidOperationException("Session not initialized");

        var parts = line.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length == 0) return;

        var cmd = parts[0].ToLower();

        switch (cmd)
        {
            case "job" when parts.Length >= 4:
                ProcessJobCommand(parts);
                break;

            case "op" when parts.Length >= 4:
                ProcessOperationCommand(parts);
                break;

            case "res" when parts.Length >= 3:
                ProcessResourceCommand(parts);
                break;

            case "schedule":
                var schedule = _session.Reschedule();
                Console.WriteLine("✅ Rescheduled:");
                PrintScheduleSummary(schedule);
                break;

            case "compare":
                if (_session.BaselineSchedule == null || _session.CurrentSchedule == null)
                {
                    Console.WriteLine("❌ Need both AS-IS and TO-BE schedules");
                    break;
                }
                var comparison = _session.CompareWithBaseline();
                PrintComparison(comparison);
                break;

            case "reset":
                _session.Reset();
                Console.WriteLine("✅ Reset to AS-IS");
                break;

            case "export" when parts.Length >= 2:
                ExportResults(parts[1]);
                break;

            case "list" when parts.Length >= 2:
                ProcessListCommand(parts[1]);
                break;

            case "help":
                ShowHelp();
                break;

            default:
                Console.WriteLine($"Unknown command: {cmd}. Type 'help' for commands.");
                break;
        }
    }

    private void ProcessJobCommand(string[] parts)
    {
        var jobId = parts[1];
        var action = parts[2].ToLower();
        var value = parts[3];

        switch (action)
        {
            case "priority":
                _session!.SetJobPriority(jobId, int.Parse(value));
                Console.WriteLine($"✅ Job {jobId} priority set to {value}");
                break;

            case "due":
                _session!.SetJobDueDate(jobId, DateTime.Parse(value));
                Console.WriteLine($"✅ Job {jobId} due date set to {value}");
                break;

            default:
                Console.WriteLine($"Unknown job action: {action}");
                break;
        }
    }

    private void ProcessOperationCommand(string[] parts)
    {
        var opId = parts[1];
        var action = parts[2].ToLower();
        var value = long.Parse(parts[3]) * 60000; // minutes to ms

        switch (action)
        {
            case "process":
                _session!.SetOperationTime(opId, processMs: value);
                Console.WriteLine($"✅ Operation {opId} process time set to {parts[3]} minutes");
                break;

            case "setup":
                _session!.SetOperationTime(opId, setupMs: value);
                Console.WriteLine($"✅ Operation {opId} setup time set to {parts[3]} minutes");
                break;

            case "wait":
                _session!.SetOperationTime(opId, waitMs: value);
                Console.WriteLine($"✅ Operation {opId} wait time set to {parts[3]} minutes");
                break;

            default:
                Console.WriteLine($"Unknown operation action: {action}");
                break;
        }
    }

    private void ProcessResourceCommand(string[] parts)
    {
        var resId = parts[1];
        var action = parts[2].ToLower();

        switch (action)
        {
            case "disable":
                _session!.DisableResource(resId);
                Console.WriteLine($"✅ Resource {resId} disabled");
                break;

            case "enable":
                _session!.EnableResource(resId);
                Console.WriteLine($"✅ Resource {resId} enabled");
                break;

            default:
                Console.WriteLine($"Unknown resource action: {action}");
                break;
        }
    }

    private void ProcessListCommand(string what)
    {
        switch (what.ToLower())
        {
            case "jobs":
                Console.WriteLine("Jobs:");
                foreach (var job in _session!.Jobs)
                {
                    Console.WriteLine($"  {job.Id}: {job.ProductName} (Priority: {job.Priority}, Ops: {job.OperationCount})");
                }
                break;

            case "ops":
            case "operations":
                Console.WriteLine("Operations:");
                foreach (var job in _session!.Jobs)
                {
                    foreach (var op in job.Operations)
                    {
                        Console.WriteLine($"  {op.Id}: {op.Name} (Process: {op.Time.ProcessMs / 60000}min)");
                    }
                }
                break;

            case "res":
            case "resources":
                Console.WriteLine("Resources:");
                foreach (var res in _session!.Resources)
                {
                    Console.WriteLine($"  {res.Id}: {res.Name} (Eff: {res.Efficiency:P0})");
                }
                break;

            default:
                Console.WriteLine($"Unknown list type: {what}");
                break;
        }
    }

    private void ShowHelp()
    {
        Console.WriteLine("Commands:");
        Console.WriteLine("  job <id> priority <n>     - Set job priority (1=highest)");
        Console.WriteLine("  job <id> due <datetime>   - Set job due date");
        Console.WriteLine("  op <id> process <mins>    - Set operation process time");
        Console.WriteLine("  op <id> setup <mins>      - Set operation setup time");
        Console.WriteLine("  op <id> wait <mins>       - Set operation wait time");
        Console.WriteLine("  res <id> disable          - Disable resource (simulate breakdown)");
        Console.WriteLine("  res <id> enable           - Enable resource");
        Console.WriteLine("  schedule                  - Run rescheduling (TO-BE)");
        Console.WriteLine("  compare                   - Compare AS-IS vs TO-BE");
        Console.WriteLine("  reset                     - Reset to AS-IS state");
        Console.WriteLine("  export <path>             - Export comparison to file");
        Console.WriteLine("  list jobs                 - List all jobs");
        Console.WriteLine("  list ops                  - List all operations");
        Console.WriteLine("  list res                  - List all resources");
        Console.WriteLine("  quit                      - Exit simulation");
    }

    private void ExportResults(string outputPath)
    {
        if (_session?.BaselineSchedule == null || _session.CurrentSchedule == null)
        {
            Console.WriteLine("❌ Need both AS-IS and TO-BE schedules to export");
            return;
        }

        var comparison = _session.CompareWithBaseline();
        ExportComparison(outputPath, _session.BaselineSchedule, _session.CurrentSchedule,
            comparison, _session.Jobs.ToList());

        Console.WriteLine($"✅ Exported to: {outputPath}");
    }

    private (List<Job>, List<Resource>) LoadInput(string path)
    {
        Console.WriteLine("Loading input file...");
        var ext = Path.GetExtension(path).ToLowerInvariant();

        if (ext == ".json")
        {
            var parser = new JsonInputParser();
            return parser.Parse(path);
        }
        else
        {
            var parser = new ExcelInputParser();
            return parser.Parse(path);
        }
    }

    private SimulationChanges LoadChanges(string path)
    {
        var json = File.ReadAllText(path);
        return JsonSerializer.Deserialize<SimulationChanges>(json,
            new JsonSerializerOptions { PropertyNameCaseInsensitive = true })
            ?? throw new InvalidOperationException("Failed to parse changes file");
    }

    private void ApplyChanges(SimulationChanges changes)
    {
        Console.WriteLine($"Applying {changes.Modifications.Count} modifications...");

        foreach (var mod in changes.Modifications)
        {
            switch (mod.Type.ToLower())
            {
                case "job":
                    if (mod.Changes.TryGetValue("priority", out var priority))
                        _session!.SetJobPriority(mod.Id, GetInt(priority));
                    if (mod.Changes.TryGetValue("dueDate", out var dueDate))
                        _session!.SetJobDueDate(mod.Id, DateTime.Parse(GetString(dueDate)));
                    break;

                case "operation":
                    if (mod.Changes.TryGetValue("processMs", out var processMs))
                        _session!.SetOperationTime(mod.Id, processMs: GetLong(processMs));
                    if (mod.Changes.TryGetValue("setupMs", out var setupMs))
                        _session!.SetOperationTime(mod.Id, setupMs: GetLong(setupMs));
                    break;

                case "resource":
                    var action = mod.Action?.ToLower();
                    if (action == "disable")
                        _session!.DisableResource(mod.Id);
                    else if (action == "enable")
                        _session!.EnableResource(mod.Id);
                    break;
            }

            Console.WriteLine($"  ✓ {mod.Type} {mod.Id}: {mod.Action ?? string.Join(", ", mod.Changes.Keys)}");
        }
    }

    private static int GetInt(object value)
    {
        if (value is JsonElement je)
            return je.GetInt32();
        return Convert.ToInt32(value);
    }

    private static long GetLong(object value)
    {
        if (value is JsonElement je)
            return je.GetInt64();
        return Convert.ToInt64(value);
    }

    private static string GetString(object value)
    {
        if (value is JsonElement je)
            return je.GetString() ?? string.Empty;
        return value.ToString() ?? string.Empty;
    }

    private void PrintScheduleSummary(Schedule schedule)
    {
        Console.WriteLine($"   Makespan:    {schedule.MakespanMs / 60000.0:F1} minutes");
        Console.WriteLine($"   Assignments: {schedule.Assignments.Count}");
        Console.WriteLine($"   Violations:  {schedule.Violations.Count}");
    }

    private void PrintComparison(ScheduleComparison comparison)
    {
        Console.WriteLine("📈 AS-IS vs TO-BE Comparison:");
        Console.WriteLine($"   {comparison.Summary}");
        Console.WriteLine();

        if (comparison.AffectedOperations.Count > 0)
        {
            Console.WriteLine("   Affected Operations:");
            foreach (var diff in comparison.AffectedOperations.Take(10))
            {
                var delta = diff.StartDeltaMinutes >= 0 ? $"+{diff.StartDeltaMinutes:F0}" : $"{diff.StartDeltaMinutes:F0}";
                var resChange = diff.ResourceChanged ? $" → {diff.ToResource}" : "";
                Console.WriteLine($"     {diff.OperationId}: {delta}min{resChange}");
            }

            if (comparison.AffectedOperations.Count > 10)
            {
                Console.WriteLine($"     ... and {comparison.AffectedOperations.Count - 10} more");
            }
        }
    }

    private void ExportComparison(string outputPath, Schedule asIs, Schedule toBe,
        ScheduleComparison comparison, List<Job> jobs)
    {
        var ext = Path.GetExtension(outputPath).ToLowerInvariant();

        if (ext == ".json")
        {
            ExportComparisonJson(outputPath, asIs, toBe, comparison);
        }
        else
        {
            ExportComparisonExcel(outputPath, asIs, toBe, comparison, jobs);
        }
    }

    private void ExportComparisonJson(string outputPath, Schedule asIs, Schedule toBe,
        ScheduleComparison comparison)
    {
        var result = new
        {
            asIs = new
            {
                makespanMs = asIs.MakespanMs,
                makespanMinutes = asIs.MakespanMs / 60000.0,
                assignments = asIs.Assignments.Count,
                violations = asIs.Violations.Count
            },
            toBe = new
            {
                makespanMs = toBe.MakespanMs,
                makespanMinutes = toBe.MakespanMs / 60000.0,
                assignments = toBe.Assignments.Count,
                violations = toBe.Violations.Count
            },
            comparison = new
            {
                makespanDeltaMs = comparison.MakespanDelta,
                makespanDeltaMinutes = comparison.MakespanDeltaMinutes,
                violationsDelta = comparison.ViolationsDelta,
                affectedOperations = comparison.AffectedOperations.Select(d => new
                {
                    operationId = d.OperationId,
                    changeType = d.ChangeType.ToString(),
                    startDeltaMinutes = d.StartDeltaMinutes,
                    endDeltaMinutes = d.EndDeltaMinutes,
                    resourceChanged = d.ResourceChanged,
                    fromResource = d.FromResource,
                    toResource = d.ToResource
                })
            }
        };

        var json = JsonSerializer.Serialize(result, new JsonSerializerOptions { WriteIndented = true });
        File.WriteAllText(outputPath, json);
    }

    private void ExportComparisonExcel(string outputPath, Schedule asIs, Schedule toBe,
        ScheduleComparison comparison, List<Job> jobs)
    {
        using var workbook = new ClosedXML.Excel.XLWorkbook();

        // Sheet 1: KPI 비교
        var kpiSheet = workbook.Worksheets.Add("KPI비교");
        kpiSheet.Cell(1, 1).Value = "지표";
        kpiSheet.Cell(1, 2).Value = "AS-IS";
        kpiSheet.Cell(1, 3).Value = "TO-BE";
        kpiSheet.Cell(1, 4).Value = "변화";

        kpiSheet.Cell(2, 1).Value = "Makespan (분)";
        kpiSheet.Cell(2, 2).Value = asIs.MakespanMs / 60000.0;
        kpiSheet.Cell(2, 3).Value = toBe.MakespanMs / 60000.0;
        kpiSheet.Cell(2, 4).Value = comparison.MakespanDeltaMinutes;

        kpiSheet.Cell(3, 1).Value = "할당 수";
        kpiSheet.Cell(3, 2).Value = asIs.Assignments.Count;
        kpiSheet.Cell(3, 3).Value = toBe.Assignments.Count;
        kpiSheet.Cell(3, 4).Value = toBe.Assignments.Count - asIs.Assignments.Count;

        kpiSheet.Cell(4, 1).Value = "위반 수";
        kpiSheet.Cell(4, 2).Value = asIs.Violations.Count;
        kpiSheet.Cell(4, 3).Value = toBe.Violations.Count;
        kpiSheet.Cell(4, 4).Value = comparison.ViolationsDelta;

        kpiSheet.Columns().AdjustToContents();

        // Sheet 2: 스케줄 비교
        var compSheet = workbook.Worksheets.Add("스케줄비교");
        compSheet.Cell(1, 1).Value = "JobId";
        compSheet.Cell(1, 2).Value = "OperationId";
        compSheet.Cell(1, 3).Value = "AS-IS 시작(분)";
        compSheet.Cell(1, 4).Value = "AS-IS 종료(분)";
        compSheet.Cell(1, 5).Value = "AS-IS 설비";
        compSheet.Cell(1, 6).Value = "TO-BE 시작(분)";
        compSheet.Cell(1, 7).Value = "TO-BE 종료(분)";
        compSheet.Cell(1, 8).Value = "TO-BE 설비";
        compSheet.Cell(1, 9).Value = "시작 변화(분)";
        compSheet.Cell(1, 10).Value = "설비 변경";

        var asIsDict = asIs.Assignments.ToDictionary(a => a.OperationId);
        var toBeDict = toBe.Assignments.ToDictionary(a => a.OperationId);

        var row = 2;
        foreach (var job in jobs)
        {
            foreach (var op in job.Operations.OrderBy(o => o.Sequence))
            {
                compSheet.Cell(row, 1).Value = job.Id;
                compSheet.Cell(row, 2).Value = op.Id;

                if (asIsDict.TryGetValue(op.Id, out var asIsA))
                {
                    compSheet.Cell(row, 3).Value = asIsA.StartMs / 60000.0;
                    compSheet.Cell(row, 4).Value = asIsA.EndMs / 60000.0;
                    compSheet.Cell(row, 5).Value = asIsA.ResourceId;
                }

                if (toBeDict.TryGetValue(op.Id, out var toBeA))
                {
                    compSheet.Cell(row, 6).Value = toBeA.StartMs / 60000.0;
                    compSheet.Cell(row, 7).Value = toBeA.EndMs / 60000.0;
                    compSheet.Cell(row, 8).Value = toBeA.ResourceId;
                }

                if (asIsA != null && toBeA != null)
                {
                    compSheet.Cell(row, 9).Value = (toBeA.StartMs - asIsA.StartMs) / 60000.0;
                    compSheet.Cell(row, 10).Value = asIsA.ResourceId != toBeA.ResourceId ? "Y" : "";
                }

                row++;
            }
        }

        compSheet.Columns().AdjustToContents();

        workbook.SaveAs(outputPath);
    }
}

/// <summary>
/// 시뮬레이션 변경사항 (배치 모드용)
/// </summary>
public class SimulationChanges
{
    public List<SimulationModification> Modifications { get; set; } = [];
}

public class SimulationModification
{
    public string Type { get; set; } = string.Empty;  // job, operation, resource
    public string Id { get; set; } = string.Empty;
    public string? Action { get; set; }  // disable, enable (for resources)
    public Dictionary<string, object> Changes { get; set; } = [];
}
