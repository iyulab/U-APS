using System.Text.Json;
using UAPS.SDK.Client;
using UAPS.SDK.Models;

namespace UAPS.SDK.Simulation;

/// <summary>
/// 시뮬레이션 세션 - AS-IS → 조작 → TO-BE 워크플로우 지원
/// </summary>
public class SimulationSession
{
    private readonly SchedulerClient _client;

    // Baseline (원본 - 불변)
    private readonly List<Job> _baselineJobs;
    private readonly List<Resource> _baselineResources;
    private readonly Schedule? _baselineSchedule;

    // Working copy (수정 가능)
    private List<Job> _workingJobs;
    private List<Resource> _workingResources;
    private readonly HashSet<string> _disabledResources = [];

    // Current state
    private Schedule? _currentSchedule;
    private readonly List<ScheduleSnapshot> _history = [];

    /// <summary>
    /// AS-IS 스케줄 (원본)
    /// </summary>
    public Schedule? BaselineSchedule => _baselineSchedule;

    /// <summary>
    /// 현재 스케줄 (TO-BE)
    /// </summary>
    public Schedule? CurrentSchedule => _currentSchedule;

    /// <summary>
    /// 현재 작업 목록
    /// </summary>
    public IReadOnlyList<Job> Jobs => _workingJobs;

    /// <summary>
    /// 현재 자원 목록
    /// </summary>
    public IReadOnlyList<Resource> Resources => _workingResources;

    /// <summary>
    /// 변경 이력
    /// </summary>
    public IReadOnlyList<ScheduleSnapshot> History => _history;

    /// <summary>
    /// 변경사항 존재 여부
    /// </summary>
    public bool HasChanges { get; private set; }

    private SimulationSession(
        List<Job> jobs,
        List<Resource> resources,
        Schedule? baselineSchedule,
        SchedulerClient? client = null)
    {
        _client = client ?? new SchedulerClient();

        _baselineJobs = jobs;
        _baselineResources = resources;
        _baselineSchedule = baselineSchedule;

        // Deep copy for working set
        _workingJobs = CloneJobs(jobs);
        _workingResources = CloneResources(resources);
        _currentSchedule = baselineSchedule;
    }

    /// <summary>
    /// 새 시뮬레이션 세션 생성 (Jobs/Resources로부터)
    /// </summary>
    public static SimulationSession Create(List<Job> jobs, List<Resource> resources)
    {
        var session = new SimulationSession(jobs, resources, null);
        session.RunInitialSchedule();
        return session;
    }

    /// <summary>
    /// 기존 스케줄로부터 시뮬레이션 세션 생성
    /// </summary>
    public static SimulationSession FromSchedule(
        List<Job> jobs,
        List<Resource> resources,
        Schedule existingSchedule)
    {
        return new SimulationSession(jobs, resources, existingSchedule);
    }

    // ===== Job 조작 =====

    /// <summary>
    /// Job 수정
    /// </summary>
    public SimulationSession ModifyJob(string jobId, Action<Job> modifier)
    {
        var job = _workingJobs.FirstOrDefault(j => j.Id == jobId)
            ?? throw new ArgumentException($"Job not found: {jobId}");

        modifier(job);
        HasChanges = true;
        return this;
    }

    /// <summary>
    /// Job 추가
    /// </summary>
    public SimulationSession AddJob(Job job)
    {
        _workingJobs.Add(CloneJob(job));
        HasChanges = true;
        return this;
    }

    /// <summary>
    /// Job 제거
    /// </summary>
    public SimulationSession RemoveJob(string jobId)
    {
        var job = _workingJobs.FirstOrDefault(j => j.Id == jobId);
        if (job != null)
        {
            _workingJobs.Remove(job);
            HasChanges = true;
        }
        return this;
    }

    /// <summary>
    /// Job 우선순위 변경
    /// </summary>
    public SimulationSession SetJobPriority(string jobId, int priority)
    {
        return ModifyJob(jobId, j => j.Priority = priority);
    }

    /// <summary>
    /// Job 납기일 변경
    /// </summary>
    public SimulationSession SetJobDueDate(string jobId, DateTime dueDate)
    {
        return ModifyJob(jobId, j => j.DueDate = dueDate);
    }

