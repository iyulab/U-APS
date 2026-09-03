namespace UAPS.SDK.Models;

/// <summary>
/// 재스케줄링 전략
/// </summary>
public enum RescheduleStrategy
{
    /// <summary>
    /// 단순 지연 전파 (영향 공정만 시프트)
    /// </summary>
    RightShift,
    /// <summary>
    /// 영향 공정만 재스케줄 (Affected Operation Rescheduling)
    /// </summary>
    AOR,
    /// <summary>
    /// 부분 재스케줄 (영향받는 자원/작업만)
    /// </summary>
    Partial,
    /// <summary>
    /// 전체 재스케줄
    /// </summary>
    Full,
    /// <summary>
    /// 전체 재생성 (Total Regeneration)
    /// </summary>
    TotalRegeneration
}

/// <summary>
/// 이벤트 분류
/// </summary>
public enum EventCategory
{
    Equipment,
    Material,
    Worker,
    Quality,
    Order
}

/// <summary>
/// 스케줄 변경 이벤트 (추상 베이스)
/// </summary>
public abstract class ScheduleEvent
{
    /// <summary>
    /// 이벤트 ID
    /// </summary>
    public string EventId { get; set; } = Guid.NewGuid().ToString();

    /// <summary>
    /// 이벤트 발생 시간
    /// </summary>
    public DateTime OccurredAt { get; set; } = DateTime.UtcNow;

    /// <summary>
    /// 이벤트 분류
    /// </summary>
    public abstract EventCategory Category { get; }

    /// <summary>
    /// 이벤트 설명
    /// </summary>
    public abstract string Description { get; }

    /// <summary>
    /// 권장 재스케줄링 전략
    /// </summary>
    public abstract RescheduleStrategy RecommendedStrategy { get; }

    /// <summary>
    /// 영향받는 리소스 ID 목록
    /// </summary>
    public abstract IReadOnlyList<string> AffectedResourceIds { get; }

    /// <summary>
    /// 영향받는 작업 ID 목록
    /// </summary>
    public abstract IReadOnlyList<string> AffectedOperationIds { get; }

    /// <summary>
    /// Engine DTO 변환
    /// </summary>
    internal abstract ScheduleEventDto ToDto();
}

// =========================================================================
// 설비 관련 이벤트
// =========================================================================

/// <summary>
/// 기계 고장 이벤트
/// </summary>
public class MachineBreakdownEvent : ScheduleEvent
{
    public string ResourceId { get; set; } = string.Empty;
    public long StartMs { get; set; }
    public long DurationMs { get; set; }

    public override EventCategory Category => EventCategory.Equipment;
    public override string Description => $"Machine breakdown: {ResourceId} for {DurationMs / 60000.0:F0}분";
    public override RescheduleStrategy RecommendedStrategy => RescheduleStrategy.Partial;
    public override IReadOnlyList<string> AffectedResourceIds => [ResourceId];
    public override IReadOnlyList<string> AffectedOperationIds => [];

    public static MachineBreakdownEvent Create(string resourceId, long startMs, long durationMs) => new()
    {
        ResourceId = resourceId,
        StartMs = startMs,
        DurationMs = durationMs
    };

    internal override ScheduleEventDto ToDto() => new()
    {
        EventType = "MachineBreakdown",
        ResourceId = ResourceId,
        StartMs = StartMs,
        DurationMs = DurationMs
    };
}

/// <summary>
/// 기계 복구 이벤트
/// </summary>
public class MachineRecoveredEvent : ScheduleEvent
{
    public string ResourceId { get; set; } = string.Empty;
    public long RecoveredAtMs { get; set; }

    public override EventCategory Category => EventCategory.Equipment;
    public override string Description => $"Machine recovered: {ResourceId}";
    public override RescheduleStrategy RecommendedStrategy => RescheduleStrategy.RightShift;
    public override IReadOnlyList<string> AffectedResourceIds => [ResourceId];
    public override IReadOnlyList<string> AffectedOperationIds => [];

