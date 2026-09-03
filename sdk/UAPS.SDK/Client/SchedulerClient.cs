using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.Json.Serialization;
using UAPS.SDK.Interop;
using UAPS.SDK.Models;

namespace UAPS.SDK.Client;

/// <summary>
/// 스케줄링 엔진 클라이언트
/// </summary>
public class SchedulerClient : IDisposable
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
    };

    private bool _disposed;

    /// <summary>
    /// 엔진 버전 조회
    /// </summary>
    public string GetVersion()
    {
        var ptr = NativeInterop.uaps_version();
        return Marshal.PtrToStringUTF8(ptr) ?? "unknown";
    }

    /// <summary>
    /// 스케줄링 실행
    /// </summary>
    public ScheduleResult Schedule(ScheduleRequest request)
    {
        var requestDto = new ScheduleRequestDto
        {
            Jobs = request.Jobs.Select(j => j.ToEngineDto()).ToList(),
            Resources = request.Resources.Select(r => r.ToEngineDto()).ToList(),
            StartTimeMs = request.StartTimeMs,
            SetupMatrices = request.SetupMatrices?.ToEngineDto(),
            SkillMatrix = request.SkillMatrix?.ToDto(),
            CertificationMatrix = request.CertificationMatrix?.ToDto(),
            WorkerTimeOverride = request.WorkerTimeOverride?.ToDto(),
            CrewManager = request.CrewManager?.ToDto(),
            DispatchingConfig = request.DispatchingConfig != null ? ConvertDispatchingConfig(request.DispatchingConfig) : null,
            MaterialManager = request.MaterialManager?.ToDto(),
            OutsourcingManager = request.OutsourcingManager?.ToDto(),
            Calendars = request.Calendars.Count > 0
                ? request.Calendars.Select(c => c.ToEngineDto()).ToList()
                : null,
        };

        var requestJson = JsonSerializer.Serialize(requestDto, JsonOptions);

        var code = NativeInterop.uaps_schedule(requestJson, out var resultPtr);

        try
        {
            if (resultPtr == IntPtr.Zero)
            {
                return new ScheduleResult
                {
                    Success = false,
                    Error = "Null result from engine"
                };
            }

            var resultJson = Marshal.PtrToStringUTF8(resultPtr);
            if (string.IsNullOrEmpty(resultJson))
            {
                return new ScheduleResult
                {
                    Success = false,
                    Error = "Empty result from engine"
                };
            }

            var response = JsonSerializer.Deserialize<ScheduleResponseDto>(resultJson, JsonOptions);
            if (response == null)
            {
                return new ScheduleResult
                {
                    Success = false,
                    Error = "Failed to deserialize response"
                };
            }

            return new ScheduleResult
            {
                Success = response.Success,
                Schedule = response.Schedule != null ? MapSchedule(response.Schedule) : null,
                Error = response.Error
            };
        }
        finally
        {
            if (resultPtr != IntPtr.Zero)
            {
                NativeInterop.uaps_free_string(resultPtr);
            }
        }
    }

    private static Schedule MapSchedule(ScheduleDto dto)
    {
        return new Schedule
        {
            Assignments = dto.Assignments.Select(a => new Assignment
            {
                OperationId = a.OperationId,
                ResourceId = a.ResourceId,
                StartMs = a.StartMs,
                EndMs = a.EndMs
            }).ToList(),
            Violations = dto.Violations.Select(v => new Violation
            {
                ConstraintType = v.ConstraintType,
                TargetId = v.TargetId,
                Amount = v.Amount,
                Description = v.Message
            }).ToList(),
            Tasks = dto.Tasks.Select(MapTask).ToList(),
            MakespanMs = dto.MakespanMs
        };
    }

    private static ScheduleTask MapTask(TaskDto dto)
    {
        return new ScheduleTask
        {
            Id = dto.Id,
            OperationId = dto.OperationId,
            JobId = dto.JobId,
            ProductName = dto.ProductName,
            PlanBeginMs = dto.PlanBeginMs,
            PlanEndMs = dto.PlanEndMs,
            ActionBeginMs = dto.ActionBeginMs,
            ActionEndMs = dto.ActionEndMs,
            Status = ParseTaskStatus(dto.Status),
            IsFixed = dto.IsFixed,
            SplitIndex = dto.SplitIndex,
            TotalSplits = dto.TotalSplits,
            SplitReason = ParseSplitReason(dto.SplitReason),
            EquipmentId = dto.EquipmentId,
            WorkerIds = dto.WorkerIds,
            WorkersRequired = dto.WorkersRequired,
            SetupMs = dto.SetupMs,
            ProcessMs = dto.ProcessMs,
            IsSplittable = dto.IsSplittable
        };
    }

    private static TaskStatus ParseTaskStatus(string status) => status switch
    {
        "InProgress" => TaskStatus.InProgress,
        "Completed" => TaskStatus.Completed,
        _ => TaskStatus.Plan
    };

    private static SplitReason ParseSplitReason(string reason) => reason switch
    {
        "BreakStart" => SplitReason.BreakStart,
        "ShiftEnd" => SplitReason.ShiftEnd,
        "Holiday" => SplitReason.Holiday,
        _ => SplitReason.None
    };

    private static DispatchingConfigDto ConvertDispatchingConfig(DispatchingConfig config)
    {
        var dto = new DispatchingConfigDto
        {
            PrimaryRule = config.PrimaryRule,
            TieBreaker = config.TieBreaker
        };

        if (config.MultiLayer != null)
        {
            dto.MultiLayer = new MultiLayerDispatchingConfigDto
            {
                DefaultRule = config.MultiLayer.DefaultRule,
                Layers = config.MultiLayer.Layers.Select(l => new DispatchingLayerDto
                {
                    Name = l.Name,
                    Rule = l.Rule,
                    Weight = l.Weight
                }).ToList(),
                SwitchRules = config.MultiLayer.SwitchRules.Select(r => new RuleSwitchRuleDto
                {
                    TargetRule = r.TargetRule,
                    Priority = r.Priority,
                    Condition = new RuleSwitchConditionDto
                    {
                        Type = r.Condition.Type,
                        Threshold = r.Condition.Threshold,
                        ResourceId = r.Condition.ResourceId
                    }
                }).ToList()
            };
        }

        return dto;
    }

    public void Dispose()
    {
        if (!_disposed)
        {
            _disposed = true;
            GC.SuppressFinalize(this);
        }
    }
}

