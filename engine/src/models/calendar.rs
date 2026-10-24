//! Calendar - 운영 시간 및 휴무 관리

use serde::{Deserialize, Serialize};

use super::local_time::{
    local_day, local_day_start, minute_of_day, weekday_monday0, UtcOffsets, DAY_MS,
};

/// 근무 윈도우 종료 이유
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkingWindowEndReason {
    /// 휴식 시작
    BreakStart,
    /// 시프트 종료
    ShiftEnd,
    /// 휴일 시작
    Holiday,
}

/// 요일
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DayOfWeek {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

/// 시간 (HH:MM)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Time {
    pub hour: u8,
    pub minute: u8,
}

impl Time {
    pub fn new(hour: u8, minute: u8) -> Self {
        Self { hour, minute }
    }

    /// 자정부터의 분
    pub fn to_minutes(&self) -> i32 {
        self.hour as i32 * 60 + self.minute as i32
    }

    /// 자정부터의 밀리초
    pub fn to_ms(&self) -> i64 {
        (self.to_minutes() as i64) * 60 * 1000
    }
}

/// 교대 근무
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shift {
    pub name: String,
    pub start: Time,
    pub end: Time,
    pub days: Vec<DayOfWeek>,
    /// 이 교대만의 휴식(주간 점심 · 야간 야식) — 캘린더 전체 휴식(`Calendar::breaks`)은 모든 교대에 그대로 든다.
    /// 자정을 넘는 교대의 자정 뒤 휴식은 그 교대(시작한 날)의 것이다. 비면 없다(옛 JSON 그대로).
    #[serde(default)]
    pub breaks: Vec<BreakTime>,
}

impl Shift {
    pub fn new(name: impl Into<String>, start: Time, end: Time, days: Vec<DayOfWeek>) -> Self {
        Self {
            name: name.into(),
            start,
            end,
            days,
            breaks: Vec::new(),
        }
    }

    /// Builder: 이 교대만의 휴식 추가
    pub fn with_break(mut self, break_time: BreakTime) -> Self {
        self.breaks.push(break_time);
        self
    }

    /// 주간 근무 (월-금 08:00-17:00)
    pub fn day_shift() -> Self {
        Self::new(
            "주간",
            Time::new(8, 0),
            Time::new(17, 0),
            vec![
                DayOfWeek::Monday,
                DayOfWeek::Tuesday,
                DayOfWeek::Wednesday,
                DayOfWeek::Thursday,
                DayOfWeek::Friday,
            ],
        )
    }

    /// 야간 근무 (월-금 17:00-02:00)
    pub fn night_shift() -> Self {
        Self::new(
            "야간",
            Time::new(17, 0),
            Time::new(2, 0), // 다음날 새벽 2시
            vec![
                DayOfWeek::Monday,
                DayOfWeek::Tuesday,
                DayOfWeek::Wednesday,
                DayOfWeek::Thursday,
                DayOfWeek::Friday,
            ],
        )
    }

    /// 시프트 길이 (분)
    pub fn duration_minutes(&self) -> i32 {
        let start = self.start.to_minutes();
        let end = self.end.to_minutes();

        if end > start {
            end - start
        } else {
            // 자정을 넘는 경우
            (24 * 60 - start) + end
        }
    }
}

/// 휴식 시간
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakTime {
    pub start: Time,
    pub end: Time,
}

impl BreakTime {
    pub fn new(start: Time, end: Time) -> Self {
        Self { start, end }
    }

    /// 점심 시간 (12:00-13:00)
    pub fn lunch() -> Self {
        Self::new(Time::new(12, 0), Time::new(13, 0))
    }

    /// 휴식 길이 (분)
    pub fn duration_minutes(&self) -> i32 {
        self.end.to_minutes() - self.start.to_minutes()
    }

    /// 하루 안 분 `minute` 이 이 휴식 안인가 — 끝이 시작보다 이르면 자정을 넘는 휴식(23:50~00:10).
    fn covers(&self, minute: i32) -> bool {
        let (start, end) = (self.start.to_minutes(), self.end.to_minutes());
        if end > start {
            minute >= start && minute < end
        } else {
            minute >= start || minute < end
        }
    }
}

/// 한 날짜의 예외 — 그날만 요일 규칙 · 휴일을 덮는다(특근 · 휴일 대체 · 임시 휴무).
///
/// - `working == false` — 그날은 쉰다(요일 교대가 있어도).
/// - `working == true` — 그날은 일한다(휴일이어도): `shifts` 에 이름이 있으면 그 교대들이, 비면 그 요일의 교대들이 시작한다
///   (평소 쉬는 요일에 일하려면 교대 이름을 적는다 — 비면 그 요일에 교대가 없어 일할 시간이 없다).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarException {
    /// 그 날짜의 epoch ms — 휴일과 같이 날짜만 읽는다(UTC 자정이 관례).
    pub date: i64,
    pub working: bool,
    /// 그날 시작하는 교대의 이름(`Shift::name`) — `working` 일 때만 읽는다.
    #[serde(default)]
    pub shifts: Vec<String>,
}