    // ===== Operation 조작 =====

    /// <summary>
    /// Operation 수정
    /// </summary>
    public SimulationSession ModifyOperation(string operationId, Action<Operation> modifier)
    {
        var operation = _workingJobs
            .SelectMany(j => j.Operations)
            .FirstOrDefault(op => op.Id == operationId)
            ?? throw new ArgumentException($"Operation not found: {operationId}");

        modifier(operation);
        HasChanges = true;
        return this;
    }

    /// <summary>
    /// Operation 시간 변경
    /// </summary>
    public SimulationSession SetOperationTime(
        string operationId,
        long? setupMs = null,
        long? processMs = null,
        long? waitMs = null)
    {
        return ModifyOperation(operationId, op =>
        {
            op.Time = new OperationTime(
                setupMs ?? op.Time.SetupMs,
                processMs ?? op.Time.ProcessMs,
                waitMs ?? op.Time.WaitMs
            );
        });
    }

    /// <summary>
    /// Operation 설비 후보 변경
    /// </summary>
    public SimulationSession SetOperationEquipment(string operationId, params string[] equipmentIds)
    {
        return ModifyOperation(operationId, op =>
        {
            var equipReq = op.RequiredResources
                .FirstOrDefault(r => r.ResourceType == ResourceType.Equipment);

            if (equipReq != null)
            {
                op.RequiredResources.Remove(equipReq);
            }

            op.RequiredResources.Add(new ResourceRequirement
            {
                ResourceType = ResourceType.Equipment,
                Quantity = 1,
                Candidates = [.. equipmentIds]
            });
        });
    }

    // ===== Resource 조작 =====

    /// <summary>
    /// 자원 비활성화 (고장, 유지보수 시뮬레이션)
    /// </summary>
    public SimulationSession DisableResource(string resourceId)
    {
        _disabledResources.Add(resourceId);
        HasChanges = true;
        return this;
    }

    /// <summary>
    /// 자원 활성화
    /// </summary>
    public SimulationSession EnableResource(string resourceId)
    {
        _disabledResources.Remove(resourceId);
        HasChanges = true;
        return this;
    }

    /// <summary>
    /// 자원 효율 변경
    /// </summary>
    public SimulationSession SetResourceEfficiency(string resourceId, double efficiency)
    {
        var resource = _workingResources.FirstOrDefault(r => r.Id == resourceId)
            ?? throw new ArgumentException($"Resource not found: {resourceId}");

        resource.Efficiency = efficiency;
        HasChanges = true;
        return this;
    }

    // ===== 이벤트 기반 조작 =====

    /// <summary>
    /// 이벤트 적용 및 자동 재스케줄링
    /// </summary>
    public Schedule ApplyEvent(ScheduleEvent scheduleEvent, RescheduleStrategy? strategy = null)
    {
        var effectiveStrategy = strategy ?? scheduleEvent.RecommendedStrategy;

        // 이벤트 유형에 따른 상태 변경
        ApplyEventToState(scheduleEvent);

        // 히스토리에 이벤트 기록
        if (_currentSchedule != null)
        {
            _history.Add(new ScheduleSnapshot
            {
                Timestamp = DateTime.UtcNow,
                Schedule = _currentSchedule,
                ModificationSummary = $"Event: {scheduleEvent.Description}"
            });
        }

        // 재스케줄링 실행
        return Reschedule();
    }

    /// <summary>
    /// 여러 이벤트 일괄 적용
    /// </summary>
    public Schedule ApplyEvents(IEnumerable<ScheduleEvent> events)
    {
        foreach (var evt in events)
        {
            ApplyEventToState(evt);
        }

        if (_currentSchedule != null)
        {
            _history.Add(new ScheduleSnapshot
            {
                Timestamp = DateTime.UtcNow,
                Schedule = _currentSchedule,
                ModificationSummary = $"Batch events: {events.Count()} events"
            });
        }

        return Reschedule();
    }

