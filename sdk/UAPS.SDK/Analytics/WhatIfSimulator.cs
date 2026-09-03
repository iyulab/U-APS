using UAPS.SDK.Client;
using UAPS.SDK.Models;

namespace UAPS.SDK.Analytics;

/// <summary>
/// What-If 시뮬레이션 시나리오 유형
/// </summary>
public enum WhatIfScenarioType
{
    /// <summary>자원 추가</summary>
    AddResource,
    /// <summary>자원 제거</summary>
    RemoveResource,
    /// <summary>자원 효율성 변경</summary>
    ChangeResourceEfficiency,
    /// <summary>작업 추가</summary>
    AddJob,
    /// <summary>작업 제거</summary>
    RemoveJob,
    /// <summary>작업 우선순위 변경</summary>
    ChangePriority,
    /// <summary>처리 시간 변경</summary>
    ChangeProcessTime,
    /// <summary>납기일 변경</summary>
    ChangeDueDate,
    /// <summary>셋업 시간 변경</summary>
    ChangeSetupTime
}

/// <summary>
/// What-If 시나리오 정의
/// </summary>
public class WhatIfScenario
{
    public string Name { get; set; } = string.Empty;
    public string Description { get; set; } = string.Empty;
    public WhatIfScenarioType Type { get; set; }
    public string TargetId { get; set; } = string.Empty;
    public Dictionary<string, object> Parameters { get; set; } = [];

    public static WhatIfScenario AddResource(string name, Resource resource)
    {
        return new WhatIfScenario
        {
            Name = name,
            Type = WhatIfScenarioType.AddResource,
            TargetId = resource.Id,
            Parameters = { ["resource"] = resource }
        };
    }

    public static WhatIfScenario RemoveResource(string name, string resourceId)
    {
        return new WhatIfScenario
        {
            Name = name,
            Type = WhatIfScenarioType.RemoveResource,
            TargetId = resourceId
        };
    }

    public static WhatIfScenario ChangeEfficiency(string name, string resourceId, double newEfficiency)
    {
        return new WhatIfScenario
        {
            Name = name,
            Type = WhatIfScenarioType.ChangeResourceEfficiency,
            TargetId = resourceId,
            Parameters = { ["efficiency"] = newEfficiency }
        };
    }

    public static WhatIfScenario ChangePriority(string name, string jobId, int newPriority)
    {
        return new WhatIfScenario
        {
            Name = name,
            Type = WhatIfScenarioType.ChangePriority,
            TargetId = jobId,
            Parameters = { ["priority"] = newPriority }
        };
    }

    public static WhatIfScenario ChangeProcessTime(string name, string operationId, double multiplier)
    {
        return new WhatIfScenario
        {
            Name = name,
            Type = WhatIfScenarioType.ChangeProcessTime,
            TargetId = operationId,
            Parameters = { ["multiplier"] = multiplier }
        };
    }
}

/// <summary>
/// What-If 시뮬레이션 결과
/// </summary>
public class WhatIfResult
{
    public string ScenarioName { get; set; } = string.Empty;
    public bool Success { get; set; }
    public string? Error { get; set; }
    public Schedule? Schedule { get; set; }
    public KpiDashboard? Kpis { get; set; }
    public WhatIfComparison? Comparison { get; set; }
}

/// <summary>
/// 기준 대비 비교 결과
/// </summary>
public class WhatIfComparison
{
    public double MakespanChangePercent { get; set; }
    public double UtilizationChangePercent { get; set; }
    public double OnTimeRateChangePercent { get; set; }
    public double QualityScoreChange { get; set; }
    public string Summary { get; set; } = string.Empty;

    public bool IsImprovement => MakespanChangePercent < 0 || OnTimeRateChangePercent > 0;
}

/// <summary>
/// What-If 시뮬레이터
/// </summary>
public class WhatIfSimulator
{
    private readonly SchedulerClient _client;

    public WhatIfSimulator(SchedulerClient client)
    {
        _client = client;
    }

