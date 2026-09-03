using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace UAPS.SDK.Models;

/// <summary>
/// 인증 레벨
/// </summary>
public enum CertificationLevel
{
    /// <summary>인증 없음</summary>
    None = 0,
    /// <summary>기본 인증 (효율 0.8)</summary>
    Basic = 1,
    /// <summary>고급 인증 (효율 1.0)</summary>
    Advanced = 2,
    /// <summary>전문가 인증 (효율 1.15)</summary>
    Expert = 3,
    /// <summary>마스터 인증 (효율 1.3)</summary>
    Master = 4
}

/// <summary>
/// 인증 대상 유형
/// </summary>
public enum CertificationTarget
{
    /// <summary>공정 유형</summary>
    OperationType,
    /// <summary>장비</summary>
    Equipment,
    /// <summary>제품</summary>
    Product
}

/// <summary>
/// 인증 레벨 확장 메서드
/// </summary>
public static class CertificationLevelExtensions
{
    /// <summary>
    /// 효율성 값 반환
    /// </summary>
    public static double GetEfficiency(this CertificationLevel level) => level switch
    {
        CertificationLevel.None => 0.0,
        CertificationLevel.Basic => 0.8,
        CertificationLevel.Advanced => 1.0,
        CertificationLevel.Expert => 1.15,
        CertificationLevel.Master => 1.3,
        _ => 1.0
    };
}

/// <summary>
/// 개별 인증 정보
/// </summary>
public class Certification
{
    /// <summary>인증 ID</summary>
    public string Id { get; set; } = string.Empty;

    /// <summary>작업자 ID</summary>
    public string WorkerId { get; set; } = string.Empty;

    /// <summary>대상 ID (공정유형/장비/제품)</summary>
    public string TargetId { get; set; } = string.Empty;

    /// <summary>대상 유형</summary>
    public CertificationTarget Target { get; set; } = CertificationTarget.OperationType;

    /// <summary>인증 레벨</summary>
    public CertificationLevel Level { get; set; } = CertificationLevel.None;

    /// <summary>발급일 (ms timestamp)</summary>
    public long IssuedAtMs { get; set; }

    /// <summary>만료일 (ms timestamp, null이면 무기한)</summary>
    public long? ExpiresAtMs { get; set; }

    /// <summary>
    /// 인증 생성
    /// </summary>
    public static Certification Create(string id, string workerId, string targetId,
        CertificationTarget target, CertificationLevel level, long issuedAtMs)
    {
        return new Certification
        {
            Id = id,
            WorkerId = workerId,
            TargetId = targetId,
            Target = target,
            Level = level,
            IssuedAtMs = issuedAtMs
        };
    }

    /// <summary>
    /// 만료일 설정
    /// </summary>
    public Certification WithExpiry(long expiresAtMs)
    {
        ExpiresAtMs = expiresAtMs;
        return this;
    }

    /// <summary>
    /// 만료일 설정 (일 단위)
    /// </summary>
    public Certification WithExpiryDays(int days)
    {
        ExpiresAtMs = IssuedAtMs + (long)days * 24 * 3600 * 1000;
        return this;
    }

    /// <summary>
    /// 유효성 확인
    /// </summary>
    public bool IsValid(long currentTimeMs)
    {
        if (Level == CertificationLevel.None) return false;
        if (ExpiresAtMs.HasValue && currentTimeMs > ExpiresAtMs.Value) return false;
        return true;
    }
}

/// <summary>
/// 인증 매트릭스 - 작업자 자격 인증 관리
/// </summary>
public class CertificationMatrix
{
    private readonly List<Certification> _certifications = new();

    /// <summary>
    /// 인증 추가
    /// </summary>
    public CertificationMatrix WithCertification(Certification cert)
    {
        _certifications.Add(cert);
        return this;
    }

    /// <summary>
    /// 공정 유형 인증 추가 (간편 메서드)
    /// </summary>
    public CertificationMatrix WithOperationCert(string workerId, string operationType,
        CertificationLevel level, long issuedAtMs)
    {
        var cert = Certification.Create(
            $"CERT-{workerId}-{operationType}",
            workerId,
            operationType,
            CertificationTarget.OperationType,
            level,
            issuedAtMs
        );
        return WithCertification(cert);
    }

    /// <summary>
    /// 장비 인증 추가 (간편 메서드)
    /// </summary>
    public CertificationMatrix WithEquipmentCert(string workerId, string equipmentId,
        CertificationLevel level, long issuedAtMs)
    {
        var cert = Certification.Create(
            $"CERT-{workerId}-{equipmentId}",
            workerId,
            equipmentId,
            CertificationTarget.Equipment,
            level,
            issuedAtMs
        );
        return WithCertification(cert);
    }

