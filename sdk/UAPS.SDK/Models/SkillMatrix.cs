using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace UAPS.SDK.Models;

/// <summary>
/// 스킬 레벨
/// </summary>
public enum SkillLevel
{
    /// <summary>수행 불가</summary>
    None,
    /// <summary>초급 (효율 50%)</summary>
    Beginner,
    /// <summary>중급 (효율 75%)</summary>
    Intermediate,
    /// <summary>고급 (효율 100%)</summary>
    Advanced,
    /// <summary>전문가 (효율 120%)</summary>
    Expert
}

/// <summary>
/// 스킬 레벨 확장 메서드
/// </summary>
public static class SkillLevelExtensions
{
    /// <summary>
    /// 효율성 값 반환 (0.0 ~ 1.2)
    /// </summary>
    public static double GetEfficiency(this SkillLevel level) => level switch
    {
        SkillLevel.None => 0.0,
        SkillLevel.Beginner => 0.5,
        SkillLevel.Intermediate => 0.75,
        SkillLevel.Advanced => 1.0,
        SkillLevel.Expert => 1.2,
        _ => 1.0
    };

    /// <summary>
    /// 숙련도 값에서 스킬 레벨 생성
    /// </summary>
    public static SkillLevel FromProficiency(double proficiency)
    {
        if (proficiency <= 0.0) return SkillLevel.None;
        if (proficiency < 0.6) return SkillLevel.Beginner;
        if (proficiency < 0.9) return SkillLevel.Intermediate;
        if (proficiency < 1.1) return SkillLevel.Advanced;
        return SkillLevel.Expert;
    }
}

/// <summary>
/// 스킬 매트릭스 - 작업자별 공정 유형에 대한 숙련도 관리
/// </summary>
public class SkillMatrix
{
    private readonly Dictionary<(string WorkerId, string OperationType), double> _proficiencies = new();
    private double _defaultProficiency = 1.0;

    /// <summary>
    /// 기본 숙련도 설정
    /// </summary>
    public SkillMatrix WithDefault(double proficiency)
    {
        _defaultProficiency = Math.Clamp(proficiency, 0.0, 2.0);
        return this;
    }

    /// <summary>
    /// 숙련도 추가
    /// </summary>
    public SkillMatrix WithSkill(string workerId, string operationType, double proficiency)
    {
        var key = (workerId, operationType);
        _proficiencies[key] = Math.Clamp(proficiency, 0.0, 2.0);
        return this;
    }

    /// <summary>
    /// 숙련도 조회
    /// </summary>
    public double GetProficiency(string workerId, string operationType)
    {
        var key = (workerId, operationType);
        return _proficiencies.TryGetValue(key, out var prof) ? prof : _defaultProficiency;
    }

    /// <summary>
    /// 스킬 레벨 조회
    /// </summary>
    public SkillLevel GetSkillLevel(string workerId, string operationType)
    {
        var proficiency = GetProficiency(workerId, operationType);
        return SkillLevelExtensions.FromProficiency(proficiency);
    }

    /// <summary>
    /// 작업자가 특정 공정을 수행할 수 있는지 확인
    /// </summary>
    public bool CanPerform(string workerId, string operationType)
    {
        return GetProficiency(workerId, operationType) > 0.0;
    }

    /// <summary>
    /// 특정 공정을 수행할 수 있는 작업자 목록 반환
    /// </summary>
    public List<(string WorkerId, double Proficiency)> GetQualifiedWorkers(string operationType)
    {
        var result = new List<(string, double)>();
        foreach (var kvp in _proficiencies)
        {
            if (kvp.Key.OperationType == operationType && kvp.Value > 0.0)
            {
                result.Add((kvp.Key.WorkerId, kvp.Value));
            }
        }
        return result;
    }

    /// <summary>
    /// 처리 시간 계산 (숙련도 적용)
    /// </summary>
    public long? CalculateProcessTime(string workerId, string operationType, long baseTimeMs)
    {
        var proficiency = GetProficiency(workerId, operationType);
        if (proficiency <= 0.0) return null;
        return (long)(baseTimeMs / proficiency);
    }

    /// <summary>
    /// 최적 작업자 선택 (가장 높은 숙련도)
    /// </summary>
    public (string WorkerId, double Proficiency)? GetBestWorker(string operationType)
    {
        var workers = GetQualifiedWorkers(operationType);
        if (workers.Count == 0) return null;

        var best = workers[0];
        foreach (var worker in workers)
        {
            if (worker.Proficiency > best.Proficiency)
                best = worker;
        }
        return best;
    }

    /// <summary>
    /// DTO 변환
    /// </summary>
    internal SkillMatrixDto ToDto()
    {
        var entries = new List<SkillEntryDto>();
        foreach (var kvp in _proficiencies)
        {
            entries.Add(new SkillEntryDto
            {
                WorkerId = kvp.Key.WorkerId,
                OperationType = kvp.Key.OperationType,
                Proficiency = kvp.Value
            });
        }

        return new SkillMatrixDto
        {
            DefaultProficiency = _defaultProficiency,
            Entries = entries
        };
    }
}

/// <summary>
/// 학습 곡선 효과 (경험에 따른 숙련도 증가)
/// </summary>
public class LearningCurve
{
    /// <summary>초기 숙련도</summary>
    public double InitialProficiency { get; }

    /// <summary>최대 숙련도</summary>
    public double MaxProficiency { get; }

    /// <summary>학습률 (0.0 ~ 1.0)</summary>
    public double LearningRate { get; }

    /// <summary>현재 경험 (작업 횟수)</summary>
    public uint Experience { get; private set; }

    public LearningCurve(double initial, double max, double rate)
    {
        InitialProficiency = Math.Clamp(initial, 0.0, 2.0);
        MaxProficiency = Math.Clamp(max, initial, 2.0);
        LearningRate = Math.Clamp(rate, 0.0, 1.0);
        Experience = 0;
    }

    /// <summary>
    /// 현재 숙련도 계산
    /// </summary>
    public double CurrentProficiency
    {
        get
        {
            // 지수적 학습 곡선: P(n) = P_max - (P_max - P_init) * e^(-rate * n)
            var range = MaxProficiency - InitialProficiency;
            var decay = Math.Exp(-LearningRate * Experience);
            return MaxProficiency - range * decay;
        }
    }

    /// <summary>
    /// 경험 추가
    /// </summary>
    public void AddExperience(uint count)
    {
        Experience += count;
    }
}

#region DTOs

/// <summary>
/// 스킬 매트릭스 DTO
/// </summary>
internal class SkillMatrixDto
{
    [JsonPropertyName("default_proficiency")]
    public double DefaultProficiency { get; set; } = 1.0;

    [JsonPropertyName("proficiencies")]
    public List<SkillEntryDto> Entries { get; set; } = new();
}

/// <summary>
/// 스킬 항목 DTO
/// </summary>
internal class SkillEntryDto
{
    [JsonPropertyName("worker_id")]
    public string WorkerId { get; set; } = string.Empty;

    [JsonPropertyName("operation_type")]
    public string OperationType { get; set; } = string.Empty;

    [JsonPropertyName("proficiency")]
    public double Proficiency { get; set; }
}

#endregion
