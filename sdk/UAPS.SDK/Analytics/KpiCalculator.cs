using UAPS.SDK.Client;
using UAPS.SDK.Models;

namespace UAPS.SDK.Analytics;

/// <summary>
/// KPI 계산 및 대시보드 데이터 생성
/// </summary>
public class KpiCalculator
{
    private readonly Schedule _schedule;
    private readonly List<Job> _jobs;
    private readonly List<Resource> _resources;

    public KpiCalculator(Schedule schedule, List<Job> jobs, List<Resource> resources)
    {
        _schedule = schedule;
        _jobs = jobs;
        _resources = resources;
    }

    /// <summary>
    /// 전체 KPI 대시보드 생성
    /// </summary>
    public KpiDashboard Calculate()
    {
        return new KpiDashboard
        {
            Summary = CalculateSummary(),
            ResourceUtilization = CalculateResourceUtilization(),
            DueDatePerformance = CalculateDueDatePerformance(),
            ThroughputMetrics = CalculateThroughputMetrics(),
            QualityMetrics = CalculateQualityMetrics()
        };
    }

    private SummaryKpi CalculateSummary()
    {
        return new SummaryKpi
        {
            TotalMakespanMinutes = _schedule.MakespanMs / 60000.0,
            TotalJobCount = _jobs.Count,
            TotalOperationCount = _jobs.Sum(j => j.Operations.Count),
            TotalAssignmentCount = _schedule.Assignments.Count,
            TotalViolationCount = _schedule.Violations.Count,
            IsOnTime = _schedule.IsOnTime()
        };
    }

    private List<ResourceUtilizationKpi> CalculateResourceUtilization()
    {
        var result = new List<ResourceUtilizationKpi>();
        if (_schedule.MakespanMs == 0) return result;

        foreach (var resource in _resources)
        {
            var assignments = _schedule.GetAssignmentsForResource(resource.Id).ToList();
            var totalWorkMs = assignments.Sum(a => a.EndMs - a.StartMs);
            var setupMs = assignments.Sum(a => GetSetupTimeForAssignment(a));
            var processMs = totalWorkMs - setupMs;

            result.Add(new ResourceUtilizationKpi
            {
                ResourceId = resource.Id,
                ResourceType = resource.Kind.ToString(),
                TotalWorkMinutes = totalWorkMs / 60000.0,
                SetupMinutes = setupMs / 60000.0,
                ProcessMinutes = processMs / 60000.0,
                IdleMinutes = (_schedule.MakespanMs - totalWorkMs) / 60000.0,
                UtilizationPercent = Math.Round(totalWorkMs * 100.0 / _schedule.MakespanMs, 2),
                AssignmentCount = assignments.Count
            });
        }

        return result;
    }

    private long GetSetupTimeForAssignment(Assignment assignment)
    {
        foreach (var job in _jobs)
        {
            var op = job.Operations.FirstOrDefault(o => o.Id == assignment.OperationId);
            if (op != null) return op.Time.SetupMs;
        }
        return 0;
    }

    private DueDatePerformanceKpi CalculateDueDatePerformance()
    {
        var jobsWithDueDate = _jobs.Where(j => j.DueDate.HasValue).ToList();
        if (jobsWithDueDate.Count == 0)
        {
            return new DueDatePerformanceKpi
            {
                TotalJobsWithDueDate = 0,
                OnTimeJobCount = 0,
                LateJobCount = 0,
                OnTimeRate = 100.0
            };
        }

        int lateCount = 0;
        double totalTardiness = 0;
        double maxTardiness = 0;
        double totalEarliness = 0;

        foreach (var job in jobsWithDueDate)
        {
            var dueMs = new DateTimeOffset(job.DueDate!.Value).ToUnixTimeMilliseconds();
            var lastOp = job.Operations.OrderByDescending(o => o.Sequence).FirstOrDefault();
            if (lastOp == null) continue;

            var assignment = _schedule.GetAssignmentForOperation(lastOp.Id);
            if (assignment == null) continue;

            var diff = assignment.EndMs - dueMs;
            if (diff > 0)
            {
                lateCount++;
                var tardiness = diff / 60000.0;
                totalTardiness += tardiness;
                if (tardiness > maxTardiness) maxTardiness = tardiness;
            }
            else
            {
                totalEarliness += Math.Abs(diff) / 60000.0;
            }
        }

        return new DueDatePerformanceKpi
        {
            TotalJobsWithDueDate = jobsWithDueDate.Count,
            OnTimeJobCount = jobsWithDueDate.Count - lateCount,
            LateJobCount = lateCount,
            OnTimeRate = Math.Round((jobsWithDueDate.Count - lateCount) * 100.0 / jobsWithDueDate.Count, 2),
            TotalTardinessMinutes = Math.Round(totalTardiness, 2),
            MaxTardinessMinutes = Math.Round(maxTardiness, 2),
            AverageTardinessMinutes = lateCount > 0 ? Math.Round(totalTardiness / lateCount, 2) : 0,
            TotalEarlinessMinutes = Math.Round(totalEarliness, 2)
        };
    }

