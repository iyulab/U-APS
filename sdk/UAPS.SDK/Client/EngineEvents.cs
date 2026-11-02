using System.Text.Json.Nodes;
using UAPS.SDK.Models;

namespace UAPS.SDK.Client;

/// <summary>
/// A <see cref="ScheduleEvent"/> as the engine's rescheduler reads it: the event's kind as the key,
/// its fields in the engine's names. Built by hand (no reflection), so trimmed and NativeAOT hosts work.
/// </summary>
internal static class EngineEvents
{
    /// <summary>The engine's JSON for <paramref name="e"/>, or <c>null</c> for a kind it does not know.</summary>
    public static JsonObject? ToEngine(ScheduleEvent e) => e switch
    {
        OperationDelayEvent d => Of("OperationDelay", ("operation_id", d.OperationId), ("delay_ms", d.DelayMs)),
        MachineBreakdownEvent b => Of("MachineBreakdown",
            ("resource_id", b.ResourceId), ("start_ms", b.StartMs), ("duration_ms", b.DurationMs)),
        UrgentOrderEvent u => Of("UrgentOrder", ("job_id", u.JobId), ("priority", u.Priority)),
        OrderCancellationEvent c => Of("OrderCancellation", ("job_id", c.JobId)),
        MaterialShortageEvent s => Of("MaterialShortage",
            ("material_id", s.MaterialId), ("shortage_qty", s.ShortageQty),
            ("affected_operations", new JsonArray([.. s.AffectedOps.Select(o => (JsonNode?)JsonValue.Create(o))]))),
        MaterialDelayEvent m => Of("MaterialDelay",
            ("material_id", m.MaterialId), ("original_arrival_ms", m.OriginalArrivalMs),
            ("new_arrival_ms", m.NewArrivalMs)),
        MaterialArrivalEvent a => Of("MaterialArrival",
            ("material_id", a.MaterialId), ("quantity", a.Quantity), ("arrival_time_ms", a.ArrivalTimeMs)),
        WorkerUnavailableEvent w => Of("WorkerUnavailable",
            ("worker_id", w.WorkerId), ("start_ms", w.StartMs), ("duration_ms", w.DurationMs),
            ("replacement_id", w.ReplacementId)),
        WorkerAvailableEvent w => Of("WorkerAvailable",
            ("worker_id", w.WorkerId), ("available_from_ms", w.AvailableFromMs)),
        WorkerSkillChangeEvent w => Of("WorkerSkillChange",
            ("worker_id", w.WorkerId), ("skill_id", w.SkillId), ("acquired", w.Acquired)),
        QualityDefectEvent q => Of("QualityDefect",
            ("operation_id", q.OperationId), ("rework_qty", q.ReworkQty),
            ("additional_time_ms", q.AdditionalTimeMs)),
        InspectionDelayEvent i => Of("InspectionDelay", ("operation_id", i.OperationId), ("delay_ms", i.DelayMs)),
        MaintenanceScheduledEvent m => Of("MaintenanceScheduled",
            ("resource_id", m.ResourceId), ("start_ms", m.StartMs), ("duration_ms", m.DurationMs)),
        EfficiencyChangeEvent f => Of("EfficiencyChange",
            ("resource_id", f.ResourceId), ("new_efficiency", f.NewEfficiency)),
        MachineRecoveredEvent r => Of("MachineRecovered",
            ("resource_id", r.ResourceId), ("recovered_at_ms", r.RecoveredAtMs)),
        DueDateChangeEvent d => Of("DueDateChange", ("job_id", d.JobId), ("new_due_date_ms", d.NewDueDateMs)),
        QuantityChangeEvent q => Of("QuantityChange", ("job_id", q.JobId), ("new_quantity", q.NewQuantity)),
        PriorityChangeEvent p => Of("PriorityChange", ("job_id", p.JobId), ("new_priority", p.NewPriority)),
        _ => null,
    };

    private static JsonObject Of(string kind, params (string Key, object? Value)[] fields)
    {
        var body = new JsonObject();
        foreach (var (key, value) in fields)
        {
            body[key] = value switch
            {
                null => null,
                JsonNode node => node,
                string s => JsonValue.Create(s),
                long l => JsonValue.Create(l),
                int i => JsonValue.Create(i),
                double d => JsonValue.Create(d),
                bool b => JsonValue.Create(b),
                _ => throw new InvalidOperationException($"no engine form for {value.GetType().Name}"),
            };
        }
        return new JsonObject { [kind] = body };
    }
}
