namespace UAPS.SDK.Models;

/// <summary>
/// 준비 시간 항목
/// </summary>
public record SetupEntry(string FromProduct, string ToProduct, long SetupMs);

/// <summary>
/// SetupMatrix - 설비별 순서 의존 준비 시간 행렬
/// </summary>
public class SetupMatrix
{
    public string ResourceId { get; set; } = string.Empty;
    public long DefaultSetupMs { get; set; }
    public List<SetupEntry> Entries { get; set; } = [];

    // ===== Factory Methods =====

    public static SetupMatrix Create(string resourceId) => new()
    {
        ResourceId = resourceId
    };

    // ===== Builder Methods =====

    public SetupMatrix WithDefaultSetup(long setupMs)
    {
        DefaultSetupMs = setupMs;
        return this;
    }

    public SetupMatrix WithSetup(string fromProduct, string toProduct, long setupMs)
    {
        Entries.Add(new SetupEntry(fromProduct, toProduct, setupMs));
        return this;
    }

    public SetupMatrix WithSymmetricSetup(string productA, string productB, long setupMs)
    {
        Entries.Add(new SetupEntry(productA, productB, setupMs));
        Entries.Add(new SetupEntry(productB, productA, setupMs));
        return this;
    }

    // ===== Engine DTO Conversion =====

    public SetupMatrixDto ToEngineDto() => new()
    {
        ResourceId = ResourceId,
        DefaultSetupMs = DefaultSetupMs,
        Entries = Entries.Select(e => new SetupEntryDto
        {
            FromProduct = e.FromProduct,
            ToProduct = e.ToProduct,
            SetupMs = e.SetupMs
        }).ToList()
    };
}

/// <summary>
/// SetupMatrixCollection - 여러 설비의 Setup Matrix 관리
/// </summary>
public class SetupMatrixCollection
{
    public List<SetupMatrix> Matrices { get; set; } = [];

    // ===== Builder Methods =====

    public SetupMatrixCollection WithMatrix(SetupMatrix matrix)
    {
        Matrices.Add(matrix);
        return this;
    }

    // ===== Engine DTO Conversion =====

    public SetupMatrixCollectionDto ToEngineDto() => new()
    {
        Matrices = Matrices.ToDictionary(
            m => m.ResourceId,
            m => m.ToEngineDto()
        )
    };
}

// ===== Engine DTOs =====

public class SetupEntryDto
{
    public string FromProduct { get; set; } = string.Empty;
    public string ToProduct { get; set; } = string.Empty;
    public long SetupMs { get; set; }
}

public class SetupMatrixDto
{
    public string ResourceId { get; set; } = string.Empty;
    public long DefaultSetupMs { get; set; }
    public List<SetupEntryDto> Entries { get; set; } = [];
}

public class SetupMatrixCollectionDto
{
    public Dictionary<string, SetupMatrixDto> Matrices { get; set; } = [];
}
