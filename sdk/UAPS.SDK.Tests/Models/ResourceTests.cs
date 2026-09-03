using FluentAssertions;
using UAPS.SDK.Models;
using Xunit;

namespace UAPS.SDK.Tests.Models;

public class ResourceTests
{
    [Fact]
    public void TimeSlot_ShouldCalculateDuration()
    {
        // Arrange
        var slot = new TimeSlot(1000, 2000);

        // Assert
        slot.DurationMs.Should().Be(1000);
        slot.Contains(1500).Should().BeTrue();
        slot.Contains(2000).Should().BeFalse();
        slot.Contains(999).Should().BeFalse();
    }

    [Fact]
    public void TimeSlot_ShouldDetectOverlap()
    {
        // Arrange
        var slot1 = new TimeSlot(1000, 2000);
        var slot2 = new TimeSlot(1500, 2500);
        var slot3 = new TimeSlot(2000, 3000);

        // Assert
        slot1.Overlaps(slot2).Should().BeTrue();
        slot1.Overlaps(slot3).Should().BeFalse(); // 경계는 겹치지 않음
    }

    [Fact]
    public void Resource_ShouldCreateEquipment()
    {
        // Arrange & Act
        var equip = Resource.Equipment("EQP-CNC-001")
            .WithCapability("milling")
            .WithCapability("drilling")
            .WithEfficiency(0.95);

        // Assert
        equip.Kind.Should().Be(ResourceKind.Equipment);
        equip.HasCapability("milling").Should().BeTrue();
        equip.HasCapability("welding").Should().BeFalse();
        equip.Efficiency.Should().Be(0.95);
    }

    [Fact]
    public void Resource_ShouldCreateWorker()
    {
        // Arrange & Act
        var worker = Resource.Worker("EMP-001")
            .WithEfficiency(1.2) // 숙련공
            .WithCapability("cnc_operation")
            .WithName("김철수");

        // Assert
        worker.Kind.Should().Be(ResourceKind.Worker);
        worker.Efficiency.Should().Be(1.2);
        worker.Name.Should().Be("김철수");
    }

    [Fact]
    public void Resource_ShouldCheckAvailability()
    {
        // Arrange
        var pmSlot = new TimeSlot(
            8 * 3600 * 1000,  // 8시
            10 * 3600 * 1000  // 10시
        );

        var equip = Resource.Equipment("EQP-001")
            .WithUnavailable(pmSlot);

        // Assert
        equip.IsAvailableAt(9 * 3600 * 1000).Should().BeFalse(); // PM 시간
        equip.IsAvailableAt(11 * 3600 * 1000).Should().BeTrue(); // PM 후
    }

    [Fact]
    public void Resource_ShouldCalculateAdjustedTime()
    {
        // Arrange
        var skilled = Resource.Worker("SKILLED")
            .WithEfficiency(1.5); // 50% 빠름
        var novice = Resource.Worker("NOVICE")
            .WithEfficiency(0.8); // 20% 느림

        var baseTime = 60000L; // 1분

        // Act & Assert
        skilled.AdjustedTimeMs(baseTime).Should().Be(40000); // 40초
        novice.AdjustedTimeMs(baseTime).Should().Be(75000);  // 75초
    }

    [Fact]
    public void Resource_ShouldConvertToEngineDto()
    {
        // Arrange
        var resource = Resource.Equipment("EQP-001")
            .WithCapability("milling")
            .WithCapacity(2)
            .WithName("CNC 1호기"); // SDK-only

        // Act
        var dto = resource.ToEngineDto();

        // Assert
        dto.Id.Should().Be("EQP-001");
        dto.Kind.Should().Be("Equipment");
        dto.Capabilities.Should().Contain("milling");
        dto.Capacity.Should().Be(2);
    }
}
