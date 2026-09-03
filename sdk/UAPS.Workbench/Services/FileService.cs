using System.IO;
using System.Text.Json;
using System.Text.Json.Serialization;
using UAPS.SDK.Client;
using UAPS.SDK.Models;

namespace UAPS.Workbench.Services;

/// <summary>
/// Service for file import/export operations
/// </summary>
public class FileService
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        WriteIndented = true,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
    };

    /// <summary>
    /// Import schedule request from JSON file
    /// </summary>
    public async Task<ScheduleRequest?> ImportJsonAsync(string filePath)
    {
        var json = await File.ReadAllTextAsync(filePath);
        var input = JsonSerializer.Deserialize<ScheduleInputDto>(json, JsonOptions);

        if (input == null) return null;

        return ConvertToRequest(input);
    }

    /// <summary>
    /// Export schedule request to JSON file
    /// </summary>
    public async Task ExportJsonAsync(string filePath, ScheduleRequest request)
    {
        var dto = ConvertToDto(request);
        var json = JsonSerializer.Serialize(dto, JsonOptions);
        await File.WriteAllTextAsync(filePath, json);
    }

    /// <summary>
    /// Export schedule result to JSON file
    /// </summary>
    public async Task ExportResultJsonAsync(string filePath, ScheduleResult result, ScheduleRequest request)
    {
        var output = new ScheduleOutputDto
        {
            Summary = new SummaryDto
            {
                MakespanMs = result.Schedule?.MakespanMs ?? 0,
                MakespanMinutes = (result.Schedule?.MakespanMs ?? 0) / 60000.0,
                TotalAssignments = result.Schedule?.Assignments.Count ?? 0,
                TotalViolations = result.Schedule?.Violations.Count ?? 0,
                IsOnTime = result.Schedule?.IsOnTime() ?? false
            },
            Assignments = result.Schedule?.Assignments.Select(a => new AssignmentOutputDto
            {
                OperationId = a.OperationId,
                ResourceId = a.ResourceId,
                StartMs = a.StartMs,
                EndMs = a.EndMs,
                DurationMs = a.DurationMs,
                StartTime = DateTimeOffset.FromUnixTimeMilliseconds(a.StartMs).ToString("yyyy-MM-ddTHH:mm:ss"),
                EndTime = DateTimeOffset.FromUnixTimeMilliseconds(a.EndMs).ToString("yyyy-MM-ddTHH:mm:ss")
            }).ToList() ?? []
        };

        var json = JsonSerializer.Serialize(output, JsonOptions);
        await File.WriteAllTextAsync(filePath, json);
    }

    private ScheduleRequest ConvertToRequest(ScheduleInputDto input)
    {
        var request = new ScheduleRequest
        {
            StartTimeMs = input.Options?.StartTimeMs ?? 0
        };

        // Convert jobs
        foreach (var jobInput in input.Jobs ?? [])
        {
            var job = Job.Create(jobInput.Id)
                .WithPriority(jobInput.Priority);

            if (!string.IsNullOrEmpty(jobInput.ProductName))
                job.ProductName = jobInput.ProductName;

            if (jobInput.Quantity > 0)
                job = job.WithQuantity(jobInput.Quantity);

            if (jobInput.DueDate.HasValue)
                job = job.WithDueDate(jobInput.DueDate.Value);

            // Convert operations
            foreach (var opInput in jobInput.Operations ?? [])
            {
                var op = Operation.Create(opInput.Id, jobInput.Id, opInput.Sequence)
                    .WithTime(opInput.SetupMs, opInput.ProcessMs, opInput.WaitMs);

                // Convert required resources
                foreach (var resReq in opInput.RequiredResources ?? [])
                {
                    if (resReq.ResourceType == "Equipment")
                    {
                        op = op.WithEquipment(resReq.Candidates?.ToArray() ?? []);
                    }
                    else if (resReq.ResourceType == "Worker")
                    {
                        op = op.WithWorkers(resReq.Quantity);
                    }
                }

                job = job.WithOperation(op);
            }

            request.Jobs.Add(job);
        }

        // Convert resources
        foreach (var resInput in input.Resources ?? [])
        {
            var resource = resInput.Type == "Equipment"
                ? Resource.Equipment(resInput.Id)
                : Resource.Worker(resInput.Id);

            if (!string.IsNullOrEmpty(resInput.Capability))
                resource = resource.WithCapability(resInput.Capability);

            resource = resource.WithEfficiency(resInput.Efficiency);

            if (!string.IsNullOrEmpty(resInput.CalendarId))
                resource.CalendarId = resInput.CalendarId;

            request.Resources.Add(resource);
        }

        // Convert calendars
        foreach (var calInput in input.Calendars ?? [])
        {
            var calendar = Calendar.Create(calInput.Id, calInput.Name ?? calInput.Id);

            foreach (var shiftInput in calInput.Shifts ?? [])
            {
                var days = (shiftInput.Days ?? []).Select(ParseDayOfWeekType).Where(d => d.HasValue).Select(d => d!.Value).ToList();
                var shift = new Shift
                {
                    Name = shiftInput.Name ?? "Shift",
                    Start = new TimeOfDay(shiftInput.Start.Hour, shiftInput.Start.Minute),
                    End = new TimeOfDay(shiftInput.End.Hour, shiftInput.End.Minute),
                    Days = days
                };
                calendar = calendar.WithShift(shift);
            }

            foreach (var breakInput in calInput.Breaks ?? [])
            {
                var breakTime = new BreakTime
                {
                    Start = new TimeOfDay(breakInput.Start.Hour, breakInput.Start.Minute),
                    End = new TimeOfDay(breakInput.End.Hour, breakInput.End.Minute)
                };
                calendar = calendar.WithBreak(breakTime);
            }

            request.Calendars.Add(calendar);
        }

        // Convert setup matrices
        if (input.SetupMatrices?.Matrices != null && input.SetupMatrices.Matrices.Count > 0)
        {
            var setupCollection = new SetupMatrixCollection();
            foreach (var matrixInput in input.SetupMatrices.Matrices)
            {
                var matrix = SetupMatrix.Create(matrixInput.ResourceId);
                foreach (var entry in matrixInput.Entries ?? [])
                {
                    matrix = matrix.WithSetup(entry.FromProduct, entry.ToProduct, entry.SetupTimeMs);
                }
                setupCollection = setupCollection.WithMatrix(matrix);
            }
            request.SetupMatrices = setupCollection;
        }

        // Convert materials
        if (input.MaterialManager != null)
        {
            var materialManager = MaterialManager.Create();
            foreach (var matInput in input.MaterialManager.Materials ?? [])
            {
                var material = Material.Create(matInput.Id, matInput.Name ?? matInput.Id)
                    .WithStock(matInput.StockQuantity)
                    .WithSafetyStock(matInput.SafetyStock)
                    .WithLeadTime(TimeSpan.FromMilliseconds(matInput.LeadTimeMs));
                materialManager = materialManager.AddMaterial(material);
            }

            // Convert BOM
            if (input.MaterialManager.Bom != null)
            {
                foreach (var (productId, entries) in input.MaterialManager.Bom)
                {
                    foreach (var entry in entries)
                    {
                        var bomEntry = new BomEntry
                        {
                            MaterialId = entry.MaterialId,
                            QuantityPerUnit = entry.QuantityPerUnit,
                            ScrapRate = entry.ScrapRate
                        };
                        materialManager = materialManager.AddBomEntry(productId, bomEntry);
                    }
                }
            }

            request.MaterialManager = materialManager;
        }

        return request;
    }

    private static DayOfWeekType? ParseDayOfWeekType(string day)
    {
        return day.ToLowerInvariant() switch
        {
            "monday" => DayOfWeekType.Monday,
            "tuesday" => DayOfWeekType.Tuesday,
            "wednesday" => DayOfWeekType.Wednesday,
            "thursday" => DayOfWeekType.Thursday,
            "friday" => DayOfWeekType.Friday,
            "saturday" => DayOfWeekType.Saturday,
            "sunday" => DayOfWeekType.Sunday,
            _ => null
        };
    }

    private ScheduleInputDto ConvertToDto(ScheduleRequest request)
    {
        return new ScheduleInputDto
        {
            Jobs = request.Jobs.Select(j => new JobInputDto
            {
                Id = j.Id,
                ProductName = j.ProductName,
                Priority = j.Priority,
                Quantity = j.Quantity,
                DueDate = j.DueDate,
                Operations = j.Operations.Select(o => new OperationInputDto
                {
                    Id = o.Id,
                    Sequence = o.Sequence,
                    SetupMs = o.Time.SetupMs,
                    ProcessMs = o.Time.ProcessMs,
                    WaitMs = o.Time.WaitMs,
                    RequiredResources = o.RequiredResources.Select(r => new ResourceRequirementInputDto
                    {
                        ResourceType = r.ResourceType.ToString(),
                        Quantity = r.Quantity,
                        Candidates = r.Candidates.ToList()
                    }).ToList()
                }).ToList()
            }).ToList(),
            Resources = request.Resources.Select(r => new ResourceInputDto
            {
                Id = r.Id,
                Type = r.Kind.ToString(),
                Capability = r.Capabilities.FirstOrDefault() ?? "",
                Efficiency = r.Efficiency
            }).ToList(),
            Options = new OptionsInputDto
            {
                StartTimeMs = request.StartTimeMs
            }
        };
    }
}

