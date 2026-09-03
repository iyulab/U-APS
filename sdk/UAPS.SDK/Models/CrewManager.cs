using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace UAPS.SDK.Models;

/// <summary>
/// 팀/Crew 정보
/// </summary>
public class Crew
{
    /// <summary>팀 ID</summary>
    public string Id { get; set; } = string.Empty;

    /// <summary>팀 이름</summary>
    public string Name { get; set; } = string.Empty;

    /// <summary>팀원 ID 목록</summary>
    public List<string> MemberIds { get; set; } = new();

    /// <summary>팀 리더 ID</summary>
    public string? LeaderId { get; set; }

    /// <summary>담당 공정 유형 목록</summary>
    public List<string> AssignedOperationTypes { get; set; } = new();

    /// <summary>담당 장비 목록</summary>
    public List<string> AssignedEquipmentIds { get; set; } = new();

    /// <summary>교대 근무 ID</summary>
    public string? ShiftId { get; set; }

    /// <summary>최소 인원 (운영 가능 최소)</summary>
    public int MinMembers { get; set; } = 1;

    /// <summary>
    /// 팀 생성
    /// </summary>
    public static Crew Create(string id, string name)
    {
        return new Crew { Id = id, Name = name };
    }

    /// <summary>
    /// 팀원 추가
    /// </summary>
    public Crew WithMember(string workerId)
    {
        if (!MemberIds.Contains(workerId))
            MemberIds.Add(workerId);
        return this;
    }

    /// <summary>
    /// 여러 팀원 추가
    /// </summary>
    public Crew WithMembers(params string[] workerIds)
    {
        foreach (var id in workerIds)
            WithMember(id);
        return this;
    }

    /// <summary>
    /// 리더 설정
    /// </summary>
    public Crew WithLeader(string workerId)
    {
        LeaderId = workerId;
        if (!MemberIds.Contains(workerId))
            MemberIds.Add(workerId);
        return this;
    }

    /// <summary>
    /// 담당 공정 유형 설정
    /// </summary>
    public Crew WithOperationType(string operationType)
    {
        if (!AssignedOperationTypes.Contains(operationType))
            AssignedOperationTypes.Add(operationType);
        return this;
    }

    /// <summary>
    /// 담당 장비 설정
    /// </summary>
    public Crew WithEquipment(string equipmentId)
    {
        if (!AssignedEquipmentIds.Contains(equipmentId))
            AssignedEquipmentIds.Add(equipmentId);
        return this;
    }

    /// <summary>
    /// 교대 근무 설정
    /// </summary>
    public Crew WithShift(string shiftId)
    {
        ShiftId = shiftId;
        return this;
    }

    /// <summary>
    /// 최소 인원 설정
    /// </summary>
    public Crew WithMinMembers(int min)
    {
        MinMembers = Math.Max(1, min);
        return this;
    }

    /// <summary>
    /// 특정 공정 유형 담당 여부
    /// </summary>
    public bool IsAssignedTo(string operationType)
    {
        return AssignedOperationTypes.Count == 0 || AssignedOperationTypes.Contains(operationType);
    }

    /// <summary>
    /// 특정 장비 담당 여부
    /// </summary>
    public bool IsAssignedToEquipment(string equipmentId)
    {
        return AssignedEquipmentIds.Count == 0 || AssignedEquipmentIds.Contains(equipmentId);
    }
}

/// <summary>
/// 팀/Crew 관리자
/// </summary>
public class CrewManager
{
    private readonly List<Crew> _crews = new();
    private readonly Dictionary<string, string> _workerToCrewMap = new();

    /// <summary>
    /// 팀 추가
    /// </summary>
    public CrewManager WithCrew(Crew crew)
    {
        _crews.Add(crew);
        foreach (var memberId in crew.MemberIds)
        {
            _workerToCrewMap[memberId] = crew.Id;
        }
        return this;
    }

    /// <summary>
    /// 팀 생성 및 추가
    /// </summary>
    public CrewManager WithCrew(string id, string name, params string[] memberIds)
    {
        var crew = Crew.Create(id, name).WithMembers(memberIds);
        return WithCrew(crew);
    }

    /// <summary>
    /// 작업자의 팀 조회
    /// </summary>
    public Crew? GetCrewForWorker(string workerId)
    {
        if (_workerToCrewMap.TryGetValue(workerId, out var crewId))
        {
            return _crews.Find(c => c.Id == crewId);
        }
        return null;
    }