impl CalendarException {
    /// 그날 쉰다.
    pub fn closed(date_ms: i64) -> Self {
        Self {
            date: date_ms,
            working: false,
            shifts: Vec::new(),
        }
    }

    /// 그날 일한다 — `shifts` 가 비면 그 요일의 교대.
    pub fn working(date_ms: i64, shifts: Vec<String>) -> Self {
        Self {
            date: date_ms,
            working: true,
            shifts,
        }
    }
}

/// Calendar - 운영 캘린더
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Calendar {
    pub id: String,
    pub name: String,
    pub shifts: Vec<Shift>,
    pub breaks: Vec<BreakTime>,
    /// 휴일 — 날짜 목록(그 날짜의 epoch ms — 날짜만 읽는다 · UTC 자정이 관례). 시간대와 무관하게 «그 날짜»다.
    pub holidays: Vec<i64>,
    /// 날짜 예외 — 그날만 요일 규칙과 휴일을 덮는다(같은 날짜가 둘이면 먼저 적힌 것).
    #[serde(default)]
    pub exceptions: Vec<CalendarException>,
    /// 교대 · 휴식 · 휴일을 읽는 벽시계의 시간대 — 비면 UTC(종전과 같다).
    #[serde(default)]
    pub utc_offsets: UtcOffsets,
}

