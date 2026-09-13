using AwesomeAssertions;
using UAPS.SDK.Analytics;
using UAPS.SDK.Client;
using UAPS.SDK.Models;
using Xunit;

namespace UAPS.SDK.Tests.Analytics;

/// <summary>
/// The four scenario types that used to be declared without an implementation.
/// Each must change the scenario request in exactly the way its name says, leave
/// the baseline request untouched, and fail loudly when it cannot change anything
/// — a scenario that silently changes nothing reports the baseline's numbers as
/// its own result, which is the one reading a caller must never get wrong.
/// </summary>
public class WhatIfJobScenarioTests
{
    private static ScheduleRequest Request() => new()
    {
        Jobs =
        [
            Job.Create("JOB-1").WithOperation(
                Operation.Create("OP-1", "JOB-1", 1)
                    .WithTime(0, 60_000, 0)
                    .WithEquipment("EQP-1")),
            Job.Create("JOB-2").WithOperation(
                Operation.Create("OP-2", "JOB-2", 1)
                    .WithTime(0, 30_000, 0)
                    .WithEquipment("EQP-1")
                    .WithDependencies("OP-1"))
        ],
        Resources = [Resource.Equipment("EQP-1")],
        SetupMatrices = new SetupMatrixCollection().WithMatrix(
            SetupMatrix.Create("EQP-1")
                .WithDefaultSetup(10_000)
                .WithSetup("A", "B", 40_000))
    };

    private static readonly WhatIfSimulator Simulator = new(null!);

    [Fact]
    public void AddJob_AppendsACopyOfTheJob()
    {
        var baseline = Request();
        var job = Job.Create("JOB-3").WithOperation(
            Operation.Create("OP-3", "JOB-3", 1).WithTime(0, 5_000, 0).WithEquipment("EQP-1"));

        var request = Simulator.ApplyScenario(baseline, WhatIfScenario.AddJob("s", job));

        request.Jobs.Select(j => j.Id).Should().Equal("JOB-1", "JOB-2", "JOB-3");
        request.Jobs[2].Should().NotBeSameAs(job, "the caller's object must not become part of a scenario request");
        request.Jobs[2].Operations.Single().Time.ProcessMs.Should().Be(5_000);
        baseline.Jobs.Should().HaveCount(2, "the baseline is never changed");
    }

    [Fact]
    public void AddJob_RefusesADuplicateId()
    {
        var act = () => Simulator.ApplyScenario(Request(), WhatIfScenario.AddJob("s", Job.Create("JOB-1")));
        act.Should().Throw<ArgumentException>().WithMessage("*JOB-1*twice*");
    }

    [Fact]
    public void RemoveJob_DropsTheJobAndTheDependenciesOnItsOperations()
    {
        var baseline = Request();

        var request = Simulator.ApplyScenario(baseline, WhatIfScenario.RemoveJob("s", "JOB-1"));

        request.Jobs.Select(j => j.Id).Should().Equal("JOB-2");
        request.Jobs[0].Operations[0].Dependencies.Should().BeEmpty("nothing is left to wait for");
        baseline.Jobs.Should().HaveCount(2);
        baseline.Jobs[1].Operations[0].Dependencies.Should().Equal("OP-1");
    }

    [Fact]
    public void RemoveJob_RefusesAJobThatIsNotThere()
    {
        var act = () => Simulator.ApplyScenario(Request(), WhatIfScenario.RemoveJob("s", "JOB-9"));
        act.Should().Throw<ArgumentException>().WithMessage("*JOB-9*not in the request*");
    }

    [Fact]
    public void ChangeDueDate_SetsTheDueDateOnTheTargetJobOnly()
    {
        var baseline = Request();
        var due = new DateTime(2026, 10, 1, 0, 0, 0, DateTimeKind.Utc);

        var request = Simulator.ApplyScenario(baseline, WhatIfScenario.ChangeDueDate("s", "JOB-2", due));

        request.Jobs[1].DueDate.Should().Be(due);
        request.Jobs[0].DueDate.Should().BeNull();
        baseline.Jobs[1].DueDate.Should().BeNull();
    }

    [Fact]
    public void ChangeSetupTime_ScalesTheResourceMatrixAndLeavesTheBaselineMatrixAlone()
    {
        var baseline = Request();

        var request = Simulator.ApplyScenario(baseline, WhatIfScenario.ChangeSetupTime("s", "EQP-1", 0.5));

        var matrix = request.SetupMatrices!.Matrices.Single();
        matrix.DefaultSetupMs.Should().Be(5_000);
        matrix.Entries.Single().SetupMs.Should().Be(20_000);
        var original = baseline.SetupMatrices!.Matrices.Single();
        original.DefaultSetupMs.Should().Be(10_000, "the baseline matrix is a different object now");
        original.Entries.Single().SetupMs.Should().Be(40_000);
        request.SetupMatrices.Should().NotBeSameAs(baseline.SetupMatrices);
    }

    [Fact]
    public void ChangeSetupTime_RefusesAResourceWithoutAMatrix()
    {
        var act = () => Simulator.ApplyScenario(Request(), WhatIfScenario.ChangeSetupTime("s", "EQP-9", 2.0));
        act.Should().Throw<ArgumentException>().WithMessage("*EQP-9*no setup matrix*");

        var none = Request();
        none.SetupMatrices = null;
        var actNone = () => Simulator.ApplyScenario(none, WhatIfScenario.ChangeSetupTime("s", "EQP-1", 2.0));
        actNone.Should().Throw<ArgumentException>();
    }

    [Fact]
    public void EveryDeclaredTypeIsHandled()
    {
        // The guard against a silently ignored type stays; every current member
        // must get past it, which it can only do by being implemented.
        foreach (var type in Enum.GetValues<WhatIfScenarioType>())
        {
            var scenario = type switch
            {
                WhatIfScenarioType.AddResource => WhatIfScenario.AddResource("s", Resource.Equipment("EQP-2")),
                WhatIfScenarioType.AddJob => WhatIfScenario.AddJob("s", Job.Create("JOB-3")),
                WhatIfScenarioType.ChangeDueDate => WhatIfScenario.ChangeDueDate("s", "JOB-1", DateTime.UtcNow),
                WhatIfScenarioType.ChangeSetupTime => WhatIfScenario.ChangeSetupTime("s", "EQP-1", 1.5),
                WhatIfScenarioType.RemoveJob => WhatIfScenario.RemoveJob("s", "JOB-2"),
                _ => new WhatIfScenario { Name = "s", Type = type, TargetId = "JOB-1" }
            };
            var act = () => Simulator.ApplyScenario(Request(), scenario);
            act.Should().NotThrow($"{type} is declared, so it must be implemented");
        }
    }
}