    public static MachineRecoveredEvent Create(string resourceId, long recoveredAtMs) => new()
    {
        ResourceId = resourceId,
        RecoveredAtMs = recoveredAtMs
    };

    internal override ScheduleEventDto ToDto() => new()
    {
        EventType = "MachineRecovered",
        ResourceId = ResourceId,
        RecoveredAtMs = RecoveredAtMs
    };
}

/// <summary>
/// 예방 보전 스케줄 이벤트
/// </summary>
public class MaintenanceScheduledEvent : ScheduleEvent
{
    public string ResourceId { get; set; } = string.Empty;
    public long StartMs { get; set; }
    public long DurationMs { get; set; }

    public override EventCategory Category => EventCategory.Equipment;
    public override string Description => $"Maintenance scheduled: {ResourceId} for {DurationMs / 60000.0:F0}분";
    public override RescheduleStrategy RecommendedStrategy => RescheduleStrategy.RightShift;
    public override IReadOnlyList<string> AffectedResourceIds => [ResourceId];
    public override IReadOnlyList<string> AffectedOperationIds => [];

    public static MaintenanceScheduledEvent Create(string resourceId, long startMs, long durationMs) => new()
    {
        ResourceId = resourceId,
        StartMs = startMs,
        DurationMs = durationMs
    };

    internal override ScheduleEventDto ToDto() => new()
    {
        EventType = "MaintenanceScheduled",
        ResourceId = ResourceId,
        StartMs = StartMs,
        DurationMs = DurationMs
    };
}

/// <summary>
/// 설비 효율 변경 이벤트
/// </summary>
public class EfficiencyChangeEvent : ScheduleEvent
{
    public string ResourceId { get; set; } = string.Empty;
    public double NewEfficiency { get; set; } = 1.0;

    public override EventCategory Category => EventCategory.Equipment;
    public override string Description => $"Efficiency changed: {ResourceId} to {NewEfficiency:P0}";
    public override RescheduleStrategy RecommendedStrategy => RescheduleStrategy.Partial;
    public override IReadOnlyList<string> AffectedResourceIds => [ResourceId];
    public override IReadOnlyList<string> AffectedOperationIds => [];

    public static EfficiencyChangeEvent Create(string resourceId, double newEfficiency) => new()
    {
        ResourceId = resourceId,
        NewEfficiency = Math.Clamp(newEfficiency, 0.0, 2.0)
    };

    internal override ScheduleEventDto ToDto() => new()
    {
        EventType = "EfficiencyChange",
        ResourceId = ResourceId,
        NewEfficiency = NewEfficiency
    };
}

// =========================================================================
// 자재 관련 이벤트
// =========================================================================

/// <summary>
/// 자재 부족 이벤트
/// </summary>
public class MaterialShortageEvent : ScheduleEvent
{
    public string MaterialId { get; set; } = string.Empty;
    public double ShortageQty { get; set; }
    public List<string> AffectedOps { get; set; } = [];

    public override EventCategory Category => EventCategory.Material;
    public override string Description => $"Material shortage: {MaterialId} ({ShortageQty} units)";
    public override RescheduleStrategy RecommendedStrategy => RescheduleStrategy.Partial;
    public override IReadOnlyList<string> AffectedResourceIds => [];
    public override IReadOnlyList<string> AffectedOperationIds => AffectedOps;

    public static MaterialShortageEvent Create(string materialId, double shortageQty, params string[] affectedOps) => new()
    {
        MaterialId = materialId,
        ShortageQty = shortageQty,
        AffectedOps = [.. affectedOps]
    };

    internal override ScheduleEventDto ToDto() => new()
    {
        EventType = "MaterialShortage",
        MaterialId = MaterialId,
        ShortageQty = ShortageQty,
        AffectedOperations = AffectedOps
    };
}

/// <summary>
/// 자재 입고 지연 이벤트
/// </summary>
public class MaterialDelayEvent : ScheduleEvent
{
    public string MaterialId { get; set; } = string.Empty;
    public long OriginalArrivalMs { get; set; }
    public long NewArrivalMs { get; set; }

