namespace UAPS.SDK.Models;

/// <summary>
/// 작업 시간 구조
/// </summary>
public record OperationTime(
    long SetupMs,
    long ProcessMs,
    long WaitMs
)
{
    public long TotalMs => SetupMs + ProcessMs + WaitMs;
}

/// <summary>
/// 자원 유형
/// </summary>
public enum ResourceType
{
    Equipment,
    Worker
}

/// <summary>
/// 자원 요구사항
/// </summary>
public record ResourceRequirement
{
    public ResourceType ResourceType { get; init; }
    public int Quantity { get; init; } = 1;
    public List<string> Candidates { get; init; } = [];
    /// <summary>
    /// 인력 부하계수 (0.1~1.0, Worker 유형에만 적용)
    /// 1.0 = 전담, 0.5 = 2개 공정 동시 담당 가능
    /// </summary>
    public double LoadFactor { get; init; } = 1.0;
}

/// <summary>
/// Operation - 단일 공정 단계
/// </summary>
public class Operation
{
    // ===== Engine Properties (→ Rust) =====

    public string Id { get; set; } = string.Empty;
    public string JobId { get; set; } = string.Empty;
    public int Sequence { get; set; }
    /// <summary>
    /// 작업 유형 (스킬 매트릭스 매핑용)
    /// </summary>
    public string? OperationType { get; set; }
    public OperationTime Time { get; set; } = new(0, 0, 0);
    public List<ResourceRequirement> RequiredResources { get; set; } = [];
    public bool IsSplittable { get; set; }
    public bool AllowParallel { get; set; }
    public DateTime? EarliestStart { get; set; }
    public long TransitTimeMs { get; set; }
    public List<string> Dependencies { get; set; } = [];
    public bool AffectsUtilization { get; set; } = true;
    /// <summary>
    /// 작업자 필요 여부 (false = 무인공정)
    /// </summary>
    public bool RequiresOperator { get; set; } = true;
    /// <summary>
    /// 자재 요구사항 목록
    /// </summary>
    public List<MaterialRequirement> MaterialRequirements { get; set; } = [];

    /// <summary>
    /// Crew/Team ID (for crew-based scheduling)
    /// </summary>
    public string? CrewId { get; set; }

    /// <summary>
    /// Outsourcing configuration (if this operation is outsourced)
    /// </summary>
    public OutsourcingConfig? OutsourcingConfig { get; set; }

    /// <summary>
    /// Time window constraints (Phase 11)
    /// </summary>
    public OperationTimeWindow? TimeWindow { get; set; }

    /// <summary>
    /// 일일 작업시간 제약 (매일 반복되는 허용 시간대)
    /// </summary>
    public DailyWorkWindow? DailyWorkWindow { get; set; }

    /// <summary>
    /// PERT 3-point estimation for duration (Phase 11)
    /// </summary>
    public PertDuration? PertDuration { get; set; }

    // ===== SDK-Only Properties (분석용) =====

    public string Name { get; set; } = string.Empty;
    public string ProcessCode { get; set; } = string.Empty;
    public string Description { get; set; } = string.Empty;
    public List<string> InspectionItems { get; set; } = [];
    public List<string> DefectTypes { get; set; } = [];
    public string WorkInstructionId { get; set; } = string.Empty;
    public decimal StandardCost { get; set; }

    // ===== Computed =====

    public long TotalTimeMs => Time.TotalMs;

    // ===== Builder Methods =====

    public static Operation Create(string id, string jobId, int sequence) => new()
    {
        Id = id,
        JobId = jobId,
        Sequence = sequence
    };

    public Operation WithTime(long setupMs, long processMs, long waitMs)
    {
        Time = new OperationTime(setupMs, processMs, waitMs);
        return this;
    }

    public Operation WithEquipment(params string[] equipmentIds)
    {
        RequiredResources.Add(new ResourceRequirement
        {
            ResourceType = ResourceType.Equipment,
            Quantity = 1,
            Candidates = [.. equipmentIds]
        });
        return this;
    }

    public Operation WithWorkers(int count)
    {
        RequiredResources.Add(new ResourceRequirement
        {
            ResourceType = ResourceType.Worker,
            Quantity = count,
            Candidates = [],
            LoadFactor = 1.0
        });
        return this;
    }

