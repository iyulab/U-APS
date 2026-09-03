namespace UAPS.SDK.Providers;

/// <summary>
/// Holiday data provider interface
/// </summary>
public interface IHolidayProvider
{
    /// <summary>
    /// Get holidays for a specific year and region
    /// </summary>
    /// <param name="year">Target year</param>
    /// <param name="region">Region code (e.g., "KR", "US", "JP")</param>
    /// <returns>List of holiday dates</returns>
    IEnumerable<DateOnly> GetHolidays(int year, string region);

    /// <summary>
    /// Get holidays for date range
    /// </summary>
    IEnumerable<DateOnly> GetHolidays(DateOnly start, DateOnly end, string region);
}

/// <summary>
/// Static holiday provider with predefined holidays
/// </summary>
public class StaticHolidayProvider : IHolidayProvider
{
    private readonly Dictionary<(int Year, string Region), List<DateOnly>> _holidays = [];

    /// <summary>
    /// Add holidays for a specific year and region
    /// </summary>
    public StaticHolidayProvider AddHolidays(int year, string region, IEnumerable<DateOnly> holidays)
    {
        var key = (year, region);
        if (!_holidays.ContainsKey(key))
        {
            _holidays[key] = [];
        }
        _holidays[key].AddRange(holidays);
        return this;
    }

    /// <summary>
    /// Add single holiday
    /// </summary>
    public StaticHolidayProvider AddHoliday(int year, string region, int month, int day)
    {
        return AddHolidays(year, region, [new DateOnly(year, month, day)]);
    }

    public IEnumerable<DateOnly> GetHolidays(int year, string region)
    {
        var key = (year, region);
        return _holidays.GetValueOrDefault(key) ?? [];
    }

    public IEnumerable<DateOnly> GetHolidays(DateOnly start, DateOnly end, string region)
    {
        for (int year = start.Year; year <= end.Year; year++)
        {
            foreach (var holiday in GetHolidays(year, region))
            {
                if (holiday >= start && holiday <= end)
                {
                    yield return holiday;
                }
            }
        }
    }
}

/// <summary>
/// Korean public holidays provider
/// </summary>
public class KoreanHolidayProvider : IHolidayProvider
{
    public IEnumerable<DateOnly> GetHolidays(int year, string region)
    {
        // Fixed holidays
        yield return new DateOnly(year, 1, 1);   // 신정
        yield return new DateOnly(year, 3, 1);   // 삼일절
        yield return new DateOnly(year, 5, 5);   // 어린이날
        yield return new DateOnly(year, 6, 6);   // 현충일
        yield return new DateOnly(year, 8, 15);  // 광복절
        yield return new DateOnly(year, 10, 3);  // 개천절
        yield return new DateOnly(year, 10, 9);  // 한글날
        yield return new DateOnly(year, 12, 25); // 크리스마스

        // Lunar holidays (approximations - real implementation should use lunar calendar)
        // 설날 (음력 1월 1일) - 예시로 고정 날짜 사용
        // 추석 (음력 8월 15일) - 예시로 고정 날짜 사용
        // Note: 실제 구현 시 음력 계산 라이브러리 필요

        // 2025년 예시
        if (year == 2025)
        {
            yield return new DateOnly(2025, 1, 28);  // 설날 연휴
            yield return new DateOnly(2025, 1, 29);  // 설날
            yield return new DateOnly(2025, 1, 30);  // 설날 연휴
            yield return new DateOnly(2025, 10, 5);  // 추석 연휴
            yield return new DateOnly(2025, 10, 6);  // 추석
            yield return new DateOnly(2025, 10, 7);  // 추석 연휴
        }
    }

    public IEnumerable<DateOnly> GetHolidays(DateOnly start, DateOnly end, string region)
    {
        for (int year = start.Year; year <= end.Year; year++)
        {
            foreach (var holiday in GetHolidays(year, region))
            {
                if (holiday >= start && holiday <= end)
                {
                    yield return holiday;
                }
            }
        }
    }
}

/// <summary>
/// Composite holiday provider that combines multiple providers
/// </summary>
public class CompositeHolidayProvider : IHolidayProvider
{
    private readonly List<IHolidayProvider> _providers = [];

    public CompositeHolidayProvider AddProvider(IHolidayProvider provider)
    {
        _providers.Add(provider);
        return this;
    }

    public IEnumerable<DateOnly> GetHolidays(int year, string region)
    {
        return _providers
            .SelectMany(p => p.GetHolidays(year, region))
            .Distinct()
            .OrderBy(d => d);
    }

    public IEnumerable<DateOnly> GetHolidays(DateOnly start, DateOnly end, string region)
    {
        return _providers
            .SelectMany(p => p.GetHolidays(start, end, region))
            .Distinct()
            .OrderBy(d => d);
    }
}