/// <summary>
/// 스케줄링 요청
/// </summary>
public class ScheduleRequest
{
    public List<Job> Jobs { get; set; } = [];
    public List<Resource> Resources { get; set; } = [];
    public long StartTimeMs { get; set; }
    public SetupMatrixCollection? SetupMatrices { get; set; }
    public List<Calendar> Calendars { get; set; } = [];
    public SkillMatrix? SkillMatrix { get; set; }
    public CertificationMatrix? CertificationMatrix { get; set; }
    /// <summary>
    /// Worker time overrides (priority: Override > SkillMatrix > default)
    /// </summary>
    public WorkerTimeOverride? WorkerTimeOverride { get; set; }
    public CrewManager? CrewManager { get; set; }
    public DispatchingConfig? DispatchingConfig { get; set; }
    /// <summary>
    /// 자재 관리자 (자재 가용성 검사 및 BOM 관리)
    /// </summary>
    public MaterialManager? MaterialManager { get; set; }

    /// <summary>
    /// 외주 관리자 (외주 공급업체 및 리드타임 관리)
    /// </summary>
    public OutsourcingManager? OutsourcingManager { get; set; }
}

/// <summary>
/// Dispatching rule configuration
/// </summary>
public class DispatchingConfig
{
    public string PrimaryRule { get; set; } = "FIFO";
    public string? TieBreaker { get; set; }