    /// <summary>
    /// 인증 여부 확인
    /// </summary>
    public bool IsCertified(string workerId, string targetId, long currentTimeMs)
    {
        foreach (var cert in _certifications)
        {
            if (cert.WorkerId == workerId && cert.TargetId == targetId && cert.IsValid(currentTimeMs))
            {
                return true;
            }
        }
        return false;
    }

    /// <summary>
    /// 인증 레벨 조회
    /// </summary>
    public CertificationLevel GetCertificationLevel(string workerId, string targetId, long currentTimeMs)
    {
        CertificationLevel highest = CertificationLevel.None;
        foreach (var cert in _certifications)
        {
            if (cert.WorkerId == workerId && cert.TargetId == targetId && cert.IsValid(currentTimeMs))
            {
                if (cert.Level > highest)
                    highest = cert.Level;
            }
        }
        return highest;
    }

    /// <summary>
    /// 처리 시간 계산 (인증 레벨 적용)
    /// </summary>
    public long? CalculateProcessTime(string workerId, string targetId, long baseTimeMs, long currentTimeMs)
    {
        var level = GetCertificationLevel(workerId, targetId, currentTimeMs);
        if (level == CertificationLevel.None) return null;

        var efficiency = level.GetEfficiency();
        return (long)(baseTimeMs / efficiency);
    }

    /// <summary>
    /// 특정 대상에 인증된 작업자 목록
    /// </summary>
    public List<(string WorkerId, CertificationLevel Level)> GetCertifiedWorkers(string targetId, long currentTimeMs)
    {
        var result = new List<(string, CertificationLevel)>();
        var seen = new HashSet<string>();

        foreach (var cert in _certifications)
        {
            if (cert.TargetId == targetId && cert.IsValid(currentTimeMs) && !seen.Contains(cert.WorkerId))
            {
                result.Add((cert.WorkerId, cert.Level));
                seen.Add(cert.WorkerId);
            }
        }
        return result;
    }

    /// <summary>
    /// 만료 예정 인증 목록 (경고용)
    /// </summary>
    public List<Certification> GetExpiringCertifications(long currentTimeMs, int daysAhead = 30)
    {
        var result = new List<Certification>();
        var futureMs = currentTimeMs + (long)daysAhead * 24 * 3600 * 1000;

        foreach (var cert in _certifications)
        {
            if (cert.ExpiresAtMs.HasValue &&
                cert.ExpiresAtMs.Value > currentTimeMs &&
                cert.ExpiresAtMs.Value <= futureMs)
            {
                result.Add(cert);
            }
        }
        return result;
    }

    /// <summary>
    /// DTO 변환
    /// </summary>
    internal CertificationMatrixDto ToDto()
    {
        var entries = new List<CertificationEntryDto>();
        foreach (var cert in _certifications)
        {
            entries.Add(new CertificationEntryDto
            {
                Id = cert.Id,
                WorkerId = cert.WorkerId,
                TargetId = cert.TargetId,
                Target = cert.Target.ToString().ToLowerInvariant(),
                Level = cert.Level.ToString().ToLowerInvariant(),
                IssuedAtMs = cert.IssuedAtMs,
                ExpiresAtMs = cert.ExpiresAtMs
            });
        }

        return new CertificationMatrixDto { Certifications = entries };
    }
}

#region DTOs

/// <summary>
/// 인증 매트릭스 DTO
/// </summary>
internal class CertificationMatrixDto
{
    [JsonPropertyName("certifications")]
    public List<CertificationEntryDto> Certifications { get; set; } = new();
}

/// <summary>
/// 인증 항목 DTO
/// </summary>
internal class CertificationEntryDto
{
    [JsonPropertyName("id")]
    public string Id { get; set; } = string.Empty;

    [JsonPropertyName("worker_id")]
    public string WorkerId { get; set; } = string.Empty;

    [JsonPropertyName("target_id")]
    public string TargetId { get; set; } = string.Empty;

    [JsonPropertyName("target")]
    public string Target { get; set; } = "operation_type";

    [JsonPropertyName("level")]
    public string Level { get; set; } = "none";

    [JsonPropertyName("issued_at_ms")]
    public long IssuedAtMs { get; set; }

    [JsonPropertyName("expires_at_ms")]
    public long? ExpiresAtMs { get; set; }
}

#endregion