    /// <summary>
    /// 단일 시나리오 실행
    /// </summary>
    public WhatIfResult RunScenario(
        ScheduleRequest baseRequest,
        ScheduleResult baseResult,
        WhatIfScenario scenario)
    {
        var result = new WhatIfResult { ScenarioName = scenario.Name };

        try
        {
            // 시나리오 적용된 요청 생성
            var modifiedRequest = ApplyScenario(baseRequest, scenario);

            // 스케줄링 실행
            var scheduleResult = _client.Schedule(modifiedRequest);

            if (!scheduleResult.Success || scheduleResult.Schedule == null)
            {
                result.Success = false;
                result.Error = scheduleResult.Error ?? "Scheduling failed";
                return result;
            }

            result.Success = true;
            result.Schedule = scheduleResult.Schedule;

            // KPI 계산
            var kpiCalc = new KpiCalculator(
                scheduleResult.Schedule,
                modifiedRequest.Jobs,
                modifiedRequest.Resources);
            result.Kpis = kpiCalc.Calculate();

            // 기준 대비 비교
            if (baseResult.Schedule != null)
            {
                var baseKpiCalc = new KpiCalculator(
                    baseResult.Schedule,
                    baseRequest.Jobs,
                    baseRequest.Resources);
                var baseKpis = baseKpiCalc.Calculate();
                result.Comparison = CompareResults(baseKpis, result.Kpis);
            }
        }
        catch (Exception ex)
        {
            result.Success = false;
            result.Error = ex.Message;
        }

        return result;
    }

    /// <summary>
    /// 다중 시나리오 병렬 실행
    /// </summary>
    public List<WhatIfResult> RunScenarios(
        ScheduleRequest baseRequest,
        ScheduleResult baseResult,
        IEnumerable<WhatIfScenario> scenarios)
    {
        return scenarios
            .Select(s => RunScenario(baseRequest, baseResult, s))
            .ToList();
    }

    /// <summary>
    /// 최적 시나리오 찾기
    /// </summary>
    public WhatIfResult? FindBestScenario(
        ScheduleRequest baseRequest,
        ScheduleResult baseResult,
        IEnumerable<WhatIfScenario> scenarios,
        Func<WhatIfResult, double> scoreFunc)
    {
        var results = RunScenarios(baseRequest, baseResult, scenarios);
        return results
            .Where(r => r.Success)
            .OrderByDescending(scoreFunc)
            .FirstOrDefault();
    }

    private ScheduleRequest ApplyScenario(ScheduleRequest baseRequest, WhatIfScenario scenario)
    {
        // 깊은 복사 (간단한 구현)
        var request = CloneRequest(baseRequest);

        switch (scenario.Type)
        {
            case WhatIfScenarioType.AddResource:
                if (scenario.Parameters.TryGetValue("resource", out var res) && res is Resource resource)
                {
                    request.Resources.Add(resource);
                }
                break;

            case WhatIfScenarioType.RemoveResource:
                request.Resources.RemoveAll(r => r.Id == scenario.TargetId);
                break;

            case WhatIfScenarioType.ChangeResourceEfficiency:
                if (scenario.Parameters.TryGetValue("efficiency", out var eff) && eff is double efficiency)
                {
                    var targetResource = request.Resources.FirstOrDefault(r => r.Id == scenario.TargetId);
                    if (targetResource != null)
                    {
                        targetResource.Efficiency = efficiency;
                    }
                }
                break;

            case WhatIfScenarioType.ChangePriority:
                if (scenario.Parameters.TryGetValue("priority", out var pri) && pri is int priority)
                {
                    var job = request.Jobs.FirstOrDefault(j => j.Id == scenario.TargetId);
                    if (job != null)
                    {
                        job.Priority = priority;
                    }
                }
                break;

            case WhatIfScenarioType.ChangeProcessTime:
                if (scenario.Parameters.TryGetValue("multiplier", out var mult) && mult is double multiplier)
                {
                    foreach (var job in request.Jobs)
                    {
                        var op = job.Operations.FirstOrDefault(o => o.Id == scenario.TargetId);
                        if (op != null)
                        {
                            op.Time = new OperationTime(
                                op.Time.SetupMs,
                                (long)(op.Time.ProcessMs * multiplier),
                                op.Time.WaitMs);
                            break;
                        }
                    }
                }
                break;
        }

        return request;
    }

