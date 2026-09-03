namespace UAPS.SDK.Models;

/// <summary>
/// Job - 스케줄링 대상 작업 단위 (생산 오더)
/// </summary>
public class Job
{
    // ===== Engine Properties (→ Rust) =====

    public string Id { get; set; } = string.Empty;
    public List<Operation> Operations { get; set; } = [];
    public int Priority { get; set; } = 100;
    public DateTime? DueDate { get; set; }
    public DateTime? EarliestStart { get; set; }
    public int Quantity { get; set; } = 1;
    public bool IsSplittable { get; set; }

    // ===== SDK-Only Properties (분석용) =====

    public string OrderNumber { get; set; } = string.Empty;
    public string ProductCode { get; set; } = string.Empty;
    public string ProductName { get; set; } = string.Empty;
    public string CustomerId { get; set; } = string.Empty;
    public string CustomerName { get; set; } = string.Empty;
    public string SalesOrder { get; set; } = string.Empty;
    public decimal UnitPrice { get; set; }
    public string Notes { get; set; } = string.Empty;
    public string CreatedBy { get; set; } = string.Empty;
    public DateTime CreatedAt { get; set; } = DateTime.UtcNow;

    // ===== Computed =====

    public long TotalTimeMs => Operations.Sum(op => op.TotalTimeMs);
    public int OperationCount => Operations.Count;
    public decimal TotalAmount => UnitPrice * Quantity;

    // ===== Builder Methods =====

    public static Job Create(string id) => new() { Id = id };

    public Job WithOperation(Operation operation)
    {
        Operations.Add(operation);
        return this;
    }

    public Job WithOperations(params Operation[] operations)
    {
        Operations.AddRange(operations);
        return this;
    }

    public Job WithPriority(int priority)
    {
        Priority = priority;
        return this;
    }

    public Job WithDueDate(DateTime dueDate)
    {
        DueDate = dueDate;
        return this;
    }

    public Job WithQuantity(int quantity)
    {
        Quantity = quantity;
        return this;
    }

    // ===== Engine DTO Conversion =====

    public JobDto ToEngineDto() => new()
    {
        Id = Id,
        Operations = Operations.Select(op => op.ToEngineDto()).ToList(),
        Priority = Priority,
        DueDate = DueDate?.ToUniversalTime(),
        EarliestStart = EarliestStart?.ToUniversalTime(),
        Quantity = Quantity,
        IsSplittable = IsSplittable
    };
}

/// <summary>
/// Engine DTO (Rust로 전송)
/// </summary>
public class JobDto
{
    public string Id { get; set; } = string.Empty;
    public List<OperationDto> Operations { get; set; } = [];
    public int Priority { get; set; }
    /// <summary>Serializes as "due_date" for Rust engine compatibility</summary>
    public DateTime? DueDate { get; set; }
    /// <summary>Serializes as "earliest_start" for Rust engine compatibility</summary>
    public DateTime? EarliestStart { get; set; }
    public int Quantity { get; set; }
    public bool IsSplittable { get; set; }
}
