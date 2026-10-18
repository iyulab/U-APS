using System.Text.Json;
using System.Text.Json.Nodes;
using AwesomeAssertions;
using UAPS.SDK.Client;
using UAPS.SDK.Models;
using Xunit;

namespace UAPS.SDK.Tests.Client;

/// <summary>
/// Pins the JSON the SDK sends to the engine. The same file is read by the
/// engine's own test (<c>engine/tests/golden.rs</c>, <c>sdk_request_fixture_is_accepted</c>),
/// so a section the SDK writes in a shape the engine cannot read fails one side
/// or the other instead of reaching a user as a parse error.
/// Regenerate with <c>UPDATE_GOLDEN=1 dotnet test</c> and review the diff.
/// </summary>
public class RequestWireContractTests
{
    private static readonly string FixturePath = Path.Combine(
        RepoRoot(), "engine", "tests", "fixtures", "sdk-request-full.json");

    private static string RepoRoot()
    {
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        while (dir != null && !File.Exists(Path.Combine(dir.FullName, "engine", "Cargo.toml")))
        {
            dir = dir.Parent;
        }
        return dir?.FullName ?? throw new InvalidOperationException("engine/Cargo.toml not found above the test binary");
    }

    /// <summary>A request with every optional section filled.</summary>
    internal static ScheduleRequest FullRequest()
    {
        const long start = 1_705_910_400_000; // 2024-01-22 08:00 UTC, a Monday
        var job1 = Job.Create("J1").WithPriority(1).WithDueDate(DateTimeOffset.FromUnixTimeMilliseconds(start + 8 * 3_600_000).UtcDateTime)
            .WithOperation(Operation.Create("J1-1", "J1", 1).WithTime(5_000, 60_000, 0)
                .WithEquipment("EQP-1").WithWorkers(1).WithMaterial("STEEL", 10))
            .WithOperation(Operation.Create("J1-2", "J1", 2).WithTime(0, 30_000, 0)
                .WithEquipment("EQP-2").WithCrew("TEAM-A"));
        job1.ProductName = "ProductA";
        job1.Operations[0].OperationType = "welding";
        var job2 = Job.Create("J2").WithPriority(2)
            .WithOperation(Operation.Create("J2-1", "J2", 1).WithTime(0, 45_000, 0)
                .WithEquipment("EQP-1").WithWorkers(1).WithDependencies("J1-2"))
            .WithOperation(Operation.Create("J2-2", "J2", 2).WithTime(0, 20_000, 0)
                .WithOutsourcing("VENDOR-1", leadTimeDays: 1));
        job2.ProductName = "ProductB";
        job2.Operations[0].OperationType = "welding";
        job2.Operations[1].OperationType = "plating";

        return new ScheduleRequest
        {
            Jobs = [job1, job2],
            Resources =
            [
                Resource.Equipment("EQP-1").WithEfficiency(1.0),
                Resource.Equipment("EQP-2"),
                Resource.Worker("W1"),
                Resource.Worker("W2"),
            ],
            StartTimeMs = start,
            SetupMatrices = new SetupMatrixCollection().WithMatrix(
                SetupMatrix.Create("EQP-1").WithDefaultSetup(10_000).WithSetup("ProductA", "ProductB", 20_000)),
            Calendars = [Calendar.DefaultDay()],
            SkillMatrix = new SkillMatrix().WithSkill("W1", "welding", 1.5).WithSkill("W2", "welding", 0.8),
            CertificationMatrix = new CertificationMatrix().WithCertification(
                Certification.Create("C1", "W1", "welding", CertificationTarget.OperationType, CertificationLevel.Advanced, start - 86_400_000)),
            WorkerTimeOverride = new WorkerTimeOverride().WithProcessTime("W2", "welding", 50_000),
            CrewManager = new CrewManager().WithCrew(Crew.Create("TEAM-A", "Team A").WithLeader("W1").WithMember("W2").WithEquipment("EQP-2")),
            DispatchingConfig = new DispatchingConfig { PrimaryRule = "EDD", TieBreaker = "SPT" },
            MaterialManager = MaterialManager.Create().AddMaterial(Material.Create("STEEL", "Steel plate").WithStock(100)),
            OutsourcingManager = OutsourcingManager.Create().WithProvider(
                OutsourcingProvider.Create("VENDOR-1", "Plating vendor").WithOperation("plating").WithLeadTimeDays(1)),
        };
    }

    [Fact]
    public void FullRequest_SerializesToThePinnedWireShape()
    {
        var actual = JsonNode.Parse(SchedulerClient.SerializeRequest(FullRequest()))!;
        var rendered = actual.ToJsonString(new JsonSerializerOptions { WriteIndented = true }) + "\n";

        if (Environment.GetEnvironmentVariable("UPDATE_GOLDEN") is not null)
        {
            Directory.CreateDirectory(Path.GetDirectoryName(FixturePath)!);
            File.WriteAllText(FixturePath, rendered);
            return;
        }

        File.Exists(FixturePath).Should().BeTrue($"{FixturePath} is missing — run with UPDATE_GOLDEN=1");
        var expected = JsonNode.Parse(File.ReadAllText(FixturePath))!;
        JsonNode.DeepEquals(expected, actual).Should().BeTrue(
            $"the request JSON differs from {FixturePath}; actual:\n{rendered}");
    }

    [Fact]
    public void FullRequest_IsScheduledByTheEngine()
    {
        using var client = new SchedulerClient();

        var result = client.Schedule(FullRequest());

        result.Success.Should().BeTrue(result.Error ?? "No error message");
        result.Schedule!.Assignments.Should().NotBeEmpty();
    }
}
