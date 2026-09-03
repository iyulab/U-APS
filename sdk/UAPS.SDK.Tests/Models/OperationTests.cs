using FluentAssertions;
using UAPS.SDK.Models;
using Xunit;

namespace UAPS.SDK.Tests.Models;

public class OperationTests
{
    [Fact]
    public void OperationTime_ShouldCalculateTotal()
    {
        // Arrange
        var time = new OperationTime(
            15 * 60 * 1000,  // 15분
            45 * 60 * 1000,  // 45분
            5 * 60 * 1000    // 5분
        );

        // Act & Assert
        time.TotalMs.Should().Be(65 * 60 * 1000); // 65분
    }

    [Fact]
    public void Operation_ShouldBuildWithFluent()
    {
        // Arrange & Act
        var op = Operation.Create("OP-001", "JOB-001", 1)
            .WithTime(
                15 * 60 * 1000,
                45 * 60 * 1000,
                5 * 60 * 1000
            )
            .WithEquipment("EQP-CNC-001");

        // Assert
        op.Id.Should().Be("OP-001");
        op.Sequence.Should().Be(1);
        op.TotalTimeMs.Should().Be(65 * 60 * 1000);
        op.RequiredResources.Should().HaveCount(1);
        op.RequiredResources[0].ResourceType.Should().Be(ResourceType.Equipment);
    }

    [Fact]
    public void Operation_ShouldConvertToEngineDto()
    {
        // Arrange
        var op = Operation.Create("OP-001", "JOB-001", 1)
            .WithTime(1000, 2000, 500)
            .WithEquipment("EQP-001", "EQP-002");

        // Act
        var dto = op.ToEngineDto();

        // Assert
        dto.Id.Should().Be("OP-001");
        dto.Time.SetupMs.Should().Be(1000);
        dto.Time.ProcessMs.Should().Be(2000);
        dto.Time.WaitMs.Should().Be(500);
        dto.RequiredResources.Should().HaveCount(1);
        dto.RequiredResources[0].Candidates.Should().Contain("EQP-001");
    }

    [Fact]
    public void Operation_SdkProperties_ShouldNotAffectEngineDto()
    {
        // Arrange
        var op = Operation.Create("OP-001", "JOB-001", 1)
            .WithTime(1000, 2000, 500);

        // SDK-only properties
        op.Name = "CNC 가공";
        op.ProcessCode = "PRC-020";
        op.StandardCost = 50000m;
        op.InspectionItems.Add("치수검사");

        // Act
        var dto = op.ToEngineDto();

        // Assert - DTO에는 SDK 속성이 없음
        dto.Should().NotBeNull();
        dto.Id.Should().Be("OP-001");
        // Name, ProcessCode, StandardCost 등은 DTO에 없음
    }
}