    private ThroughputKpi CalculateThroughputMetrics()
    {
        if (_schedule.MakespanMs == 0 || _jobs.Count == 0)
        {
            return new ThroughputKpi();
        }

        var totalQuantity = _jobs.Sum(j => j.Quantity);
        var makespanHours = _schedule.MakespanMs / 3600000.0;

        // 흐름 시간 계산 (각 Job의 완료시간 - 시작시간)
        var flowTimes = new List<double>();
        foreach (var job in _jobs)
        {
            var firstOp = job.Operations.OrderBy(o => o.Sequence).FirstOrDefault();
            var lastOp = job.Operations.OrderByDescending(o => o.Sequence).FirstOrDefault();

            if (firstOp == null || lastOp == null) continue;

            var firstAssignment = _schedule.GetAssignmentForOperation(firstOp.Id);
            var lastAssignment = _schedule.GetAssignmentForOperation(lastOp.Id);

            if (firstAssignment != null && lastAssignment != null)
            {
                flowTimes.Add((lastAssignment.EndMs - firstAssignment.StartMs) / 60000.0);
            }
        }

        return new ThroughputKpi
        {
            JobsPerHour = Math.Round(_jobs.Count / makespanHours, 2),
            UnitsPerHour = Math.Round(totalQuantity / makespanHours, 2),
            AverageFlowTimeMinutes = flowTimes.Count > 0 ? Math.Round(flowTimes.Average(), 2) : 0,
            MaxFlowTimeMinutes = flowTimes.Count > 0 ? Math.Round(flowTimes.Max(), 2) : 0,
            MinFlowTimeMinutes = flowTimes.Count > 0 ? Math.Round(flowTimes.Min(), 2) : 0
        };
    }

    private QualityKpi CalculateQualityMetrics()
    {
        var violationsByType = _schedule.Violations
            .GroupBy(v => v.ConstraintType)
            .ToDictionary(g => g.Key, g => g.Count());

        return new QualityKpi
        {
            TotalViolations = _schedule.Violations.Count,
            ViolationsByType = violationsByType,
            ScheduleQualityScore = CalculateQualityScore()
        };
    }

    private double CalculateQualityScore()
    {
        // 품질 점수 계산 (100점 만점)
        double score = 100.0;

        // 위반사항 감점 (건당 5점)
        score -= _schedule.Violations.Count * 5;

        // 자원 활용률 반영 (활용률 80% 이상이면 보너스)
        if (_resources.Count > 0 && _schedule.MakespanMs > 0)
        {
            var avgUtilization = _schedule.Assignments
                .GroupBy(a => a.ResourceId)
                .Average(g => g.Sum(a => a.EndMs - a.StartMs) * 100.0 / _schedule.MakespanMs);

            if (avgUtilization >= 80) score += 5;
            if (avgUtilization >= 90) score += 5;
        }

        // 납기 준수율 반영
        var dueDatePerf = CalculateDueDatePerformance();
        if (dueDatePerf.OnTimeRate >= 95) score += 10;
        else if (dueDatePerf.OnTimeRate >= 80) score += 5;
        else if (dueDatePerf.OnTimeRate < 50) score -= 10;

        return Math.Max(0, Math.Min(100, Math.Round(score, 1)));
    }
}

#region KPI Data Classes

/// <summary>
/// KPI 대시보드 전체 데이터
/// </summary>
public class KpiDashboard
{
    public SummaryKpi Summary { get; set; } = new();
    public List<ResourceUtilizationKpi> ResourceUtilization { get; set; } = [];
    public DueDatePerformanceKpi DueDatePerformance { get; set; } = new();
    public ThroughputKpi ThroughputMetrics { get; set; } = new();
    public QualityKpi QualityMetrics { get; set; } = new();
}

/// <summary>
/// 요약 KPI
/// </summary>
public class SummaryKpi
{
    public double TotalMakespanMinutes { get; set; }
    public int TotalJobCount { get; set; }
    public int TotalOperationCount { get; set; }
    public int TotalAssignmentCount { get; set; }
    public int TotalViolationCount { get; set; }
    public bool IsOnTime { get; set; }
}

/// <summary>
/// 자원 활용률 KPI
/// </summary>
public class ResourceUtilizationKpi
{
    public string ResourceId { get; set; } = string.Empty;
    public string ResourceType { get; set; } = string.Empty;
    public double TotalWorkMinutes { get; set; }
    public double SetupMinutes { get; set; }
    public double ProcessMinutes { get; set; }
    public double IdleMinutes { get; set; }
    public double UtilizationPercent { get; set; }
    public int AssignmentCount { get; set; }
}

/// <summary>
/// 납기 성과 KPI
/// </summary>
public class DueDatePerformanceKpi
{
    public int TotalJobsWithDueDate { get; set; }
    public int OnTimeJobCount { get; set; }
    public int LateJobCount { get; set; }
    public double OnTimeRate { get; set; }
    public double TotalTardinessMinutes { get; set; }
    public double MaxTardinessMinutes { get; set; }
    public double AverageTardinessMinutes { get; set; }
    public double TotalEarlinessMinutes { get; set; }
}

/// <summary>
/// 처리량 KPI
/// </summary>
public class ThroughputKpi
{
    public double JobsPerHour { get; set; }
    public double UnitsPerHour { get; set; }
    public double AverageFlowTimeMinutes { get; set; }
    public double MaxFlowTimeMinutes { get; set; }
    public double MinFlowTimeMinutes { get; set; }
}

/// <summary>
/// 품질 KPI
/// </summary>
public class QualityKpi
{
    public int TotalViolations { get; set; }
    public Dictionary<string, int> ViolationsByType { get; set; } = [];
    public double ScheduleQualityScore { get; set; }
}

#endregion