    /// <summary>
    /// Multi-layer dispatching configuration (optional, overrides PrimaryRule/TieBreaker if set)
    /// </summary>
    public MultiLayerDispatchingConfig? MultiLayer { get; set; }
}

/// <summary>
/// Multi-layer dispatching configuration for complex strategies
/// </summary>
public class MultiLayerDispatchingConfig
{
    /// <summary>
    /// Ordered list of dispatching layers
    /// </summary>
    public List<DispatchingLayer> Layers { get; set; } = [];

    /// <summary>
    /// Dynamic rule switch conditions
    /// </summary>
    public List<RuleSwitchRule> SwitchRules { get; set; } = [];

    /// <summary>
    /// Default rule when no switch condition matches
    /// </summary>
    public string DefaultRule { get; set; } = "FIFO";
}

/// <summary>
/// A single dispatching layer in multi-layer strategy
/// </summary>
public class DispatchingLayer
{
    /// <summary>
    /// Layer name for identification
    /// </summary>
    public string Name { get; set; } = string.Empty;

    /// <summary>
    /// Dispatching rule for this layer
    /// </summary>
    public string Rule { get; set; } = "FIFO";

    /// <summary>
    /// Weight (0.0 = tie-breaker only, 1.0 = primary)
    /// </summary>
    public double Weight { get; set; } = 1.0;

    public static DispatchingLayer Primary(string name, string rule) =>
        new() { Name = name, Rule = rule, Weight = 1.0 };

    public static DispatchingLayer TieBreaker(string name, string rule) =>
        new() { Name = name, Rule = rule, Weight = 0.0 };

    public static DispatchingLayer Weighted(string name, string rule, double weight) =>
        new() { Name = name, Rule = rule, Weight = Math.Clamp(weight, 0.0, 1.0) };
}

/// <summary>
/// Dynamic rule switching configuration
/// </summary>
public class RuleSwitchRule
{
    /// <summary>
    /// Condition type for rule switching
    /// </summary>
    public RuleSwitchCondition Condition { get; set; } = new();

    /// <summary>
    /// Target rule when condition is met
    /// </summary>
    public string TargetRule { get; set; } = "FIFO";

    /// <summary>
    /// Priority (higher = evaluated first)
    /// </summary>
    public int Priority { get; set; } = 0;
}

/// <summary>
/// Condition for dynamic rule switching
/// </summary>
public class RuleSwitchCondition
{
    /// <summary>
    /// Condition type: TimeAfter, TimeBefore, DelayedJobsExceed,
    /// UtilizationAbove, UtilizationBelow, QueueLengthAbove, HasUrgentJobs, Always
    /// </summary>
    public string Type { get; set; } = "Always";

    /// <summary>
    /// Threshold value (interpretation depends on Type)
    /// </summary>
    public double Threshold { get; set; }

    /// <summary>
    /// Resource ID (for utilization-based conditions)
    /// </summary>
    public string? ResourceId { get; set; }

    // Factory methods for common conditions
    public static RuleSwitchCondition Always() => new() { Type = "Always" };

    public static RuleSwitchCondition TimeAfter(long thresholdMs) =>
        new() { Type = "TimeAfter", Threshold = thresholdMs };

    public static RuleSwitchCondition TimeBefore(long thresholdMs) =>
        new() { Type = "TimeBefore", Threshold = thresholdMs };

    public static RuleSwitchCondition DelayedJobsExceed(int count) =>
        new() { Type = "DelayedJobsExceed", Threshold = count };

    public static RuleSwitchCondition QueueLengthAbove(int threshold) =>
        new() { Type = "QueueLengthAbove", Threshold = threshold };

    public static RuleSwitchCondition HasUrgentJobs(int priorityThreshold) =>
        new() { Type = "HasUrgentJobs", Threshold = priorityThreshold };