    public override EventCategory Category => EventCategory.Material;
    public override string Description => $"Material delayed: {MaterialId} by {(NewArrivalMs - OriginalArrivalMs) / 60000.0:F0}분";
    public override RescheduleStrategy RecommendedStrategy => RescheduleStrategy.RightShift;
    public override IReadOnlyList<string> AffectedResourceIds => [];
    public override IReadOnlyList<string> AffectedOperationIds => [];

    public static MaterialDelayEvent Create(string materialId, long originalArrivalMs, long newArrivalMs) => new()
    {
        MaterialId = materialId,
        OriginalArrivalMs = originalArrivalMs,
        NewArrivalMs = newArrivalMs
    };

    internal override ScheduleEventDto ToDto() => new()
    {
        EventType = "MaterialDelay",
        MaterialId = MaterialId,
        OriginalArrivalMs = OriginalArrivalMs,
        NewArrivalMs = NewArrivalMs
    };
}

/// <summary>
/// 자재 긴급 입고 이벤트
/// </summary>
public class MaterialArrivalEvent : ScheduleEvent
{
    public string MaterialId { get; set; } = string.Empty;
    public double Quantity { get; set; }
    public long ArrivalTimeMs { get; set; }

    public override EventCategory Category => EventCategory.Material;
    public override string Description => $"Material arrival: {MaterialId} ({Quantity} units)";
    public override RescheduleStrategy RecommendedStrategy => RescheduleStrategy.RightShift;
    public override IReadOnlyList<string> AffectedResourceIds => [];
    public override IReadOnlyList<string> AffectedOperationIds => [];

    public static MaterialArrivalEvent Create(string materialId, double quantity, long arrivalTimeMs) => new()
    {
        MaterialId = materialId,
        Quantity = quantity,
        ArrivalTimeMs = arrivalTimeMs
    };

    internal override ScheduleEventDto ToDto() => new()
    {
        EventType = "MaterialArrival",
        MaterialId = MaterialId,
        Quantity = Quantity,
        ArrivalTimeMs = ArrivalTimeMs
    };
}

// =========================================================================
// 작업자 관련 이벤트
// =========================================================================

/// <summary>
/// 작업자 불가 이벤트
/// </summary>
public class WorkerUnavailableEvent : ScheduleEvent
{
    public string WorkerId { get; set; } = string.Empty;
    public long StartMs { get; set; }
    public long DurationMs { get; set; }
    public string? ReplacementId { get; set; }

    public override EventCategory Category => EventCategory.Worker;
    public override string Description => $"Worker unavailable: {WorkerId} for {DurationMs / 60000.0:F0}분";
    public override RescheduleStrategy RecommendedStrategy => ReplacementId != null ? RescheduleStrategy.RightShift : RescheduleStrategy.Partial;
    public override IReadOnlyList<string> AffectedResourceIds => ReplacementId != null ? [WorkerId, ReplacementId] : [WorkerId];
    public override IReadOnlyList<string> AffectedOperationIds => [];

    public static WorkerUnavailableEvent Create(string workerId, long startMs, long durationMs, string? replacementId = null) => new()
    {
        WorkerId = workerId,
        StartMs = startMs,
        DurationMs = durationMs,
        ReplacementId = replacementId
    };

    internal override ScheduleEventDto ToDto() => new()
    {
        EventType = "WorkerUnavailable",
        WorkerId = WorkerId,
        StartMs = StartMs,
        DurationMs = DurationMs,
        ReplacementId = ReplacementId
    };
}

/// <summary>
/// 작업자 복귀 이벤트
/// </summary>
public class WorkerAvailableEvent : ScheduleEvent
{
    public string WorkerId { get; set; } = string.Empty;
    public long AvailableFromMs { get; set; }

    public override EventCategory Category => EventCategory.Worker;
    public override string Description => $"Worker available: {WorkerId}";
    public override RescheduleStrategy RecommendedStrategy => RescheduleStrategy.RightShift;
    public override IReadOnlyList<string> AffectedResourceIds => [WorkerId];
    public override IReadOnlyList<string> AffectedOperationIds => [];