    /// <summary>
    /// 작업자 요구사항 추가 (부하계수 포함)
    /// </summary>
    public Operation WithWorkersLoad(int count, double loadFactor)
    {
        RequiredResources.Add(new ResourceRequirement
        {
            ResourceType = ResourceType.Worker,
            Quantity = count,
            Candidates = [],
            LoadFactor = Math.Clamp(loadFactor, 0.1, 1.0)
        });
        return this;
    }

    /// <summary>
    /// 무인공정 설정
    /// </summary>
    public Operation Unmanned()
    {
        RequiresOperator = false;
        return this;
    }

    /// <summary>
    /// 자재 요구사항 추가
    /// </summary>
    public Operation WithMaterial(string materialId, double quantity)
    {
        MaterialRequirements.Add(MaterialRequirement.Create(materialId, quantity));
        return this;
    }

    /// <summary>
    /// 자재 요구사항 추가 (가용일 포함)
    /// </summary>
    public Operation WithMaterial(string materialId, double quantity, DateTime availableFrom)
    {
        MaterialRequirements.Add(MaterialRequirement.Create(materialId, quantity).WithAvailability(availableFrom));
        return this;
    }

    /// <summary>
    /// 자재 요구사항 객체 추가
    /// </summary>
    public Operation WithMaterialRequirement(MaterialRequirement requirement)
    {
        MaterialRequirements.Add(requirement);
        return this;
    }

    /// <summary>
    /// Crew/Team 설정 (조 단위 일괄 할당)
    /// </summary>
    public Operation WithCrew(string crewId)
    {
        CrewId = crewId;
        return this;
    }

    /// <summary>
    /// 외주 공정 설정
    /// </summary>
    public Operation WithOutsourcing(OutsourcingConfig config)
    {
        OutsourcingConfig = config;
        return this;
    }

    /// <summary>
    /// 외주 공정 설정 (간편)
    /// </summary>
    public Operation WithOutsourcing(string providerId, int leadTimeDays = 0)
    {
        var config = Models.OutsourcingConfig.Create(providerId);
        if (leadTimeDays > 0)
            config.WithLeadTimeDays(leadTimeDays);
        OutsourcingConfig = config;
        return this;
    }

    /// <summary>
    /// 시간 윈도우 제약 설정 (Phase 11)
    /// </summary>
    public Operation WithTimeWindow(OperationTimeWindow timeWindow)
    {
        TimeWindow = timeWindow;
        return this;
    }

    /// <summary>
    /// 최소 시작 시간 설정 (Phase 11)
    /// </summary>
    public Operation WithEarliestStart(long earliestStartMs)
    {
        TimeWindow = OperationTimeWindow.WithEarliestStart(earliestStartMs);
        return this;
    }

    /// <summary>
    /// 마감 시간 설정 (Phase 11)
    /// </summary>
    public Operation WithDeadline(long latestEndMs)
    {
        TimeWindow = OperationTimeWindow.WithDeadline(latestEndMs);
        return this;
    }

    /// <summary>
    /// 시간 범위 제약 설정 (Phase 11)
    /// </summary>
    public Operation WithTimeBounds(long earliestStartMs, long latestEndMs)
    {
        TimeWindow = OperationTimeWindow.Bounded(earliestStartMs, latestEndMs);
        return this;
    }

    /// <summary>
    /// 일일 작업시간 제약 설정
    /// </summary>
    public Operation WithDailyWorkWindow(TimeSpan begin, TimeSpan end)
    {
        DailyWorkWindow = Models.DailyWorkWindow.FromTimeSpan(begin, end);
        return this;
    }

    /// <summary>
    /// PERT 3-point estimation 설정 (Phase 11)
    /// </summary>
    public Operation WithPert(long optimisticMs, long mostLikelyMs, long pessimisticMs)
    {
        PertDuration = new PertDuration(optimisticMs, mostLikelyMs, pessimisticMs);
        return this;
    }

    /// <summary>
    /// Check if this operation has time window constraints (Phase 11)
    /// </summary>
    public bool HasTimeWindow => TimeWindow?.IsConstrained ?? false;

    /// <summary>
    /// Check if this operation has PERT estimation (Phase 11)
    /// </summary>
    public bool HasPert => PertDuration != null;

