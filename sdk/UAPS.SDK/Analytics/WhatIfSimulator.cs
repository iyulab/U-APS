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

    /// <summary>작업 추가 — 기준선의 사본에 <paramref name="job"/> 의 사본이 들어간다.</summary>
    public static WhatIfScenario AddJob(string name, Job job)
    {
        return new WhatIfScenario
        {
            Name = name,
            Type = WhatIfScenarioType.AddJob,
            TargetId = job.Id,
            Parameters = { ["job"] = job }
        };
    }

    /// <summary>
    /// 작업 제거 — 그 작업의 공정을 선행으로 삼던 다른 작업의 의존도 함께 끊긴다
    /// (없어진 공정을 기다릴 수는 없다).
    /// </summary>
    public static WhatIfScenario RemoveJob(string name, string jobId)
    {
        return new WhatIfScenario
        {
            Name = name,
            Type = WhatIfScenarioType.RemoveJob,
            TargetId = jobId
        };
    }

    /// <summary>납기 변경.</summary>
    public static WhatIfScenario ChangeDueDate(string name, string jobId, DateTime newDueDate)
    {
        return new WhatIfScenario
        {
            Name = name,
            Type = WhatIfScenarioType.ChangeDueDate,
            TargetId = jobId,
            Parameters = { ["dueDate"] = newDueDate }
        };
    }

    /// <summary>
    /// 셋업 시간 변경 — 설비 <paramref name="resourceId"/> 의 순서 의존 준비 시간 행렬
    /// (기본값과 항목 전부)에 <paramref name="multiplier"/> 를 곱한다. 그 설비에 행렬이
    /// 없으면 시나리오는 실패로 보고된다: 아무것도 안 바뀐 결과를 성공으로 내는 것이
    /// 이 시뮬레이터가 피하는 단 하나의 오독이다.
    /// </summary>
    public static WhatIfScenario ChangeSetupTime(string name, string resourceId, double multiplier)
    {
        return new WhatIfScenario
        {
            Name = name,
            Type = WhatIfScenarioType.ChangeSetupTime,
            TargetId = resourceId,
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

    internal ScheduleRequest ApplyScenario(ScheduleRequest baseRequest, WhatIfScenario scenario)
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

            case WhatIfScenarioType.AddJob:
                if (scenario.Parameters.TryGetValue("job", out var addJob) && addJob is Job newJob)
                {
                    if (request.Jobs.Any(j => j.Id == newJob.Id))
                    {
                        throw new ArgumentException(
                            $"Job '{newJob.Id}' is already in the request; a what-if cannot add it twice.");
                    }
                    // A copy, so the caller's object never becomes part of a scenario request.
                    request.Jobs.Add(newJob.DeepClone());
                }
                else
                {
                    throw new ArgumentException("AddJob needs a Job in Parameters[\"job\"].");
                }
                break;

            case WhatIfScenarioType.RemoveJob:
            {
                var removed = request.Jobs.FirstOrDefault(j => j.Id == scenario.TargetId)
                    ?? throw new ArgumentException($"Job '{scenario.TargetId}' is not in the request.");
                request.Jobs.Remove(removed);
                // Operations elsewhere that waited on the removed job's operations
                // have nothing left to wait for.
                var gone = removed.Operations.Select(o => o.Id).ToHashSet();
                foreach (var op in request.Jobs.SelectMany(j => j.Operations))
                {
                    op.Dependencies.RemoveAll(gone.Contains);
                }
                break;
            }

            case WhatIfScenarioType.ChangeDueDate:
                if (scenario.Parameters.TryGetValue("dueDate", out var due) && due is DateTime dueDate)
                {
                    var job = request.Jobs.FirstOrDefault(j => j.Id == scenario.TargetId)
                        ?? throw new ArgumentException($"Job '{scenario.TargetId}' is not in the request.");
                    job.DueDate = dueDate;
                }
                else
                {
                    throw new ArgumentException("ChangeDueDate needs a DateTime in Parameters[\"dueDate\"].");
                }
                break;

            case WhatIfScenarioType.ChangeSetupTime:
                if (scenario.Parameters.TryGetValue("multiplier", out var setupMult) && setupMult is double setupMultiplier)
                {
                    var matrix = request.SetupMatrices?.Matrices.FirstOrDefault(m => m.ResourceId == scenario.TargetId)
                        ?? throw new ArgumentException(
                            $"Resource '{scenario.TargetId}' has no setup matrix; there is no setup time to change.");
                    matrix.DefaultSetupMs = (long)(matrix.DefaultSetupMs * setupMultiplier);
                    matrix.Entries = [.. matrix.Entries.Select(e => e with { SetupMs = (long)(e.SetupMs * setupMultiplier) })];
                }
                else
                {
                    throw new ArgumentException("ChangeSetupTime needs a double in Parameters[\"multiplier\"].");
                }
                break;

            // Every declared type is handled above. A new enum member that is not
            // must fail loudly rather than return the request untouched: a
            // scenario that silently changes nothing reports a success whose
            // numbers match the baseline, which is exactly what a change with no
            // effect also reports, and a caller cannot tell the two apart.
            default:
                throw new NotSupportedException(
                    $"What-if scenario type '{scenario.Type}' is declared but not implemented.");
        }

        return request;
    }

    /// <summary>
    /// 시나리오가 올라앉을 요청의 독립된 사본.
    /// </summary>
    /// <remarks>
    /// 예전에는 빌더로 각 객체를 다시 지었고, 빌더가 옮기는 것만 살아남았다 —
    /// Job 24개 필드 중 6개, Operation 26개 중 5개, Resource 18개 중 3개. 즉
    /// 시나리오는 의존관계·자재·시간창·달력·능력·용량·비가동 구간이 사라진
    /// <em>다른 문제</em>를 풀고 그것을 기준선과 비교하고 있었다. 재구성이 아니라
    /// 복사이므로, 모델에 필드가 늘어도 사본이 뒤처지지 않는다.
    ///
    /// 셋업 행렬은 <see cref="WhatIfScenarioType.ChangeSetupTime"/> 이 고치므로 함께
    /// 복사한다. 스킬·인증 행렬과 조 편성은 아직 어떤 시나리오도 수정하지 않아 참조를
    /// 공유한다 — 수정하는 시나리오가 생기면 그때 함께 복사해야 한다.
    /// </remarks>
    internal ScheduleRequest CloneRequest(ScheduleRequest original)
    {
        return new ScheduleRequest
        {
            Jobs = [.. original.Jobs.Select(j => j.DeepClone())],
            Resources = [.. original.Resources.Select(r => r.DeepClone())],
            StartTimeMs = original.StartTimeMs,
            SetupMatrices = original.SetupMatrices is null ? null : new SetupMatrixCollection
            {
                Matrices = [.. original.SetupMatrices.Matrices.Select(m => new SetupMatrix
                {
                    ResourceId = m.ResourceId,
                    DefaultSetupMs = m.DefaultSetupMs,
                    Entries = [.. m.Entries]
                })]
            },
            SkillMatrix = original.SkillMatrix,
            CertificationMatrix = original.CertificationMatrix,
            CrewManager = original.CrewManager
        };
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
