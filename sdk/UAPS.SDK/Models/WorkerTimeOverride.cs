using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace UAPS.SDK.Models;

/// <summary>
/// Worker time override entry for specific operation type.
/// Priority: Override > SkillMatrix adjustment > default value
/// </summary>
public class TimeOverrideEntry
{
    /// <summary>Setup time override (ms, null uses default)</summary>
    public long? SetupMs { get; set; }

    /// <summary>Process time override (ms, null uses default)</summary>
    public long? ProcessMs { get; set; }

    /// <summary>Wait time override (ms, null uses default)</summary>
    public long? WaitMs { get; set; }

    /// <summary>
    /// Create empty entry (all null)
    /// </summary>
    public TimeOverrideEntry() { }

    /// <summary>
    /// Create entry with all times specified
    /// </summary>
    public static TimeOverrideEntry Full(long setupMs, long processMs, long waitMs) => new()
    {
        SetupMs = setupMs,
        ProcessMs = processMs,
        WaitMs = waitMs
    };

    /// <summary>
    /// Create entry with process time only (most common case)
    /// </summary>
    public static TimeOverrideEntry ProcessOnly(long processMs) => new()
    {
        ProcessMs = processMs
    };

    /// <summary>Set setup time</summary>
    public TimeOverrideEntry WithSetup(long setupMs)
    {
        SetupMs = setupMs;
        return this;
    }

    /// <summary>Set process time</summary>
    public TimeOverrideEntry WithProcess(long processMs)
    {
        ProcessMs = processMs;
        return this;
    }

    /// <summary>Set wait time</summary>
    public TimeOverrideEntry WithWait(long waitMs)
    {
        WaitMs = waitMs;
        return this;
    }
}

/// <summary>
/// Worker time override matrix.
/// Explicit time overrides for specific worker + operation type combinations.
/// Higher priority than SkillMatrix efficiency adjustments.
/// </summary>
public class WorkerTimeOverride
{
    private readonly Dictionary<(string WorkerId, string OperationType), TimeOverrideEntry> _overrides = new();

    /// <summary>
    /// Add override entry
    /// </summary>
    public WorkerTimeOverride WithOverride(string workerId, string operationType, TimeOverrideEntry entry)
    {
        var key = (workerId, operationType);
        _overrides[key] = entry;
        return this;
    }

    /// <summary>
    /// Add full time override (setup, process, wait)
    /// </summary>
    public WorkerTimeOverride WithFullTime(string workerId, string operationType, long setupMs, long processMs, long waitMs)
    {
        return WithOverride(workerId, operationType, TimeOverrideEntry.Full(setupMs, processMs, waitMs));
    }

    /// <summary>
    /// Add process time only override (most common case)
    /// </summary>
    public WorkerTimeOverride WithProcessTime(string workerId, string operationType, long processMs)
    {
        return WithOverride(workerId, operationType, TimeOverrideEntry.ProcessOnly(processMs));
    }

    /// <summary>
    /// Get override entry
    /// </summary>
    public TimeOverrideEntry? GetOverride(string workerId, string operationType)
    {
        var key = (workerId, operationType);
        return _overrides.TryGetValue(key, out var entry) ? entry : null;
    }

    /// <summary>
    /// Check if override exists
    /// </summary>
    public bool HasOverride(string workerId, string operationType)
    {
        return GetOverride(workerId, operationType) != null;
    }

    /// <summary>
    /// Get setup time override (null if not set)
    /// </summary>
    public long? GetSetupTime(string workerId, string operationType)
    {
        return GetOverride(workerId, operationType)?.SetupMs;
    }

    /// <summary>
    /// Get process time override (null if not set)
    /// </summary>
    public long? GetProcessTime(string workerId, string operationType)
    {
        return GetOverride(workerId, operationType)?.ProcessMs;
    }

    /// <summary>
    /// Get wait time override (null if not set)
    /// </summary>
    public long? GetWaitTime(string workerId, string operationType)
    {
        return GetOverride(workerId, operationType)?.WaitMs;
    }

    /// <summary>
    /// Calculate times with override priority.
    /// Returns (setup, process, wait) - uses override if available, otherwise default.
    /// </summary>
    public (long Setup, long Process, long Wait) CalculateTimes(
        string workerId,
        string operationType,
        long defaultSetup,
        long defaultProcess,
        long defaultWait)
    {
        var entry = GetOverride(workerId, operationType);
        if (entry == null)
        {
            return (defaultSetup, defaultProcess, defaultWait);
        }

        return (
            entry.SetupMs ?? defaultSetup,
            entry.ProcessMs ?? defaultProcess,
            entry.WaitMs ?? defaultWait
        );
    }

    /// <summary>
    /// Get all overrides for a specific worker
    /// </summary>
    public List<(string OperationType, TimeOverrideEntry Entry)> GetWorkerOverrides(string workerId)
    {
        var result = new List<(string, TimeOverrideEntry)>();
        foreach (var kvp in _overrides)
        {
            if (kvp.Key.WorkerId == workerId)
            {
                result.Add((kvp.Key.OperationType, kvp.Value));
            }
        }
        return result;
    }

    /// <summary>
    /// Get all overrides for a specific operation type
    /// </summary>
    public List<(string WorkerId, TimeOverrideEntry Entry)> GetOperationOverrides(string operationType)
    {
        var result = new List<(string, TimeOverrideEntry)>();
        foreach (var kvp in _overrides)
        {
            if (kvp.Key.OperationType == operationType)
            {
                result.Add((kvp.Key.WorkerId, kvp.Value));
            }
        }
        return result;
    }

    /// <summary>Number of override entries</summary>
    public int Count => _overrides.Count;

    /// <summary>Check if empty</summary>
    public bool IsEmpty => _overrides.Count == 0;

    /// <summary>
    /// Convert to DTO for serialization
    /// </summary>
    internal WorkerTimeOverrideDto ToDto()
    {
        var entries = new List<TimeOverrideEntryDto>();
        foreach (var kvp in _overrides)
        {
            entries.Add(new TimeOverrideEntryDto
            {
                WorkerId = kvp.Key.WorkerId,
                OperationType = kvp.Key.OperationType,
                SetupMs = kvp.Value.SetupMs,
                ProcessMs = kvp.Value.ProcessMs,
                WaitMs = kvp.Value.WaitMs
            });
        }

        return new WorkerTimeOverrideDto { Entries = entries };
    }
}

#region DTOs

/// <summary>
/// Worker time override DTO for serialization
/// </summary>
internal class WorkerTimeOverrideDto
{
    [JsonPropertyName("overrides")]
    public List<TimeOverrideEntryDto> Entries { get; set; } = new();
}

/// <summary>
/// Time override entry DTO
/// </summary>
internal class TimeOverrideEntryDto
{
    [JsonPropertyName("worker_id")]
    public string WorkerId { get; set; } = string.Empty;

    [JsonPropertyName("operation_type")]
    public string OperationType { get; set; } = string.Empty;

    [JsonPropertyName("setup_ms")]
    public long? SetupMs { get; set; }

    [JsonPropertyName("process_ms")]
    public long? ProcessMs { get; set; }

    [JsonPropertyName("wait_ms")]
    public long? WaitMs { get; set; }
}

#endregion
