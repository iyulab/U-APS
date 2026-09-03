namespace UAPS.SDK.Models;

/// <summary>
/// 자재 요구사항 - 공정별 필요 자재
/// </summary>
public class MaterialRequirement
{
    /// <summary>
    /// 자재 ID
    /// </summary>
    public string MaterialId { get; set; } = string.Empty;

    /// <summary>
    /// 필요 수량
    /// </summary>
    public double Quantity { get; set; }

    /// <summary>
    /// 자재 가용일 (이 날짜 이후 사용 가능)
    /// </summary>
    public DateTime? AvailabilityDate { get; set; }

    public static MaterialRequirement Create(string materialId, double quantity) => new()
    {
        MaterialId = materialId,
        Quantity = quantity
    };

    public MaterialRequirement WithAvailability(DateTime date)
    {
        AvailabilityDate = date;
        return this;
    }

    /// <summary>
    /// 지정 시점에 자재가 가용한지 확인
    /// </summary>
    public bool IsAvailableAt(DateTime time)
    {
        return AvailabilityDate == null || time >= AvailabilityDate.Value;
    }

    internal MaterialRequirementDto ToDto() => new()
    {
        MaterialId = MaterialId,
        Quantity = Quantity,
        AvailabilityDate = AvailabilityDate?.ToString("O")
    };
}

/// <summary>
/// 자재 마스터 - 자재 기본 정보
/// </summary>
public class Material
{
    /// <summary>
    /// 자재 ID
    /// </summary>
    public string Id { get; set; } = string.Empty;

    /// <summary>
    /// 자재명
    /// </summary>
    public string Name { get; set; } = string.Empty;

    /// <summary>
    /// 단위
    /// </summary>
    public string Unit { get; set; } = "EA";

    /// <summary>
    /// 현재 재고량
    /// </summary>
    public double StockQuantity { get; set; }

    /// <summary>
    /// 안전재고 수량
    /// </summary>
    public double SafetyStock { get; set; }

    /// <summary>
    /// 리드타임 (ms)
    /// </summary>
    public long LeadTimeMs { get; set; }

    public static Material Create(string id, string name) => new()
    {
        Id = id,
        Name = name
    };

    public Material WithStock(double quantity)
    {
        StockQuantity = quantity;
        return this;
    }

    public Material WithSafetyStock(double quantity)
    {
        SafetyStock = quantity;
        return this;
    }

    public Material WithLeadTime(TimeSpan leadTime)
    {
        LeadTimeMs = (long)leadTime.TotalMilliseconds;
        return this;
    }

    /// <summary>
    /// 가용 재고량 (현재 재고 - 안전재고)
    /// </summary>
    public double AvailableStock => Math.Max(StockQuantity - SafetyStock, 0);

    /// <summary>
    /// 안전재고 미달 여부
    /// </summary>
    public bool IsBelowSafetyStock => StockQuantity < SafetyStock;

    internal MaterialDto ToDto() => new()
    {
        Id = Id,
        Name = Name,
        Unit = Unit,
        StockQuantity = StockQuantity,
        SafetyStock = SafetyStock,
        LeadTimeMs = LeadTimeMs
    };
}

/// <summary>
/// BOM 항목 - 제품별 자재 소요량
/// </summary>
public class BomEntry
{
    /// <summary>
    /// 자재 ID
    /// </summary>
    public string MaterialId { get; set; } = string.Empty;

    /// <summary>
    /// 단위당 소요량
    /// </summary>
    public double QuantityPerUnit { get; set; }

    /// <summary>
    /// 손실률 (0.0 ~ 1.0)
    /// </summary>
    public double ScrapRate { get; set; }

    public static BomEntry Create(string materialId, double quantityPerUnit) => new()
    {
        MaterialId = materialId,
        QuantityPerUnit = quantityPerUnit
    };

    public BomEntry WithScrapRate(double rate)
    {
        ScrapRate = Math.Clamp(rate, 0.0, 1.0);
        return this;
    }

    /// <summary>
    /// 총 소요량 계산 (손실률 포함)
    /// </summary>
    public double CalculateRequirement(double productionQuantity)
    {
        return QuantityPerUnit * productionQuantity * (1.0 + ScrapRate);
    }

    internal BomEntryDto ToDto() => new()
    {
        MaterialId = MaterialId,
        QuantityPerUnit = QuantityPerUnit,
        ScrapRate = ScrapRate
    };
}

/// <summary>
/// 자재 관리자 - 자재 가용성 검사 및 BOM 관리
/// </summary>
public class MaterialManager
{
    /// <summary>
    /// 자재 마스터
    /// </summary>
    public List<Material> Materials { get; set; } = [];

    /// <summary>
    /// BOM (제품ID → 항목 목록)
    /// </summary>
    public Dictionary<string, List<BomEntry>> Bom { get; set; } = [];

    public static MaterialManager Create() => new();

    public MaterialManager AddMaterial(Material material)
    {
        Materials.Add(material);
        return this;
    }

    public MaterialManager AddBomEntry(string productId, BomEntry entry)
    {
        if (!Bom.ContainsKey(productId))
        {
            Bom[productId] = [];
        }
        Bom[productId].Add(entry);
        return this;
    }

    /// <summary>
    /// 제품의 자재 소요량 계산
    /// </summary>
    public List<MaterialRequirement> CalculateRequirements(string productId, double quantity)
    {
        if (!Bom.TryGetValue(productId, out var entries))
        {
            return [];
        }

        return entries.Select(e => MaterialRequirement.Create(
            e.MaterialId,
            e.CalculateRequirement(quantity)
        )).ToList();
    }

    internal MaterialManagerDto ToDto() => new()
    {
        Materials = Materials.Select(m => m.ToDto()).ToList(),
        Bom = Bom.ToDictionary(
            kvp => kvp.Key,
            kvp => kvp.Value.Select(e => e.ToDto()).ToList()
        )
    };
}

// DTOs for JSON serialization

public class MaterialRequirementDto
{
    public string MaterialId { get; set; } = string.Empty;
    public double Quantity { get; set; }
    public string? AvailabilityDate { get; set; }
}

public class MaterialDto
{
    public string Id { get; set; } = string.Empty;
    public string Name { get; set; } = string.Empty;
    public string Unit { get; set; } = "EA";
    public double StockQuantity { get; set; }
    public double SafetyStock { get; set; }
    public long LeadTimeMs { get; set; }
}

public class BomEntryDto
{
    public string MaterialId { get; set; } = string.Empty;
    public double QuantityPerUnit { get; set; }
    public double ScrapRate { get; set; }
}

public class MaterialManagerDto
{
    public List<MaterialDto> Materials { get; set; } = [];
    public Dictionary<string, List<BomEntryDto>> Bom { get; set; } = [];
}
