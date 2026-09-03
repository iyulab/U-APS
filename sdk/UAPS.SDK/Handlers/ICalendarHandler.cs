namespace UAPS.SDK.Handlers;

/// <summary>
/// Calendar pattern to TimeWindow array transformation handler
/// </summary>
public interface ICalendarHandler
{
    /// <summary>
    /// Expand calendar pattern to explicit TimeWindow array
    /// </summary>
    /// <param name="pattern">User-friendly calendar pattern</param>
    /// <param name="range">Date range to expand</param>
    /// <returns>Explicit TimeWindow array for engine</returns>
    IEnumerable<TimeWindow> Expand(CalendarPattern pattern, DateRange range);
}

/// <summary>
/// Time window representing a working period (explicit data for engine)
/// </summary>
public record TimeWindow
{
    /// <summary>
    /// Start time in Unix epoch milliseconds
    /// </summary>
    public long StartMs { get; init; }

    /// <summary>
    /// End time in Unix epoch milliseconds
    /// </summary>
    public long EndMs { get; init; }

    public TimeWindow(long startMs, long endMs)
    {
        StartMs = startMs;
        EndMs = endMs;
    }

    /// <summary>
    /// Create from DateTime objects
    /// </summary>
    public static TimeWindow FromDateTime(DateTime start, DateTime end)
    {
        return new TimeWindow(
            new DateTimeOffset(start).ToUnixTimeMilliseconds(),
            new DateTimeOffset(end).ToUnixTimeMilliseconds()
        );
    }

    /// <summary>
    /// Duration in milliseconds
    /// </summary>
    public long DurationMs => EndMs - StartMs;
}

/// <summary>
/// Date range for calendar expansion
/// </summary>
public record DateRange
{
    public DateOnly Start { get; init; }
    public DateOnly End { get; init; }

    public DateRange(DateOnly start, DateOnly end)
    {
        Start = start;
        End = end;
    }

    /// <summary>
    /// Create from DateTime objects
    /// </summary>
    public static DateRange FromDateTime(DateTime start, DateTime end)
    {
        return new DateRange(
            DateOnly.FromDateTime(start),
            DateOnly.FromDateTime(end)
        );
    }

    /// <summary>
    /// Get all dates in range
    /// </summary>
    public IEnumerable<DateOnly> GetDates()
    {
        for (var date = Start; date <= End; date = date.AddDays(1))
        {
            yield return date;
        }
    }
}

/// <summary>
/// User-friendly calendar pattern
/// </summary>
public class CalendarPattern
{
    /// <summary>
    /// Working days of week
    /// </summary>
    public DayOfWeek[] WorkingDays { get; set; } =
        [DayOfWeek.Monday, DayOfWeek.Tuesday, DayOfWeek.Wednesday,
         DayOfWeek.Thursday, DayOfWeek.Friday];

    /// <summary>
    /// Daily working start time
    /// </summary>
    public TimeOnly StartTime { get; set; } = new(9, 0);

    /// <summary>
    /// Daily working end time
    /// </summary>
    public TimeOnly EndTime { get; set; } = new(18, 0);

    /// <summary>
    /// Break periods (lunch, etc.)
    /// </summary>
    public List<BreakPeriod> Breaks { get; set; } = [];

    /// <summary>
    /// Holidays (non-working days)
    /// </summary>
    public List<DateOnly> Holidays { get; set; } = [];

    /// <summary>
    /// Create default weekday pattern (Mon-Fri, 9:00-18:00)
    /// </summary>
    public static CalendarPattern Weekdays() => new();

    /// <summary>
    /// Create 24/7 pattern
    /// </summary>
    public static CalendarPattern Continuous() => new()
    {
        WorkingDays = Enum.GetValues<DayOfWeek>(),
        StartTime = new TimeOnly(0, 0),
        EndTime = new TimeOnly(23, 59, 59, 999)
    };

    /// <summary>
    /// Builder: Set working hours
    /// </summary>
    public CalendarPattern WithWorkingHours(TimeOnly start, TimeOnly end)
    {
        StartTime = start;
        EndTime = end;
        return this;
    }

    /// <summary>
    /// Builder: Set working days
    /// </summary>
    public CalendarPattern WithWorkingDays(params DayOfWeek[] days)
    {
        WorkingDays = days;
        return this;
    }

    /// <summary>
    /// Builder: Add break period
    /// </summary>
    public CalendarPattern WithBreak(TimeOnly start, TimeOnly end)
    {
        Breaks.Add(new BreakPeriod(start, end));
        return this;
    }

    /// <summary>
    /// Builder: Add holidays
    /// </summary>
    public CalendarPattern WithHolidays(IEnumerable<DateOnly> holidays)
    {
        Holidays.AddRange(holidays);
        return this;
    }
}

/// <summary>
/// Break period during working day
/// </summary>
public record BreakPeriod(TimeOnly Start, TimeOnly End);
