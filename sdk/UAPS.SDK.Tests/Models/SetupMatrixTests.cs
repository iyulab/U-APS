using FluentAssertions;
using UAPS.SDK.Models;
using Xunit;

namespace UAPS.SDK.Tests.Models;

public class SetupMatrixTests
{
    [Fact]
    public void SetupMatrix_ShouldBuildWithFluent()
    {
        // Arrange & Act
        var matrix = SetupMatrix.Create("EQP-001")
            .WithDefaultSetup(30000)
            .WithSetup("ProductA", "ProductB", 60000)
            .WithSetup("ProductB", "ProductA", 45000);

        // Assert
        matrix.ResourceId.Should().Be("EQP-001");
        matrix.DefaultSetupMs.Should().Be(30000);
        matrix.Entries.Should().HaveCount(2);
    }

    [Fact]
    public void SetupMatrix_SymmetricSetup_ShouldAddBothDirections()
    {
        // Arrange & Act
        var matrix = SetupMatrix.Create("EQP-001")
            .WithSymmetricSetup("Red", "Blue", 20000);

        // Assert
        matrix.Entries.Should().HaveCount(2);
        matrix.Entries.Should().Contain(e => e.FromProduct == "Red" && e.ToProduct == "Blue");
        matrix.Entries.Should().Contain(e => e.FromProduct == "Blue" && e.ToProduct == "Red");
    }

    [Fact]
    public void SetupMatrix_ShouldConvertToEngineDto()
    {
        // Arrange
        var matrix = SetupMatrix.Create("EQP-001")
            .WithDefaultSetup(10000)
            .WithSetup("A", "B", 20000);

        // Act
        var dto = matrix.ToEngineDto();

        // Assert
        dto.ResourceId.Should().Be("EQP-001");
        dto.DefaultSetupMs.Should().Be(10000);
        dto.Entries.Should().HaveCount(1);
        dto.Entries[0].FromProduct.Should().Be("A");
        dto.Entries[0].ToProduct.Should().Be("B");
        dto.Entries[0].SetupMs.Should().Be(20000);
    }

    [Fact]
    public void SetupMatrixCollection_ShouldManageMultipleMatrices()
    {
        // Arrange & Act
        var collection = new SetupMatrixCollection()
            .WithMatrix(SetupMatrix.Create("EQP-001").WithDefaultSetup(10000))
            .WithMatrix(SetupMatrix.Create("EQP-002").WithDefaultSetup(20000));

        // Assert
        collection.Matrices.Should().HaveCount(2);
    }

    [Fact]
    public void SetupMatrixCollection_ShouldConvertToEngineDto()
    {
        // Arrange
        var collection = new SetupMatrixCollection()
            .WithMatrix(SetupMatrix.Create("EQP-001").WithDefaultSetup(10000))
            .WithMatrix(SetupMatrix.Create("EQP-002").WithDefaultSetup(20000));

        // Act
        var dto = collection.ToEngineDto();

        // Assert
        dto.Matrices.Should().HaveCount(2);
        dto.Matrices.Should().ContainKey("EQP-001");
        dto.Matrices.Should().ContainKey("EQP-002");
        dto.Matrices["EQP-001"].DefaultSetupMs.Should().Be(10000);
        dto.Matrices["EQP-002"].DefaultSetupMs.Should().Be(20000);
    }
}