    private void ApplyEventToState(ScheduleEvent evt)
    {
        switch (evt)
        {
            case MachineBreakdownEvent breakdown:
                DisableResource(breakdown.ResourceId);
                break;

            case MachineRecoveredEvent recovered:
                EnableResource(recovered.ResourceId);
                break;

            case EfficiencyChangeEvent efficiency:
                SetResourceEfficiency(efficiency.ResourceId, efficiency.NewEfficiency);
                break;

            case WorkerUnavailableEvent workerUnavailable:
                DisableResource(workerUnavailable.WorkerId);
                break;

            case WorkerAvailableEvent workerAvailable:
                EnableResource(workerAvailable.WorkerId);
                break;

            case OperationDelayEvent delay:
                ModifyOperation(delay.OperationId, op =>
                {
                    op.Time = new OperationTime(
                        op.Time.SetupMs,
                        op.Time.ProcessMs + delay.DelayMs,
                        op.Time.WaitMs
                    );
                });
                break;

            case ProcessTimeChangeEvent processChange:
                ModifyOperation(processChange.OperationId, op =>
                {
                    op.Time = new OperationTime(
                        op.Time.SetupMs,
                        processChange.NewDurationMs,
                        op.Time.WaitMs
                    );
                });
                break;

            case QualityDefectEvent defect:
                ModifyOperation(defect.OperationId, op =>
                {
                    op.Time = new OperationTime(
                        op.Time.SetupMs,
                        op.Time.ProcessMs + defect.AdditionalTimeMs,
                        op.Time.WaitMs
                    );
                });
                break;

            case InspectionDelayEvent inspection:
                ModifyOperation(inspection.OperationId, op =>
                {
                    op.Time = new OperationTime(
                        op.Time.SetupMs,
                        op.Time.ProcessMs,
                        op.Time.WaitMs + inspection.DelayMs
                    );
                });
                break;

            case OrderCancellationEvent cancellation:
                RemoveJob(cancellation.JobId);
                break;

            case DueDateChangeEvent dueDate:
                ModifyJob(dueDate.JobId, j =>
                {
                    j.DueDate = DateTimeOffset.FromUnixTimeMilliseconds(dueDate.NewDueDateMs).DateTime;
                });
                break;

            case QuantityChangeEvent quantity:
                ModifyJob(quantity.JobId, j =>
                {
                    j.Quantity = (int)quantity.NewQuantity;
                });
                break;

            case PriorityChangeEvent priority:
                ModifyJob(priority.JobId, j =>
                {
                    j.Priority = priority.NewPriority;
                });
                break;

            case UrgentOrderEvent urgent:
                ModifyJob(urgent.JobId, j =>
                {
                    j.Priority = urgent.Priority;
                });
                break;

            // Material events - mark as HasChanges but don't modify state directly
            // (material constraints are handled by MaterialManager in ScheduleRequest)
            case MaterialShortageEvent:
            case MaterialDelayEvent:
            case MaterialArrivalEvent:
            case MaintenanceScheduledEvent:
            case WorkerSkillChangeEvent:
                HasChanges = true;
                break;
        }
    }

    // ===== 스케줄링 =====

    /// <summary>
    /// 재스케줄링 실행
    /// </summary>
    public Schedule Reschedule()
    {
        // Save current state to history
        if (_currentSchedule != null)
        {
            _history.Add(new ScheduleSnapshot
            {
                Timestamp = DateTime.UtcNow,
                Schedule = _currentSchedule,
                ModificationSummary = "Before reschedule"
            });
        }

        // Filter out disabled resources from operations
        var effectiveJobs = ApplyResourceConstraints(_workingJobs);
        var effectiveResources = _workingResources
            .Where(r => !_disabledResources.Contains(r.Id))
            .ToList();

        var request = new ScheduleRequest
        {
            Jobs = effectiveJobs,
            Resources = effectiveResources
        };

        var result = _client.Schedule(request);

        if (!result.Success || result.Schedule == null)
        {
            throw new InvalidOperationException(
                $"Scheduling failed: {result.Error ?? "Unknown error"}");
        }

        _currentSchedule = result.Schedule;
        HasChanges = false;

        return _currentSchedule;
    }

    private void RunInitialSchedule()
    {
        var request = new ScheduleRequest
        {
            Jobs = _workingJobs,
            Resources = _workingResources
        };

        var result = _client.Schedule(request);
        if (result.Success && result.Schedule != null)
        {
            _currentSchedule = result.Schedule;
        }
    }

