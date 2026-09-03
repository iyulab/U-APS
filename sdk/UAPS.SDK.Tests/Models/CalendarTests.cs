using FluentAssertions;
using UAPS.SDK.Models;
using Xunit;

namespace UAPS.SDK.Tests.Models;

public class CalendarTests
{
    [Fact]
    public void TimeOfDay_ShouldCalculateMinutes()
    {
        // Arrange
        var time = new TimeOfDay(8, 30);

        // Act & Assert
        time.ToMinutes().Should().Be(510); // 8*60 + 30
        time.ToMs().Should().Be(510 * 60 * 1000);
    }

    [Fact]
    public void Shift_DayShift_ShouldHaveCorrectDuration()
    {
        // Arrange & Act
        var shift = Shift.DayShift();

        // Assert
        shift.Name.Should().Be("주간");
        shift.DurationMinutes().Should().Be(9 * 60); // 8:00-17:00 = 9시간
        shift.Days.Should().HaveCount(5);
    }

    [Fact]
    public void Shift_NightShift_ShouldHandleMidnightCrossing()
    {
        // Arrange & Act
        var shift = Shift.NightShift();

        // Assert
        shift.DurationMinutes().Should().Be(9 * 60); // 17:00-02:00 = 9시간
    }

    [Fact]
    public void BreakTime_Lunch_ShouldBe60Minutes()
    {
        // Arrange & Act
        var lunch = BreakTime.Lunch();

        // Assert
        lunch.DurationMinutes().Should().Be(60);
    }

    [Fact]
    public void Calendar_DefaultDay_ShouldHave8WorkingHours()
    {
        // Arrange & Act
        var cal = Calendar.DefaultDay();

        // Assert
        cal.Id.Should().Be("CAL-DAY");
        cal.WorkingMinutesPerDay().Should().Be(480); // 9시간 - 1시간 점심 = 8시간
    }

    [Fact]
    public void Calendar_ShouldBuildWithFluent()
    {
        // Arrange & Act
        var cal = Calendar.Create("CAL-001", "커스텀 캘린더")
            .WithShift(Shift.DayShift())
            .WithShift(Shift.NightShift())
            .WithBreak(BreakTime.Lunch())
            .WithHolidayMs(1705708800000); // 2024-01-20

        // Assert
        cal.Shifts.Should().HaveCount(2);
        cal.Breaks.Should().HaveCount(1);
        cal.Holidays.Should().HaveCount(1);
    }

    [Fact]
    public void Calendar_IsHoliday_ShouldDetectHoliday()
    {
        // Arrange
        var holidayMs = 1705708800000L; // 2024-01-20 00:00:00 UTC
        var cal = Calendar.Create("CAL-001", "Test")
            .WithHolidayMs(holidayMs);

        // Act & Assert
        cal.IsHoliday(holidayMs + 3600000).Should().BeTrue(); // 같은 날 +1시간
        cal.IsHoliday(holidayMs + 24 * 60 * 60 * 1000).Should().BeFalse(); // 다음 날
    }

    [Fact]
    public void Calendar_WithHoliday_ShouldConvertDateTimeToMs()
    {
        // Arrange
        var holiday = new DateTime(2024, 1, 20, 0, 0, 0, DateTimeKind.Utc);

        // Act
        var cal = Calendar.Create("CAL-001", "Test")
            .WithHoliday(holiday);

        // Assert
        cal.Holidays.Should().HaveCount(1);
        cal.IsHoliday(new DateTimeOffset(holiday).ToUnixTimeMilliseconds()).Should().BeTrue();
    }

    [Fact]
    public void Calendar_ShouldConvertToEngineDto()
    {
        // Arrange
        var cal = Calendar.Create("CAL-001", "Test")
            .WithShift(Shift.DayShift())
            .WithBreak(BreakTime.Lunch())
            .WithHolidayMs(1705708800000);

        // Act
        var dto = cal.ToEngineDto();

        // Assert
        dto.Id.Should().Be("CAL-001");
        dto.Name.Should().Be("Test");
        dto.Shifts.Should().HaveCount(1);
        dto.Shifts[0].Start.Hour.Should().Be(8);
        dto.Shifts[0].End.Hour.Should().Be(17);
        dto.Breaks.Should().HaveCount(1);
        dto.Holidays.Should().Contain(1705708800000);
    }
}
