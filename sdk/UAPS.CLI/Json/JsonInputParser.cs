using System.Text.Json;
using System.Text.Json.Serialization;
using UAPS.SDK.Models;

namespace UAPS.CLI.Json;

/// <summary>
/// JSON input file parser
/// </summary>
public class JsonInputParser
{
    private static readonly JsonSerializerOptions Options = new()
    {
        PropertyNameCaseInsensitive = true,
        Converters = { new JsonStringEnumConverter() },
        ReadCommentHandling = JsonCommentHandling.Skip,
        AllowTrailingCommas = true
    };

    /// <summary>
    /// Parse JSON input file
    /// </summary>
    public (List<Job> Jobs, List<Resource> Resources) Parse(string filePath)
    {
        var json = File.ReadAllText(filePath);
        var input = JsonSerializer.Deserialize<ScheduleInput>(json, Options)
            ?? throw new InvalidOperationException("Failed to parse JSON input");

        var jobs = ParseJobs(input.Jobs);
        var resources = ParseResources(input.Resources);

        return (jobs, resources);
    }

    private List<Job> ParseJobs(List<JobInput>? jobInputs)
    {
        if (jobInputs == null) return [];

        return jobInputs.Select(j => new Job
        {
            Id = j.Id,
            ProductName = j.ProductName ?? string.Empty,
            Priority = j.Priority,
            Quantity = j.Quantity,
            DueDate = j.GetEffectiveDueDate(),
            Operations = j.Operations?.Select(o => new Operation
            {
                Id = o.Id,
                JobId = j.Id,
                Sequence = o.Sequence,
                Time = new OperationTime(o.SetupMs, o.ProcessMs, o.WaitMs),
                RequiredResources = o.RequiredResources?.Select(r => new ResourceRequirement
                {
                    ResourceType = r.ResourceType,
                    Quantity = r.Quantity,
                    Candidates = r.Candidates ?? []
                }).ToList() ?? []
            }).ToList() ?? []
        }).ToList();
    }

    private List<Resource> ParseResources(List<ResourceInput>? resourceInputs)
    {
        if (resourceInputs == null) return [];

        return resourceInputs.Select(r => new Resource
        {
            Id = r.Id,
            Kind = r.Type == ResourceType.Worker ? ResourceKind.Worker : ResourceKind.Equipment,
            Capabilities = r.Capability != null ? [r.Capability] : [],
            Efficiency = r.Efficiency
        }).ToList();
    }
}

#region Input DTOs

public class ScheduleInput
{
    public List<JobInput>? Jobs { get; set; }
    public List<ResourceInput>? Resources { get; set; }
    public ScheduleOptionsInput? Options { get; set; }
}

public class JobInput
{
    public required string Id { get; set; }
    public string? ProductName { get; set; }
    public int Priority { get; set; } = 100;
    public int Quantity { get; set; } = 1;
    /// <summary>Due date as ISO datetime string (e.g., "1970-01-01T00:30:00Z")</summary>
    public DateTime? DueDate { get; set; }
    /// <summary>Due date as milliseconds from epoch (alternative to DueDate)</summary>
    public long? DueDateMs { get; set; }
    public List<OperationInput>? Operations { get; set; }

    /// <summary>Get effective due date, preferring DueDateMs if both are set</summary>
    public DateTime? GetEffectiveDueDate()
    {
        if (DueDateMs.HasValue)
            return DateTimeOffset.FromUnixTimeMilliseconds(DueDateMs.Value).UtcDateTime;
        return DueDate;
    }
}

public class OperationInput
{
    public required string Id { get; set; }
    public int Sequence { get; set; }
    public long SetupMs { get; set; }
    public long ProcessMs { get; set; }
    public long WaitMs { get; set; }
    public List<ResourceRequirementInput>? RequiredResources { get; set; }
}

public class ResourceRequirementInput
{
    public ResourceType ResourceType { get; set; }
    public int Quantity { get; set; } = 1;
    public List<string>? Candidates { get; set; }
}

public class ResourceInput
{
    public required string Id { get; set; }
    public ResourceType Type { get; set; }
    public string? Capability { get; set; }
    public double Efficiency { get; set; } = 1.0;
}

public class ScheduleOptionsInput
{
    public long StartTimeMs { get; set; }
    public string? Algorithm { get; set; }
}

#endregion
