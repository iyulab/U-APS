namespace UAPS.SDK.Models;

/// <summary>
/// Order date calculation mode for outsourcing operations
/// </summary>
public enum OrderDateMode
{
    /// <summary>Calculate from earliest start time</summary>
    FromEarliestStart,
    /// <summary>Calculate backward from due date</summary>
    BackwardFromDueDate,
    /// <summary>Use fixed order date</summary>
    FixedDate
}

/// <summary>
/// Outsourcing provider (vendor) information
/// </summary>
public class OutsourcingProvider
{
    /// <summary>Provider ID</summary>
    public string Id { get; set; } = string.Empty;

    /// <summary>Provider name</summary>
    public string Name { get; set; } = string.Empty;

    /// <summary>List of operation types this provider can handle</summary>
    public List<string> SupportedOperations { get; set; } = [];

    /// <summary>Default lead time in milliseconds</summary>
    public long DefaultLeadTimeMs { get; set; }

    /// <summary>Maximum daily capacity (number of operations)</summary>
    public int? DailyCapacity { get; set; }

    /// <summary>Whether this provider is currently active</summary>
    public bool IsActive { get; set; } = true;

    /// <summary>Cost per operation (optional)</summary>
    public double? CostPerOperation { get; set; }

    /// <summary>Quality rating (0.0 to 1.0, higher is better)</summary>
    public double QualityRating { get; set; } = 0.8;

    /// <summary>Working days (1=Monday to 7=Sunday)</summary>
    public List<int> WorkingDays { get; set; } = [1, 2, 3, 4, 5]; // Mon-Fri default

    // ===== Builder Methods =====

    public static OutsourcingProvider Create(string id, string name) => new()
    {
        Id = id,
        Name = name
    };

    /// <summary>Add a supported operation type</summary>
    public OutsourcingProvider WithOperation(string operationType)
    {
        if (!SupportedOperations.Contains(operationType))
            SupportedOperations.Add(operationType);
        return this;
    }

    /// <summary>Add multiple supported operation types</summary>
    public OutsourcingProvider WithOperations(params string[] operationTypes)
    {
        foreach (var op in operationTypes)
            WithOperation(op);
        return this;
    }

    /// <summary>Set default lead time in days</summary>
    public OutsourcingProvider WithLeadTimeDays(int days)
    {
        DefaultLeadTimeMs = days * 24L * 3600_000;
        return this;
    }

    /// <summary>Set default lead time in milliseconds</summary>
    public OutsourcingProvider WithLeadTimeMs(long ms)
    {
        DefaultLeadTimeMs = ms;
        return this;
    }

    /// <summary>Set daily capacity</summary>
    public OutsourcingProvider WithDailyCapacity(int capacity)
    {
        DailyCapacity = capacity;
        return this;
    }

    /// <summary>Set cost per operation</summary>
    public OutsourcingProvider WithCost(double cost)
    {
        CostPerOperation = cost;
        return this;
    }

    /// <summary>Set quality rating (0.0 to 1.0)</summary>
    public OutsourcingProvider WithQuality(double rating)
    {
        QualityRating = Math.Clamp(rating, 0.0, 1.0);
        return this;
    }

    /// <summary>Set active status</summary>
    public OutsourcingProvider Active(bool isActive = true)
    {
        IsActive = isActive;
        return this;
    }

    /// <summary>Set working days</summary>
    public OutsourcingProvider WithWorkingDays(params int[] days)
    {
        WorkingDays = [.. days];
        return this;
    }

    public OutsourcingProviderDto ToDto() => new()
    {
        Id = Id,
        Name = Name,
        SupportedOperations = SupportedOperations,
        DefaultLeadTimeMs = DefaultLeadTimeMs,
        DailyCapacity = DailyCapacity,
        IsActive = IsActive,
        CostPerOperation = CostPerOperation,
        QualityRating = QualityRating,
        WorkingDays = WorkingDays.Select(d => (byte)d).ToList()
    };
}

/// <summary>
/// Outsourcing configuration for an operation
/// </summary>
public class OutsourcingConfig
{
    /// <summary>Provider ID to use</summary>
    public string ProviderId { get; set; } = string.Empty;

    /// <summary>Custom lead time override (ms)</summary>
    public long? LeadTimeMs { get; set; }

