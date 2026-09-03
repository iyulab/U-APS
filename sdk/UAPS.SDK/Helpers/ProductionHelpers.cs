using UAPS.SDK.Models;

namespace UAPS.SDK.Helpers;

/// <summary>
/// 생산 용어로 접근하는 Alias Helper 함수들
/// </summary>
public static class ProductionHelpers
{
    // ===== Job Aliases =====

    /// <summary>생산 프로젝트 생성</summary>
    public static Job CreateProject(string name, DateTime dueDate) =>
        Job.Create(Guid.NewGuid().ToString())
            .WithDueDate(dueDate);

    /// <summary>생산 오더 생성</summary>
    public static Job CreateOrder(string orderId, int priority = 100) =>
        Job.Create(orderId)
            .WithPriority(priority);

    /// <summary>긴급 오더 생성</summary>
    public static Job CreateUrgentOrder(string orderId) =>
        Job.Create(orderId)
            .WithPriority(1);

    // ===== Operation Aliases =====

    /// <summary>작업 태스크 생성</summary>
    public static Operation CreateTask(string name, TimeSpan duration) =>
        Operation.Create(Guid.NewGuid().ToString(), "", 0)
            .WithTime(0, (long)duration.TotalMilliseconds, 0);

    /// <summary>공정 단계 생성</summary>
    public static Operation CreateProcessStep(
        int sequence,
        TimeSpan setupTime,
        TimeSpan processTime,
        TimeSpan waitTime,
        params string[] equipmentIds)
    {
        var op = Operation.Create(Guid.NewGuid().ToString(), "", sequence)
            .WithTime(
                (long)setupTime.TotalMilliseconds,
                (long)processTime.TotalMilliseconds,
                (long)waitTime.TotalMilliseconds
            );

        if (equipmentIds.Length > 0)
            op.WithEquipment(equipmentIds);

        return op;
    }

    /// <summary>공정 단계 생성 (분 단위)</summary>
    public static Operation CreateProcessStepMinutes(
        int sequence,
        int setupMinutes,
        int processMinutes,
        int waitMinutes,
        params string[] equipmentIds) =>
        CreateProcessStep(
            sequence,
            TimeSpan.FromMinutes(setupMinutes),
            TimeSpan.FromMinutes(processMinutes),
            TimeSpan.FromMinutes(waitMinutes),
            equipmentIds
        );

    // ===== Resource Aliases =====

    /// <summary>설비 생성</summary>
    public static Resource CreateEquipment(string id, string name) =>
        Resource.Equipment(id)
            .WithName(name);

    /// <summary>작업자 생성</summary>
    public static Resource CreateWorker(string id, string name, double skillLevel = 1.0) =>
        Resource.Worker(id)
            .WithName(name)
            .WithEfficiency(skillLevel);

    /// <summary>숙련공 생성 (효율 1.2)</summary>
    public static Resource CreateSkilledWorker(string id, string name) =>
        CreateWorker(id, name, 1.2);

    /// <summary>신입 생성 (효율 0.8)</summary>
    public static Resource CreateNoviceWorker(string id, string name) =>
        CreateWorker(id, name, 0.8);

    // ===== Time Helpers =====

    /// <summary>PM 일정 추가</summary>
    public static Resource AddPMSchedule(
        this Resource resource,
        DateTime start,
        DateTime end)
    {
        var startMs = new DateTimeOffset(start).ToUnixTimeMilliseconds();
        var endMs = new DateTimeOffset(end).ToUnixTimeMilliseconds();
        return resource.WithUnavailable(new TimeSlot(startMs, endMs));
    }

    /// <summary>납기 설정</summary>
    public static Job SetDueDate(Job job, DateTime dueDate)
    {
        job.DueDate = dueDate;
        return job;
    }

    /// <summary>우선순위 설정</summary>
    public static Job SetPriority(Job job, int priority)
    {
        job.Priority = priority;
        return job;
    }

    // ===== TimeSpan Extensions =====

    /// <summary>분 단위로 TimeSpan 생성</summary>
    public static TimeSpan Minutes(this int value) =>
        TimeSpan.FromMinutes(value);

    /// <summary>시간 단위로 TimeSpan 생성</summary>
    public static TimeSpan Hours(this int value) =>
        TimeSpan.FromHours(value);

    /// <summary>초 단위로 TimeSpan 생성</summary>
    public static TimeSpan Seconds(this int value) =>
        TimeSpan.FromSeconds(value);
}
