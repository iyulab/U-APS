//! Calendar - 운영 시간 및 휴무 관리

use serde::{Deserialize, Serialize};

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
}

impl Shift {
    pub fn new(name: impl Into<String>, start: Time, end: Time, days: Vec<DayOfWeek>) -> Self {
        Self {
            name: name.into(),
            start,
            end,
            days,
        }
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
}

/// Calendar - 운영 캘린더
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Calendar {
    pub id: String,
    pub name: String,
    pub shifts: Vec<Shift>,
    pub breaks: Vec<BreakTime>,
    /// 휴일 (epoch ms 목록)
    pub holidays: Vec<i64>,
}

impl Calendar {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            shifts: Vec::new(),
            breaks: Vec::new(),
            holidays: Vec::new(),
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

    /// Builder: 휴일 추가
    pub fn with_holiday(mut self, date_ms: i64) -> Self {
        self.holidays.push(date_ms);
        self
    }

    /// 특정 시간이 휴일인지 확인
    pub fn is_holiday(&self, time_ms: i64) -> bool {
        // 간단히 날짜만 비교 (00:00 기준)
        let day_ms = 24 * 60 * 60 * 1000;
        let day_start = (time_ms / day_ms) * day_ms;

        self.holidays.iter().any(|&h| {
            let h_start = (h / day_ms) * day_ms;
            h_start == day_start
        })
    }

    /// 하루 작업 가능 시간 (분)
    pub fn working_minutes_per_day(&self) -> i32 {
        let shift_total: i32 = self.shifts.iter().map(|s| s.duration_minutes()).sum();
        let break_total: i32 = self.breaks.iter().map(|b| b.duration_minutes()).sum();
        shift_total - break_total
    }

    /// 특정 시간이 근무 시간인지 확인
    pub fn is_working_time(&self, time_ms: i64) -> bool {
        // 휴일 체크
        if self.is_holiday(time_ms) {
            return false;
        }

        // 시프트가 없으면 항상 근무 가능 (24/7)
        if self.shifts.is_empty() {
            return true;
        }

        // 요일 계산 (1970-01-01 = 목요일 = 3 in Mon=0 system)
        let day_ms = 24 * 60 * 60 * 1000i64;
        let days_since_epoch = time_ms / day_ms;
        let day_of_week = ((days_since_epoch + 3) % 7) as usize; // 0=월요일
        let current_day = match day_of_week {
            0 => DayOfWeek::Monday,
            1 => DayOfWeek::Tuesday,
            2 => DayOfWeek::Wednesday,
            3 => DayOfWeek::Thursday,
            4 => DayOfWeek::Friday,
            5 => DayOfWeek::Saturday,
            _ => DayOfWeek::Sunday,
        };

        // 하루 내 시간 (분)
        let time_in_day_ms = time_ms % day_ms;
        let time_in_day_min = (time_in_day_ms / 60000) as i32;

        // 휴식 시간 체크
        for break_time in &self.breaks {
            let break_start = break_time.start.to_minutes();
            let break_end = break_time.end.to_minutes();
            if time_in_day_min >= break_start && time_in_day_min < break_end {
                return false;
            }
        }

        // 시프트 체크
        for shift in &self.shifts {
            if !shift.days.contains(&current_day) {
                continue;
            }

            let shift_start = shift.start.to_minutes();
            let shift_end = shift.end.to_minutes();

            if shift_end > shift_start {
                // 일반 시프트 (같은 날 시작-종료)
                if time_in_day_min >= shift_start && time_in_day_min < shift_end {
                    return true;
                }
            } else {
                // 자정을 넘는 시프트
                if time_in_day_min >= shift_start || time_in_day_min < shift_end {
                    return true;
                }
            }
        }

        false
    }

