namespace UAPS.SDK.Models;

/// <summary>
/// 자원 종류
/// </summary>
public enum ResourceKind
{
    Equipment,
    Worker
}

/// <summary>
/// 시간 슬롯
/// </summary>
public record TimeSlot(long StartMs, long EndMs)
{
    public long DurationMs => EndMs - StartMs;

    public bool Contains(long timeMs) => timeMs >= StartMs && timeMs < EndMs;

    public bool Overlaps(TimeSlot other) =>
        StartMs < other.EndMs && other.StartMs < EndMs;
}

/// <summary>
/// Resource - 자원 (장비/작업자)
/// </summary>
public class Resource
{
    // ===== Engine Properties (→ Rust) =====

    public string Id { get; set; } = string.Empty;
    public ResourceKind Kind { get; set; }
    public List<string> Capabilities { get; set; } = [];
    public int Capacity { get; set; } = 1;
    public double Efficiency { get; set; } = 1.0;
    public string CalendarId { get; set; } = string.Empty;
    public List<TimeSlot> Unavailable { get; set; } = [];
    /// <summary>
    /// 연결된 자원 ID (로봇-설비 연동 등)
    /// </summary>
    public string? LinkedResourceId { get; set; }
    /// <summary>
    /// 로봇 여부
    /// </summary>
    public bool IsRobot { get; set; }

    // ===== SDK-Only Properties (분석용) =====

    public string Name { get; set; } = string.Empty;
    public string Location { get; set; } = string.Empty;
    public string Manufacturer { get; set; } = string.Empty;
    public string Model { get; set; } = string.Empty;
    public string SerialNumber { get; set; } = string.Empty;
    public DateTime? AcquisitionDate { get; set; }
    public string Status { get; set; } = string.Empty;
    public string ManagerId { get; set; } = string.Empty;
    public decimal HourlyCost { get; set; }

    // Worker-specific SDK properties
    public string EmployeeId { get; set; } = string.Empty;
    public string Department { get; set; } = string.Empty;
    public string Position { get; set; } = string.Empty;
    public DateTime? HireDate { get; set; }
    public string Contact { get; set; } = string.Empty;
    public List<string> Certifications { get; set; } = [];
    public string TeamId { get; set; } = string.Empty;

    // ===== Computed =====

    public bool IsAvailableAt(long timeMs) =>
        !Unavailable.Any(slot => slot.Contains(timeMs));

    public bool HasCapability(string capability) =>
        Capabilities.Contains(capability);

    public long AdjustedTimeMs(long baseTimeMs) =>
        (long)(baseTimeMs / Efficiency);

    // ===== Factory Methods =====

    public static Resource Equipment(string id) => new()
    {
        Id = id,
        Kind = ResourceKind.Equipment
    };

    public static Resource Worker(string id) => new()
    {
        Id = id,
        Kind = ResourceKind.Worker
    };

    // ===== Builder Methods =====

    public Resource WithCapability(string capability)
    {
        Capabilities.Add(capability);
        return this;
    }

    public Resource WithEfficiency(double efficiency)
    {
        Efficiency = efficiency;
        return this;
    }

    public Resource WithCapacity(int capacity)
    {
        Capacity = capacity;
        return this;
    }

    public Resource WithUnavailable(TimeSlot slot)
    {
        Unavailable.Add(slot);
        return this;
    }

    public Resource WithName(string name)
    {
        Name = name;
        return this;
    }

    /// <summary>
    /// 연결 자원 설정 (로봇-설비 연동)
    /// </summary>
    public Resource WithLinkedResource(string resourceId)
    {
        LinkedResourceId = resourceId;
        return this;
    }

    /// <summary>
    /// 로봇으로 설정
    /// </summary>
    public Resource AsRobot()
    {
        IsRobot = true;
        return this;
    }

    // ===== Engine DTO Conversion =====

    public ResourceDto ToEngineDto() => new()
    {
        Id = Id,
        Kind = Kind.ToString(),
        Capabilities = Capabilities,
        Capacity = Capacity,
        Efficiency = Efficiency,
        Availability = [], // Default empty availability
        Unavailable = Unavailable.Select(s => new TimeSlotDto
        {
            StartMs = s.StartMs,
            EndMs = s.EndMs
        }).ToList(),
        LinkedResourceId = LinkedResourceId,
        IsRobot = IsRobot
    };
}

/// <summary>
/// Engine DTO (Rust로 전송)
/// </summary>
public class ResourceDto
{
    public string Id { get; set; } = string.Empty;
    public string Kind { get; set; } = string.Empty;
    public List<string> Capabilities { get; set; } = [];
    public int Capacity { get; set; }
    public double Efficiency { get; set; }
    public List<TimeSlotDto> Availability { get; set; } = [];
    public List<TimeSlotDto> Unavailable { get; set; } = [];
    public string? LinkedResourceId { get; set; }
    public bool IsRobot { get; set; }
}

public class TimeSlotDto
{
    public long StartMs { get; set; }
    public long EndMs { get; set; }
}
