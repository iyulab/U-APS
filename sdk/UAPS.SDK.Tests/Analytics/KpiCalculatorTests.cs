using FluentAssertions;
using UAPS.SDK.Analytics;
using UAPS.SDK.Client;
using UAPS.SDK.Models;
using Xunit;

namespace UAPS.SDK.Tests.Analytics;

/// <summary>
/// KpiCalculator 단위 테스트
/// </summary>
public class KpiCalculatorTests
{
    [Fact]
    public void Calculate_EmptySchedule_ShouldReturnZeroKpis()
    {
        // Arrange
        var schedule = new Schedule
        {
            Assignments = [],
            MakespanMs = 0
        };
        var jobs = new List<Job>();
        var resources = new List<Resource>();

        var calculator = new KpiCalculator(schedule, jobs, resources);

        // Act
        var dashboard = calculator.Calculate();

        // Assert
        dashboard.Summary.TotalMakespanMinutes.Should().Be(0);
        dashboard.Summary.TotalJobCount.Should().Be(0);
        dashboard.ResourceUtilization.Should().BeEmpty();
    }

    [Fact]
    public void Calculate_SingleJobSchedule_ShouldCalculateCorrectMakespan()
    {
        // Arrange
        var schedule = new Schedule
        {
            Assignments =
            [
                new Assignment { OperationId = "OP-001", ResourceId = "EQP-001", StartMs = 0, EndMs = 600000 }
            ],
            MakespanMs = 600000 // 10분
        };

        var job = Job.Create("JOB-001")
            .WithOperation(
                Operation.Create("OP-001", "JOB-001", 1)
                    .WithTime(60000, 540000, 0) // 1분 셋업, 9분 가공
                    .WithEquipment("EQP-001")
            );

        var resource = Resource.Equipment("EQP-001");

        var calculator = new KpiCalculator(schedule, [job], [resource]);

        // Act
        var dashboard = calculator.Calculate();

        // Assert
        dashboard.Summary.TotalMakespanMinutes.Should().Be(10);
        dashboard.Summary.TotalJobCount.Should().Be(1);
        dashboard.Summary.TotalOperationCount.Should().Be(1);
        dashboard.Summary.TotalAssignmentCount.Should().Be(1);
    }

    [Fact]
    public void Calculate_ResourceUtilization_ShouldBeCorrect()
    {
        // Arrange
        var schedule = new Schedule
        {
            Assignments =
            [
                new Assignment { OperationId = "OP-001", ResourceId = "EQP-001", StartMs = 0, EndMs = 300000 },
                new Assignment { OperationId = "OP-002", ResourceId = "EQP-001", StartMs = 300000, EndMs = 600000 }
            ],
            MakespanMs = 600000 // 10분
        };

        var job = Job.Create("JOB-001")
            .WithOperation(
                Operation.Create("OP-001", "JOB-001", 1)
                    .WithTime(0, 300000, 0)
                    .WithEquipment("EQP-001")
            )
            .WithOperation(
                Operation.Create("OP-002", "JOB-001", 2)
                    .WithTime(0, 300000, 0)
                    .WithEquipment("EQP-001")
            );

        var resource = Resource.Equipment("EQP-001");

        var calculator = new KpiCalculator(schedule, [job], [resource]);

        // Act
        var dashboard = calculator.Calculate();

        // Assert
        var utilization = dashboard.ResourceUtilization.First();
        utilization.ResourceId.Should().Be("EQP-001");
        utilization.UtilizationPercent.Should().Be(100); // 전체 시간 활용
        utilization.TotalWorkMinutes.Should().Be(10);
        utilization.AssignmentCount.Should().Be(2);
    }

    [Fact]
    public void Calculate_DueDatePerformance_AllOnTime()
    {
        // Arrange
        var dueDate = DateTime.UtcNow.AddHours(1);
        var schedule = new Schedule
        {
            Assignments =
            [
                new Assignment { OperationId = "OP-001", ResourceId = "EQP-001", StartMs = 0, EndMs = 600000 }
            ],
            MakespanMs = 600000
        };

        var job = Job.Create("JOB-001")
            .WithDueDate(dueDate)
            .WithOperation(
                Operation.Create("OP-001", "JOB-001", 1)
                    .WithTime(0, 600000, 0)
                    .WithEquipment("EQP-001")
            );

        var resource = Resource.Equipment("EQP-001");

        var calculator = new KpiCalculator(schedule, [job], [resource]);

        // Act
        var dashboard = calculator.Calculate();

        // Assert
        dashboard.DueDatePerformance.TotalJobsWithDueDate.Should().Be(1);
        dashboard.DueDatePerformance.OnTimeJobCount.Should().Be(1);
        dashboard.DueDatePerformance.LateJobCount.Should().Be(0);
        dashboard.DueDatePerformance.OnTimeRate.Should().Be(100);
    }

