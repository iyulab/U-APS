using FluentAssertions;
using UAPS.SDK.Models;
using Xunit;

namespace UAPS.SDK.Tests;

public class SkillMatrixTests
{
    [Fact]
    public void SkillLevel_GetEfficiency_ReturnsCorrectValues()
    {
        SkillLevel.None.GetEfficiency().Should().Be(0.0);
        SkillLevel.Beginner.GetEfficiency().Should().Be(0.5);
        SkillLevel.Intermediate.GetEfficiency().Should().Be(0.75);
        SkillLevel.Advanced.GetEfficiency().Should().Be(1.0);
        SkillLevel.Expert.GetEfficiency().Should().Be(1.2);
    }

    [Fact]
    public void SkillLevel_FromProficiency_ReturnsCorrectLevel()
    {
        SkillLevelExtensions.FromProficiency(0.0).Should().Be(SkillLevel.None);
        SkillLevelExtensions.FromProficiency(0.4).Should().Be(SkillLevel.Beginner);
        SkillLevelExtensions.FromProficiency(0.8).Should().Be(SkillLevel.Intermediate);
        SkillLevelExtensions.FromProficiency(1.0).Should().Be(SkillLevel.Advanced);
        SkillLevelExtensions.FromProficiency(1.5).Should().Be(SkillLevel.Expert);
    }

    [Fact]
    public void SkillMatrix_WithSkill_StoresAndRetrievesProficiency()
    {
        var matrix = new SkillMatrix()
            .WithSkill("W1", "CNC", 1.0)
            .WithSkill("W1", "Assembly", 0.7)
            .WithSkill("W2", "CNC", 0.5);

        matrix.GetProficiency("W1", "CNC").Should().Be(1.0);
        matrix.GetProficiency("W1", "Assembly").Should().Be(0.7);
        matrix.GetProficiency("W2", "CNC").Should().Be(0.5);
    }

    [Fact]
    public void SkillMatrix_WithDefault_ReturnsDefaultForUnknownSkills()
    {
        var matrix = new SkillMatrix()
            .WithDefault(0.8)
            .WithSkill("W1", "CNC", 1.0);

        matrix.GetProficiency("W1", "CNC").Should().Be(1.0);
        matrix.GetProficiency("W1", "Unknown").Should().Be(0.8);
        matrix.GetProficiency("W2", "CNC").Should().Be(0.8);
    }

    [Fact]
    public void SkillMatrix_GetSkillLevel_ReturnsCorrectLevel()
    {
        var matrix = new SkillMatrix()
            .WithSkill("W1", "CNC", 1.0)
            .WithSkill("W2", "CNC", 0.5);

        matrix.GetSkillLevel("W1", "CNC").Should().Be(SkillLevel.Advanced);
        matrix.GetSkillLevel("W2", "CNC").Should().Be(SkillLevel.Beginner);
    }

    [Fact]
    public void SkillMatrix_CanPerform_ReturnsTrueForPositiveProficiency()
    {
        var matrix = new SkillMatrix()
            .WithSkill("W1", "CNC", 1.0)
            .WithSkill("W2", "CNC", 0.0);

        matrix.CanPerform("W1", "CNC").Should().BeTrue();
        matrix.CanPerform("W2", "CNC").Should().BeFalse();
    }

    [Fact]
    public void SkillMatrix_GetQualifiedWorkers_ReturnsWorkersWithPositiveProficiency()
    {
        var matrix = new SkillMatrix()
            .WithSkill("W1", "CNC", 1.0)
            .WithSkill("W2", "CNC", 0.5)
            .WithSkill("W3", "CNC", 0.0)
            .WithSkill("W4", "Assembly", 0.8);

        var workers = matrix.GetQualifiedWorkers("CNC");

        workers.Should().HaveCount(2);
        workers.Should().Contain(w => w.WorkerId == "W1" && w.Proficiency == 1.0);
        workers.Should().Contain(w => w.WorkerId == "W2" && w.Proficiency == 0.5);
    }

    [Fact]
    public void SkillMatrix_CalculateProcessTime_AppliesProficiency()
    {
        var matrix = new SkillMatrix()
            .WithSkill("W1", "CNC", 1.0)
            .WithSkill("W2", "CNC", 0.5)
            .WithSkill("W3", "CNC", 0.0);

        var baseTime = 60_000L; // 60 seconds

        // W1: proficiency 1.0 -> 60 seconds
        matrix.CalculateProcessTime("W1", "CNC", baseTime).Should().Be(60_000);

        // W2: proficiency 0.5 -> 120 seconds
        matrix.CalculateProcessTime("W2", "CNC", baseTime).Should().Be(120_000);

        // W3: proficiency 0.0 -> null (cannot perform)
        matrix.CalculateProcessTime("W3", "CNC", baseTime).Should().BeNull();
    }

    [Fact]
    public void SkillMatrix_GetBestWorker_ReturnsWorkerWithHighestProficiency()
    {
        var matrix = new SkillMatrix()
            .WithSkill("W1", "CNC", 0.8)
            .WithSkill("W2", "CNC", 1.2)
            .WithSkill("W3", "CNC", 0.5);

        var best = matrix.GetBestWorker("CNC");

        best.Should().NotBeNull();
        best!.Value.WorkerId.Should().Be("W2");
        best!.Value.Proficiency.Should().Be(1.2);
    }

    [Fact]
    public void SkillMatrix_GetBestWorker_ReturnsNullWhenNoQualifiedWorkers()
    {
        var matrix = new SkillMatrix()
            .WithSkill("W1", "Assembly", 1.0);

        var best = matrix.GetBestWorker("CNC");

        best.Should().BeNull();
    }

    [Fact]
    public void SkillMatrix_ClampsProficiencyValues()
    {
        var matrix = new SkillMatrix()
            .WithSkill("W1", "CNC", 3.0)  // Should clamp to 2.0
            .WithSkill("W2", "CNC", -1.0); // Should clamp to 0.0

        matrix.GetProficiency("W1", "CNC").Should().Be(2.0);
        matrix.GetProficiency("W2", "CNC").Should().Be(0.0);
    }

    [Fact]
    public void LearningCurve_InitialProficiency_ReturnsInitialValue()
    {
        var curve = new LearningCurve(0.5, 1.0, 0.1);

        curve.CurrentProficiency.Should().BeApproximately(0.5, 0.01);
    }

    [Fact]
    public void LearningCurve_AddExperience_IncreasesProficiency()
    {
        var curve = new LearningCurve(0.5, 1.0, 0.1);

        var initial = curve.CurrentProficiency;
        curve.AddExperience(10);
        var afterExperience = curve.CurrentProficiency;

        afterExperience.Should().BeGreaterThan(initial);
        afterExperience.Should().BeLessThanOrEqualTo(1.0);
    }

    [Fact]
    public void LearningCurve_ConvergesToMaxProficiency()
    {
        var curve = new LearningCurve(0.5, 1.0, 0.1);

        // Add lots of experience
        curve.AddExperience(1000);

        // Should be very close to max
        curve.CurrentProficiency.Should().BeApproximately(1.0, 0.01);
    }

}
