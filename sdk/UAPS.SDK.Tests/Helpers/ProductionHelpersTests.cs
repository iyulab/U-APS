using FluentAssertions;
using UAPS.SDK.Helpers;
using UAPS.SDK.Models;
using Xunit;

namespace UAPS.SDK.Tests.Helpers;

public class ProductionHelpersTests
{
    [Fact]
    public void CreateOrder_ShouldCreateJobWithPriority()
    {
        // Arrange & Act
        var order = ProductionHelpers.CreateOrder("PO-2501-001", 1);

        // Assert
        order.Id.Should().Be("PO-2501-001");
        order.Priority.Should().Be(1);
    }

    [Fact]
    public void CreateUrgentOrder_ShouldHaveHighestPriority()
    {
        // Arrange & Act
        var urgent = ProductionHelpers.CreateUrgentOrder("URGENT-001");

        // Assert
        urgent.Priority.Should().Be(1);
    }

    [Fact]
    public void CreateProcessStepMinutes_ShouldConvertToMilliseconds()
    {
        // Arrange & Act
        var step = ProductionHelpers.CreateProcessStepMinutes(
            sequence: 1,
            setupMinutes: 15,
            processMinutes: 45,
            waitMinutes: 5,
            "EQP-001"
        );

        // Assert
        step.Time.SetupMs.Should().Be(15 * 60 * 1000);
        step.Time.ProcessMs.Should().Be(45 * 60 * 1000);
        step.Time.WaitMs.Should().Be(5 * 60 * 1000);
        step.TotalTimeMs.Should().Be(65 * 60 * 1000);
    }

    [Fact]
    public void CreateEquipment_ShouldCreateWithName()
    {
        // Arrange & Act
        var machine = ProductionHelpers.CreateEquipment("EQP-CNC-001", "CNC 밀링머신 1호기");

        // Assert
        machine.Id.Should().Be("EQP-CNC-001");
        machine.Name.Should().Be("CNC 밀링머신 1호기");
        machine.Kind.Should().Be(ResourceKind.Equipment);
    }

    [Fact]
    public void CreateSkilledWorker_ShouldHaveHighEfficiency()
    {
        // Arrange & Act
        var skilled = ProductionHelpers.CreateSkilledWorker("EMP-001", "김철수");

        // Assert
        skilled.Efficiency.Should().Be(1.2);
        skilled.Name.Should().Be("김철수");
    }

    [Fact]
    public void CreateNoviceWorker_ShouldHaveLowEfficiency()
    {
        // Arrange & Act
        var novice = ProductionHelpers.CreateNoviceWorker("EMP-002", "이영희");

        // Assert
        novice.Efficiency.Should().Be(0.8);
    }

    [Fact]
    public void AddPMSchedule_ShouldAddUnavailableSlot()
    {
        // Arrange
        var machine = ProductionHelpers.CreateEquipment("EQP-001", "CNC");
        var pmStart = new DateTime(2025, 1, 20, 8, 0, 0, DateTimeKind.Utc);
        var pmEnd = new DateTime(2025, 1, 20, 10, 0, 0, DateTimeKind.Utc);

        // Act
        machine.AddPMSchedule(pmStart, pmEnd);

        // Assert
        machine.Unavailable.Should().HaveCount(1);

        var pmTimeMs = new DateTimeOffset(new DateTime(2025, 1, 20, 9, 0, 0, DateTimeKind.Utc))
            .ToUnixTimeMilliseconds();
        machine.IsAvailableAt(pmTimeMs).Should().BeFalse();
    }

    [Fact]
    public void TimeSpanExtensions_ShouldWork()
    {
        // Arrange & Act
        var minutes = 30.Minutes();
        var hours = 2.Hours();
        var seconds = 45.Seconds();

        // Assert
        minutes.TotalMinutes.Should().Be(30);
        hours.TotalHours.Should().Be(2);
        seconds.TotalSeconds.Should().Be(45);
    }

    [Fact]
    public void Integration_ShouldCreateCompleteProductionOrder()
    {
        // Arrange & Act - 현업 시나리오
        var order = ProductionHelpers.CreateOrder("PO-2501-001", priority: 1)
            .WithDueDate(DateTime.UtcNow.AddDays(5))
            .WithQuantity(500);

        order.ProductCode = "PRD-A001";
        order.ProductName = "알루미늄 프레임 A타입";
        order.CustomerName = "삼성전자";

        var op1 = ProductionHelpers.CreateProcessStepMinutes(1, 10, 15, 0, "EQP-CUT-001");
        op1.Name = "원자재 절단";
        op1.JobId = order.Id;
        op1.Id = "OP-001";

        var op2 = ProductionHelpers.CreateProcessStepMinutes(2, 15, 45, 5, "EQP-CNC-001");
        op2.Name = "CNC 가공";
        op2.JobId = order.Id;
        op2.Id = "OP-002";

        order.WithOperation(op1).WithOperation(op2);

        // Assert
        order.OperationCount.Should().Be(2);
        order.TotalTimeMs.Should().Be(90 * 60 * 1000); // 90분

        // Engine DTO 변환
        var dto = order.ToEngineDto();
        dto.Operations.Should().HaveCount(2);
        dto.Priority.Should().Be(1);
    }
}
