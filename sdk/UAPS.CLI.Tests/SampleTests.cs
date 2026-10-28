using AwesomeAssertions;
using UAPS.CLI.Json;
using UAPS.SDK.Client;
using Xunit;

namespace UAPS.CLI.Tests;

/// <summary>
/// The samples are small enough to check by hand, and samples/README.md says what each one
/// must give. These tests read every sample through the CLI's JSON parser, schedule it with
/// this repository's engine and assert those answers, so a change to the parser or the engine
/// that breaks a documented result fails here rather than in a user's hands.
/// </summary>
public class SampleTests
{
    private static string Sample(string name) => Path.Combine(AppContext.BaseDirectory, "samples", name);

    private static (Schedule Schedule, List<UAPS.SDK.Models.Job> Jobs) Run(string name, string? rule = null)
    {
        var (jobs, resources) = new JsonInputParser().Parse(Sample(name));
        using var client = new SchedulerClient();
        var request = new ScheduleRequest { Jobs = jobs, Resources = resources };
        if (rule != null)
        {
            request.DispatchingConfig = new DispatchingConfig { PrimaryRule = rule };
        }
        var result = client.Schedule(request);
        result.Success.Should().BeTrue(result.Error);
        return (result.Schedule!, jobs);
    }

    public static TheoryData<string> Samples()
    {
        var data = new TheoryData<string>();
        foreach (var path in Directory.GetFiles(Path.Combine(AppContext.BaseDirectory, "samples"), "*.json").Order())
        {
            data.Add(Path.GetFileName(path));
        }
        return data;
    }

    [Theory]
    [MemberData(nameof(Samples))]
    public void Every_operation_of_every_sample_is_scheduled_once(string name)
    {
        var (schedule, jobs) = Run(name);
        var operations = jobs.SelectMany(j => j.Operations).Select(o => o.Id).Order().ToList();
        schedule.Assignments
            .Where(a => a.Part == null || a.Part.Index == 0)
            .Select(a => a.OperationId)
            .Distinct()
            .Order()
            .Should().Equal(operations);
        schedule.Assignments.Should().OnlyContain(a => a.EndMs > a.StartMs);
    }

    [Fact]
    public void Sample_001_makespan_is_the_sum_of_its_setup_and_process_times()
    {
        var (schedule, _) = Run("001-single-job.json");
        schedule.MakespanMs.Should().Be((300_000 + 600_000) + (120_000 + 900_000) + (0 + 300_000));
    }

    [Fact]
    public void Sample_002_makespan_is_the_same_under_any_rule()
    {
        var spt = Run("002-competing-jobs.json", "SPT").Schedule.MakespanMs;
        var edd = Run("002-competing-jobs.json", "EDD").Schedule.MakespanMs;
        spt.Should().Be(edd);
    }

    [Fact]
    public void Sample_003_both_jobs_start_at_time_zero()
    {
        var (schedule, _) = Run("003-equipment-and-worker.json");
        foreach (var op in new[] { "OP-A-1", "OP-B-1" })
        {
            schedule.Assignments.Where(a => a.OperationId == op).Should().OnlyContain(a => a.StartMs == 0, op);
        }
    }

    [Fact]
    public void Sample_004_puts_one_job_on_the_faster_machine_and_the_other_on_the_slower()
    {
        var (schedule, _) = Run("004-machines-with-their-own-times.json");
        var byResource = schedule.Assignments.ToDictionary(a => a.ResourceId);
        byResource["MILL-NEW"].Should().Match<Assignment>(a => a.StartMs == 0 && a.EndMs == 300_000);
        byResource["MILL-OLD"].Should().Match<Assignment>(a => a.StartMs == 0 && a.EndMs == 600_000);
        schedule.MakespanMs.Should().Be(600_000);
    }
}