    public static RuleSwitchCondition UtilizationAbove(string resourceId, double threshold) =>
        new() { Type = "UtilizationAbove", ResourceId = resourceId, Threshold = threshold };

    public static RuleSwitchCondition UtilizationBelow(string resourceId, double threshold) =>
        new() { Type = "UtilizationBelow", ResourceId = resourceId, Threshold = threshold };
}

/// <summary>
/// Preset dispatching strategies
/// </summary>
public static class DispatchingPresets
{
    /// <summary>
    /// Due date focused: EDD with SPT tie-breaker
    /// </summary>
    public static MultiLayerDispatchingConfig DueDateFocused => new()
    {
        DefaultRule = DispatchingRules.EDD,
        Layers = [
            DispatchingLayer.Primary("due_date", DispatchingRules.EDD),
            DispatchingLayer.TieBreaker("tie_breaker", DispatchingRules.SPT)
        ]
    };

    /// <summary>
    /// Throughput maximize: SPT only
    /// </summary>
    public static MultiLayerDispatchingConfig ThroughputMaximize => new()
    {
        DefaultRule = DispatchingRules.SPT,
        Layers = [
            DispatchingLayer.Primary("shortest_first", DispatchingRules.SPT)
        ]
    };

    /// <summary>
    /// Balanced: Critical Ratio with FIFO tie-breaker
    /// </summary>
    public static MultiLayerDispatchingConfig Balanced => new()
    {
        DefaultRule = DispatchingRules.CR,
        Layers = [
            DispatchingLayer.Primary("critical_ratio", DispatchingRules.CR),
            DispatchingLayer.TieBreaker("tie_breaker", DispatchingRules.FIFO)
        ]
    };

    /// <summary>
    /// Urgency response: Priority with EDD secondary
    /// </summary>
    public static MultiLayerDispatchingConfig UrgencyResponse => new()
    {
        DefaultRule = DispatchingRules.PRIORITY,
        Layers = [
            DispatchingLayer.Primary("priority", DispatchingRules.PRIORITY),
            DispatchingLayer.Weighted("due_date", DispatchingRules.EDD, 0.5)
        ]
    };

    /// <summary>
    /// Load balancing: LPUL with WINQ
    /// </summary>
    public static MultiLayerDispatchingConfig LoadBalancing => new()
    {
        DefaultRule = DispatchingRules.LPUL,
        Layers = [
            DispatchingLayer.Primary("utilization", DispatchingRules.LPUL),
            DispatchingLayer.Weighted("queue", DispatchingRules.WINQ, 0.5)
        ]
    };

    /// <summary>
    /// Fair queue: Simple FIFO
    /// </summary>
    public static MultiLayerDispatchingConfig FairQueue => new()
    {
        DefaultRule = DispatchingRules.FIFO,
        Layers = [
            DispatchingLayer.Primary("fifo", DispatchingRules.FIFO)
        ]
    };

    /// <summary>
    /// Adaptive: Changes rule based on conditions
    /// </summary>
    public static MultiLayerDispatchingConfig Adaptive => new()
    {
        DefaultRule = DispatchingRules.CR,
        Layers = [
            DispatchingLayer.Primary("critical_ratio", DispatchingRules.CR)
        ],
        SwitchRules = [
            new RuleSwitchRule
            {
                Condition = RuleSwitchCondition.HasUrgentJobs(1),
                TargetRule = DispatchingRules.PRIORITY,
                Priority = 100
            },
            new RuleSwitchRule
            {
                Condition = RuleSwitchCondition.DelayedJobsExceed(5),
                TargetRule = DispatchingRules.EDD,
                Priority = 50
            }
        ]
    };

    /// <summary>
    /// WeightedTardiness: ATC with WSPT tie-breaker for minimizing weighted tardiness
    /// </summary>
    public static MultiLayerDispatchingConfig WeightedTardiness => new()
    {
        DefaultRule = DispatchingRules.ATC,
        Layers = [
            DispatchingLayer.Primary("atc", DispatchingRules.ATC),
            DispatchingLayer.TieBreaker("wspt", DispatchingRules.WSPT)
        ]
    };

