using FluentAssertions;
using UAPS.SDK.Client;
using UAPS.SDK.Helpers;
using UAPS.SDK.Models;
using Xunit;

namespace UAPS.SDK.Tests.Client;

/// <summary>
/// SchedulerClient 통합 테스트
/// 참고: 실제 테스트는 Rust DLL 빌드 후 가능
/// </summary>
public class SchedulerClientTests
{
    [Fact]
    public void Schedule_SingleJob_ShouldReturnAssignments()
    {
        // Arrange
        using var client = new SchedulerClient();

        var job = Job.Create("JOB-001")
            .WithOperation(
                Operation.Create("OP-001", "JOB-001", 1)
                    .WithTime(0, 30000, 0)
                    .WithEquipment("EQP-001")
            );

        var resource = Resource.Equipment("EQP-001");

        var request = new ScheduleRequest
        {
            Jobs = [job],
            Resources = [resource],
            StartTimeMs = 0
        };

        // Act
        var result = client.Schedule(request);

        // Assert
        result.Success.Should().BeTrue(result.Error ?? "No error message");
        result.Schedule.Should().NotBeNull();
        result.Schedule!.Assignments.Should().HaveCount(1);
        result.Schedule.MakespanMs.Should().Be(30000);
    }

    [Fact]
    public void Schedule_MultipleJobs_ShouldRespectPriority()
    {
        // Arrange
        using var client = new SchedulerClient();

        var jobHigh = Job.Create("JOB-HIGH")
            .WithPriority(1)
            .WithOperation(
                Operation.Create("OP-HIGH", "JOB-HIGH", 1)
                    .WithTime(0, 30000, 0)
                    .WithEquipment("EQP-001")
            );

        var jobLow = Job.Create("JOB-LOW")
            .WithPriority(10)
            .WithOperation(
                Operation.Create("OP-LOW", "JOB-LOW", 1)
                    .WithTime(0, 20000, 0)
                    .WithEquipment("EQP-001")
            );

        var resource = Resource.Equipment("EQP-001");

        var request = new ScheduleRequest
        {
            Jobs = [jobLow, jobHigh],
            Resources = [resource]
        };

        // Act
        var result = client.Schedule(request);

        // Assert
        result.Success.Should().BeTrue();

        var opHigh = result.Schedule!.GetAssignmentForOperation("OP-HIGH");
        var opLow = result.Schedule.GetAssignmentForOperation("OP-LOW");

        opHigh!.StartMs.Should().Be(0); // 우선순위 높은 것 먼저
        opLow!.StartMs.Should().Be(30000);
    }

    [Fact]
    public void Schedule_WithHelpers_ShouldWork()
    {
        // Arrange
        using var client = new SchedulerClient();

        var order = ProductionHelpers.CreateOrder("PO-2501-001", priority: 1)
            .WithQuantity(100);

        order.WithOperation(
            ProductionHelpers.CreateProcessStepMinutes(1, 15, 45, 5, "EQP-CNC-001")
        );

        var machine = ProductionHelpers.CreateEquipment("EQP-CNC-001", "CNC 1호기");

        var request = new ScheduleRequest
        {
            Jobs = [order],
            Resources = [machine]
        };

        // Act
        var result = client.Schedule(request);

        // Assert
        result.Success.Should().BeTrue();
        result.Schedule!.MakespanMs.Should().Be(65 * 60 * 1000); // 65분
    }

    [Fact]
    public void GetVersion_ShouldReturnEngineVersion()
    {
        // Arrange
        using var client = new SchedulerClient();

        // Act
        var version = client.GetVersion();

        // Assert
        version.Should().NotBeNullOrEmpty();
        version.Should().MatchRegex(@"\d+\.\d+\.\d+");
    }
}

/// <summary>
/// 단위 테스트 (DLL 불필요)
/// </summary>
public class ScheduleResultTests
{
    [Fact]
    public void Schedule_ShouldFindAssignmentByOperation()
    {
        // Arrange
        var schedule = new Schedule
        {
            Assignments =
            [
                new Assignment { OperationId = "OP-001", ResourceId = "EQP-001", StartMs = 0, EndMs = 1000 },
                new Assignment { OperationId = "OP-002", ResourceId = "EQP-001", StartMs = 1000, EndMs = 2000 }
            ],
            MakespanMs = 2000
        };

        // Act
        var found = schedule.GetAssignmentForOperation("OP-001");
        var notFound = schedule.GetAssignmentForOperation("OP-999");

        // Assert
        found.Should().NotBeNull();
        found!.StartMs.Should().Be(0);
        notFound.Should().BeNull();
    }

    [Fact]
    public void Schedule_ShouldFindAssignmentsByResource()
    {
        // Arrange
        var schedule = new Schedule
        {
            Assignments =
            [
                new Assignment { OperationId = "OP-001", ResourceId = "EQP-001", StartMs = 0, EndMs = 1000 },
                new Assignment { OperationId = "OP-002", ResourceId = "EQP-002", StartMs = 0, EndMs = 1500 },
                new Assignment { OperationId = "OP-003", ResourceId = "EQP-001", StartMs = 1000, EndMs = 2000 }
            ]
        };

        // Act
        var eqp1Assignments = schedule.GetAssignmentsForResource("EQP-001").ToList();

        // Assert
        eqp1Assignments.Should().HaveCount(2);
    }

    [Fact]
    public void Assignment_ShouldCalculateDuration()
    {
        // Arrange
        var assignment = new Assignment
        {
            StartMs = 1000,
            EndMs = 5000
        };

        // Assert
        assignment.DurationMs.Should().Be(4000);
    }
}
