using AwesomeAssertions;
using UAPS.SDK.Client;
using UAPS.SDK.Models;
using UAPS.SDK.Simulation;
using Xunit;

namespace UAPS.SDK.Tests.Simulation;

/// <summary>
/// An oven with lot A (6 h, from 06:00) and lot B (8 h, from 12:00): the time
/// the oven cannot work — a calendar break, its own unavailable slot, a
/// maintenance event — holds in a plain schedule and in every reschedule of a
/// simulation session.
/// </summary>
public class SessionAvailabilityTests
{
    private const long Hour = 3_600_000;
    private static readonly DateTime Day = new(1970, 1, 1, 0, 0, 0, DateTimeKind.Utc);

    private static Job Lot(string id, long from, long hours, double due, bool interruptible = true)
    {
        var op = Operation.Create(id + "-O1", id, 1).WithTime(0, hours * Hour, 0).WithEquipment("OVEN").WithEarliestStart(from * Hour);
        if (!interruptible)
            op.Uninterruptible();
        var job = Job.Create(id).WithPriority(1).WithDueDate(Day.AddHours(due));
        job.Operations.Add(op);
        return job;
    }

    private static Calendar BreakEightToSixteen() => Calendar.Create("CAL", "oven").WithTimeZone("UTC")
        .WithShift(new Shift { Name = "ALL", Start = new TimeOfDay(0, 0), End = new TimeOfDay(23, 59), Days = [.. Enum.GetValues<DayOfWeekType>()] })
        .WithBreak(new BreakTime { Start = new TimeOfDay(8, 0), End = new TimeOfDay(16, 0) });

    private static void NoWorkInsideEightToSixteen(Schedule schedule)
    {
        schedule.Tasks.Should().NotBeEmpty();
        foreach (var t in schedule.Tasks)
            (t.PlanEndMs <= 8 * Hour || t.PlanBeginMs >= 16 * Hour).Should().BeTrue($"{t.OperationId} works {t.PlanBeginMs / Hour}..{t.PlanEndMs / Hour} h");
    }

    private static Assignment Of(Schedule s, string op) => s.Assignments.Single(a => a.OperationId == op);

    [Fact]
    public void A_session_from_a_request_keeps_its_calendar_and_earliest_starts()
    {
        var request = new ScheduleRequest
        {
            Jobs = [Lot("LOT-A", 6, 6, 14), Lot("LOT-B", 12, 8, 22)],
            Resources = [Resource.Equipment("OVEN").WithCalendar("CAL")],
            Calendars = [BreakEightToSixteen()],
            StartTimeMs = 0,
        };
        using var client = new SchedulerClient();
        var plan = client.Schedule(request).Schedule!;
        NoWorkInsideEightToSixteen(plan);

        var again = SimulationSession.FromSchedule(request, plan).Reschedule();

        NoWorkInsideEightToSixteen(again);
        Of(again, "LOT-A-O1").StartMs.Should().BeGreaterThanOrEqualTo(6 * Hour);
        Of(again, "LOT-B-O1").StartMs.Should().BeGreaterThanOrEqualTo(12 * Hour);
        again.MakespanMs.Should().Be(plan.MakespanMs);
    }

    [Fact]
    public void A_resources_unavailable_slot_holds_in_a_plain_schedule()
    {
        var request = new ScheduleRequest
        {
            Jobs = [Lot("LOT-A", 6, 6, 14), Lot("LOT-B", 12, 8, 22)],
            Resources = [Resource.Equipment("OVEN").WithUnavailable(new TimeSlot(8 * Hour, 16 * Hour))],
            StartTimeMs = 0,
        };
        using var client = new SchedulerClient();
        NoWorkInsideEightToSixteen(client.Schedule(request).Schedule!);
    }

    [Fact]
    public void A_maintenance_event_blocks_its_resource_in_the_reschedule()
    {
        var request = new ScheduleRequest
        {
            Jobs = [Lot("LOT-A", 6, 6, 14), Lot("LOT-B", 12, 8, 22)],
            Resources = [Resource.Equipment("OVEN")],
            StartTimeMs = 0,
        };
        var session = SimulationSession.Create(request);
        var before = session.CurrentSchedule!;

        var after = session.ApplyEvent(MaintenanceScheduledEvent.Create("OVEN", 8 * Hour, 8 * Hour));

        NoWorkInsideEightToSixteen(after);
        after.MakespanMs.Should().BeGreaterThan(before.MakespanMs);
        session.Resources.Single().Unavailable.Should().Equal(new TimeSlot(8 * Hour, 16 * Hour));
    }

    /// <summary>
    /// A local strategy keeps what the event does not touch: a maintenance on the oven leaves the
    /// press's lot exactly where it was, and the oven's lots move out of the maintenance.
    /// </summary>
    [Theory]
    [InlineData(RescheduleStrategy.AOR)]
    [InlineData(RescheduleStrategy.Partial)]
    [InlineData(RescheduleStrategy.RightShift)]
    public void A_local_strategy_keeps_the_unaffected_lot_and_moves_the_rest_out_of_the_slot(RescheduleStrategy strategy)
    {
        var press = Job.Create("LOT-C").WithPriority(1).WithDueDate(Day.AddHours(30));
        press.Operations.Add(Operation.Create("LOT-C-O1", "LOT-C", 1).WithTime(0, 5 * Hour, 0).WithEquipment("PRESS").WithEarliestStart(7 * Hour));
        var request = new ScheduleRequest
        {
            Jobs = [Lot("LOT-A", 6, 6, 14), Lot("LOT-B", 12, 8, 22), press],
            Resources = [Resource.Equipment("OVEN"), Resource.Equipment("PRESS")],
            StartTimeMs = 0,
        };
        var session = SimulationSession.Create(request);
        var before = session.CurrentSchedule!;

        var after = session.ApplyEvent(MaintenanceScheduledEvent.Create("OVEN", 8 * Hour, 8 * Hour), strategy);

        Of(after, "LOT-C-O1").Should().BeEquivalentTo(Of(before, "LOT-C-O1"));
        foreach (var t in after.Tasks.Where(t => t.OperationId != "LOT-C-O1"))
            (t.PlanEndMs <= 8 * Hour || t.PlanBeginMs >= 16 * Hour).Should().BeTrue($"{t.OperationId} works {t.PlanBeginMs / Hour}..{t.PlanEndMs / Hour} h");
        // The maintenance makes the oven's lots late; nothing else breaks.
        after.Violations.Where(v => v.ConstraintType != "DueDate").Should().BeEmpty();
    }

    [Fact]
    public void An_uninterruptible_lot_runs_whole_outside_the_break()
    {
        var request = new ScheduleRequest
        {
            Jobs = [Lot("LOT-A", 6, 6, 14, interruptible: false), Lot("LOT-B", 12, 8, 22, interruptible: false)],
            Resources = [Resource.Equipment("OVEN").WithCalendar("CAL")],
            Calendars = [BreakEightToSixteen()],
            StartTimeMs = 0,
        };
        using var client = new SchedulerClient();
        var plan = client.Schedule(request).Schedule!;

        NoWorkInsideEightToSixteen(plan);
        foreach (var op in new[] { "LOT-A-O1", "LOT-B-O1" })
        {
            plan.Tasks.Where(t => t.OperationId == op).Should().ContainSingle($"{op} runs in one stretch");
            var a = Of(plan, op);
            a.EndMs.Should().BeGreaterThanOrEqualTo(a.StartMs);
        }
        (Of(plan, "LOT-A-O1").DurationMs).Should().Be(6 * Hour);
    }
}