    /// <summary>
    /// Get preset by name
    /// </summary>
    public static MultiLayerDispatchingConfig? GetPreset(string name) => name.ToUpperInvariant() switch
    {
        "DUE_DATE_FOCUSED" or "DUE-DATE-FOCUSED" or "DUEDATE" => DueDateFocused,
        "THROUGHPUT" or "THROUGHPUT_MAXIMIZE" => ThroughputMaximize,
        "BALANCED" => Balanced,
        "URGENCY" or "URGENCY_RESPONSE" => UrgencyResponse,
        "LOAD_BALANCING" or "LOAD-BALANCING" or "LOADBALANCE" => LoadBalancing,
        "FAIR" or "FAIR_QUEUE" or "FAIRQUEUE" => FairQueue,
        "ADAPTIVE" => Adaptive,
        "WEIGHTED_TARDINESS" or "WEIGHTED-TARDINESS" or "ATC" => WeightedTardiness,
        _ => null
    };

    /// <summary>
    /// Get all preset names
    /// </summary>
    public static string[] AllPresets =>
        ["DUE_DATE_FOCUSED", "THROUGHPUT", "BALANCED", "URGENCY", "LOAD_BALANCING", "FAIR", "ADAPTIVE", "WEIGHTED_TARDINESS"];
}

/// <summary>
/// Available dispatching rules
/// </summary>
public static class DispatchingRules
{
    // Time-based rules
    public const string SPT = "SPT";    // Shortest Processing Time
    public const string LPT = "LPT";    // Longest Processing Time
    public const string LWKR = "LWKR";  // Least Work Remaining
    public const string MWKR = "MWKR";  // Most Work Remaining
    public const string WSPT = "WSPT";  // Weighted Shortest Processing Time

    // Due date rules
    public const string EDD = "EDD";    // Earliest Due Date
    public const string MST = "MST";    // Minimum Slack Time
    public const string CR = "CR";      // Critical Ratio
    public const string SRO = "S/RO";   // Slack per Remaining Operations
    public const string ATC = "ATC";    // Apparent Tardiness Cost

    // Queue/Load rules
    public const string FIFO = "FIFO";  // First In First Out
    public const string WINQ = "WINQ";  // Work In Next Queue
    public const string LPUL = "LPUL";  // Least Planned Utilization Level

    // Priority-based
    public const string PRIORITY = "PRIORITY";

    /// <summary>
    /// Get all available rule names
    /// </summary>
    public static string[] All => [SPT, LPT, LWKR, MWKR, WSPT, EDD, MST, CR, SRO, ATC, FIFO, WINQ, LPUL, PRIORITY];

    /// <summary>
    /// Check if a rule name is valid
    /// </summary>
    public static bool IsValid(string? rule)
    {
        if (string.IsNullOrEmpty(rule)) return false;
        return All.Contains(rule.ToUpperInvariant());
    }
}

/// <summary>
/// 스케줄링 결과
/// </summary>
public class ScheduleResult
{
    public bool Success { get; set; }
    public Schedule? Schedule { get; set; }
    public string? Error { get; set; }
}

/// <summary>
/// 스케줄
/// </summary>
public class Schedule
{
    public List<Assignment> Assignments { get; set; } = [];
    public List<Violation> Violations { get; set; } = [];
    public List<ScheduleTask> Tasks { get; set; } = [];
    public long MakespanMs { get; set; }

    public IEnumerable<Assignment> GetAssignmentsForResource(string resourceId) =>
        Assignments.Where(a => a.ResourceId == resourceId);

    public Assignment? GetAssignmentForOperation(string operationId) =>
        Assignments.FirstOrDefault(a => a.OperationId == operationId);

