using System;
using System.Text.Json.Serialization;

namespace UAPS.SDK.Models;

/// <summary>
/// Operation Time Window - Phase 11
/// Defines time constraints for operation scheduling
/// </summary>
public class OperationTimeWindow
{
    /// <summary>Earliest start time (ms from epoch, null = unconstrained)</summary>
    [JsonPropertyName("earliest_start_ms")]
    public long? EarliestStartMs { get; set; }

    /// <summary>Latest start time (ms from epoch, null = unconstrained)</summary>
    [JsonPropertyName("latest_start_ms")]
    public long? LatestStartMs { get; set; }

    /// <summary>Earliest end time (ms from epoch, null = unconstrained)</summary>
    [JsonPropertyName("earliest_end_ms")]
    public long? EarliestEndMs { get; set; }

    /// <summary>Latest end time (ms from epoch, null = unconstrained)</summary>
    [JsonPropertyName("latest_end_ms")]
    public long? LatestEndMs { get; set; }

    /// <summary>Hard constraint (true) or soft constraint with penalty (false)</summary>
    [JsonPropertyName("is_hard")]
    public bool IsHard { get; set; } = true;

    /// <summary>Penalty per millisecond of violation (for soft constraints)</summary>
    [JsonPropertyName("penalty_per_ms")]
    public double PenaltyPerMs { get; set; } = 0.0;

    /// <summary>Create a new time window with earliest start constraint</summary>
    public static OperationTimeWindow WithEarliestStart(long earliestStartMs)
        => new() { EarliestStartMs = earliestStartMs };

    /// <summary>Create a new time window with deadline (latest end) constraint</summary>
    public static OperationTimeWindow WithDeadline(long latestEndMs)
        => new() { LatestEndMs = latestEndMs };

    /// <summary>Create a bounded time window [earliest_start, latest_end]</summary>
    public static OperationTimeWindow Bounded(long earliestStartMs, long latestEndMs)
        => new() { EarliestStartMs = earliestStartMs, LatestEndMs = latestEndMs };

    /// <summary>Make this a soft constraint with penalty</summary>
    public OperationTimeWindow AsSoft(double penaltyPerMs)
    {
        IsHard = false;
        PenaltyPerMs = penaltyPerMs;
        return this;
    }

    /// <summary>
    /// Validate if a proposed start/end time fits within the window
    /// </summary>
    /// <returns>(valid, earlyViolationMs, lateViolationMs)</returns>
    public (bool Valid, long EarlyViolation, long LateViolation) Validate(long startMs, long endMs)
    {
        long earlyViolation = 0;
        long lateViolation = 0;

        if (EarliestStartMs.HasValue && startMs < EarliestStartMs.Value)
            earlyViolation = EarliestStartMs.Value - startMs;

        if (LatestStartMs.HasValue && startMs > LatestStartMs.Value)
            lateViolation = Math.Max(lateViolation, startMs - LatestStartMs.Value);

        if (EarliestEndMs.HasValue && endMs < EarliestEndMs.Value)
            earlyViolation = Math.Max(earlyViolation, EarliestEndMs.Value - endMs);

        if (LatestEndMs.HasValue && endMs > LatestEndMs.Value)
            lateViolation = Math.Max(lateViolation, endMs - LatestEndMs.Value);

        bool valid = earlyViolation == 0 && lateViolation == 0;
        return (valid, earlyViolation, lateViolation);
    }

    /// <summary>Calculate penalty for constraint violation</summary>
    public double CalculatePenalty(long startMs, long endMs)
    {
        if (IsHard) return 0.0;

        var (_, early, late) = Validate(startMs, endMs);
        return (early + late) * PenaltyPerMs;
    }

    /// <summary>Check if any constraint is defined</summary>
    public bool IsConstrained =>
        EarliestStartMs.HasValue ||
        LatestStartMs.HasValue ||
        EarliestEndMs.HasValue ||
        LatestEndMs.HasValue;
}

/// <summary>
/// PERT 3-point estimation for operation duration - Phase 11
/// Uses the formula: Mean = (O + 4M + P) / 6
/// </summary>
public class PertDuration
{
    /// <summary>Optimistic duration (best case, ms)</summary>
    [JsonPropertyName("optimistic_ms")]
    public long OptimisticMs { get; set; }

    /// <summary>Most likely duration (ms)</summary>
    [JsonPropertyName("most_likely_ms")]
    public long MostLikelyMs { get; set; }

    /// <summary>Pessimistic duration (worst case, ms)</summary>
    [JsonPropertyName("pessimistic_ms")]
    public long PessimisticMs { get; set; }

    public PertDuration() { }

    public PertDuration(long optimisticMs, long mostLikelyMs, long pessimisticMs)
    {
        OptimisticMs = optimisticMs;
        MostLikelyMs = mostLikelyMs;
        PessimisticMs = pessimisticMs;
    }

    /// <summary>Calculate PERT mean: (O + 4M + P) / 6</summary>
    public double MeanMs =>
        (OptimisticMs + 4.0 * MostLikelyMs + PessimisticMs) / 6.0;

    /// <summary>Calculate PERT standard deviation: (P - O) / 6</summary>
    public double StdDevMs =>
        (PessimisticMs - OptimisticMs) / 6.0;

    /// <summary>Calculate PERT variance: ((P - O) / 6)^2</summary>
    public double VarianceMsSq =>
        StdDevMs * StdDevMs;

    /// <summary>
    /// Get duration at specific confidence level using z-score
    /// Common z-scores: 50% = 0.0, 85% = 1.0, 95% = 1.645, 99% = 2.326
    /// </summary>
    public double AtConfidence(double zScore) =>
        MeanMs + zScore * StdDevMs;

    /// <summary>Create PERT estimation from three time estimates</summary>
    public static PertDuration Create(long optimisticMs, long mostLikelyMs, long pessimisticMs)
        => new(optimisticMs, mostLikelyMs, pessimisticMs);

    /// <summary>Create PERT from variance estimate (reverse calculation)</summary>
    public static PertDuration FromVariance(long mostLikelyMs, double varianceMsSq)
    {
        double stdDev = Math.Sqrt(varianceMsSq);
        double range = stdDev * 6.0;
        long optimistic = (long)(mostLikelyMs - range / 2.0);
        long pessimistic = (long)(mostLikelyMs + range / 2.0);
        return new(optimistic, mostLikelyMs, pessimistic);
    }
}