    [Fact]
    public void Calculate_DueDatePerformance_WithLateJobs()
    {
        // Arrange - 납기를 과거로 설정하여 지연 발생
        var pastDueDate = DateTimeOffset.FromUnixTimeMilliseconds(300000).DateTime; // 5분 전
        var schedule = new Schedule
        {
            Assignments =
            [
                new Assignment { OperationId = "OP-001", ResourceId = "EQP-001", StartMs = 0, EndMs = 600000 }
            ],
            MakespanMs = 600000
        };

        var job = Job.Create("JOB-001")
            .WithDueDate(pastDueDate)
            .WithOperation(
                Operation.Create("OP-001", "JOB-001", 1)
                    .WithTime(0, 600000, 0)
                    .WithEquipment("EQP-001")
            );

        var resource = Resource.Equipment("EQP-001");

        var calculator = new KpiCalculator(schedule, [job], [resource]);

        // Act
        var dashboard = calculator.Calculate();

        // Assert
        dashboard.DueDatePerformance.LateJobCount.Should().Be(1);
        dashboard.DueDatePerformance.OnTimeRate.Should().Be(0);
        dashboard.DueDatePerformance.TotalTardinessMinutes.Should().BeGreaterThan(0);
    }

    [Fact]
    public void Calculate_ThroughputMetrics_ShouldBeCorrect()
    {
        // Arrange - 1시간에 2개 작업
        var schedule = new Schedule
        {
            Assignments =
            [
                new Assignment { OperationId = "OP-001", ResourceId = "EQP-001", StartMs = 0, EndMs = 1800000 },
                new Assignment { OperationId = "OP-002", ResourceId = "EQP-001", StartMs = 1800000, EndMs = 3600000 }
            ],
            MakespanMs = 3600000 // 1시간
        };

        var job1 = Job.Create("JOB-001")
            .WithQuantity(50)
            .WithOperation(
                Operation.Create("OP-001", "JOB-001", 1)
                    .WithTime(0, 1800000, 0)
                    .WithEquipment("EQP-001")
            );

        var job2 = Job.Create("JOB-002")
            .WithQuantity(50)
            .WithOperation(
                Operation.Create("OP-002", "JOB-002", 1)
                    .WithTime(0, 1800000, 0)
                    .WithEquipment("EQP-001")
            );

        var resource = Resource.Equipment("EQP-001");

        var calculator = new KpiCalculator(schedule, [job1, job2], [resource]);

        // Act
        var dashboard = calculator.Calculate();

        // Assert
        dashboard.ThroughputMetrics.JobsPerHour.Should().Be(2);
        dashboard.ThroughputMetrics.UnitsPerHour.Should().Be(100);
    }

    [Fact]
    public void Calculate_QualityMetrics_NoViolations()
    {
        // Arrange
        var schedule = new Schedule
        {
            Assignments =
            [
                new Assignment { OperationId = "OP-001", ResourceId = "EQP-001", StartMs = 0, EndMs = 600000 }
            ],
            Violations = [],
            MakespanMs = 600000
        };

        var job = Job.Create("JOB-001")
            .WithOperation(
                Operation.Create("OP-001", "JOB-001", 1)
                    .WithTime(0, 600000, 0)
                    .WithEquipment("EQP-001")
            );

        var resource = Resource.Equipment("EQP-001");

        var calculator = new KpiCalculator(schedule, [job], [resource]);

        // Act
        var dashboard = calculator.Calculate();

        // Assert
        dashboard.QualityMetrics.TotalViolations.Should().Be(0);
        dashboard.QualityMetrics.ScheduleQualityScore.Should().BeGreaterOrEqualTo(80);
    }

    [Fact]
    public void Calculate_QualityMetrics_WithViolations()
    {
        // Arrange - 많은 위반사항으로 점수가 확실히 감소하도록 설정
        var violations = new List<Violation>();
        for (int i = 0; i < 10; i++)
        {
            violations.Add(new Violation { ConstraintType = "DueDate", Description = "Late" });
        }
        violations.Add(new Violation { ConstraintType = "ResourceConflict", Description = "Overlap" });

        var schedule = new Schedule
        {
            Assignments =
            [
                new Assignment { OperationId = "OP-001", ResourceId = "EQP-001", StartMs = 0, EndMs = 300000 }
            ],
            Violations = violations,
            MakespanMs = 600000 // 50% 활용률 (보너스 없음)
        };

        var job = Job.Create("JOB-001")
            .WithOperation(
                Operation.Create("OP-001", "JOB-001", 1)
                    .WithTime(0, 300000, 0)
                    .WithEquipment("EQP-001")
            );

        var resource = Resource.Equipment("EQP-001");

        var calculator = new KpiCalculator(schedule, [job], [resource]);

        // Act
        var dashboard = calculator.Calculate();

        // Assert
        dashboard.QualityMetrics.TotalViolations.Should().Be(11);
        dashboard.QualityMetrics.ViolationsByType.Should().ContainKey("DueDate");
        dashboard.QualityMetrics.ViolationsByType["DueDate"].Should().Be(10);
        dashboard.QualityMetrics.ViolationsByType["ResourceConflict"].Should().Be(1);
        // 11 violations * 5점 = 55점 감점, 납기 없으므로 +10점 → 100 - 55 + 10 = 55
        dashboard.QualityMetrics.ScheduleQualityScore.Should().BeLessThan(100);
        dashboard.QualityMetrics.ScheduleQualityScore.Should().BeLessOrEqualTo(55);
    }
}