    public IEnumerable<ScheduleTask> GetTasksForOperation(string operationId) =>
        Tasks.Where(t => t.OperationId == operationId);

    public IEnumerable<ScheduleTask> GetTasksForResource(string resourceId) =>
        Tasks.Where(t => t.EquipmentId == resourceId);

    public bool IsOnTime() => Violations.Count == 0;
}

/// <summary>
/// 제약조건 위반
/// </summary>
public class Violation
{
    public string ConstraintType { get; set; } = string.Empty;
    public string TargetId { get; set; } = string.Empty;
    public double Amount { get; set; }
    public string Description { get; set; } = string.Empty;
}

/// <summary>
/// 할당 결과
/// </summary>
public class Assignment
{
    public string OperationId { get; set; } = string.Empty;
    public string ResourceId { get; set; } = string.Empty;
    public long StartMs { get; set; }
    public long EndMs { get; set; }

    public long DurationMs => EndMs - StartMs;
}

/// <summary>
/// Task 분할 이유
/// </summary>
public enum SplitReason
{
    None,
    BreakStart,
    ShiftEnd,
    Holiday
}

/// <summary>
/// Task 상태
/// </summary>
public enum TaskStatus
{
    Plan,
    InProgress,
    Completed
}

/// <summary>
/// Task - 공정작업 (Operation의 실제 스케줄링 단위)
/// </summary>
public class ScheduleTask
{
    /// <summary>Task ID (예: "OP-001-T001")</summary>
    public string Id { get; set; } = string.Empty;

    /// <summary>부모 Operation ID</summary>
    public string OperationId { get; set; } = string.Empty;

    /// <summary>부모 Job ID</summary>
    public string JobId { get; set; } = string.Empty;

    /// <summary>제품명</summary>
    public string? ProductName { get; set; }

    // ========== 시간 정보 ==========
    /// <summary>계획 시작 시간 (epoch ms)</summary>
    public long PlanBeginMs { get; set; }

    /// <summary>계획 종료 시간 (epoch ms)</summary>
    public long PlanEndMs { get; set; }

    /// <summary>실적 시작 시간 (epoch ms)</summary>
    public long? ActionBeginMs { get; set; }

    /// <summary>실적 종료 시간 (epoch ms)</summary>
    public long? ActionEndMs { get; set; }

    // ========== 상태 정보 ==========
    /// <summary>Task 상태</summary>
    public TaskStatus Status { get; set; } = TaskStatus.Plan;

    /// <summary>사용자 고정 여부</summary>
    public bool IsFixed { get; set; }

    // ========== 분할 정보 ==========
    /// <summary>분할 인덱스 (0, 1, 2...)</summary>
    public int SplitIndex { get; set; }

    /// <summary>총 분할 개수</summary>
    public int TotalSplits { get; set; } = 1;

    /// <summary>분할 이유</summary>
    public SplitReason SplitReason { get; set; } = SplitReason.None;

    // ========== 자원 정보 ==========
    /// <summary>할당된 설비 ID</summary>
    public string? EquipmentId { get; set; }

    /// <summary>할당된 작업자 ID 목록</summary>
    public List<string> WorkerIds { get; set; } = [];

    /// <summary>필요 작업자 수 (0=무인공정)</summary>
    public int WorkersRequired { get; set; }

    // ========== 공정 속성 ==========
    /// <summary>Setup 시간 (ms)</summary>
    public long SetupMs { get; set; }

    /// <summary>Process 시간 (ms)</summary>
    public long ProcessMs { get; set; }

    /// <summary>분할 가능 여부</summary>
    public bool IsSplittable { get; set; }

    // ========== 계산 속성 ==========
    /// <summary>Task 시간 (ms)</summary>
    public long DurationMs => PlanEndMs - PlanBeginMs;

    /// <summary>Action(실적)이 있는지 확인</summary>
    public bool HasAction => ActionBeginMs.HasValue || ActionEndMs.HasValue;