    private List<Job> ApplyResourceConstraints(List<Job> jobs)
    {
        if (_disabledResources.Count == 0)
            return jobs;

        // Remove disabled resources from operation candidates
        var result = CloneJobs(jobs);
        foreach (var job in result)
        {
            foreach (var op in job.Operations)
            {
                foreach (var req in op.RequiredResources)
                {
                    req.Candidates.RemoveAll(c => _disabledResources.Contains(c));
                }
            }
        }
        return result;
    }

    // ===== 비교 =====

    /// <summary>
    /// 두 스케줄 비교
    /// </summary>
    public ScheduleComparison Compare(Schedule from, Schedule to)
    {
        var comparison = new ScheduleComparison
        {
            FromSchedule = from,
            ToSchedule = to,
            MakespanDelta = to.MakespanMs - from.MakespanMs,
            ViolationsDelta = to.Violations.Count - from.Violations.Count
        };

        // Compare assignments
        var fromAssignments = from.Assignments.ToDictionary(a => a.OperationId);
        var toAssignments = to.Assignments.ToDictionary(a => a.OperationId);

        foreach (var opId in fromAssignments.Keys.Union(toAssignments.Keys))
        {
            fromAssignments.TryGetValue(opId, out var fromA);
            toAssignments.TryGetValue(opId, out var toA);

            if (fromA == null && toA != null)
            {
                comparison.AffectedOperations.Add(new OperationDiff
                {
                    OperationId = opId,
                    ChangeType = OperationChangeType.Added,
                    ToStart = toA.StartMs,
                    ToEnd = toA.EndMs,
                    ToResource = toA.ResourceId
                });
            }
            else if (fromA != null && toA == null)
            {
                comparison.AffectedOperations.Add(new OperationDiff
                {
                    OperationId = opId,
                    ChangeType = OperationChangeType.Removed,
                    FromStart = fromA.StartMs,
                    FromEnd = fromA.EndMs,
                    FromResource = fromA.ResourceId
                });
            }
            else if (fromA != null && toA != null)
            {
                if (fromA.StartMs != toA.StartMs ||
                    fromA.EndMs != toA.EndMs ||
                    fromA.ResourceId != toA.ResourceId)
                {
                    comparison.AffectedOperations.Add(new OperationDiff
                    {
                        OperationId = opId,
                        ChangeType = OperationChangeType.Modified,
                        FromStart = fromA.StartMs,
                        FromEnd = fromA.EndMs,
                        FromResource = fromA.ResourceId,
                        ToStart = toA.StartMs,
                        ToEnd = toA.EndMs,
                        ToResource = toA.ResourceId,
                        StartDelta = toA.StartMs - fromA.StartMs,
                        EndDelta = toA.EndMs - fromA.EndMs,
                        ResourceChanged = fromA.ResourceId != toA.ResourceId
                    });
                }
            }
        }

        return comparison;
    }

    /// <summary>
    /// AS-IS vs 현재 스케줄 비교
    /// </summary>
    public ScheduleComparison CompareWithBaseline()
    {
        if (_baselineSchedule == null)
            throw new InvalidOperationException("No baseline schedule available");
        if (_currentSchedule == null)
            throw new InvalidOperationException("No current schedule available");

        return Compare(_baselineSchedule, _currentSchedule);
    }

    // ===== 상태 관리 =====

    /// <summary>
    /// AS-IS 상태로 복원
    /// </summary>
    public SimulationSession Reset()
    {
        _workingJobs = CloneJobs(_baselineJobs);
        _workingResources = CloneResources(_baselineResources);
        _disabledResources.Clear();
        _currentSchedule = _baselineSchedule;
        HasChanges = false;
        return this;
    }

    /// <summary>
    /// 이전 상태로 롤백
    /// </summary>
    public SimulationSession Rollback()
    {
        if (_history.Count == 0)
            throw new InvalidOperationException("No history to rollback to");

        var previous = _history[^1];
        _history.RemoveAt(_history.Count - 1);
        _currentSchedule = previous.Schedule;
        HasChanges = true;
        return this;
    }

