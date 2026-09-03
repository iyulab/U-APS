using FluentAssertions;
using UAPS.SDK.Analytics;
using UAPS.SDK.Client;
using UAPS.SDK.Models;
using Xunit;

namespace UAPS.SDK.Tests.Analytics;

/// <summary>
/// WhatIfScenario 단위 테스트
/// </summary>
public class WhatIfScenarioTests
{
    [Fact]
    public void AddResource_ShouldCreateCorrectScenario()
    {
        // Arrange
        var resource = Resource.Equipment("NEW-EQP-001");

        // Act
        var scenario = WhatIfScenario.AddResource("Add new machine", resource);

        // Assert
        scenario.Type.Should().Be(WhatIfScenarioType.AddResource);
        scenario.TargetId.Should().Be("NEW-EQP-001");
        scenario.Parameters.Should().ContainKey("resource");
    }

    [Fact]
    public void RemoveResource_ShouldCreateCorrectScenario()
    {
        // Act
        var scenario = WhatIfScenario.RemoveResource("Remove old machine", "OLD-EQP-001");

        // Assert
        scenario.Type.Should().Be(WhatIfScenarioType.RemoveResource);
        scenario.TargetId.Should().Be("OLD-EQP-001");
    }

    [Fact]
    public void ChangeEfficiency_ShouldCreateCorrectScenario()
    {
        // Act
        var scenario = WhatIfScenario.ChangeEfficiency("Upgrade machine", "EQP-001", 1.2);

        // Assert
        scenario.Type.Should().Be(WhatIfScenarioType.ChangeResourceEfficiency);
        scenario.TargetId.Should().Be("EQP-001");
        scenario.Parameters.Should().ContainKey("efficiency");
        scenario.Parameters["efficiency"].Should().Be(1.2);
    }

    [Fact]
    public void ChangePriority_ShouldCreateCorrectScenario()
    {
        // Act
        var scenario = WhatIfScenario.ChangePriority("Urgent order", "JOB-001", 1);

        // Assert
        scenario.Type.Should().Be(WhatIfScenarioType.ChangePriority);
        scenario.TargetId.Should().Be("JOB-001");
        scenario.Parameters["priority"].Should().Be(1);
    }

    [Fact]
    public void ChangeProcessTime_ShouldCreateCorrectScenario()
    {
        // Act
        var scenario = WhatIfScenario.ChangeProcessTime("Speed up", "OP-001", 0.8);

        // Assert
        scenario.Type.Should().Be(WhatIfScenarioType.ChangeProcessTime);
        scenario.TargetId.Should().Be("OP-001");
        scenario.Parameters["multiplier"].Should().Be(0.8);
    }
}

/// <summary>
/// WhatIfComparison 단위 테스트
/// </summary>
public class WhatIfComparisonTests
{
    [Fact]
    public void IsImprovement_DecreasedMakespan_ShouldBeTrue()
    {
        // Arrange
        var comparison = new WhatIfComparison
        {
            MakespanChangePercent = -10.0,
            UtilizationChangePercent = 0,
            OnTimeRateChangePercent = 0
        };

        // Assert
        comparison.IsImprovement.Should().BeTrue();
    }

    [Fact]
    public void IsImprovement_IncreasedOnTimeRate_ShouldBeTrue()
    {
        // Arrange
        var comparison = new WhatIfComparison
        {
            MakespanChangePercent = 0,
            UtilizationChangePercent = 0,
            OnTimeRateChangePercent = 5.0
        };

        // Assert
        comparison.IsImprovement.Should().BeTrue();
    }

    [Fact]
    public void IsImprovement_IncreasedMakespan_ShouldBeFalse()
    {
        // Arrange
        var comparison = new WhatIfComparison
        {
            MakespanChangePercent = 10.0,
            UtilizationChangePercent = 5.0,
            OnTimeRateChangePercent = 0
        };

        // Assert
        comparison.IsImprovement.Should().BeFalse();
    }
}

/// <summary>
/// WhatIfResult 단위 테스트
/// </summary>
public class WhatIfResultTests
{
    [Fact]
    public void WhatIfResult_DefaultValues_ShouldBeCorrect()
    {
        // Act
        var result = new WhatIfResult();

        // Assert
        result.ScenarioName.Should().BeEmpty();
        result.Success.Should().BeFalse();
        result.Schedule.Should().BeNull();
        result.Kpis.Should().BeNull();
        result.Comparison.Should().BeNull();
    }

    [Fact]
    public void WhatIfResult_SuccessfulScenario_ShouldHaveAllData()
    {
        // Arrange
        var result = new WhatIfResult
        {
            ScenarioName = "Test Scenario",
            Success = true,
            Schedule = new Schedule
            {
                Assignments = [],
                MakespanMs = 1000
            },
            Kpis = new KpiDashboard(),
            Comparison = new WhatIfComparison
            {
                MakespanChangePercent = -5.0
            }
        };

        // Assert
        result.Success.Should().BeTrue();
        result.Schedule.Should().NotBeNull();
        result.Kpis.Should().NotBeNull();
        result.Comparison.Should().NotBeNull();
        result.Comparison.IsImprovement.Should().BeTrue();
    }
}
