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
/// Calendar - 운영 캘린더
/// </summary>
public class Calendar
{
    public string Id { get; set; } = string.Empty;
    public string Name { get; set; } = string.Empty;
    public List<Shift> Shifts { get; set; } = [];
    public List<BreakTime> Breaks { get; set; } = [];
    public List<long> Holidays { get; set; } = [];

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

    // ===== Computed =====

    public int WorkingMinutesPerDay()
    {
        int shiftTotal = Shifts.Sum(s => s.DurationMinutes());
        int breakTotal = Breaks.Sum(b => b.DurationMinutes());
        return shiftTotal - breakTotal;
    }

    public bool IsHoliday(long timeMs)
    {
        long dayMs = 24 * 60 * 60 * 1000;
        long dayStart = (timeMs / dayMs) * dayMs;
        return Holidays.Any(h => (h / dayMs) * dayMs == dayStart);
    }

    // ===== Engine DTO Conversion =====

    public CalendarDto ToEngineDto() => new()
    {
        Id = Id,
        Name = Name,
        Shifts = Shifts.Select(s => new ShiftDto
        {
            Name = s.Name,
            Start = new TimeDto { Hour = s.Start.Hour, Minute = s.Start.Minute },
            End = new TimeDto { Hour = s.End.Hour, Minute = s.End.Minute },
            Days = s.Days.Select(d => d.ToString()).ToList()
        }).ToList(),
        Breaks = Breaks.Select(b => new BreakTimeDto
        {
            Start = new TimeDto { Hour = b.Start.Hour, Minute = b.Start.Minute },
            End = new TimeDto { Hour = b.End.Hour, Minute = b.End.Minute }
        }).ToList(),
        Holidays = Holidays
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
}

public class BreakTimeDto
{
    public TimeDto Start { get; set; } = new();
    public TimeDto End { get; set; } = new();
}

public class CalendarDto
{
    public string Id { get; set; } = string.Empty;
    public string Name { get; set; } = string.Empty;
    public List<ShiftDto> Shifts { get; set; } = [];
    public List<BreakTimeDto> Breaks { get; set; } = [];
    public List<long> Holidays { get; set; } = [];
}