    private ScheduleRequest CloneRequest(ScheduleRequest original)
    {
        // 단순화된 복사 - 실제로는 더 깊은 복사 필요
        return new ScheduleRequest
        {
            Jobs = original.Jobs.Select(CloneJob).ToList(),
            Resources = original.Resources.Select(CloneResource).ToList(),
            StartTimeMs = original.StartTimeMs,
            SetupMatrices = original.SetupMatrices,
            SkillMatrix = original.SkillMatrix,
            CertificationMatrix = original.CertificationMatrix,
            CrewManager = original.CrewManager
        };
    }

    private Job CloneJob(Job original)
    {
        var clone = Job.Create(original.Id)
            .WithPriority(original.Priority)
            .WithQuantity(original.Quantity);

        clone.ProductName = original.ProductName;

        if (original.DueDate.HasValue)
            clone = clone.WithDueDate(original.DueDate.Value);

        foreach (var op in original.Operations)
        {
            var clonedOp = Operation.Create(op.Id, op.JobId, op.Sequence)
                .WithTime(op.Time.SetupMs, op.Time.ProcessMs, op.Time.WaitMs);

            foreach (var req in op.RequiredResources)
            {
                if (req.ResourceType == ResourceType.Equipment)
                    clonedOp = clonedOp.WithEquipment(req.Candidates.ToArray());
                else
                    clonedOp = clonedOp.WithWorkers(req.Quantity);
            }

            clone = clone.WithOperation(clonedOp);
        }

        return clone;
    }

    private Resource CloneResource(Resource original)
    {
        var clone = original.Kind == ResourceKind.Equipment
            ? Resource.Equipment(original.Id)
            : Resource.Worker(original.Id);

        return clone.WithEfficiency(original.Efficiency);
    }

    private WhatIfComparison CompareResults(KpiDashboard baseline, KpiDashboard scenario)
    {
        var makespanChange = baseline.Summary.TotalMakespanMinutes > 0
            ? (scenario.Summary.TotalMakespanMinutes - baseline.Summary.TotalMakespanMinutes) * 100.0 / baseline.Summary.TotalMakespanMinutes
            : 0;

        var baseAvgUtil = baseline.ResourceUtilization.Count > 0
            ? baseline.ResourceUtilization.Average(r => r.UtilizationPercent)
            : 0;
        var scenarioAvgUtil = scenario.ResourceUtilization.Count > 0
            ? scenario.ResourceUtilization.Average(r => r.UtilizationPercent)
            : 0;
        var utilChange = baseAvgUtil > 0
            ? (scenarioAvgUtil - baseAvgUtil) * 100.0 / baseAvgUtil
            : 0;

        var onTimeChange = scenario.DueDatePerformance.OnTimeRate - baseline.DueDatePerformance.OnTimeRate;
        var qualityChange = scenario.QualityMetrics.ScheduleQualityScore - baseline.QualityMetrics.ScheduleQualityScore;

        var comparison = new WhatIfComparison
        {
            MakespanChangePercent = Math.Round(makespanChange, 2),
            UtilizationChangePercent = Math.Round(utilChange, 2),
            OnTimeRateChangePercent = Math.Round(onTimeChange, 2),
            QualityScoreChange = Math.Round(qualityChange, 2)
        };

        // 요약 생성
        var changes = new List<string>();
        if (makespanChange != 0)
            changes.Add($"Makespan {(makespanChange > 0 ? "+" : "")}{makespanChange:F1}%");
        if (utilChange != 0)
            changes.Add($"활용률 {(utilChange > 0 ? "+" : "")}{utilChange:F1}%");
        if (onTimeChange != 0)
            changes.Add($"납기준수 {(onTimeChange > 0 ? "+" : "")}{onTimeChange:F1}%p");

        comparison.Summary = changes.Count > 0 ? string.Join(", ", changes) : "변화 없음";

        return comparison;
    }
}