    public static WorkerAvailableEvent Create(string workerId, long availableFromMs) => new()
    {
        WorkerId = workerId,
        AvailableFromMs = availableFromMs
    };

    internal override ScheduleEventDto ToDto() => new()
    {
        EventType = "WorkerAvailable",
        WorkerId = WorkerId,
        AvailableFromMs = AvailableFromMs
    };
}

/// <summary>
/// 작업자 기술 변경 이벤트
/// </summary>
public class WorkerSkillChangeEvent : ScheduleEvent
{
    public string WorkerId { get; set; } = string.Empty;
    public string SkillId { get; set; } = string.Empty;
    public bool Acquired { get; set; }

    public override EventCategory Category => EventCategory.Worker;
    public override string Description => $"Worker skill {(Acquired ? "acquired" : "expired")}: {WorkerId} - {SkillId}";
    public override RescheduleStrategy RecommendedStrategy => RescheduleStrategy.Partial;
    public override IReadOnlyList<string> AffectedResourceIds => [WorkerId];
    public override IReadOnlyList<string> AffectedOperationIds => [];

    public static WorkerSkillChangeEvent SkillAcquired(string workerId, string skillId) => new()
    {
        WorkerId = workerId,
        SkillId = skillId,
        Acquired = true
    };

    public static WorkerSkillChangeEvent SkillExpired(string workerId, string skillId) => new()
    {
        WorkerId = workerId,
        SkillId = skillId,
        Acquired = false
    };

    internal override ScheduleEventDto ToDto() => new()
    {
        EventType = "WorkerSkillChange",
        WorkerId = WorkerId,
        SkillId = SkillId,
        Acquired = Acquired
    };
}

// =========================================================================
// 품질 관련 이벤트
// =========================================================================

/// <summary>
/// 품질 불량 이벤트 (재작업 필요)
/// </summary>
public class QualityDefectEvent : ScheduleEvent
{
    public string OperationId { get; set; } = string.Empty;
    public double ReworkQty { get; set; }
    public long AdditionalTimeMs { get; set; }

    public override EventCategory Category => EventCategory.Quality;
    public override string Description => $"Quality defect: {OperationId} ({ReworkQty} units, +{AdditionalTimeMs / 60000.0:F0}분)";
    public override RescheduleStrategy RecommendedStrategy => RescheduleStrategy.RightShift;
    public override IReadOnlyList<string> AffectedResourceIds => [];
    public override IReadOnlyList<string> AffectedOperationIds => [OperationId];

    public static QualityDefectEvent Create(string operationId, double reworkQty, long additionalTimeMs) => new()
    {
        OperationId = operationId,
        ReworkQty = reworkQty,
        AdditionalTimeMs = additionalTimeMs
    };

    internal override ScheduleEventDto ToDto() => new()
    {
        EventType = "QualityDefect",
        OperationId = OperationId,
        ReworkQty = ReworkQty,
        AdditionalTimeMs = AdditionalTimeMs
    };
}

/// <summary>
/// 검사 지연 이벤트
/// </summary>
public class InspectionDelayEvent : ScheduleEvent
{
    public string OperationId { get; set; } = string.Empty;
    public long DelayMs { get; set; }

    public override EventCategory Category => EventCategory.Quality;
    public override string Description => $"Inspection delayed: {OperationId} by {DelayMs / 60000.0:F0}분";
    public override RescheduleStrategy RecommendedStrategy => RescheduleStrategy.RightShift;
    public override IReadOnlyList<string> AffectedResourceIds => [];
    public override IReadOnlyList<string> AffectedOperationIds => [OperationId];

    public static InspectionDelayEvent Create(string operationId, long delayMs) => new()
    {
        OperationId = operationId,
        DelayMs = delayMs
    };

    internal override ScheduleEventDto ToDto() => new()
    {
        EventType = "InspectionDelay",
        OperationId = OperationId,
        DelayMs = DelayMs
    };
}

