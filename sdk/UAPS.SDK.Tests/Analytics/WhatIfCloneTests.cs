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

    [Fact]
    public void CloneRequest_PreservesFieldsNoBuilderCarries()
    {
        // 예전 복제는 빌더로 다시 지었고, 빌더가 옮기지 않는 것은 전부 사라졌다.
        // 아래는 그때 사라지던 것들이며, 전부 스케줄 결과를 바꾼다.
        var op = Operation.Create("OP-1", "JOB-1", 1)
            .WithTime(0, 600_000, 0)
            .WithEquipment("PRESS-1")
            .WithMaterial("STEEL-4MM", 2.5)
            .Unmanned();
        op.Dependencies.Add("OP-0");
        op.OperationType = "pressing";
        op.TransitTimeMs = 90_000;

        var resource = Resource.Equipment("PRESS-1");
        resource.Capabilities.Add("pressing");
        resource.Capacity = 3;
        resource.Unavailable.Add(new TimeSlot(0, 60_000));

        var original = new ScheduleRequest
        {
            Jobs = [Job.Create("JOB-1").WithOperation(op)],
            Resources = [resource]
        };

        var clone = new WhatIfSimulator(null!).CloneRequest(original);

        var clonedOp = clone.Jobs.Single().Operations.Single();
        clonedOp.Dependencies.Should().Equal("OP-0");
        clonedOp.MaterialRequirements.Should().HaveCount(1);
        clonedOp.OperationType.Should().Be("pressing");
        clonedOp.TransitTimeMs.Should().Be(90_000);
        clonedOp.RequiresOperator.Should().BeFalse("Unmanned() was set on the original");

        var clonedResource = clone.Resources.Single();
        clonedResource.Capabilities.Should().Equal("pressing");
        clonedResource.Capacity.Should().Be(3);
        clonedResource.Unavailable.Should().Equal(new TimeSlot(0, 60_000));
    }

    [Fact]
    public void CloneRequest_DoesNotShareResourceCollectionsWithTheOriginal()
    {
        var resource = Resource.Equipment("PRESS-1");
        resource.Capabilities.Add("pressing");
        var original = new ScheduleRequest { Jobs = [], Resources = [resource] };

        var clone = new WhatIfSimulator(null!).CloneRequest(original);
        clone.Resources.Single().Capabilities.Add("welding");

        // Equal(params string[]) would read a because-message as another expected
        // element, so the reason goes on a separate assertion.
        resource.Capabilities.Should().Equal(["pressing"]);
        resource.Capabilities.Should().NotContain("welding",
            "editing the scenario must not edit the baseline");
    }
}
