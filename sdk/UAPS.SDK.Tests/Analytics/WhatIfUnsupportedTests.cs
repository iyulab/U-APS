using AwesomeAssertions;
using UAPS.SDK.Analytics;
using UAPS.SDK.Client;
using UAPS.SDK.Models;
using Xunit;

namespace UAPS.SDK.Tests.Analytics;

/// <summary>
/// Four of the nine declared scenario types have no implementation. Returning
/// the request untouched made them report a success whose numbers matched the
/// baseline — the same thing a caller sees when a real change turns out not to
/// matter. A what-if that cannot tell those two apart is worse than one that
/// refuses.
/// </summary>
public class WhatIfUnsupportedTests
{
    public static TheoryData<WhatIfScenarioType> Unimplemented =>
    [
        WhatIfScenarioType.AddJob,
        WhatIfScenarioType.RemoveJob,
        WhatIfScenarioType.ChangeDueDate,
        WhatIfScenarioType.ChangeSetupTime
    ];

    public static TheoryData<WhatIfScenarioType> Implemented =>
    [
        WhatIfScenarioType.AddResource,
        WhatIfScenarioType.RemoveResource,
        WhatIfScenarioType.ChangeResourceEfficiency,
        WhatIfScenarioType.ChangePriority,
        WhatIfScenarioType.ChangeProcessTime
    ];

    private static ScheduleRequest MinimalRequest() => new()
    {
        Jobs =
        [
            Job.Create("JOB-1").WithOperation(
                Operation.Create("OP-1", "JOB-1", 1)
                    .WithTime(0, 60_000, 0)
                    .WithEquipment("EQP-1"))
        ],
        Resources = [Resource.Equipment("EQP-1")]
    };

    [Theory]
    [MemberData(nameof(Unimplemented))]
    public void ApplyScenario_RefusesATypeItDoesNotImplement(WhatIfScenarioType type)
    {
        var simulator = new WhatIfSimulator(null!);
        var scenario = new WhatIfScenario { Name = "s", Type = type, TargetId = "JOB-1" };

        var act = () => simulator.ApplyScenario(MinimalRequest(), scenario);

        act.Should().Throw<NotSupportedException>()
            .WithMessage("*not implemented*",
                "silently returning the baseline reads as 'this change did nothing'");
    }

    [Theory]
    [MemberData(nameof(Implemented))]
    public void ApplyScenario_AcceptsEveryTypeItDoesImplement(WhatIfScenarioType type)
    {
        var simulator = new WhatIfSimulator(null!);
        var scenario = new WhatIfScenario { Name = "s", Type = type, TargetId = "JOB-1" };

        var act = () => simulator.ApplyScenario(MinimalRequest(), scenario);

        act.Should().NotThrow("the guard must not catch a type that is handled");
    }
}
