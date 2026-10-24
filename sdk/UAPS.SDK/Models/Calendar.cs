namespace UAPS.SDK.Models;

/// <summary>
/// 요일
/// </summary>
public enum DayOfWeekType
{
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday
}

/// <summary>
/// 시간 (HH:MM)
/// </summary>
public record TimeOfDay(int Hour, int Minute)
{
    public int ToMinutes() => Hour * 60 + Minute;
    public long ToMs() => ToMinutes() * 60 * 1000;
}

/// <summary>
/// 교대 근무
/// </summary>
public class Shift
{
    public string Name { get; set; } = string.Empty;
    public TimeOfDay Start { get; set; } = new(0, 0);
    public TimeOfDay End { get; set; } = new(0, 0);
    public List<DayOfWeekType> Days { get; set; } = [];

    /// <summary>
    /// Breaks of this shift only (a day shift's lunch, a night shift's meal break). The calendar's <see cref="Calendar.Breaks"/> still apply to every shift.
    /// A break after midnight in an overnight shift belongs to the shift that started the day before.
    /// </summary>
    public List<BreakTime> Breaks { get; set; } = [];

    /// <summary>Add a break of this shift only.</summary>
    public Shift WithBreak(BreakTime breakTime)
    {
        Breaks.Add(breakTime);
        return this;
    }

    // ===== Factory Methods =====

    public static Shift DayShift() => new()
    {
        Name = "주간",
        Start = new TimeOfDay(8, 0),
        End = new TimeOfDay(17, 0),
        Days = [DayOfWeekType.Monday, DayOfWeekType.Tuesday, DayOfWeekType.Wednesday,
                DayOfWeekType.Thursday, DayOfWeekType.Friday]
    };

    public static Shift NightShift() => new()
    {
        Name = "야간",
        Start = new TimeOfDay(17, 0),
        End = new TimeOfDay(2, 0),
        Days = [DayOfWeekType.Monday, DayOfWeekType.Tuesday, DayOfWeekType.Wednesday,
                DayOfWeekType.Thursday, DayOfWeekType.Friday]
    };

    // ===== Computed =====

    public int DurationMinutes()
    {
        int start = Start.ToMinutes();
        int end = End.ToMinutes();
        return end > start ? end - start : (24 * 60 - start) + end;
    }
}

/// <summary>
/// 휴식 시간
/// </summary>
public class BreakTime
{
    public TimeOfDay Start { get; set; } = new(0, 0);
    public TimeOfDay End { get; set; } = new(0, 0);

    // ===== Factory Methods =====

    public static BreakTime Lunch() => new()
    {
        Start = new TimeOfDay(12, 0),
        End = new TimeOfDay(13, 0)
    };

    // ===== Computed =====

    public int DurationMinutes() => End.ToMinutes() - Start.ToMinutes();
}

/// <summary>
/// A one-date exception that overrides the weekday rule and the holidays for that date (overtime day, swapped holiday, closure).
/// <para><see cref="Working"/> <c>false</c>: no shift starts that date. <c>true</c>: the shifts named in <see cref="Shifts"/> start that date,
/// or, when none are named, that weekday's shifts — even on a holiday. To work on a weekday that normally has no shift, name the shifts.</para>
/// </summary>
public class CalendarException
{
    /// <summary>The date, read like a holiday (its epoch ms — UTC midnight by convention).</summary>
    public long Date { get; set; }
    public bool Working { get; set; }
    /// <summary>Names of the shifts (<see cref="Shift.Name"/>) that start that date — read only when <see cref="Working"/>.</summary>
    public List<string> Shifts { get; set; } = [];

    public static CalendarException Closed(DateOnly date) => new() { Date = EpochMs(date), Working = false };

    public static CalendarException WorkingDay(DateOnly date, params string[] shifts) => new() { Date = EpochMs(date), Working = true, Shifts = [.. shifts] };

    private static long EpochMs(DateOnly date) => new DateTimeOffset(date.ToDateTime(TimeOnly.MinValue), TimeSpan.Zero).ToUnixTimeMilliseconds();
}

/// <summary>
/// Calendar - 운영 캘린더
/// </summary>
public class Calendar
{
    public string Id { get; set; } = string.Empty;
    public string Name { get; set; } = string.Empty;
    public List<Shift> Shifts { get; set; } = [];
    public List<BreakTime> Breaks { get; set; } = [];
    public List<long> Holidays { get; set; } = [];

    /// <summary>One-date exceptions that override the weekday rule and the holidays (the first one wins when a date repeats).</summary>
    public List<CalendarException> Exceptions { get; set; } = [];

    /// <summary>
    /// The time zone whose wall clock the shifts, breaks and holidays are read on. <c>null</c> reads them in UTC
    /// (as before this property existed).
    /// </summary>
    public TimeZoneInfo? TimeZone { get; set; }

    // ===== Factory Methods =====

    public static Calendar Create(string id, string name) => new()
    {
        Id = id,
        Name = name
    };

    public static Calendar DefaultDay() => new()
    {
        Id = "CAL-DAY",
        Name = "주간 근무",
        Shifts = [Shift.DayShift()],
        Breaks = [BreakTime.Lunch()]
    };

    // ===== Builder Methods =====

    public Calendar WithShift(Shift shift)
    {
        Shifts.Add(shift);
        return this;
    }

    public Calendar WithBreak(BreakTime breakTime)
    {
        Breaks.Add(breakTime);
        return this;
    }

