using System.Text.Json;
using System.Text.Json.Serialization;
using UAPS.SDK.Client;
using UAPS.SDK.Models;

namespace UAPS.CLI.Json;

/// <summary>
/// JSON output file writer
/// </summary>
public class JsonOutputWriter
{
    private static readonly JsonSerializerOptions Options = new()
    {
        WriteIndented = true,
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        Converters = { new JsonStringEnumConverter() }
    };

    /// <summary>
    /// Write schedule result to JSON file
    /// </summary>
    public void Write(Schedule schedule, List<Job> jobs, string filePath)
    {
        var output = new ScheduleOutput
        {
            Summary = new ScheduleSummary
            {
                MakespanMs = schedule.MakespanMs,
                MakespanMinutes = schedule.MakespanMs / 60000.0,
                TotalAssignments = schedule.Assignments.Count,
                TotalViolations = schedule.Violations.Count,
                IsOnTime = schedule.IsOnTime()
            },
            Assignments = schedule.Assignments.Select(a => new AssignmentOutput
            {
                OperationId = a.OperationId,
                ResourceId = a.ResourceId,
                StartMs = a.StartMs,
                EndMs = a.EndMs,
                DurationMs = a.EndMs - a.StartMs,
                StartTime = DateTimeOffset.FromUnixTimeMilliseconds(a.StartMs).DateTime,
                EndTime = DateTimeOffset.FromUnixTimeMilliseconds(a.EndMs).DateTime
            }).ToList(),
            Violations = schedule.Violations.Select(v => new ViolationOutput
            {
                ConstraintType = v.ConstraintType,
                TargetId = v.TargetId,
                Amount = v.Amount,
                Description = v.Description
            }).ToList(),
            JobSummaries = jobs.Select(j => CreateJobSummary(j, schedule)).ToList(),
            GanttData = CreateGanttData(schedule, jobs)
        };

        var json = JsonSerializer.Serialize(output, Options);
        File.WriteAllText(filePath, json);
    }

    private JobSummaryOutput CreateJobSummary(Job job, Schedule schedule)
    {
        var jobAssignments = job.Operations
            .Select(op => schedule.Assignments.FirstOrDefault(a => a.OperationId == op.Id))
            .Where(a => a != null)
            .ToList();

        var startMs = jobAssignments.Count > 0 ? jobAssignments.Min(a => a!.StartMs) : 0;
        var endMs = jobAssignments.Count > 0 ? jobAssignments.Max(a => a!.EndMs) : 0;

        var violation = schedule.Violations.FirstOrDefault(v => v.TargetId == job.Id);

        return new JobSummaryOutput
        {
            JobId = job.Id,
            ProductName = job.ProductName,
            Priority = job.Priority,
            StartMs = startMs,
            EndMs = endMs,
            DurationMs = endMs - startMs,
            DueDate = job.DueDate,
            IsOnTime = violation == null,
            DelayMs = violation?.Amount ?? 0
        };
    }

    private GanttDataOutput CreateGanttData(Schedule schedule, List<Job> jobs)
    {
        // Group assignments by resource for Gantt chart
        var resourceTasks = schedule.Assignments
            .GroupBy(a => a.ResourceId)
            .ToDictionary(
                g => g.Key,
                g => g.Select(a => new GanttTask
                {
                    OperationId = a.OperationId,
                    JobId = GetJobIdFromOperation(a.OperationId, jobs),
                    StartMs = a.StartMs,
                    EndMs = a.EndMs
                }).OrderBy(t => t.StartMs).ToList()
            );

        return new GanttDataOutput
        {
            ResourceTasks = resourceTasks
        };
    }

    private string GetJobIdFromOperation(string operationId, List<Job> jobs)
    {
        foreach (var job in jobs)
        {
            if (job.Operations.Any(op => op.Id == operationId))
            {
                return job.Id;
            }
        }
        return "UNKNOWN";
    }
}

#region Output DTOs

public class ScheduleOutput
{
    public required ScheduleSummary Summary { get; set; }
    public required List<AssignmentOutput> Assignments { get; set; }
    public required List<ViolationOutput> Violations { get; set; }
    public required List<JobSummaryOutput> JobSummaries { get; set; }
    public required GanttDataOutput GanttData { get; set; }
}

public class ScheduleSummary
{
    public long MakespanMs { get; set; }
    public double MakespanMinutes { get; set; }
    public int TotalAssignments { get; set; }
    public int TotalViolations { get; set; }
    public bool IsOnTime { get; set; }
}

public class AssignmentOutput
{
    public required string OperationId { get; set; }
    public required string ResourceId { get; set; }
    public long StartMs { get; set; }
    public long EndMs { get; set; }
    public long DurationMs { get; set; }
    public DateTime StartTime { get; set; }
    public DateTime EndTime { get; set; }
}

public class ViolationOutput
{
    public required string ConstraintType { get; set; }
    public required string TargetId { get; set; }
    public double Amount { get; set; }
    public string? Description { get; set; }
}

public class JobSummaryOutput
{
    public required string JobId { get; set; }
    public string? ProductName { get; set; }
    public int Priority { get; set; }
    public long StartMs { get; set; }
    public long EndMs { get; set; }
    public long DurationMs { get; set; }
    public DateTime? DueDate { get; set; }
    public bool IsOnTime { get; set; }
    public double DelayMs { get; set; }
}

public class GanttDataOutput
{
    public required Dictionary<string, List<GanttTask>> ResourceTasks { get; set; }
}

public class GanttTask
{
    public required string OperationId { get; set; }
    public required string JobId { get; set; }
    public long StartMs { get; set; }
    public long EndMs { get; set; }
}

#endregion