    // ===== Deep Clone Helpers =====

    private static List<Job> CloneJobs(List<Job> jobs)
    {
        return jobs.Select(CloneJob).ToList();
    }

    private static Job CloneJob(Job job)
    {
        return new Job
        {
            Id = job.Id,
            Priority = job.Priority,
            DueDate = job.DueDate,
            EarliestStart = job.EarliestStart,
            Quantity = job.Quantity,
            IsSplittable = job.IsSplittable,
            OrderNumber = job.OrderNumber,
            ProductCode = job.ProductCode,
            ProductName = job.ProductName,
            Operations = job.Operations.Select(CloneOperation).ToList()
        };
    }

    private static Operation CloneOperation(Operation op)
    {
        return new Operation
        {
            Id = op.Id,
            JobId = op.JobId,
            Sequence = op.Sequence,
            Time = new OperationTime(op.Time.SetupMs, op.Time.ProcessMs, op.Time.WaitMs),
            RequiredResources = op.RequiredResources.Select(r => new ResourceRequirement
            {
                ResourceType = r.ResourceType,
                Quantity = r.Quantity,
                Candidates = [.. r.Candidates]
            }).ToList(),
            IsSplittable = op.IsSplittable,
            AllowParallel = op.AllowParallel,
            Dependencies = [.. op.Dependencies],
            Name = op.Name,
            ProcessCode = op.ProcessCode
        };
    }

    private static List<Resource> CloneResources(List<Resource> resources)
    {
        return resources.Select(r => new Resource
        {
            Id = r.Id,
            Name = r.Name,
            Kind = r.Kind,
            Capabilities = [.. r.Capabilities],
            Capacity = r.Capacity,
            Efficiency = r.Efficiency,
            CalendarId = r.CalendarId,
            Unavailable = r.Unavailable.Select(s => new TimeSlot(s.StartMs, s.EndMs)).ToList()
        }).ToList();
    }
}

/// <summary>
/// 스케줄 스냅샷 (히스토리용)
/// </summary>
public class ScheduleSnapshot
{
    public DateTime Timestamp { get; set; }
    public Schedule Schedule { get; set; } = new();
    public string ModificationSummary { get; set; } = string.Empty;
}

/// <summary>
/// 스케줄 비교 결과
/// </summary>
public class ScheduleComparison
{
    public Schedule FromSchedule { get; set; } = new();
    public Schedule ToSchedule { get; set; } = new();
    public long MakespanDelta { get; set; }
    public int ViolationsDelta { get; set; }
    public List<OperationDiff> AffectedOperations { get; set; } = [];

    /// <summary>
    /// Makespan 변화 (분 단위)
    /// </summary>
    public double MakespanDeltaMinutes => MakespanDelta / 60000.0;

    /// <summary>
    /// 영향받은 공정 수
    /// </summary>
    public int AffectedCount => AffectedOperations.Count;

    /// <summary>
    /// 변경 요약
    /// </summary>
    public string Summary =>
        $"Makespan: {(MakespanDelta >= 0 ? "+" : "")}{MakespanDeltaMinutes:F1}분, " +
        $"영향 공정: {AffectedCount}개, " +
        $"위반 변화: {(ViolationsDelta >= 0 ? "+" : "")}{ViolationsDelta}";
}

/// <summary>
/// Operation 변경 유형
/// </summary>
public enum OperationChangeType
{
    Added,
    Removed,
    Modified
}

/// <summary>
/// Operation 변경 상세
/// </summary>
public class OperationDiff
{
    public string OperationId { get; set; } = string.Empty;
    public OperationChangeType ChangeType { get; set; }

    // From (AS-IS)
    public long FromStart { get; set; }
    public long FromEnd { get; set; }
    public string? FromResource { get; set; }

    // To (TO-BE)
    public long ToStart { get; set; }
    public long ToEnd { get; set; }
    public string? ToResource { get; set; }

    // Delta
    public long StartDelta { get; set; }
    public long EndDelta { get; set; }
    public bool ResourceChanged { get; set; }

    public double StartDeltaMinutes => StartDelta / 60000.0;
    public double EndDeltaMinutes => EndDelta / 60000.0;
}