impl Calendar {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            shifts: Vec::new(),
            breaks: Vec::new(),
            holidays: Vec::new(),
            exceptions: Vec::new(),
            utc_offsets: UtcOffsets::utc(),
        }
    }

    /// 기본 주간 캘린더
    pub fn default_day() -> Self {
        Self {
            id: "CAL-DAY".to_string(),
            name: "주간 근무".to_string(),
            shifts: vec![Shift::day_shift()],
            breaks: vec![BreakTime::lunch()],
            holidays: Vec::new(),
            exceptions: Vec::new(),
            utc_offsets: UtcOffsets::utc(),
        }
    }

    /// Builder: Shift 추가
    pub fn with_shift(mut self, shift: Shift) -> Self {
        self.shifts.push(shift);
        self
    }

    /// Builder: 휴식 추가
    pub fn with_break(mut self, break_time: BreakTime) -> Self {
        self.breaks.push(break_time);
        self
    }

    /// Builder: 벽시계의 시간대(UTC 와의 차 일정표)
    pub fn with_utc_offsets(mut self, utc_offsets: UtcOffsets) -> Self {
        self.utc_offsets = utc_offsets;
        self
    }

    /// Builder: 휴일 추가
    pub fn with_holiday(mut self, date_ms: i64) -> Self {
        self.holidays.push(date_ms);
        self
    }

    /// Builder: 날짜 예외 추가(특근 · 휴일 대체 · 임시 휴무)
    pub fn with_exception(mut self, exception: CalendarException) -> Self {
        self.exceptions.push(exception);
        self
    }

    /// 특정 시각(UTC ms)이 휴일인지 — 그 시각의 **현지 날짜**가 휴일 목록에 있는가.
    pub fn is_holiday(&self, time_ms: i64) -> bool {
        self.holiday_on(local_day(self.utc_offsets.to_local(time_ms)))
    }

    /// 하루 작업 가능 시간 (분)
    pub fn working_minutes_per_day(&self) -> i32 {
        let shift_total: i32 = self.shifts.iter().map(|s| s.duration_minutes()).sum();
        let break_total: i32 = self.breaks.iter().map(|b| b.duration_minutes()).sum();
        let shift_break_total: i32 = self
            .shifts
            .iter()
            .flat_map(|s| s.breaks.iter())
            .map(|b| b.duration_minutes())
            .sum();
        shift_total - break_total - shift_break_total
    }

    /// 특정 시각(UTC ms)이 근무 시간인지 — 교대 · 휴식 · 휴일을 캘린더 시간대의 벽시계로 읽는다.
    pub fn is_working_time(&self, time_ms: i64) -> bool {
        self.working_local(self.utc_offsets.to_local(time_ms))
    }

    /// 특정 시각(UTC ms) 이후 다음 근무 시작 시각(UTC ms). 찾지 못하면 그대로 돌려준다.
    pub fn next_working_time(&self, from_ms: i64) -> i64 {
        // 시프트가 없으면 현재 시간 반환 (24/7)
        if self.shifts.is_empty() {
            return from_ms;
        }
        let local = self.utc_offsets.to_local(from_ms);
        match self.next_working_local(local) {
            Some(next) if next != local => self.utc_offsets.to_utc(next).max(from_ms),
            _ => from_ms,
        }
    }

    /// 현재 근무 윈도우의 종료 시각(UTC ms)과 이유 — 다음 비근무(휴식 · 시프트 종료)가 시작하는 때.
    pub fn find_working_window_end(&self, from_ms: i64) -> Option<(i64, WorkingWindowEndReason)> {
        let local = self.utc_offsets.to_local(from_ms);
        self.window_end_local(local)
            .map(|(end, reason)| (self.utc_offsets.to_utc(end).max(from_ms), reason))
    }

    // ── 현지 벽시계(naive local ms) 위의 계산 — 날짜 · 요일 · 하루 안 분은 `local_time` 도우미 하나로.

    fn holiday_on(&self, day: i64) -> bool {
        // 휴일 값은 «날짜»다 — 그 날짜의 epoch ms(UTC 자정이 관례)에서 날짜만 읽는다.
        self.holidays.iter().any(|&h| local_day(h) == day)
    }

    fn day_of_week(day: i64) -> DayOfWeek {
        match weekday_monday0(day) {
            0 => DayOfWeek::Monday,
            1 => DayOfWeek::Tuesday,
            2 => DayOfWeek::Wednesday,
            3 => DayOfWeek::Thursday,
            4 => DayOfWeek::Friday,
            5 => DayOfWeek::Saturday,
            _ => DayOfWeek::Sunday,
        }
    }

    fn exception_on(&self, day: i64) -> Option<&CalendarException> {
        self.exceptions.iter().find(|e| local_day(e.date) == day)
    }

    /// 그 날(현지 날짜)을 쉬는가 — 예외가 있으면 그것(쉰다 · 일한다), 없으면 휴일.
    fn day_off(&self, day: i64) -> bool {
        match self.exception_on(day) {
            Some(e) => !e.working,
            None => self.holiday_on(day),
        }
    }

    /// 그 날(현지 날짜) 그 교대가 시작하는가 — 예외가 있으면 예외가(쉰다 → 아니다 · 일한다 → 적힌 교대, 비면 그 요일의 교대 · 휴일이어도),
    /// 없으면 요일이 맞고 휴일이 아니다.
    fn shift_starts_on(&self, shift: &Shift, day: i64) -> bool {
        match self.exception_on(day) {
            Some(e) if !e.working => false,
            Some(e) if !e.shifts.is_empty() => e.shifts.iter().any(|name| name == &shift.name),
            Some(_) => shift.days.contains(&Self::day_of_week(day)),
            None => shift.days.contains(&Self::day_of_week(day)) && !self.holiday_on(day),
        }
    }

    fn in_break(&self, minute: i32) -> bool {
        self.breaks.iter().any(|b| b.covers(minute))
    }

    /// 그 시각(현지 날짜 `day` · 하루 안 분 `minute`)이 그 교대의 시간 안인가 — 휴식은 보지 않는다.
    /// 자정을 넘는 교대의 자정 뒤 몫은 **전날 시작한** 교대의 것이다.
    fn on_shift(&self, shift: &Shift, day: i64, minute: i32) -> bool {
        let (start, end) = (shift.start.to_minutes(), shift.end.to_minutes());
        if end > start {
            self.shift_starts_on(shift, day) && minute >= start && minute < end
        } else {
            (minute >= start && self.shift_starts_on(shift, day))
                || (minute < end && self.shift_starts_on(shift, day - 1))
        }
    }

    fn working_local(&self, local: i64) -> bool {
        let day = local_day(local);
        if self.shifts.is_empty() {
            // 시프트가 없으면 쉬는 날(예외 · 휴일)이 아닌 날 내내 근무 (24/7)
            return !self.day_off(day);
        }
        let minute = minute_of_day(local);
        if self.in_break(minute) {
            return false;
        }
        // 그 시각에 근무 중인 교대가 하나라도 있고 그 교대가 자기 휴식 중이 아니면 근무다.
        self.shifts.iter().any(|shift| {
            self.on_shift(shift, day, minute) && !shift.breaks.iter().any(|b| b.covers(minute))
        })
    }

    fn next_working_local(&self, from: i64) -> Option<i64> {
        let mut current = from;
        // 긴 연휴(명절 + 대체공휴일 + 주말)를 넘도록 한 달을 본다.
        for _ in 0..32 {
            if self.working_local(current) {
                return Some(current);
            }
            let day = local_day(current);
            let day_start = local_day_start(current);
            let minute = minute_of_day(current);

            // 오늘 근무가 다시 설 수 있는 때 — 아직 시작하지 않은 교대의 시작 · 휴식(캘린더 전체 · 교대마다)의 끝 —
            // 를 이른 순으로 보고 처음으로 근무인 때. 휴식 안에서 시작하는 교대는 그 휴식의 끝이 후보로 잡는다.
            let mut candidates: Vec<i64> = self
                .shifts
                .iter()
                .filter(|s| self.shift_starts_on(s, day))
                .map(|s| s.start.to_minutes())
                .chain(
                    self.breaks
                        .iter()
                        .chain(self.shifts.iter().flat_map(|s| s.breaks.iter()))
                        .map(|b| b.end.to_minutes()),
                )
                .filter(|&m| m > minute)
                .map(|m| day_start + m as i64 * 60_000)
                .collect();
            candidates.sort_unstable();
            if let Some(next) = candidates.into_iter().find(|&c| self.working_local(c)) {
                return Some(next);
            }

            // 다음 날 자정으로
            current = day_start + DAY_MS;
        }
        None
    }

    fn window_end_local(&self, from: i64) -> Option<(i64, WorkingWindowEndReason)> {
        // 시프트가 없으면 24/7 - 윈도우 끝 없음
        if self.shifts.is_empty() || !self.working_local(from) {
            return None;
        }
        let day = local_day(from);
        let day_start = local_day_start(from);
        let minute = minute_of_day(from);

        // 지금 근무 중인 시프트의 끝 — 가장 이른 것
        let shift_end = self
            .shifts
            .iter()
            .filter(|shift| self.on_shift(shift, day, minute))
            .map(|shift| {
                let (start, end) = (shift.start.to_minutes(), shift.end.to_minutes());
                if end > start || minute < end {
                    day_start + end as i64 * 60_000
                } else {
                    day_start + DAY_MS + end as i64 * 60_000
                }
            })
            .min()?;

        // 그 끝 전에 시작하는 휴식(오늘 · 자정을 넘는 시프트면 다음 날 것까지) — 캘린더 전체 휴식은 언제나 멈추고,
        // 지금 근무 중인 교대의 휴식은 그때 다른 교대가 이어 근무하지 않을 때만 멈춘다.
        let days = [day_start, day_start + DAY_MS];
        let calendar_break = days
            .iter()
            .flat_map(|&d| {
                self.breaks
                    .iter()
                    .map(move |b| d + b.start.to_minutes() as i64 * 60_000)
            })
            .filter(|&t| t > from && t < shift_end);
        let shift_break = self
            .shifts
            .iter()
            .filter(|s| self.on_shift(s, day, minute))
            .flat_map(|s| s.breaks.iter())
            .flat_map(|b| {
                days.iter()
                    .map(move |&d| d + b.start.to_minutes() as i64 * 60_000)
            })
            .filter(|&t| t > from && t < shift_end && !self.working_local(t));
        let break_start = calendar_break.chain(shift_break).min();

        Some(match break_start {
            Some(t) => (t, WorkingWindowEndReason::BreakStart),
            None => (shift_end, WorkingWindowEndReason::ShiftEnd),
        })
    }

    /// 연속 근무 가능 시간 계산 (현재 윈도우에서 남은 시간)
    pub fn remaining_working_time_in_window(&self, from_ms: i64) -> i64 {
        match self.find_working_window_end(from_ms) {
            Some((end_ms, _)) => (end_ms - from_ms).max(0),
            None => i64::MAX, // 24/7 또는 비근무 시간
        }
    }

    /// 근무 시간 내에서 작업을 수행할 때 실제 종료 시간 계산
    /// duration_ms만큼 작업이 필요할 때, 근무 시간만 계산하여 종료 시간 반환
    pub fn calculate_end_time(&self, start_ms: i64, duration_ms: i64) -> i64 {
        // 시프트가 없으면 단순 계산 (24/7)
        if self.shifts.is_empty() {
            return start_ms + duration_ms;
        }

        let min_ms = 60 * 1000i64;
        let mut remaining_ms = duration_ms;
        let mut current_ms = self.next_working_time(start_ms);

        // 최대 30일 탐색
        for _ in 0..30 * 24 * 60 {
            if remaining_ms <= 0 {
                break;
            }

            // 현재 근무 시간이면 작업 진행
            if self.is_working_time(current_ms) {
                remaining_ms -= min_ms;
                current_ms += min_ms;
            } else {
                // 비근무 시간이면 다음 근무 시간으로 점프
                current_ms = self.next_working_time(current_ms + min_ms);
            }
        }

        current_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time() {
        let time = Time::new(8, 30);

        assert_eq!(time.to_minutes(), 510); // 8*60 + 30
        assert_eq!(time.to_ms(), 510 * 60 * 1000);
    }

    #[test]
    fn test_shift_duration() {
        let day = Shift::day_shift();
        assert_eq!(day.duration_minutes(), 9 * 60); // 8:00-17:00 = 9시간

        let night = Shift::night_shift();
        assert_eq!(night.duration_minutes(), 9 * 60); // 17:00-02:00 = 9시간
    }

    #[test]
    fn test_break_time() {
        let lunch = BreakTime::lunch();
        assert_eq!(lunch.duration_minutes(), 60);
    }

    #[test]
    fn test_calendar_working_minutes() {
        let cal = Calendar::default_day();

        // 9시간 근무 - 1시간 점심 = 8시간 = 480분
        assert_eq!(cal.working_minutes_per_day(), 480);
    }

    #[test]
    fn test_calendar_holiday() {
        let holiday_ms = 1705708800000i64; // 2024-01-20 00:00:00 UTC

        let cal = Calendar::new("CAL-001", "Test").with_holiday(holiday_ms);

        // 같은 날 다른 시간
        assert!(cal.is_holiday(holiday_ms + 3600000)); // +1시간

        // 다른 날
        let next_day = holiday_ms + 24 * 60 * 60 * 1000;
        assert!(!cal.is_holiday(next_day));
    }

    #[test]
    fn test_calendar_builder() {
        let cal = Calendar::new("CAL-001", "커스텀")
            .with_shift(Shift::day_shift())
            .with_shift(Shift::night_shift())
            .with_break(BreakTime::lunch())
            .with_break(BreakTime::new(Time::new(15, 0), Time::new(15, 15)));

        assert_eq!(cal.shifts.len(), 2);
        assert_eq!(cal.breaks.len(), 2);

        // 18시간 근무 - 75분 휴식 = 1005분
        assert_eq!(cal.working_minutes_per_day(), 1005);
    }

    #[test]
    fn test_is_working_time_day_shift() {
        let cal = Calendar::default_day();

        // 2024-01-22 (월요일) 00:00 UTC = 1705881600000
        let monday_midnight = 1705881600000i64;

        // 2024-01-22 (월요일) 10:00 UTC - 근무 시간
        let monday_10am = monday_midnight + 10 * 60 * 60 * 1000;
        assert!(cal.is_working_time(monday_10am));

        // 2024-01-22 (월요일) 12:30 UTC - 점심시간 (비근무)
        let monday_lunch = monday_midnight + (12 * 60 + 30) * 60 * 1000;
        assert!(!cal.is_working_time(monday_lunch));

        // 2024-01-22 (월요일) 20:00 UTC - 근무 외 시간
        let monday_8pm = monday_midnight + 20 * 60 * 60 * 1000;
        assert!(!cal.is_working_time(monday_8pm));
    }

    #[test]
    fn test_is_working_time_weekend() {
        let cal = Calendar::default_day();

        // 2024-01-20 (토요일) 00:00 UTC = 1705708800000
        let saturday_midnight = 1705708800000i64;

        // 2024-01-20 (토요일) 10:00 - 주말 (비근무)
        let saturday_10am = saturday_midnight + 10 * 60 * 60 * 1000;
        assert!(!cal.is_working_time(saturday_10am));

        // 2024-01-21 (일요일) 10:00 - 주말 (비근무)
        let sunday_10am = saturday_10am + 24 * 60 * 60 * 1000;
        assert!(!cal.is_working_time(sunday_10am));
    }

    #[test]
    fn test_is_working_time_no_shifts() {
        // 시프트 없음 = 24/7 운영
        let cal = Calendar::new("CAL-24-7", "24/7");

        let any_time = 1705914000000i64;
        assert!(cal.is_working_time(any_time));

        // 주말도 근무
        let weekend = 1705741200000i64; // 토요일
        assert!(cal.is_working_time(weekend));
    }

    #[test]
    fn test_next_working_time_during_work() {
        let cal = Calendar::default_day();

        // 2024-01-22 (월요일) 00:00 UTC = 1705881600000
        let monday_midnight = 1705881600000i64;

        // 이미 근무 시간이면 그대로 반환
        let monday_10am = monday_midnight + 10 * 60 * 60 * 1000;
        assert_eq!(cal.next_working_time(monday_10am), monday_10am);
    }

    #[test]
    fn test_next_working_time_before_shift() {
        let cal = Calendar::default_day();

        // 2024-01-22 (월요일) 00:00 UTC = 1705881600000
        let monday_midnight = 1705881600000i64;

        // 월요일 06:00 -> 08:00으로 이동
        let monday_6am = monday_midnight + 6 * 60 * 60 * 1000;
        let monday_8am = monday_midnight + 8 * 60 * 60 * 1000;
        let result = cal.next_working_time(monday_6am);

        // 08:00 시프트 시작으로 이동
        assert_eq!(result, monday_8am);
    }

    #[test]
    fn test_next_working_time_weekend_to_monday() {
        let cal = Calendar::default_day();

        // 2024-01-20 (토요일) 00:00 UTC = 1705708800000
        let saturday_midnight = 1705708800000i64;
        // 2024-01-22 (월요일) 00:00 UTC = 1705881600000
        let monday_midnight = 1705881600000i64;

        // 토요일 10:00 -> 월요일 08:00으로 이동
        let saturday_10am = saturday_midnight + 10 * 60 * 60 * 1000;
        let result = cal.next_working_time(saturday_10am);

        // 결과는 월요일 08:00
        let monday_8am = monday_midnight + 8 * 60 * 60 * 1000;
        assert_eq!(result, monday_8am);
    }

    #[test]
    fn test_calculate_end_time_simple() {
        // 24/7 캘린더
        let cal = Calendar::new("CAL-24-7", "24/7");

        let start = 1000000i64;
        let duration = 60000i64; // 1분

        let end = cal.calculate_end_time(start, duration);
        assert_eq!(end, start + duration);
    }

    #[test]
    fn test_calculate_end_time_with_shifts() {
        let cal = Calendar::default_day();

        // 2024-01-22 (월요일) 00:00 UTC = 1705881600000
        let monday_midnight = 1705881600000i64;

        // 월요일 16:00 시작, 2시간 작업
        // 시프트: 08:00-17:00, 점심: 12:00-13:00
        // 16:00 + 1시간 = 17:00 (시프트 종료)
        // 나머지 1시간은 다음 날 08:00부터 진행 -> 09:00 종료
        let monday_4pm = monday_midnight + 16 * 60 * 60 * 1000;
        let duration = 2 * 60 * 60 * 1000i64; // 2시간

        let end = cal.calculate_end_time(monday_4pm, duration);

        // 화요일 09:00 = 월요일 + 1일 + 9시간
        let tuesday_9am = monday_midnight + 24 * 60 * 60 * 1000 + 9 * 60 * 60 * 1000;
        assert_eq!(end, tuesday_9am);
    }

    #[test]
    fn test_find_working_window_end_at_break() {
        let cal = Calendar::default_day();

        // 2024-01-22 (월요일) 00:00 UTC = 1705881600000
        let monday_midnight = 1705881600000i64;

        // 월요일 10:00 - 점심시간 12:00까지 남음
        let monday_10am = monday_midnight + 10 * 60 * 60 * 1000;
        let result = cal.find_working_window_end(monday_10am);

        assert!(result.is_some());
        let (end_ms, reason) = result.unwrap();
        let monday_noon = monday_midnight + 12 * 60 * 60 * 1000;
        assert_eq!(end_ms, monday_noon);
        assert_eq!(reason, WorkingWindowEndReason::BreakStart);
    }

    #[test]
    fn test_find_working_window_end_at_shift_end() {
        let cal = Calendar::default_day();

        // 2024-01-22 (월요일) 00:00 UTC = 1705881600000
        let monday_midnight = 1705881600000i64;

        // 월요일 14:00 - 시프트 종료 17:00까지 남음
        let monday_2pm = monday_midnight + 14 * 60 * 60 * 1000;
        let result = cal.find_working_window_end(monday_2pm);

        assert!(result.is_some());
        let (end_ms, reason) = result.unwrap();
        let monday_5pm = monday_midnight + 17 * 60 * 60 * 1000;
        assert_eq!(end_ms, monday_5pm);
        assert_eq!(reason, WorkingWindowEndReason::ShiftEnd);
    }

    #[test]
    fn test_find_working_window_end_24_7() {
        // 24/7 캘린더 - 윈도우 끝 없음
        let cal = Calendar::new("CAL-24-7", "24/7");

        let any_time = 1705914000000i64;
        let result = cal.find_working_window_end(any_time);

        assert!(result.is_none());
    }

    #[test]
    fn test_remaining_working_time_in_window() {
        let cal = Calendar::default_day();

        // 2024-01-22 (월요일) 00:00 UTC = 1705881600000
        let monday_midnight = 1705881600000i64;

        // 월요일 11:00 - 점심 12:00까지 1시간 남음
        let monday_11am = monday_midnight + 11 * 60 * 60 * 1000;
        let remaining = cal.remaining_working_time_in_window(monday_11am);
        assert_eq!(remaining, 60 * 60 * 1000); // 1시간

        // 월요일 15:00 - 시프트 종료 17:00까지 2시간 남음
        let monday_3pm = monday_midnight + 15 * 60 * 60 * 1000;
        let remaining = cal.remaining_working_time_in_window(monday_3pm);
        assert_eq!(remaining, 2 * 60 * 60 * 1000); // 2시간
    }

    // ── 시간대 — 교대 · 휴일을 캘린더 시간대의 벽시계로 읽는다(비면 UTC — 위 시험들이 그 경우다).

    const HOUR: i64 = 60 * 60 * 1000;
    /// 2027-02-08 00:00 UTC — 월요일
    const MON_UTC: i64 = 1_802_044_800_000;

    fn seoul_day_shift() -> Calendar {
        Calendar::new("KST", "1공장")
            .with_shift(Shift::day_shift())
            .with_break(BreakTime::lunch())
            .with_utc_offsets(UtcOffsets::fixed(9 * 60))
    }

    #[test]
    fn test_time_zone_reads_shifts_on_the_local_wall_clock() {
        let kst = seoul_day_shift();
        let utc = Calendar::default_day();
        // 월요일 08:00 서울 = 일요일 23:00 UTC
        let monday_8am_seoul = MON_UTC - HOUR;
        assert!(kst.is_working_time(monday_8am_seoul));
        assert!(!utc.is_working_time(monday_8am_seoul)); // UTC 로 읽으면 일요일 밤
                                                         // 월요일 12:30 서울(점심) — 휴식
        assert!(!kst.is_working_time(MON_UTC - 9 * HOUR + 12 * HOUR + 30 * 60_000));
        // 월요일 17:00 서울 = 08:00 UTC — 끝났다
        assert!(!kst.is_working_time(MON_UTC + 8 * HOUR));
    }

    #[test]
    fn test_time_zone_reads_holidays_as_local_dates() {
        // 휴일은 날짜다 — 2027-02-09(그 날짜의 UTC 자정 ms).
        let holiday = MON_UTC + 24 * HOUR;
        let kst = seoul_day_shift().with_holiday(holiday);
        // 화요일 08:30 서울 = 월요일 23:30 UTC — 서울의 날짜로는 휴일
        let tuesday_830_seoul = MON_UTC + 23 * HOUR + 30 * 60_000;
        assert!(kst.is_holiday(tuesday_830_seoul));
        assert!(!kst.is_working_time(tuesday_830_seoul));
        let utc = Calendar::default_day().with_holiday(holiday);
        assert!(!utc.is_holiday(tuesday_830_seoul)); // UTC 로 읽으면 아직 월요일
    }

    #[test]
    fn test_time_zone_returns_absolute_instants() {
        let kst = seoul_day_shift();
        // 일요일 12:00 서울 → 다음 근무 = 월요일 08:00 서울 = 일요일 23:00 UTC
        let sunday_noon_seoul = MON_UTC - 12 * HOUR - 9 * HOUR;
        assert_eq!(kst.next_working_time(sunday_noon_seoul), MON_UTC - HOUR);
        // 월요일 09:00 서울에서 창의 끝 = 12:00 서울(점심) = 03:00 UTC
        let (end, reason) = kst.find_working_window_end(MON_UTC).unwrap();
        assert_eq!(
            (end, reason),
            (MON_UTC + 3 * HOUR, WorkingWindowEndReason::BreakStart)
        );
    }

    #[test]
    fn test_overnight_shift_tail_belongs_to_the_day_it_started() {
        // 금요일에만 시작하는 22:00~06:00 교대
        let cal = Calendar::new("N", "야간").with_shift(Shift::new(
            "금요 야간",
            Time::new(22, 0),
            Time::new(6, 0),
            vec![DayOfWeek::Friday],
        ));
        let friday = MON_UTC + 4 * 24 * HOUR;
        assert!(cal.is_working_time(friday + 23 * HOUR));
        assert!(cal.is_working_time(friday + 24 * HOUR + 3 * HOUR)); // 토요일 03:00 — 금요일 교대의 몫
        assert!(!cal.is_working_time(friday + 3 * HOUR)); // 금요일 03:00 — 목요일 교대는 없다
                                                          // 금요일 23:00 에서 창의 끝 = 토요일 06:00
        assert_eq!(
            cal.find_working_window_end(friday + 23 * HOUR),
            Some((friday + 30 * HOUR, WorkingWindowEndReason::ShiftEnd))
        );
        // 시작한 날이 휴일이면 자정 뒤 몫도 없다
        let off = cal.clone().with_holiday(friday);
        assert!(!off.is_working_time(friday + 27 * HOUR));
    }

    #[test]
    fn test_overnight_window_stops_at_a_break_after_midnight() {
        let cal = Calendar::new("N", "야간")
            .with_shift(Shift::new(
                "야간",
                Time::new(22, 0),
                Time::new(6, 0),
                vec![DayOfWeek::Monday],
            ))
            .with_break(BreakTime::new(Time::new(2, 0), Time::new(2, 30)));
        assert_eq!(
            cal.find_working_window_end(MON_UTC + 23 * HOUR),
            Some((MON_UTC + 26 * HOUR, WorkingWindowEndReason::BreakStart))
        );
        // 휴식 뒤에 다시 이어진다
        assert_eq!(
            cal.next_working_time(MON_UTC + 26 * HOUR),
            MON_UTC + 26 * HOUR + 30 * 60_000
        );
    }

    #[test]
    fn test_next_working_time_crosses_a_long_holiday() {
        // 화요일부터 아흐레 연휴(주말 포함 — 다음 주 수요일까지) 뒤의 첫 근무일까지 찾는다 — 종전 8일 탐색은 찾지 못하고 시작 시각을 그대로 돌려줬다.
        let mut cal = Calendar::default_day();
        for d in 1..10 {
            cal = cal.with_holiday(MON_UTC + d * 24 * HOUR);
        }
        let next = cal.next_working_time(MON_UTC + 18 * HOUR);
        assert_eq!(next, MON_UTC + 10 * 24 * HOUR + 8 * HOUR); // 다음 주 목요일 08:00
    }

    #[test]
    fn test_exception_closes_a_working_day_and_opens_a_holiday() {
        // 월요일 임시 휴무 · 화요일은 휴일인데 일한다(휴일 대체) — 예외가 요일 규칙과 휴일을 덮는다.
        let day = 24 * HOUR;
        let cal = Calendar::default_day()
            .with_holiday(MON_UTC + day)
            .with_exception(CalendarException::closed(MON_UTC))
            .with_exception(CalendarException::working(MON_UTC + day, Vec::new()));
        assert!(!cal.is_working_time(MON_UTC + 10 * HOUR));
        assert!(cal.is_working_time(MON_UTC + day + 10 * HOUR));
        // 월요일 아침에서 다음 근무는 화요일 08:00(월요일은 쉰다)
        assert_eq!(
            cal.next_working_time(MON_UTC + 7 * HOUR),
            MON_UTC + day + 8 * HOUR
        );
    }

    #[test]
    fn test_working_exception_on_a_rest_day_names_its_shifts() {
        // 토요일 특근 — 평소 쉬는 요일은 교대 이름을 적어야 일한다(비면 그 요일에 교대가 없어 일할 시간이 없다).
        let sat = MON_UTC + 5 * 24 * HOUR;
        let named = Calendar::default_day()
            .with_exception(CalendarException::working(sat, vec!["주간".to_string()]));
        assert!(named.is_working_time(sat + 10 * HOUR));
        assert_eq!(
            named.find_working_window_end(sat + 10 * HOUR),
            Some((sat + 12 * HOUR, WorkingWindowEndReason::BreakStart))
        );
        let unnamed =
            Calendar::default_day().with_exception(CalendarException::working(sat, Vec::new()));
        assert!(!unnamed.is_working_time(sat + 10 * HOUR));
    }

    #[test]
    fn test_exception_on_a_round_the_clock_calendar_and_from_json() {
        // 교대 없는(24/7) 캘린더도 쉬는 날 예외를 따른다 · JSON 은 `exceptions` 를 빼도 읽힌다(종전 요청 그대로).
        let cal = Calendar::new("C", "상시").with_exception(CalendarException::closed(MON_UTC));
        assert!(!cal.is_working_time(MON_UTC + HOUR));
        assert!(cal.is_working_time(MON_UTC + 24 * HOUR + HOUR));
        let old: Calendar =
            serde_json::from_str(r#"{"id":"C","name":"n","shifts":[],"breaks":[],"holidays":[]}"#)
                .unwrap();
        assert!(old.exceptions.is_empty());
        let new: Calendar = serde_json::from_str(r#"{"id":"C","name":"n","shifts":[],"breaks":[],"holidays":[],"exceptions":[{"date":0,"working":false}]}"#).unwrap();
        assert!(!new.exceptions[0].working && new.exceptions[0].shifts.is_empty());
    }

    #[test]
    fn test_each_shift_keeps_its_own_breaks() {
        // 주간(08~17 · 점심 12~13) · 야간(22~06 · 야식 02:00~02:30) — 휴식은 그 교대의 것이다.
        let cal = Calendar::new("C", "두 교대")
            .with_shift(Shift::day_shift().with_break(BreakTime::lunch()))
            .with_shift(
                Shift::new(
                    "야간",
                    Time::new(22, 0),
                    Time::new(6, 0),
                    vec![DayOfWeek::Monday],
                )
                .with_break(BreakTime::new(Time::new(2, 0), Time::new(2, 30))),
            );
        assert!(!cal.is_working_time(MON_UTC + 12 * HOUR + 30 * 60_000));
        assert_eq!(
            cal.find_working_window_end(MON_UTC + 9 * HOUR),
            Some((MON_UTC + 12 * HOUR, WorkingWindowEndReason::BreakStart))
        );
        assert_eq!(
            cal.next_working_time(MON_UTC + 12 * HOUR + 10 * 60_000),
            MON_UTC + 13 * HOUR
        );
        // 야간의 자정 뒤 휴식은 월요일에 시작한 교대의 것 — 화요일 02:00 에 멈추고 02:30 에 잇는다.
        assert_eq!(
            cal.find_working_window_end(MON_UTC + 23 * HOUR),
            Some((MON_UTC + 26 * HOUR, WorkingWindowEndReason::BreakStart))
        );
        assert_eq!(
            cal.next_working_time(MON_UTC + 26 * HOUR),
            MON_UTC + 26 * HOUR + 30 * 60_000
        );
        assert_eq!(cal.working_minutes_per_day(), 9 * 60 + 8 * 60 - 60 - 30);
    }

    #[test]
    fn test_a_shift_break_does_not_stop_a_parallel_shift() {
        // A 08~17(휴식 12~13) · B 10~14(휴식 없음) — 12:30 에는 B 가 일한다 · 창은 B 가 끝나는 14:00 까지.
        let week = vec![DayOfWeek::Monday];
        let cal = Calendar::new("C", "겹침")
            .with_shift(
                Shift::new("A", Time::new(8, 0), Time::new(17, 0), week.clone())
                    .with_break(BreakTime::lunch()),
            )
            .with_shift(Shift::new("B", Time::new(10, 0), Time::new(14, 0), week));
        assert!(cal.is_working_time(MON_UTC + 12 * HOUR + 30 * 60_000));
        assert_eq!(
            cal.find_working_window_end(MON_UTC + 11 * HOUR),
            Some((MON_UTC + 14 * HOUR, WorkingWindowEndReason::ShiftEnd))
        );
        // 교대의 휴식 없는 옛 JSON 도 읽힌다
        let old: Shift = serde_json::from_str(r#"{"name":"n","start":{"hour":8,"minute":0},"end":{"hour":17,"minute":0},"days":["Monday"]}"#).unwrap();
        assert!(old.breaks.is_empty());
    }
}