    /// <summary>불변 여부 - action이 있으면 불변</summary>
    public bool IsImmutable => HasAction || Status == TaskStatus.Completed;
}

// Internal DTOs for JSON serialization
internal class ScheduleRequestDto
{
    public List<JobDto> Jobs { get; set; } = [];
    public List<ResourceDto> Resources { get; set; } = [];
    public long StartTimeMs { get; set; }
    public SetupMatrixCollectionDto? SetupMatrices { get; set; }
    public SkillMatrixDto? SkillMatrix { get; set; }
    public CertificationMatrixDto? CertificationMatrix { get; set; }
    public WorkerTimeOverrideDto? WorkerTimeOverride { get; set; }
    public CrewManagerDto? CrewManager { get; set; }
    public DispatchingConfigDto? DispatchingConfig { get; set; }
    public MaterialManagerDto? MaterialManager { get; set; }
    public OutsourcingManagerDto? OutsourcingManager { get; set; }
    public List<CalendarDto>? Calendars { get; set; }
}

internal class DispatchingConfigDto
{
    public string PrimaryRule { get; set; } = "FIFO";
    public string? TieBreaker { get; set; }
    public MultiLayerDispatchingConfigDto? MultiLayer { get; set; }
}

internal class MultiLayerDispatchingConfigDto
{
    public List<DispatchingLayerDto> Layers { get; set; } = [];
    public List<RuleSwitchRuleDto> SwitchRules { get; set; } = [];
    public string DefaultRule { get; set; } = "FIFO";
}

internal class DispatchingLayerDto
{
    public string Name { get; set; } = string.Empty;
    public string Rule { get; set; } = "FIFO";
    public double Weight { get; set; } = 1.0;
}

internal class RuleSwitchRuleDto
{
    public RuleSwitchConditionDto Condition { get; set; } = new();
    public string TargetRule { get; set; } = "FIFO";
    public int Priority { get; set; } = 0;
}

internal class RuleSwitchConditionDto
{
    public string Type { get; set; } = "Always";
    public double Threshold { get; set; }
    public string? ResourceId { get; set; }
}

internal class ScheduleResponseDto
{
    public bool Success { get; set; }
    public ScheduleDto? Schedule { get; set; }
    public string? Error { get; set; }
}

internal class ScheduleDto
{
    public List<AssignmentDto> Assignments { get; set; } = [];
    public long MakespanMs { get; set; }
    public List<ViolationDto> Violations { get; set; } = [];
    public List<TaskDto> Tasks { get; set; } = [];
}

internal class ViolationDto
{
    public string ConstraintId { get; set; } = string.Empty;
    public string ConstraintType { get; set; } = string.Empty;
    public string TargetId { get; set; } = string.Empty;
    public double Amount { get; set; }
    public string Message { get; set; } = string.Empty;
}

internal class AssignmentDto
{
    public string OperationId { get; set; } = string.Empty;
    public string ResourceId { get; set; } = string.Empty;
    public long StartMs { get; set; }
    public long EndMs { get; set; }
}

internal class TaskDto
{
    public string Id { get; set; } = string.Empty;
    public string OperationId { get; set; } = string.Empty;
    public string JobId { get; set; } = string.Empty;
    public string? ProductName { get; set; }
    public long PlanBeginMs { get; set; }
    public long PlanEndMs { get; set; }
    public long? ActionBeginMs { get; set; }
    public long? ActionEndMs { get; set; }
    public string Status { get; set; } = "Plan";
    public bool IsFixed { get; set; }
    public int SplitIndex { get; set; }
    public int TotalSplits { get; set; }
    public string SplitReason { get; set; } = "None";
    public string? EquipmentId { get; set; }
    public List<string> WorkerIds { get; set; } = [];
    public int WorkersRequired { get; set; }
    public long SetupMs { get; set; }
    public long ProcessMs { get; set; }
    public bool IsSplittable { get; set; }
}