// =========================================================================
// 주문 관련 이벤트
// =========================================================================

/// <summary>
/// 공정 지연 이벤트
/// </summary>
public class OperationDelayEvent : ScheduleEvent
{
    public string OperationId { get; set; } = string.Empty;
    public long DelayMs { get; set; }

    public override EventCategory Category => EventCategory.Order;
    public override string Description => $"Operation delayed: {OperationId} by {DelayMs / 60000.0:F0}분";
    public override RescheduleStrategy RecommendedStrategy => RescheduleStrategy.RightShift;
    public override IReadOnlyList<string> AffectedResourceIds => [];
    public override IReadOnlyList<string> AffectedOperationIds => [OperationId];

    public static OperationDelayEvent Create(string operationId, long delayMs) => new()
    {
        OperationId = operationId,
        DelayMs = delayMs
    };

    internal override ScheduleEventDto ToDto() => new()
    {
        EventType = "OperationDelay",
        OperationId = OperationId,
        DelayMs = DelayMs
    };
}

/// <summary>
/// 긴급 주문 삽입 이벤트
/// </summary>
public class UrgentOrderEvent : ScheduleEvent
{
    public string JobId { get; set; } = string.Empty;
    public int Priority { get; set; }

    public override EventCategory Category => EventCategory.Order;
    public override string Description => $"Urgent order: {JobId} (priority: {Priority})";
    public override RescheduleStrategy RecommendedStrategy => RescheduleStrategy.Full;
    public override IReadOnlyList<string> AffectedResourceIds => [];
    public override IReadOnlyList<string> AffectedOperationIds => [];

    public static UrgentOrderEvent Create(string jobId, int priority) => new()
    {
        JobId = jobId,
        Priority = priority
    };

    internal override ScheduleEventDto ToDto() => new()
    {
        EventType = "UrgentOrder",
        JobId = JobId,
        Priority = Priority
    };
}

/// <summary>
/// 주문 취소 이벤트
/// </summary>
public class OrderCancellationEvent : ScheduleEvent
{
    public string JobId { get; set; } = string.Empty;

    public override EventCategory Category => EventCategory.Order;
    public override string Description => $"Order cancelled: {JobId}";
    public override RescheduleStrategy RecommendedStrategy => RescheduleStrategy.Partial;
    public override IReadOnlyList<string> AffectedResourceIds => [];
    public override IReadOnlyList<string> AffectedOperationIds => [];

    public static OrderCancellationEvent Create(string jobId) => new()
    {
        JobId = jobId
    };

    internal override ScheduleEventDto ToDto() => new()
    {
        EventType = "OrderCancellation",
        JobId = JobId
    };
}

/// <summary>
/// 납기 변경 이벤트
/// </summary>
public class DueDateChangeEvent : ScheduleEvent
{
    public string JobId { get; set; } = string.Empty;
    public long NewDueDateMs { get; set; }

    public override EventCategory Category => EventCategory.Order;
    public override string Description => $"Due date changed: {JobId}";
    public override RescheduleStrategy RecommendedStrategy => RescheduleStrategy.Partial;
    public override IReadOnlyList<string> AffectedResourceIds => [];
    public override IReadOnlyList<string> AffectedOperationIds => [];

    public static DueDateChangeEvent Create(string jobId, long newDueDateMs) => new()
    {
        JobId = jobId,
        NewDueDateMs = newDueDateMs
    };

    public static DueDateChangeEvent Create(string jobId, DateTime newDueDate) => new()
    {
        JobId = jobId,
        NewDueDateMs = new DateTimeOffset(newDueDate).ToUnixTimeMilliseconds()
    };

    internal override ScheduleEventDto ToDto() => new()
    {
        EventType = "DueDateChange",
        JobId = JobId,
        NewDueDateMs = NewDueDateMs
    };
}

/// <summary>
/// 수량 변경 이벤트
/// </summary>
public class QuantityChangeEvent : ScheduleEvent
{
    public string JobId { get; set; } = string.Empty;
    public long NewQuantity { get; set; }