    /// 특정 시간 이후 다음 근무 시작 시간 찾기 (ms)
    pub fn next_working_time(&self, from_ms: i64) -> i64 {
        // 시프트가 없으면 현재 시간 반환 (24/7)
        if self.shifts.is_empty() {
            return from_ms;
        }

        let day_ms = 24 * 60 * 60 * 1000i64;
        let mut current_ms = from_ms;

        // 최대 8일 탐색 (1주일 + 여유)
        for _ in 0..8 {
            if self.is_working_time(current_ms) {
                return current_ms;
            }

            // 다음 시프트 시작 시간으로 이동
            let day_start_ms = (current_ms / day_ms) * day_ms;
            let time_in_day_min = ((current_ms % day_ms) / 60000) as i32;

            // 요일 계산 (1970-01-01 = 목요일 = 3 in Mon=0 system)
            let days_since_epoch = current_ms / day_ms;
            let day_of_week = ((days_since_epoch + 3) % 7) as usize;
            let current_day = match day_of_week {
                0 => DayOfWeek::Monday,
                1 => DayOfWeek::Tuesday,
                2 => DayOfWeek::Wednesday,
                3 => DayOfWeek::Thursday,
                4 => DayOfWeek::Friday,
                5 => DayOfWeek::Saturday,
                _ => DayOfWeek::Sunday,
            };

            // 휴일이면 다음 날로
            if self.is_holiday(current_ms) {
                current_ms = day_start_ms + day_ms;
                continue;
            }

            // 휴식 시간 중인지 체크 - 시프트 내 휴식이면 휴식 종료 시간 반환
            for break_time in &self.breaks {
                let break_start = break_time.start.to_minutes();
                let break_end = break_time.end.to_minutes();
                if time_in_day_min >= break_start && time_in_day_min < break_end {
                    // 현재 활성화된 시프트가 있는지 확인
                    for shift in &self.shifts {
                        if !shift.days.contains(&current_day) {
                            continue;
                        }
                        let shift_start = shift.start.to_minutes();
                        let shift_end = shift.end.to_minutes();

                        // 휴식 시간이 시프트 범위 내에 있으면 휴식 종료 시간 반환
                        let in_shift = if shift_end > shift_start {
                            break_start >= shift_start && break_end <= shift_end
                        } else {
                            // 자정을 넘는 시프트
                            break_start >= shift_start || break_end <= shift_end
                        };

                        if in_shift {
                            let resume_ms = day_start_ms + (break_end as i64) * 60000;
                            if self.is_working_time(resume_ms) {
                                return resume_ms;
                            }
                        }
                    }
                }
            }

            // 오늘 시프트 중 아직 시작하지 않은 것 찾기
            let mut next_start_today: Option<i32> = None;
            for shift in &self.shifts {
                if !shift.days.contains(&current_day) {
                    continue;
                }
                let shift_start = shift.start.to_minutes();
                if shift_start > time_in_day_min {
                    match next_start_today {
                        None => next_start_today = Some(shift_start),
                        Some(prev) if shift_start < prev => next_start_today = Some(shift_start),
                        _ => {}
                    }
                }
            }

            if let Some(start_min) = next_start_today {
                // 오늘 시프트 시작까지 이동
                current_ms = day_start_ms + (start_min as i64) * 60000;

                // 휴식 시간이면 휴식 후로 이동
                for break_time in &self.breaks {
                    let break_start = break_time.start.to_minutes();
                    let break_end = break_time.end.to_minutes();
                    let current_min = ((current_ms % day_ms) / 60000) as i32;
                    if current_min >= break_start && current_min < break_end {
                        current_ms = day_start_ms + (break_end as i64) * 60000;
                    }
                }

                if self.is_working_time(current_ms) {
                    return current_ms;
                }
            }

            // 다음 날로 이동
            current_ms = day_start_ms + day_ms;
        }

        // 탐색 실패 시 원본 반환
        from_ms
    }

    /// 현재 근무 윈도우의 종료 시간 찾기
    /// 다음 비근무 시간(휴식/시프트종료)까지 남은 시간을 반환
    /// Returns: (window_end_ms, reason) - 비근무 시작 시간과 이유
    pub fn find_working_window_end(&self, from_ms: i64) -> Option<(i64, WorkingWindowEndReason)> {
        // 시프트가 없으면 24/7 - 윈도우 끝 없음
        if self.shifts.is_empty() {
            return None;
        }

        // 현재 시간이 근무 시간이 아니면 None
        if !self.is_working_time(from_ms) {
            return None;
        }

        let day_ms = 24 * 60 * 60 * 1000i64;
        let day_start_ms = (from_ms / day_ms) * day_ms;
        let time_in_day_min = ((from_ms % day_ms) / 60000) as i32;

        // 요일 계산
        let days_since_epoch = from_ms / day_ms;
        let day_of_week = ((days_since_epoch + 3) % 7) as usize;
        let current_day = match day_of_week {
            0 => DayOfWeek::Monday,
            1 => DayOfWeek::Tuesday,
            2 => DayOfWeek::Wednesday,
            3 => DayOfWeek::Thursday,
            4 => DayOfWeek::Friday,
            5 => DayOfWeek::Saturday,
            _ => DayOfWeek::Sunday,
        };

        let mut earliest_end: Option<(i64, WorkingWindowEndReason)> = None;

        // 1. 휴식 시간 체크 - 가장 빠른 휴식 시작 시간 찾기
        for break_time in &self.breaks {
            let break_start_min = break_time.start.to_minutes();
            if break_start_min > time_in_day_min {
                let break_start_ms = day_start_ms + (break_start_min as i64) * 60000;
                match earliest_end {
                    None => {
                        earliest_end = Some((break_start_ms, WorkingWindowEndReason::BreakStart))
                    }
                    Some((current, _)) if break_start_ms < current => {
                        earliest_end = Some((break_start_ms, WorkingWindowEndReason::BreakStart));
                    }
                    _ => {}
                }
            }
        }

        // 2. 시프트 종료 시간 체크
        for shift in &self.shifts {
            if !shift.days.contains(&current_day) {
                continue;
            }

            let shift_start_min = shift.start.to_minutes();
            let shift_end_min = shift.end.to_minutes();

            let shift_end_ms = if shift_end_min > shift_start_min {
                // 일반 시프트
                if time_in_day_min >= shift_start_min && time_in_day_min < shift_end_min {
                    day_start_ms + (shift_end_min as i64) * 60000
                } else {
                    continue;
                }
            } else {
                // 자정을 넘는 시프트
                if time_in_day_min >= shift_start_min {
                    // 오늘 시작, 내일 종료
                    day_start_ms + day_ms + (shift_end_min as i64) * 60000
                } else if time_in_day_min < shift_end_min {
                    // 어제 시작, 오늘 종료
                    day_start_ms + (shift_end_min as i64) * 60000
                } else {
                    continue;
                }
            };

            match earliest_end {
                None => earliest_end = Some((shift_end_ms, WorkingWindowEndReason::ShiftEnd)),
                Some((current, _)) if shift_end_ms < current => {
                    earliest_end = Some((shift_end_ms, WorkingWindowEndReason::ShiftEnd));
                }
                _ => {}
            }
        }

        earliest_end
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
}
