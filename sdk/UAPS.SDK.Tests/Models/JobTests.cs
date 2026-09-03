using FluentAssertions;
using UAPS.SDK.Models;
using Xunit;

namespace UAPS.SDK.Tests.Models;

public class JobTests
{
    [Fact]
    public void Job_ShouldCreateWithDefaults()
    {
        // Arrange & Act
        var job = Job.Create("JOB-001");

        // Assert
        job.Id.Should().Be("JOB-001");
        job.Priority.Should().Be(100);
        job.Quantity.Should().Be(1);
        job.Operations.Should().BeEmpty();
    }

    [Fact]
    public void Job_ShouldBuildWithOperations()
    {
        // Arrange
        var op1 = Operation.Create("OP-001", "JOB-001", 1)
            .WithTime(1000, 2000, 500);
        var op2 = Operation.Create("OP-002", "JOB-001", 2)
            .WithTime(500, 3000, 1000);

        // Act
        var job = Job.Create("JOB-001")
            .WithOperation(op1)
            .WithOperation(op2)
            .WithPriority(1);

        // Assert
        job.OperationCount.Should().Be(2);
        job.TotalTimeMs.Should().Be(8000); // 3500 + 4500
        job.Priority.Should().Be(1);
    }

    [Fact]
    public void Job_ShouldCalculateTotalTime()
    {
        // Arrange & Act
        var job = Job.Create("JOB-001")
            .WithOperation(
                Operation.Create("OP-001", "JOB-001", 1)
                    .WithTime(15 * 60000, 45 * 60000, 5 * 60000) // 65분
            )
            .WithOperation(
                Operation.Create("OP-002", "JOB-001", 2)
                    .WithTime(10 * 60000, 30 * 60000, 0) // 40분
            );

        // Assert
        job.TotalTimeMs.Should().Be(105 * 60000); // 105분
    }

    [Fact]
    public void Job_ShouldConvertToEngineDto()
    {
        // Arrange
        var dueDate = DateTime.UtcNow.AddDays(7);
        var job = Job.Create("JOB-001")
            .WithOperation(
                Operation.Create("OP-001", "JOB-001", 1)
                    .WithTime(1000, 2000, 500)
            )
            .WithPriority(1)
            .WithDueDate(dueDate)
            .WithQuantity(100);

        // Act
        var dto = job.ToEngineDto();

        // Assert
        dto.Id.Should().Be("JOB-001");
        dto.Operations.Should().HaveCount(1);
        dto.Priority.Should().Be(1);
        dto.DueDate.Should().BeCloseTo(dueDate, TimeSpan.FromSeconds(1));
        dto.Quantity.Should().Be(100);
    }

    [Fact]
    public void Job_ShouldCalculateTotalAmount()
    {
        // Arrange & Act
        var job = Job.Create("JOB-001")
            .WithQuantity(100);
        job.UnitPrice = 5000m;

        // Assert
        job.TotalAmount.Should().Be(500000m);
    }
}