    /// <summary>Expected delivery time (ms from epoch)</summary>
    public long? ExpectedDeliveryMs { get; set; }

    /// <summary>Order date calculation mode</summary>
    public OrderDateMode OrderDateMode { get; set; } = OrderDateMode.FromEarliestStart;

    /// <summary>Priority (lower = higher priority)</summary>
    public int Priority { get; set; } = 100;

    // ===== Builder Methods =====

    public static OutsourcingConfig Create(string providerId) => new()
    {
        ProviderId = providerId
    };

    /// <summary>Set lead time in days</summary>
    public OutsourcingConfig WithLeadTimeDays(int days)
    {
        LeadTimeMs = days * 24L * 3600_000;
        return this;
    }

    /// <summary>Set lead time in milliseconds</summary>
    public OutsourcingConfig WithLeadTimeMs(long ms)
    {
        LeadTimeMs = ms;
        return this;
    }

    /// <summary>Set expected delivery date</summary>
    public OutsourcingConfig WithExpectedDelivery(DateTime delivery)
    {
        ExpectedDeliveryMs = new DateTimeOffset(delivery).ToUnixTimeMilliseconds();
        return this;
    }

    /// <summary>Set order date mode</summary>
    public OutsourcingConfig WithOrderDateMode(OrderDateMode mode)
    {
        OrderDateMode = mode;
        return this;
    }

    /// <summary>Set priority</summary>
    public OutsourcingConfig WithPriority(int priority)
    {
        Priority = priority;
        return this;
    }

    public OutsourcingConfigDto ToDto() => new()
    {
        ProviderId = ProviderId,
        LeadTimeMs = LeadTimeMs,
        ExpectedDeliveryMs = ExpectedDeliveryMs,
        OrderDateMode = OrderDateMode.ToString(),
        Priority = Priority
    };
}

/// <summary>
/// Outsourcing manager - coordinates providers
/// </summary>
public class OutsourcingManager
{
    private readonly Dictionary<string, OutsourcingProvider> _providers = [];

    /// <summary>All registered providers</summary>
    public IReadOnlyCollection<OutsourcingProvider> Providers => _providers.Values.ToList();

    // ===== Builder Methods =====

    public static OutsourcingManager Create() => new();

    /// <summary>Add a provider</summary>
    public OutsourcingManager WithProvider(OutsourcingProvider provider)
    {
        _providers[provider.Id] = provider;
        return this;
    }

    /// <summary>Get provider by ID</summary>
    public OutsourcingProvider? GetProvider(string id) =>
        _providers.TryGetValue(id, out var provider) ? provider : null;

    /// <summary>Get providers that support a specific operation type</summary>
    public IEnumerable<OutsourcingProvider> ProvidersForOperation(string operationType) =>
        _providers.Values.Where(p => p.IsActive && p.SupportedOperations.Contains(operationType));

    /// <summary>Get the best provider for an operation type (by quality rating)</summary>
    public OutsourcingProvider? BestProviderForOperation(string operationType) =>
        ProvidersForOperation(operationType)
            .OrderByDescending(p => p.QualityRating)
            .FirstOrDefault();

    public OutsourcingManagerDto ToDto() => new()
    {
        Providers = _providers.ToDictionary(
            kvp => kvp.Key,
            kvp => kvp.Value.ToDto()
        )
    };
}

// ===== DTOs for Engine Communication =====

public class OutsourcingProviderDto
{
    public string Id { get; set; } = string.Empty;
    public string Name { get; set; } = string.Empty;
    public List<string> SupportedOperations { get; set; } = [];
    public long DefaultLeadTimeMs { get; set; }
    public int? DailyCapacity { get; set; }
    public bool IsActive { get; set; } = true;
    public double? CostPerOperation { get; set; }
    public double QualityRating { get; set; } = 0.8;
    public List<byte> WorkingDays { get; set; } = [1, 2, 3, 4, 5];
}

public class OutsourcingConfigDto
{
    public string ProviderId { get; set; } = string.Empty;
    public long? LeadTimeMs { get; set; }
    public long? ExpectedDeliveryMs { get; set; }
    public string OrderDateMode { get; set; } = "FromEarliestStart";
    public int Priority { get; set; } = 100;
}

public class OutsourcingManagerDto
{
    public Dictionary<string, OutsourcingProviderDto> Providers { get; set; } = [];
}