    public override EventCategory Category => EventCategory.Order;
    public override string Description => $"Quantity changed: {JobId} to {NewQuantity}";
    public override RescheduleStrategy RecommendedStrategy => RescheduleStrategy.Partial;
    public override IReadOnlyList<string> AffectedResourceIds => [];
    public override IReadOnlyList<string> AffectedOperationIds => [];

    public static QuantityChangeEvent Create(string jobId, long newQuantity) => new()
    {
        JobId = jobId,
        NewQuantity = newQuantity
    };

    internal override ScheduleEventDto ToDto() => new()
    {
        EventType = "QuantityChange",
        JobId = JobId,
        NewQuantity = NewQuantity
    };
}

/// <summary>
/// 우선순위 변경 이벤트
/// </summary>
public class PriorityChangeEvent : ScheduleEvent
{
    public string JobId { get; set; } = string.Empty;
    public int NewPriority { get; set; }

    public override EventCategory Category => EventCategory.Order;
    public override string Description => $"Priority changed: {JobId} to {NewPriority}";
    public override RescheduleStrategy RecommendedStrategy => RescheduleStrategy.Partial;
    public override IReadOnlyList<string> AffectedResourceIds => [];
    public override IReadOnlyList<string> AffectedOperationIds => [];

    public static PriorityChangeEvent Create(string jobId, int newPriority) => new()
    {
        JobId = jobId,
        NewPriority = newPriority
    };

    internal override ScheduleEventDto ToDto() => new()
    {
        EventType = "PriorityChange",
        JobId = JobId,
        NewPriority = NewPriority
    };
}

/// <summary>
/// 처리 시간 변경 이벤트
/// </summary>
public class ProcessTimeChangeEvent : ScheduleEvent
{
    public string OperationId { get; set; } = string.Empty;
    public long NewDurationMs { get; set; }

    public override EventCategory Category => EventCategory.Order;
    public override string Description => $"Process time changed: {OperationId} to {NewDurationMs / 60000.0:F0}분";
    public override RescheduleStrategy RecommendedStrategy => RescheduleStrategy.RightShift;
    public override IReadOnlyList<string> AffectedResourceIds => [];
    public override IReadOnlyList<string> AffectedOperationIds => [OperationId];

    public static ProcessTimeChangeEvent Create(string operationId, long newDurationMs) => new()
    {
        OperationId = operationId,
        NewDurationMs = newDurationMs
    };

    internal override ScheduleEventDto ToDto() => new()
    {
        EventType = "ProcessTimeChange",
        OperationId = OperationId,
        NewDurationMs = NewDurationMs
    };
}

// =========================================================================
// Internal DTOs for JSON serialization
// =========================================================================

internal class ScheduleEventDto
{
    public string EventType { get; set; } = string.Empty;

    // Equipment
    public string? ResourceId { get; set; }
    public long? StartMs { get; set; }
    public long? DurationMs { get; set; }
    public long? RecoveredAtMs { get; set; }
    public double? NewEfficiency { get; set; }

    // Material
    public string? MaterialId { get; set; }
    public double? ShortageQty { get; set; }
    public List<string>? AffectedOperations { get; set; }
    public long? OriginalArrivalMs { get; set; }
    public long? NewArrivalMs { get; set; }
    public double? Quantity { get; set; }
    public long? ArrivalTimeMs { get; set; }

    // Worker
    public string? WorkerId { get; set; }
    public string? ReplacementId { get; set; }
    public long? AvailableFromMs { get; set; }
    public string? SkillId { get; set; }
    public bool? Acquired { get; set; }

    // Quality
    public string? OperationId { get; set; }
    public double? ReworkQty { get; set; }
    public long? AdditionalTimeMs { get; set; }
    public long? DelayMs { get; set; }

    // Order
    public string? JobId { get; set; }
    public int? Priority { get; set; }
    public long? NewDueDateMs { get; set; }
    public long? NewQuantity { get; set; }
    public int? NewPriority { get; set; }
    public long? NewDurationMs { get; set; }
}
