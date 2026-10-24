using System.Text.Json;
using AwesomeAssertions;
using UAPS.SDK.Client;
using UAPS.SDK.Models;
using Xunit;

namespace UAPS.SDK.Tests.Models;

/// <summary>
/// A calendar read on a time zone's wall clock: the SDK lays the zone out as the offsets the engine reads,
/// and the engine places work inside the zone's shifts — not inside the same clock times in UTC.
/// </summary>
public class CalendarTimeZoneTests
{
    private const long Hour = 3_600_000;
    // 2027-02-08 00:00 UTC, a Monday
    private const long MondayUtc = 1_802_044_800_000;

    [Fact]
    public void A_zone_without_daylight_saving_is_one_offset()
    {
        var offsets = UtcOffsetsDto.For(TimeZoneInfo.FindSystemTimeZoneById("Asia/Seoul"),
            DateTimeOffset.FromUnixTimeMilliseconds(MondayUtc), DateTimeOffset.FromUnixTimeMilliseconds(MondayUtc).AddDays(367));

        offsets.BaseMinutes.Should().Be(540);
        offsets.Changes.Should().BeEmpty();
    }

    [Fact]
    public void A_daylight_saving_zone_lists_its_changes_to_the_minute()
    {
        var offsets = UtcOffsetsDto.For(TimeZoneInfo.FindSystemTimeZoneById("America/New_York"),
            new DateTimeOffset(2027, 1, 1, 0, 0, 0, TimeSpan.Zero), new DateTimeOffset(2028, 1, 1, 0, 0, 0, TimeSpan.Zero));

        offsets.BaseMinutes.Should().Be(-300);
        offsets.Changes.Select(c => (DateTimeOffset.FromUnixTimeMilliseconds(c.AtMs), c.OffsetMinutes)).Should().Equal(
            (new DateTimeOffset(2027, 3, 14, 7, 0, 0, TimeSpan.Zero), -240),
            (new DateTimeOffset(2027, 11, 7, 6, 0, 0, TimeSpan.Zero), -300));
    }

    [Fact]
    public void The_engine_calendar_carries_the_zone_only_when_one_is_set()
    {
        Calendar.DefaultDay().ToEngineDto(MondayUtc).UtcOffsets.Should().BeNull();
        var dto = Calendar.DefaultDay().WithTimeZone("Asia/Seoul").ToEngineDto(MondayUtc);
        dto.UtcOffsets!.BaseMinutes.Should().Be(540);

        var json = JsonSerializer.Serialize(dto, new JsonSerializerOptions { PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower });
        json.Should().Contain("\"utc_offsets\":{\"base_minutes\":540,\"changes\":[]}");
    }

    [Fact]
    public void A_holiday_is_the_date_on_the_calendars_wall_clock()
    {
        // 2027-02-09 is a holiday; 08:30 that day in Seoul is still 2027-02-08 in UTC.
        var seoul = Calendar.DefaultDay().WithTimeZone("Asia/Seoul").WithHoliday(new DateTime(2027, 2, 9));
        var tuesday830Seoul = MondayUtc + 23 * Hour + 30 * 60_000;
        seoul.IsHoliday(tuesday830Seoul).Should().BeTrue();
        Calendar.DefaultDay().WithHoliday(new DateTime(2027, 2, 9)).IsHoliday(tuesday830Seoul).Should().BeFalse();
    }

    [Fact]
    public void The_engine_places_work_inside_the_zones_shift()
    {
        // Monday 00:00 in Seoul is Sunday 15:00 UTC. A Seoul day shift starts Monday 08:00 Seoul = Sunday 23:00 UTC;
        // read in UTC the same shift would start Monday 08:00 UTC, nine hours later.
        var start = MondayUtc - 9 * Hour;
        ScheduleRequest Request(Calendar calendar) => new()
        {
            Jobs = [Job.Create("J1").WithOperation(Operation.Create("J1-1", "J1", 1).WithTime(0, Hour, 0).WithEquipment("EQP-1"))],
            Resources = [Resource.Equipment("EQP-1").WithCalendar(calendar.Id)],
            StartTimeMs = start,
            Calendars = [calendar],
        };
        using var client = new SchedulerClient();

        var seoul = client.Schedule(Request(Calendar.DefaultDay().WithTimeZone("Asia/Seoul")));
        var utc = client.Schedule(Request(Calendar.DefaultDay()));

        seoul.Success.Should().BeTrue(seoul.Error ?? "no error");
        utc.Success.Should().BeTrue(utc.Error ?? "no error");
        seoul.Schedule!.Assignments.Single().StartMs.Should().Be(MondayUtc - Hour);
        utc.Schedule!.Assignments.Single().StartMs.Should().Be(MondayUtc + 8 * Hour);
    }
}
