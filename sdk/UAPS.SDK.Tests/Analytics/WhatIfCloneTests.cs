using AwesomeAssertions;
using UAPS.SDK.Analytics;
using UAPS.SDK.Client;
using UAPS.SDK.Models;
using Xunit;

namespace UAPS.SDK.Tests.Analytics;

/// <summary>
/// A what-if scenario is only meaningful if it differs from the baseline in
/// exactly the way the scenario says. The clone that scenarios are built on
/// used to rebuild each resource requirement through the builders, and the
/// worker builder carries only a count -- so a worker-candidate restriction
/// disappeared and the load factor reset, and the scenario silently answered a
/// different question from the one it was compared against.
/// </summary>
public class WhatIfCloneTests
{
    private static ScheduleRequest RequestWithRestrictedCell()
    {
        var op = Operation.Create("OP-1", "JOB-1", 1)
            .WithTime(0, 600_000, 0)
            .WithEquipment("PRESS-1", "PRESS-2")
            .WithWorkerCandidates(1, "ALICE", "BOB");

        // A second requirement that is not at full load, so the clone has to
        // carry LoadFactor across as well as the candidate list.
        op.RequiredResources.Add(new ResourceRequirement
        {
            ResourceType = ResourceType.Worker,
            Quantity = 1,
            Candidates = ["CAROL"],
            LoadFactor = 0.5
        });

        return new ScheduleRequest
        {
            Jobs = [Job.Create("JOB-1").WithOperation(op)],
            Resources =
            [
                Resource.Equipment("PRESS-1"),
                Resource.Equipment("PRESS-2"),
                Resource.Worker("ALICE"),
                Resource.Worker("BOB"),
                Resource.Worker("CAROL")
            ]
        };
    }

    [Fact]
    public void CloneRequest_PreservesEveryResourceRequirementField()
    {
        var original = RequestWithRestrictedCell();
        var simulator = new WhatIfSimulator(null!); // CloneRequest does not touch the client

        var clone = simulator.CloneRequest(original);

        var cloned = clone.Jobs.Single().Operations.Single().RequiredResources;
        var source = original.Jobs.Single().Operations.Single().RequiredResources;

        cloned.Should().BeEquivalentTo(source, "a scenario must start from the same problem");
    }

    [Fact]
    public void CloneRequest_DoesNotShareCandidateListsWithTheOriginal()
    {
        var original = RequestWithRestrictedCell();
        var simulator = new WhatIfSimulator(null!);

        var clone = simulator.CloneRequest(original);
        clone.Jobs.Single().Operations.Single().RequiredResources[0].Candidates.Add("MALLORY");

        original.Jobs.Single().Operations.Single().RequiredResources[0].Candidates
            .Should().NotContain("MALLORY", "editing the scenario must not edit the baseline");
    }
}