    /// <summary>
    /// Get effective process time (PERT mean or fixed) (Phase 11)
    /// </summary>
    public long EffectiveProcessMs => PertDuration != null
        ? (long)Math.Round(PertDuration.MeanMs)
        : Time.ProcessMs;

    /// <summary>
    /// Get process time at confidence level (Phase 11)
    /// z_scores: 0.0=50%, 1.0=85%, 1.645=95%, 2.326=99%
    /// </summary>
    public long ProcessMsAtConfidence(double zScore) => PertDuration != null
        ? (long)Math.Round(PertDuration.AtConfidence(zScore))
        : Time.ProcessMs;

    // ===== Engine DTO Conversion =====

    public OperationDto ToEngineDto() => new()
    {
        Id = Id,
        JobId = JobId,
        Sequence = Sequence,
        OperationType = OperationType,
        Time = new OperationTimeDto
        {
            SetupMs = Time.SetupMs,
            ProcessMs = Time.ProcessMs,
            WaitMs = Time.WaitMs
        },
        RequiredResources = RequiredResources.Select(r => new ResourceRequirementDto
        {
            ResourceType = r.ResourceType.ToString(),
            Quantity = r.Quantity,
            Candidates = r.Candidates,
            LoadFactor = r.LoadFactor
        }).ToList(),
        MaterialRequirements = MaterialRequirements.Select(m => m.ToDto()).ToList(),
        IsSplittable = IsSplittable,
        AllowParallel = AllowParallel,
        Dependencies = Dependencies,
        RequiresOperator = RequiresOperator,
        CrewId = CrewId,
        OutsourcingConfig = OutsourcingConfig?.ToDto(),
        TimeWindow = TimeWindow,
        PertDuration = PertDuration,
        DailyWorkWindow = DailyWorkWindow != null ? new DailyWorkWindowDto
        {
            BeginMinute = DailyWorkWindow.BeginMinute,
            EndMinute = DailyWorkWindow.EndMinute,
        } : null,
    };
}

/// <summary>
/// Engine DTO (Rust로 전송)
/// </summary>
public class OperationDto
{
    public string Id { get; set; } = string.Empty;
    public string JobId { get; set; } = string.Empty;
    public int Sequence { get; set; }
    public string? OperationType { get; set; }
    public OperationTimeDto Time { get; set; } = new();
    public List<ResourceRequirementDto> RequiredResources { get; set; } = [];
    public List<MaterialRequirementDto> MaterialRequirements { get; set; } = [];
    public bool IsSplittable { get; set; }
    public bool AllowParallel { get; set; }
    public List<string> Dependencies { get; set; } = [];
    public bool RequiresOperator { get; set; } = true;
    public string? CrewId { get; set; }
    public OutsourcingConfigDto? OutsourcingConfig { get; set; }
    public OperationTimeWindow? TimeWindow { get; set; }
    public PertDuration? PertDuration { get; set; }
    public DailyWorkWindowDto? DailyWorkWindow { get; set; }
}

public class OperationTimeDto
{
    public long SetupMs { get; set; }
    public long ProcessMs { get; set; }
    public long WaitMs { get; set; }
}

public class ResourceRequirementDto
{
    public string ResourceType { get; set; } = string.Empty;
    public int Quantity { get; set; }
    public List<string> Candidates { get; set; } = [];
    public double LoadFactor { get; set; } = 1.0;
}

/// <summary>
/// 일일 작업시간 제약 — 특정 공정이 하루 중 허용되는 시간대
/// </summary>
public class DailyWorkWindow
{
    /// <summary>작업 시작 가능 시각 (자정부터 분, 예: 540=09:00)</summary>
    public int BeginMinute { get; set; }

    /// <summary>작업 종료 시각 (자정부터 분, 예: 1080=18:00)</summary>
    public int EndMinute { get; set; }

    public static DailyWorkWindow FromTimeSpan(TimeSpan begin, TimeSpan end) => new()
    {
        BeginMinute = (int)begin.TotalMinutes,
        EndMinute = (int)end.TotalMinutes,
    };
}

public class DailyWorkWindowDto
{
    public int BeginMinute { get; set; }
    public int EndMinute { get; set; }
}
