namespace UAPS.SDK.Handlers;

/// <summary>
/// Default calendar handler that expands recurring patterns to TimeWindows
/// </summary>
public class RecurrenceHandler : ICalendarHandler
{
    /// <summary>
    /// Expand calendar pattern to explicit TimeWindow array
    /// </summary>
    public IEnumerable<TimeWindow> Expand(CalendarPattern pattern, DateRange range)
    {
        foreach (var date in range.GetDates())
        {
            // Skip non-working days
            if (!pattern.WorkingDays.Contains(date.DayOfWeek))
                continue;

            // Skip holidays
            if (pattern.Holidays.Contains(date))
                continue;

            // Generate time windows for working periods
            foreach (var window in GenerateDailyWindows(date, pattern))
            {
                yield return window;
            }
        }
    }

    /// <summary>
    /// Generate time windows for a single day, accounting for breaks
    /// </summary>
    private IEnumerable<TimeWindow> GenerateDailyWindows(DateOnly date, CalendarPattern pattern)
    {
        var workStart = pattern.StartTime;
        var workEnd = pattern.EndTime;

        if (pattern.Breaks.Count == 0)
        {
            // No breaks - single window
            yield return CreateWindow(date, workStart, workEnd);
            yield break;
        }

        // Sort breaks by start time
        var sortedBreaks = pattern.Breaks
            .OrderBy(b => b.Start)
            .ToList();

        var currentStart = workStart;

        foreach (var breakPeriod in sortedBreaks)
        {
            // Window before break
            if (currentStart < breakPeriod.Start)
            {
                yield return CreateWindow(date, currentStart, breakPeriod.Start);
            }

            currentStart = breakPeriod.End;
        }

        // Window after last break
        if (currentStart < workEnd)
        {
            yield return CreateWindow(date, currentStart, workEnd);
        }
    }

    /// <summary>
    /// Create TimeWindow from date and time components
    /// </summary>
    private static TimeWindow CreateWindow(DateOnly date, TimeOnly start, TimeOnly end)
    {
        var startDateTime = date.ToDateTime(start);
        var endDateTime = date.ToDateTime(end);

        return TimeWindow.FromDateTime(startDateTime, endDateTime);
    }
}

/// <summary>
/// Shift-based calendar handler for multi-shift operations
/// </summary>
public class ShiftHandler : ICalendarHandler
{
    private readonly List<ShiftDefinition> _shifts = [];

    /// <summary>
    /// Add shift definition
    /// </summary>
    public ShiftHandler AddShift(string name, TimeOnly start, TimeOnly end, DayOfWeek[] workingDays)
    {
        _shifts.Add(new ShiftDefinition(name, start, end, workingDays));
        return this;
    }

    /// <summary>
    /// Create standard 2-shift pattern (day/night)
    /// </summary>
    public static ShiftHandler TwoShift()
    {
        var weekdays = new[] {
            DayOfWeek.Monday, DayOfWeek.Tuesday, DayOfWeek.Wednesday,
            DayOfWeek.Thursday, DayOfWeek.Friday
        };

        return new ShiftHandler()
            .AddShift("Day", new TimeOnly(6, 0), new TimeOnly(14, 0), weekdays)
            .AddShift("Night", new TimeOnly(14, 0), new TimeOnly(22, 0), weekdays);
    }

    /// <summary>
    /// Create standard 3-shift pattern (24h coverage)
    /// </summary>
    public static ShiftHandler ThreeShift()
    {
        var allDays = Enum.GetValues<DayOfWeek>();

        return new ShiftHandler()
            .AddShift("Morning", new TimeOnly(6, 0), new TimeOnly(14, 0), allDays)
            .AddShift("Afternoon", new TimeOnly(14, 0), new TimeOnly(22, 0), allDays)
            .AddShift("Night", new TimeOnly(22, 0), new TimeOnly(6, 0), allDays);
    }

    public IEnumerable<TimeWindow> Expand(CalendarPattern pattern, DateRange range)
    {
        foreach (var date in range.GetDates())
        {
            // Skip holidays from pattern
            if (pattern.Holidays.Contains(date))
                continue;

            foreach (var shift in _shifts)
            {
                if (!shift.WorkingDays.Contains(date.DayOfWeek))
                    continue;

                // Handle overnight shifts
                if (shift.End < shift.Start)
                {
                    // Night shift spanning midnight
                    var startDateTime = date.ToDateTime(shift.Start);
                    var endDateTime = date.AddDays(1).ToDateTime(shift.End);
                    yield return TimeWindow.FromDateTime(startDateTime, endDateTime);
                }
                else
                {
                    var startDateTime = date.ToDateTime(shift.Start);
                    var endDateTime = date.ToDateTime(shift.End);
                    yield return TimeWindow.FromDateTime(startDateTime, endDateTime);
                }
            }
        }
    }
}

/// <summary>
/// Shift definition
/// </summary>
public record ShiftDefinition(string Name, TimeOnly Start, TimeOnly End, DayOfWeek[] WorkingDays);
