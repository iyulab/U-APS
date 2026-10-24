namespace UAPS.SDK.Models;

/// <summary>
/// A time zone as the engine reads it: an offset from UTC and the instants it changes (daylight saving).
/// The engine schedules on absolute instants but reads a calendar's shifts, breaks and holidays on the
/// zone's wall clock; this schedule is all it needs from the zone, built here from a <see cref="TimeZoneInfo"/>.
/// </summary>
public sealed class UtcOffsetsDto
{
    /// <summary>Offset before the first change, in minutes east of UTC (Asia/Seoul: 540).</summary>
    public int BaseMinutes { get; set; }

    /// <summary>Changes in ascending order.</summary>
    public List<OffsetChangeDto> Changes { get; set; } = [];

    /// <summary>
    /// The offsets of <paramref name="zone"/> over [<paramref name="from"/>, <paramref name="to"/>] — the base offset
    /// in force at <paramref name="from"/> and every change in between, to the minute.
    /// </summary>
    public static UtcOffsetsDto For(TimeZoneInfo zone, DateTimeOffset from, DateTimeOffset to)
    {
        ArgumentNullException.ThrowIfNull(zone);
        if (to < from) throw new ArgumentException("The range ends before it starts.", nameof(to));

        var dto = new UtcOffsetsDto { BaseMinutes = Minutes(zone, from) };
        var current = dto.BaseMinutes;
        // Offsets change at most a few times a year: step a day at a time, then narrow a change to the minute.
        for (var day = from; day < to; day = day.AddDays(1))
        {
            var next = day.AddDays(1) < to ? day.AddDays(1) : to;
            var offset = Minutes(zone, next);
            if (offset == current) continue;
            DateTimeOffset low = day, high = next;
            while (high - low > TimeSpan.FromMinutes(1))
            {
                var mid = low + (high - low) / 2;
                if (Minutes(zone, mid) == current) low = mid; else high = mid;
            }
            var at = new DateTimeOffset(high.UtcTicks - high.UtcTicks % TimeSpan.TicksPerMinute, TimeSpan.Zero);
            dto.Changes.Add(new OffsetChangeDto { AtMs = at.ToUnixTimeMilliseconds(), OffsetMinutes = offset });
            current = offset;
        }
        return dto;
    }

    private static int Minutes(TimeZoneInfo zone, DateTimeOffset at) => (int)zone.GetUtcOffset(at).TotalMinutes;
}

/// <summary>From <see cref="AtMs"/> (UTC epoch milliseconds) on, local time is UTC + <see cref="OffsetMinutes"/>.</summary>
public sealed class OffsetChangeDto
{
    public long AtMs { get; set; }
    public int OffsetMinutes { get; set; }
}