    /// <summary>
    /// 팀 ID로 조회
    /// </summary>
    public Crew? GetCrew(string crewId)
    {
        return _crews.Find(c => c.Id == crewId);
    }

    /// <summary>
    /// 모든 팀 목록
    /// </summary>
    public IReadOnlyList<Crew> GetAllCrews() => _crews.AsReadOnly();

    /// <summary>
    /// 특정 공정 유형 담당 팀 목록
    /// </summary>
    public List<Crew> GetCrewsForOperationType(string operationType)
    {
        return _crews.FindAll(c => c.IsAssignedTo(operationType));
    }

    /// <summary>
    /// 특정 장비 담당 팀 목록
    /// </summary>
    public List<Crew> GetCrewsForEquipment(string equipmentId)
    {
        return _crews.FindAll(c => c.IsAssignedToEquipment(equipmentId));
    }

    /// <summary>
    /// 같은 팀 여부 확인
    /// </summary>
    public bool AreInSameCrew(string workerId1, string workerId2)
    {
        if (!_workerToCrewMap.TryGetValue(workerId1, out var crew1)) return false;
        if (!_workerToCrewMap.TryGetValue(workerId2, out var crew2)) return false;
        return crew1 == crew2;
    }

    /// <summary>
    /// 팀의 가용 작업자 수 계산
    /// </summary>
    public int GetAvailableMemberCount(string crewId, HashSet<string>? busyWorkerIds = null)
    {
        var crew = GetCrew(crewId);
        if (crew == null) return 0;

        if (busyWorkerIds == null) return crew.MemberIds.Count;

        int available = 0;
        foreach (var memberId in crew.MemberIds)
        {
            if (!busyWorkerIds.Contains(memberId))
                available++;
        }
        return available;
    }

    /// <summary>
    /// 팀 운영 가능 여부
    /// </summary>
    public bool IsCrewOperational(string crewId, HashSet<string>? busyWorkerIds = null)
    {
        var crew = GetCrew(crewId);
        if (crew == null) return false;

        var available = GetAvailableMemberCount(crewId, busyWorkerIds);
        return available >= crew.MinMembers;
    }

    /// <summary>
    /// 작업 할당 가능한 팀원 목록
    /// </summary>
    public List<string> GetAvailableMembers(string crewId, HashSet<string>? busyWorkerIds = null)
    {
        var crew = GetCrew(crewId);
        if (crew == null) return new List<string>();

        var result = new List<string>();
        foreach (var memberId in crew.MemberIds)
        {
            if (busyWorkerIds == null || !busyWorkerIds.Contains(memberId))
                result.Add(memberId);
        }
        return result;
    }

    /// <summary>
    /// DTO 변환
    /// </summary>
    internal CrewManagerDto ToDto()
    {
        var crewDtos = new List<CrewDto>();
        foreach (var crew in _crews)
        {
            crewDtos.Add(new CrewDto
            {
                Id = crew.Id,
                Name = crew.Name,
                MemberIds = crew.MemberIds,
                LeaderId = crew.LeaderId,
                AssignedOperationTypes = crew.AssignedOperationTypes,
                AssignedEquipmentIds = crew.AssignedEquipmentIds,
                ShiftId = crew.ShiftId,
                MinMembers = crew.MinMembers
            });
        }

        return new CrewManagerDto { Crews = crewDtos };
    }
}

#region DTOs

/// <summary>
/// Crew 관리자 DTO
/// </summary>
internal class CrewManagerDto
{
    [JsonPropertyName("crews")]
    public List<CrewDto> Crews { get; set; } = new();
}

/// <summary>
/// Crew DTO
/// </summary>
internal class CrewDto
{
    [JsonPropertyName("id")]
    public string Id { get; set; } = string.Empty;

    [JsonPropertyName("name")]
    public string Name { get; set; } = string.Empty;

    [JsonPropertyName("member_ids")]
    public List<string> MemberIds { get; set; } = new();

    [JsonPropertyName("leader_id")]
    public string? LeaderId { get; set; }

    [JsonPropertyName("assigned_operation_types")]
    public List<string> AssignedOperationTypes { get; set; } = new();

    [JsonPropertyName("assigned_equipment_ids")]
    public List<string> AssignedEquipmentIds { get; set; } = new();

    [JsonPropertyName("shift_id")]
    public string? ShiftId { get; set; }

    [JsonPropertyName("min_members")]
    public int MinMembers { get; set; } = 1;
}

#endregion