    /// <summary>Read the calendar on <paramref name="zone"/>'s wall clock.</summary>
    public Calendar WithTimeZone(TimeZoneInfo zone)
    {
        TimeZone = zone;
        return this;
    }

    /// <summary>Read the calendar on the wall clock of the IANA time zone <paramref name="ianaId"/> (for example <c>Asia/Seoul</c>).</summary>
    public Calendar WithTimeZone(string ianaId) => WithTimeZone(TimeZoneInfo.FindSystemTimeZoneById(ianaId));

    public Calendar WithHoliday(DateTime date)
    {
        Holidays.Add(new DateTimeOffset(date.Date, TimeSpan.Zero).ToUnixTimeMilliseconds());
        return this;
    }

    public Calendar WithHolidayMs(long timestampMs)
    {
        Holidays.Add(timestampMs);
        return this;
    }

    /// <summary>Add a one-date exception (overtime day, swapped holiday, closure).</summary>
    public Calendar WithException(CalendarException exception)
    {
        Exceptions.Add(exception);
        return this;
    }

    // ===== Computed =====

    public int WorkingMinutesPerDay()
    {
        int shiftTotal = Shifts.Sum(s => s.DurationMinutes());
        int breakTotal = Breaks.Sum(b => b.DurationMinutes());
        int shiftBreakTotal = Shifts.Sum(s => s.Breaks.Sum(b => b.DurationMinutes()));
        return shiftTotal - breakTotal - shiftBreakTotal;
    }

    /// <summary>Whether the instant falls on a holiday — holidays are dates, compared with the date on the calendar's wall clock.</summary>
    public bool IsHoliday(long timeMs)
    {
        const long dayMs = 24 * 60 * 60 * 1000;
        var instant = DateTimeOffset.FromUnixTimeMilliseconds(timeMs);
        var local = TimeZone is null ? instant : TimeZoneInfo.ConvertTime(instant, TimeZone);
        var day = Math.Floor((local.DateTime - DateTime.UnixEpoch).TotalMilliseconds / dayMs);
        return Holidays.Any(h => Math.Floor((double)h / dayMs) == day);
    }

    // ===== Engine DTO Conversion =====

    /// <summary>The engine's calendar, its time zone laid out from now over a planning year.</summary>
    public CalendarDto ToEngineDto() => ToEngineDto(DateTimeOffset.UtcNow.ToUnixTimeMilliseconds());

    /// <summary>The engine's calendar, its time zone laid out over the planning horizon from <paramref name="startTimeMs"/> (a day before it to 367 days after).</summary>
    public CalendarDto ToEngineDto(long startTimeMs) => new()
    {
        UtcOffsets = TimeZone is null
            ? null
            : UtcOffsetsDto.For(
                TimeZone,
                DateTimeOffset.FromUnixTimeMilliseconds(startTimeMs).AddDays(-1),
                DateTimeOffset.FromUnixTimeMilliseconds(startTimeMs).AddDays(367)),
        Id = Id,
        Name = Name,
        Shifts = Shifts.Select(s => new ShiftDto
        {
            Name = s.Name,
            Start = new TimeDto { Hour = s.Start.Hour, Minute = s.Start.Minute },
            End = new TimeDto { Hour = s.End.Hour, Minute = s.End.Minute },
            Days = s.Days.Select(d => d.ToString()).ToList(),
            Breaks = s.Breaks.Count == 0 ? null : s.Breaks.Select(b => new BreakTimeDto
            {
                Start = new TimeDto { Hour = b.Start.Hour, Minute = b.Start.Minute },
                End = new TimeDto { Hour = b.End.Hour, Minute = b.End.Minute }
            }).ToList()
        }).ToList(),
        Breaks = Breaks.Select(b => new BreakTimeDto
        {
            Start = new TimeDto { Hour = b.Start.Hour, Minute = b.Start.Minute },
            End = new TimeDto { Hour = b.End.Hour, Minute = b.End.Minute }
        }).ToList(),
        Holidays = Holidays,
        Exceptions = Exceptions.Count == 0 ? null : Exceptions.Select(e => new CalendarExceptionDto { Date = e.Date, Working = e.Working, Shifts = [.. e.Shifts] }).ToList()
    };
}

// ===== Engine DTOs =====

public class TimeDto
{
    public int Hour { get; set; }
    public int Minute { get; set; }
}

public class ShiftDto
{
    public string Name { get; set; } = string.Empty;
    public TimeDto Start { get; set; } = new();
    public TimeDto End { get; set; } = new();
    public List<string> Days { get; set; } = [];

    /// <summary>This shift's own breaks — omitted when there are none.</summary>
    public List<BreakTimeDto>? Breaks { get; set; }
}

public class BreakTimeDto
{
    public TimeDto Start { get; set; } = new();
    public TimeDto End { get; set; } = new();
}

public class CalendarExceptionDto
{
    public long Date { get; set; }
    public bool Working { get; set; }
    public List<string> Shifts { get; set; } = [];
}

public class CalendarDto
{
    public string Id { get; set; } = string.Empty;
    public string Name { get; set; } = string.Empty;
    public List<ShiftDto> Shifts { get; set; } = [];
    public List<BreakTimeDto> Breaks { get; set; } = [];
    public List<long> Holidays { get; set; } = [];
    /// <summary>One-date exceptions — omitted when there are none (the engine reads a missing list as none).</summary>
    public List<CalendarExceptionDto>? Exceptions { get; set; }

    /// <summary>The wall clock's time zone — omitted for UTC.</summary>
    public UtcOffsetsDto? UtcOffsets { get; set; }
}