#region Input DTOs

internal class ScheduleInputDto
{
    public List<JobInputDto>? Jobs { get; set; }
    public List<ResourceInputDto>? Resources { get; set; }
    public List<CalendarInputDto>? Calendars { get; set; }
    public SetupMatricesInputDto? SetupMatrices { get; set; }
    public MaterialManagerInputDto? MaterialManager { get; set; }
    public OptionsInputDto? Options { get; set; }
}

internal class JobInputDto
{
    public string Id { get; set; } = string.Empty;
    public string? ProductName { get; set; }
    public int Priority { get; set; } = 100;
    public int Quantity { get; set; } = 1;
    public DateTime? DueDate { get; set; }
    public List<OperationInputDto>? Operations { get; set; }
}

internal class OperationInputDto
{
    public string Id { get; set; } = string.Empty;
    public int Sequence { get; set; }
    public long SetupMs { get; set; }
    public long ProcessMs { get; set; }
    public long WaitMs { get; set; }
    public List<ResourceRequirementInputDto>? RequiredResources { get; set; }
}

internal class ResourceRequirementInputDto
{
    public string ResourceType { get; set; } = "Equipment";
    public int Quantity { get; set; } = 1;
    public List<string>? Candidates { get; set; }
}

internal class ResourceInputDto
{
    public string Id { get; set; } = string.Empty;
    public string? Name { get; set; }
    public string Type { get; set; } = "Equipment";
    public string? Capability { get; set; }
    public double Efficiency { get; set; } = 1.0;
    public string? CalendarId { get; set; }
}

internal class OptionsInputDto
{
    public long StartTimeMs { get; set; }
    public string? Algorithm { get; set; }
}

internal class CalendarInputDto
{
    public string Id { get; set; } = string.Empty;
    public string? Name { get; set; }
    public List<ShiftInputDto>? Shifts { get; set; }
    public List<BreakInputDto>? Breaks { get; set; }
    public List<string>? Holidays { get; set; }
}

internal class ShiftInputDto
{
    public string? Name { get; set; }
    public TimeInputDto Start { get; set; } = new();
    public TimeInputDto End { get; set; } = new();
    public List<string>? Days { get; set; }
}

internal class TimeInputDto
{
    public int Hour { get; set; }
    public int Minute { get; set; }
}

internal class BreakInputDto
{
    public TimeInputDto Start { get; set; } = new();
    public TimeInputDto End { get; set; } = new();
}

internal class SetupMatricesInputDto
{
    public List<SetupMatrixInputDto>? Matrices { get; set; }
}

internal class SetupMatrixInputDto
{
    public string ResourceId { get; set; } = string.Empty;
    public List<SetupEntryInputDto>? Entries { get; set; }
}

internal class SetupEntryInputDto
{
    public string FromProduct { get; set; } = string.Empty;
    public string ToProduct { get; set; } = string.Empty;
    public long SetupTimeMs { get; set; }
}

internal class MaterialManagerInputDto
{
    public List<MaterialInputDto>? Materials { get; set; }
    public Dictionary<string, List<BomEntryInputDto>>? Bom { get; set; }
}

internal class MaterialInputDto
{
    public string Id { get; set; } = string.Empty;
    public string? Name { get; set; }
    public string? Unit { get; set; }
    public double StockQuantity { get; set; }
    public double SafetyStock { get; set; }
    public long LeadTimeMs { get; set; }
}

internal class BomEntryInputDto
{
    public string MaterialId { get; set; } = string.Empty;
    public double QuantityPerUnit { get; set; }
    public double ScrapRate { get; set; }
}

#endregion

#region Output DTOs

internal class ScheduleOutputDto
{
    public SummaryDto Summary { get; set; } = new();
    public List<AssignmentOutputDto> Assignments { get; set; } = [];
}

internal class SummaryDto
{
    public long MakespanMs { get; set; }
    public double MakespanMinutes { get; set; }
    public int TotalAssignments { get; set; }
    public int TotalViolations { get; set; }
    public bool IsOnTime { get; set; }
}

internal class AssignmentOutputDto
{
    public string OperationId { get; set; } = string.Empty;
    public string ResourceId { get; set; } = string.Empty;
    public long StartMs { get; set; }
    public long EndMs { get; set; }
    public long DurationMs { get; set; }
    public string StartTime { get; set; } = string.Empty;
    public string EndTime { get; set; } = string.Empty;
}

#endregion
