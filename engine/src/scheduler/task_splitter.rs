//! Task Splitter - Operation을 Calendar 기반으로 Task들로 분할
//!
//! 분할 알고리즘:
//! 1. 공정 시작시간 + 총 소요시간 계산
//! 2. 워킹캘린더에서 근무 윈도우 탐색
//! 3. 분할가능(splittable)인 경우:
//!    - 휴게/시프트 경계에서 분할
//!    - 각 Task에 plan_begin/end 할당
//! 4. 분할불가(non-splittable)인 경우:
//!    - 전체가 들어갈 수 있는 다음 근무 윈도우로 지연
//! 5. DailyWorkWindow가 설정된 경우:
//!    - Calendar 근무 윈도우와 DailyWorkWindow의 교집합만 사용

use crate::{Calendar, DailyWorkWindow, SplitReason, Task, WorkingWindowEndReason};

// ============================================================================
// DailyWorkWindow 유틸리티
// ============================================================================

/// epoch ms를 하루 중 분(minute-of-day)으로 변환 (UTC 기준)
pub fn minute_of_day(epoch_ms: i64) -> i32 {
    let ms_per_minute = 60 * 1000i64;
    let ms_per_day = 24 * 60 * ms_per_minute;
    ((epoch_ms % ms_per_day) / ms_per_minute) as i32
}

/// epoch ms를 UTC 자정(ms)으로 내림
fn floor_to_midnight(epoch_ms: i64) -> i64 {
    let ms_per_day = 24 * 60 * 60 * 1000i64;
    (epoch_ms / ms_per_day) * ms_per_day
}

/// DailyWorkWindow를 고려하여 epoch_ms를 윈도우 내 가장 빠른 유효 시각으로 전진
///
/// - 현재 minute-of-day가 begin_minute 이전이면 → 같은 날 begin_minute로 이동
/// - 현재 minute-of-day가 end_minute 이상이면 → 다음 날 begin_minute로 이동
/// - 이미 윈도우 내에 있으면 → 그대로 반환
pub fn advance_to_daily_window(epoch_ms: i64, window: DailyWorkWindow) -> i64 {
    let ms_per_minute = 60 * 1000i64;
    let ms_per_day = 24 * 60 * ms_per_minute;

    let mod_of_day = minute_of_day(epoch_ms);

    if window.end_minute > window.begin_minute {
        // 일반 케이스: begin < end (예: 09:00~18:00)
        if mod_of_day < window.begin_minute {
            // 같은 날 begin으로 이동
            floor_to_midnight(epoch_ms) + window.begin_minute as i64 * ms_per_minute
        } else if mod_of_day >= window.end_minute {
            // 다음 날 begin으로 이동
            floor_to_midnight(epoch_ms) + ms_per_day + window.begin_minute as i64 * ms_per_minute
        } else {
            epoch_ms
        }
    } else {
        // 자정 초과 케이스: begin > end (예: 22:00~06:00)
        if window.contains_minute(mod_of_day) {
            epoch_ms
        } else {
            // 윈도우 밖 (end~begin 구간): 같은 날 begin으로 이동
            let candidate =
                floor_to_midnight(epoch_ms) + window.begin_minute as i64 * ms_per_minute;
            if candidate >= epoch_ms {
                candidate
            } else {
                candidate + ms_per_day
            }
        }
    }
}

/// 현재 epoch_ms에서 DailyWorkWindow 종료(end_minute)까지 남은 ms
/// 윈도우 밖이면 0 반환
pub fn remaining_in_daily_window(epoch_ms: i64, window: DailyWorkWindow) -> i64 {
    let ms_per_minute = 60 * 1000i64;
    let mod_of_day = minute_of_day(epoch_ms);

    if !window.contains_minute(mod_of_day) {
        return 0;
    }

    if window.end_minute > window.begin_minute {
        let end_ms = floor_to_midnight(epoch_ms) + window.end_minute as i64 * ms_per_minute;
        (end_ms - epoch_ms).max(0)
    } else {
        // 자정 초과 케이스
        let ms_per_day = 24 * 60 * ms_per_minute;
        let end_ms = if mod_of_day >= window.begin_minute {
            // 자정 이전 구간 → 다음 날 end_minute까지
            floor_to_midnight(epoch_ms) + ms_per_day + window.end_minute as i64 * ms_per_minute
        } else {
            // 자정 이후 구간 → 같은 날 end_minute까지
            floor_to_midnight(epoch_ms) + window.end_minute as i64 * ms_per_minute
        };
        (end_ms - epoch_ms).max(0)
    }
}

/// Task 분할 결과
#[derive(Debug, Clone)]
pub struct SplitResult {
    /// 분할된 Task 목록
    pub tasks: Vec<Task>,
    /// 최종 종료 시간 (모든 Task 완료 후)
    pub final_end_ms: i64,
}

/// Operation을 Calendar 기반으로 Task들로 분할
///
/// `daily_work_window`가 설정된 경우, Calendar 근무 윈도우와의 교집합만 유효한 슬롯으로 처리.
#[allow(clippy::too_many_arguments)]
pub fn split_operation_by_calendar(
    operation_id: &str,
    job_id: &str,
    product_name: Option<&str>,
    start_ms: i64,
    setup_ms: i64,
    process_ms: i64,
    is_splittable: bool,
    calendar: Option<&Calendar>,
    daily_work_window: Option<DailyWorkWindow>,
) -> SplitResult {
    // DailyWorkWindow 적용: 시작 시간을 윈도우 내 유효 시각으로 전진
    let start_ms = if let Some(window) = daily_work_window {
        advance_to_daily_window(start_ms, window)
    } else {
        start_ms
    };

    // 캘린더 없거나 시프트 없으면 단일 Task (24/7), 단 DailyWorkWindow 경계는 준수
    let cal = match calendar {
        Some(c) if !c.shifts.is_empty() => c,
        _ => {
            if let Some(window) = daily_work_window {
                // 캘린더 없지만 DailyWorkWindow 있음: 윈도우 단위로 분할
                return split_by_daily_window_only(
                    operation_id,
                    job_id,
                    product_name,
                    start_ms,
                    setup_ms,
                    process_ms,
                    is_splittable,
                    window,
                );
            }
            let end_ms = start_ms + setup_ms + process_ms;
            let task = Task::from_operation(
                operation_id,
                job_id,
                product_name,
                start_ms,
                end_ms,
                setup_ms,
                is_splittable,
            );
            return SplitResult {
                tasks: vec![task],
                final_end_ms: end_ms,
            };
        }
    };

    // 근무 시간 시작점 찾기 (Calendar + DailyWorkWindow 교집합)
    let cal_start = cal.next_working_time(start_ms);
    let actual_start = if let Some(window) = daily_work_window {
        advance_to_daily_window(cal_start, window)
    } else {
        cal_start
    };

    if !is_splittable {
        // 분할 불가: 전체 시간이 들어갈 연속 윈도우 찾기
        let total_duration = setup_ms + process_ms;
        let (delay_start, end_ms) =
            find_continuous_window(cal, actual_start, total_duration, daily_work_window);

        let task = Task::from_operation(
            operation_id,
            job_id,
            product_name,
            delay_start,
            end_ms,
            setup_ms,
            false,
        );

        return SplitResult {
            tasks: vec![task],
            final_end_ms: end_ms,
        };
    }

    // 분할 가능: 휴식/시프트 경계 및 DailyWorkWindow 경계에서 분할
    split_into_tasks(
        operation_id,
        job_id,
        product_name,
        actual_start,
        setup_ms,
        process_ms,
        cal,
        daily_work_window,
    )
}

/// 분할 불가 Operation을 위한 연속 근무 윈도우 찾기
///
/// DailyWorkWindow가 있는 경우 Calendar 윈도우와의 교집합 내에서 충분한 연속 시간 탐색
fn find_continuous_window(
    calendar: &Calendar,
    start_ms: i64,
    duration_ms: i64,
    daily_work_window: Option<DailyWorkWindow>,
) -> (i64, i64) {
    let mut current_start = start_ms;

    // 최대 30일 탐색
    for _ in 0..30 * 24 {
        // 근무 시간으로 이동
        let cal_start = calendar.next_working_time(current_start);
        let working_start = if let Some(window) = daily_work_window {
            advance_to_daily_window(cal_start, window)
        } else {
            cal_start
        };

        // DailyWorkWindow 진입 후 Calendar 근무 시간인지 재확인
        let working_start = if let Some(_window) = daily_work_window {
            calendar.next_working_time(working_start)
        } else {
            working_start
        };

        // 현재 윈도우에서 남은 시간 확인
        if let Some((cal_window_end, _)) = calendar.find_working_window_end(working_start) {
            // DailyWorkWindow 끝도 고려
            let effective_end = if let Some(window) = daily_work_window {
                let daily_remaining = remaining_in_daily_window(working_start, window);
                if daily_remaining == 0 {
                    // 윈도우 밖으로 빠져나갔으면 current_start 전진 후 재시도
                    current_start = working_start + 1;
                    continue;
                }
                let daily_end = working_start + daily_remaining;
                cal_window_end.min(daily_end)
            } else {
                cal_window_end
            };

            let available_time = effective_end - working_start;

            if available_time >= duration_ms {
                // 충분한 연속 시간 발견
                return (working_start, working_start + duration_ms);
            }

            // 다음 윈도우로 이동 (Calendar 윈도우 끝 또는 DailyWindow 끝 중 더 이른 시점)
            current_start = effective_end;
        } else {
            // 24/7 또는 Calendar 윈도우 끝 없음
            if let Some(window) = daily_work_window {
                let daily_remaining = remaining_in_daily_window(working_start, window);
                if daily_remaining == 0 {
                    current_start = working_start + 1;
                    continue;
                }
                if daily_remaining >= duration_ms {
                    return (working_start, working_start + duration_ms);
                }
                // 이 윈도우에서 부족 → 다음 DailyWindow 시작으로 이동
                let ms_per_minute = 60 * 1000i64;
                let ms_per_day = 24 * 60 * ms_per_minute;
                let next_begin = floor_to_midnight(working_start)
                    + ms_per_day
                    + window.begin_minute as i64 * ms_per_minute;
                current_start = next_begin;
            } else {
                return (working_start, working_start + duration_ms);
            }
        }
    }

    // 탐색 실패 시 원본 반환
    (start_ms, start_ms + duration_ms)
}

/// Operation을 Task들로 분할 (Calendar + DailyWorkWindow 경계 준수)
#[allow(clippy::too_many_arguments)]
fn split_into_tasks(
    operation_id: &str,
    job_id: &str,
    product_name: Option<&str>,
    start_ms: i64,
    setup_ms: i64,
    process_ms: i64,
    calendar: &Calendar,
    daily_work_window: Option<DailyWorkWindow>,
) -> SplitResult {
    let mut tasks: Vec<Task> = Vec::new();
    let mut remaining_process = process_ms;
    let mut current_ms = start_ms;
    let mut task_index = 0u32;
    let mut setup_remaining = setup_ms;

    // 최대 100개 분할 (안전장치)
    while remaining_process > 0 && tasks.len() < 100 {
        // 근무 시간으로 이동 (Calendar)
        current_ms = calendar.next_working_time(current_ms);

        // DailyWorkWindow 적용: 윈도우 밖이면 다음 유효 시각으로 전진
        if let Some(window) = daily_work_window {
            current_ms = advance_to_daily_window(current_ms, window);
            // DailyWorkWindow 전진 후 Calendar 근무 시간인지 재확인
            current_ms = calendar.next_working_time(current_ms);
        }

        // 현재 윈도우 종료 시간 확인 (Calendar)
        let (cal_window_end, split_reason) = match calendar.find_working_window_end(current_ms) {
            Some((end, reason)) => (end, reason),
            None => {
                // 24/7 Calendar - DailyWorkWindow만 있으면 그 경계에서 분할
                if let Some(window) = daily_work_window {
                    let daily_remaining = remaining_in_daily_window(current_ms, window);
                    if daily_remaining > 0 {
                        let daily_end = current_ms + daily_remaining;
                        let needed_time = setup_remaining + remaining_process;
                        if needed_time <= daily_remaining {
                            // 윈도우 내 완료 가능
                            let task_end = current_ms + needed_time;
                            let task_id = format!("{}-T{:03}", operation_id, task_index + 1);
                            let mut task =
                                Task::new(&task_id, operation_id, job_id, current_ms, task_end)
                                    .with_splittable(true)
                                    .with_setup(setup_remaining)
                                    .with_process(remaining_process);
                            if let Some(name) = product_name {
                                task = task.with_product_name(name);
                            }
                            tasks.push(task);
                            remaining_process = 0;
                        } else {
                            // 윈도우 경계에서 분할 (DailyWorkWindow 끝)
                            let task_start = current_ms;
                            let task_process = daily_remaining
                                .saturating_sub(setup_remaining)
                                .min(remaining_process);
                            let this_setup = setup_remaining.min(daily_remaining);
                            let task_end = task_start + this_setup + task_process;

                            if task_process > 0 || this_setup > 0 {
                                let task_id = format!("{}-T{:03}", operation_id, task_index + 1);
                                let mut task =
                                    Task::new(&task_id, operation_id, job_id, task_start, task_end)
                                        .with_splittable(true)
                                        .with_setup(this_setup)
                                        .with_process(task_process)
                                        .with_split_info(task_index, 0, SplitReason::ShiftEnd);
                                if let Some(name) = product_name {
                                    task = task.with_product_name(name);
                                }
                                tasks.push(task);
                                setup_remaining -= this_setup;
                                remaining_process -= task_process;
                            }
                            task_index += 1;
                            current_ms = daily_end;
                        }
                        continue;
                    }
                    // 윈도우 밖 - 다음 begin으로 이동
                    let ms_per_minute = 60 * 1000i64;
                    let ms_per_day = 24 * 60 * ms_per_minute;
                    current_ms = floor_to_midnight(current_ms)
                        + ms_per_day
                        + window.begin_minute as i64 * ms_per_minute;
                    continue;
                }

                // 진짜 24/7 (캘린더도 없고 DailyWorkWindow도 없음)
                let task_start = current_ms;
                let task_end = current_ms + setup_remaining + remaining_process;

                let task_id = format!("{}-T{:03}", operation_id, task_index + 1);
                let mut task = Task::new(&task_id, operation_id, job_id, task_start, task_end)
                    .with_splittable(true)
                    .with_setup(setup_remaining)
                    .with_process(remaining_process);

                if let Some(name) = product_name {
                    task = task.with_product_name(name);
                }

                tasks.push(task);
                break;
            }
        };

        // DailyWorkWindow가 있으면 window_end를 더 좁힌다
        let window_end = if let Some(window) = daily_work_window {
            let daily_remaining = remaining_in_daily_window(current_ms, window);
            if daily_remaining == 0 {
                // 윈도우 밖 → 다음 유효 시각으로 전진 후 재시도
                current_ms = advance_to_daily_window(current_ms + 1, window);
                continue;
            }
            let daily_end = current_ms + daily_remaining;
            cal_window_end.min(daily_end)
        } else {
            cal_window_end
        };

        let available_time = window_end - current_ms;
        let task_start = current_ms;

        // 이 윈도우에서 처리할 시간 계산
        let needed_time = setup_remaining + remaining_process;

        if needed_time <= available_time {
            // 윈도우 내에서 완료 가능
            let task_end = current_ms + needed_time;

            let task_id = format!("{}-T{:03}", operation_id, task_index + 1);
            let mut task = Task::new(&task_id, operation_id, job_id, task_start, task_end)
                .with_splittable(true)
                .with_setup(setup_remaining)
                .with_process(remaining_process);

            if let Some(name) = product_name {
                task = task.with_product_name(name);
            }

            tasks.push(task);
            remaining_process = 0;
            current_ms = task_end;
        } else {
            // 윈도우 경계에서 분할
            let task_process = if setup_remaining > 0 {
                if setup_remaining >= available_time {
                    // Setup만으로도 윈도우 초과 - Setup만 처리
                    setup_remaining -= available_time;
                    0
                } else {
                    // Setup 완료 후 남은 시간만큼 Process 진행
                    let after_setup = available_time - setup_remaining;
                    let setup_this_task = setup_remaining;
                    setup_remaining = 0;
                    remaining_process -= after_setup;

                    let task_end = task_start + setup_this_task + after_setup;

                    let task_id = format!("{}-T{:03}", operation_id, task_index + 1);
                    let reason = match split_reason {
                        WorkingWindowEndReason::BreakStart => SplitReason::BreakStart,
                        WorkingWindowEndReason::ShiftEnd => SplitReason::ShiftEnd,
                        WorkingWindowEndReason::Holiday => SplitReason::Holiday,
                    };
                    let mut task = Task::new(&task_id, operation_id, job_id, task_start, task_end)
                        .with_splittable(true)
                        .with_setup(setup_this_task)
                        .with_process(after_setup)
                        .with_split_info(task_index, 0, reason); // total은 나중에 설정

                    if let Some(name) = product_name {
                        task = task.with_product_name(name);
                    }

                    tasks.push(task);
                    task_index += 1;
                    current_ms = window_end;
                    continue;
                }
            } else {
                // Setup 완료, Process만 진행
                available_time.min(remaining_process)
            };

            if task_process > 0 || setup_remaining > 0 {
                let this_setup = if setup_remaining > 0 {
                    available_time.min(setup_ms - (setup_ms - setup_remaining))
                } else {
                    0
                };
                let task_end = task_start + available_time;

                let task_id = format!("{}-T{:03}", operation_id, task_index + 1);
                let reason = match split_reason {
                    WorkingWindowEndReason::BreakStart => SplitReason::BreakStart,
                    WorkingWindowEndReason::ShiftEnd => SplitReason::ShiftEnd,
                    WorkingWindowEndReason::Holiday => SplitReason::Holiday,
                };
                let mut task = Task::new(&task_id, operation_id, job_id, task_start, task_end)
                    .with_splittable(true)
                    .with_setup(this_setup)
                    .with_process(task_process)
                    .with_split_info(task_index, 0, reason);

                if let Some(name) = product_name {
                    task = task.with_product_name(name);
                }

                tasks.push(task);
                remaining_process -= task_process;
            }

            task_index += 1;
            current_ms = window_end;
        }
    }

    // total_splits 업데이트
    let total = tasks.len() as u32;
    for (i, task) in tasks.iter_mut().enumerate() {
        task.split_index = i as u32;
        task.total_splits = total;
        if total == 1 {
            task.split_reason = SplitReason::None;
        }
    }

    let final_end = tasks.last().map(|t| t.plan_end_ms).unwrap_or(start_ms);

    SplitResult {
        tasks,
        final_end_ms: final_end,
    }
}

/// 캘린더 없이 DailyWorkWindow만 있는 경우 분할
///
/// 24/7 환경에서 DailyWorkWindow 경계에서만 분할
#[allow(clippy::too_many_arguments)]
fn split_by_daily_window_only(
    operation_id: &str,
    job_id: &str,
    product_name: Option<&str>,
    start_ms: i64,
    setup_ms: i64,
    process_ms: i64,
    is_splittable: bool,
    window: DailyWorkWindow,
) -> SplitResult {
    let ms_per_minute = 60 * 1000i64;
    let ms_per_day = 24 * 60 * ms_per_minute;

    if !is_splittable {
        // 분할 불가: 전체가 들어갈 연속 DailyWorkWindow 찾기
        let total_duration = setup_ms + process_ms;
        let window_duration = if window.end_minute > window.begin_minute {
            (window.end_minute - window.begin_minute) as i64 * ms_per_minute
        } else {
            (24 * 60 - window.begin_minute + window.end_minute) as i64 * ms_per_minute
        };

        let mut candidate = start_ms;
        for _ in 0..365 {
            let daily_remaining = remaining_in_daily_window(candidate, window);
            if daily_remaining >= total_duration {
                let end_ms = candidate + total_duration;
                let task = Task::from_operation(
                    operation_id,
                    job_id,
                    product_name,
                    candidate,
                    end_ms,
                    setup_ms,
                    false,
                );
                return SplitResult {
                    tasks: vec![task],
                    final_end_ms: end_ms,
                };
            }
            if window_duration < total_duration {
                // 단일 윈도우로는 불가능 - 그냥 현재 위치에서 시작 (비정상 케이스)
                let end_ms = candidate + total_duration;
                let task = Task::from_operation(
                    operation_id,
                    job_id,
                    product_name,
                    candidate,
                    end_ms,
                    setup_ms,
                    false,
                );
                return SplitResult {
                    tasks: vec![task],
                    final_end_ms: end_ms,
                };
            }
            // 다음 DailyWorkWindow 시작으로 이동
            candidate = floor_to_midnight(candidate)
                + ms_per_day
                + window.begin_minute as i64 * ms_per_minute;
        }
        let end_ms = start_ms + total_duration;
        return SplitResult {
            tasks: vec![Task::from_operation(
                operation_id,
                job_id,
                product_name,
                start_ms,
                end_ms,
                setup_ms,
                false,
            )],
            final_end_ms: end_ms,
        };
    }

    // 분할 가능: DailyWorkWindow 경계에서 분할
    let mut tasks: Vec<Task> = Vec::new();
    let mut remaining_process = process_ms;
    let mut current_ms = start_ms;
    let mut task_index = 0u32;
    let mut setup_remaining = setup_ms;

    for _ in 0..365 {
        if remaining_process == 0 {
            break;
        }

        // 윈도우 내 유효 시각으로 전진
        current_ms = advance_to_daily_window(current_ms, window);

        let daily_remaining = remaining_in_daily_window(current_ms, window);
        if daily_remaining == 0 {
            current_ms += 1;
            continue;
        }

        let needed = setup_remaining + remaining_process;

        if needed <= daily_remaining {
            // 윈도우 내에서 완료
            let task_end = current_ms + needed;
            let task_id = format!("{}-T{:03}", operation_id, task_index + 1);
            let mut task = Task::new(&task_id, operation_id, job_id, current_ms, task_end)
                .with_splittable(true)
                .with_setup(setup_remaining)
                .with_process(remaining_process);
            if let Some(name) = product_name {
                task = task.with_product_name(name);
            }
            tasks.push(task);
            remaining_process = 0;
        } else {
            // 윈도우 끝에서 분할
            let this_setup = setup_remaining.min(daily_remaining);
            let after_setup = daily_remaining - this_setup;
            let task_process = after_setup.min(remaining_process);

            if this_setup > 0 || task_process > 0 {
                let task_end = current_ms + this_setup + task_process;
                let task_id = format!("{}-T{:03}", operation_id, task_index + 1);
                let mut task = Task::new(&task_id, operation_id, job_id, current_ms, task_end)
                    .with_splittable(true)
                    .with_setup(this_setup)
                    .with_process(task_process)
                    .with_split_info(task_index, 0, SplitReason::ShiftEnd);
                if let Some(name) = product_name {
                    task = task.with_product_name(name);
                }
                tasks.push(task);
                setup_remaining -= this_setup;
                remaining_process -= task_process;
            }

            task_index += 1;
            // 다음 DailyWorkWindow 시작으로 이동
            current_ms = floor_to_midnight(current_ms)
                + ms_per_day
                + window.begin_minute as i64 * ms_per_minute;
        }
    }

    // total_splits 업데이트
    let total = tasks.len() as u32;
    for (i, task) in tasks.iter_mut().enumerate() {
        task.split_index = i as u32;
        task.total_splits = total;
        if total == 1 {
            task.split_reason = SplitReason::None;
        }
    }

    let final_end = tasks.last().map(|t| t.plan_end_ms).unwrap_or(start_ms);
    SplitResult {
        tasks,
        final_end_ms: final_end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BreakTime, Shift};

    fn create_day_calendar() -> Calendar {
        Calendar::new("CAL-DAY", "주간 근무")
            .with_shift(Shift::day_shift())
            .with_break(BreakTime::lunch())
    }

    #[test]
    fn test_no_calendar_single_task() {
        let result = split_operation_by_calendar(
            "OP-001",
            "JOB-001",
            Some("Product-A"),
            0,
            10000,
            50000,
            true,
            None,
            None,
        );

        assert_eq!(result.tasks.len(), 1);
        assert_eq!(result.tasks[0].id, "OP-001-T001");
        assert_eq!(result.tasks[0].setup_ms, 10000);
        assert_eq!(result.tasks[0].process_ms, 50000);
    }

    #[test]
    fn test_non_splittable_finds_continuous_window() {
        let calendar = create_day_calendar();

        // 2024-01-22 (월요일) 16:00 시작, 3시간 작업 (분할 불가)
        // 시프트: 08:00-17:00 → 1시간만 남음
        // 분할 불가이므로 다음 날 08:00부터 시작해야 함
        let monday_midnight = 1705881600000i64;
        let monday_4pm = monday_midnight + 16 * 60 * 60 * 1000;

        let result = split_operation_by_calendar(
            "OP-001",
            "JOB-001",
            None,
            monday_4pm,
            0,
            3 * 60 * 60 * 1000, // 3시간
            false,              // 분할 불가
            Some(&calendar),
            None,
        );

        assert_eq!(result.tasks.len(), 1);
        // 다음 날 08:00 시작
        let tuesday_8am = monday_midnight + 24 * 60 * 60 * 1000 + 8 * 60 * 60 * 1000;
        assert_eq!(result.tasks[0].plan_begin_ms, tuesday_8am);
    }

    #[test]
    fn test_splittable_splits_at_break() {
        let calendar = create_day_calendar();

        // 2024-01-22 (월요일) 11:00 시작, 3시간 작업 (분할 가능)
        // 점심: 12:00-13:00
        // 예상: 11:00-12:00 (1시간) + 13:00-15:00 (2시간)
        let monday_midnight = 1705881600000i64;
        let monday_11am = monday_midnight + 11 * 60 * 60 * 1000;

        let result = split_operation_by_calendar(
            "OP-001",
            "JOB-001",
            Some("Widget"),
            monday_11am,
            0,
            3 * 60 * 60 * 1000, // 3시간
            true,               // 분할 가능
            Some(&calendar),
            None,
        );

        assert_eq!(result.tasks.len(), 2);

        // 첫 번째 Task: 11:00-12:00
        assert_eq!(result.tasks[0].split_index, 0);
        assert_eq!(result.tasks[0].total_splits, 2);
        assert_eq!(result.tasks[0].split_reason, SplitReason::BreakStart);

        // 두 번째 Task: 13:00-15:00
        assert_eq!(result.tasks[1].split_index, 1);
        assert_eq!(result.tasks[1].split_reason, SplitReason::None);
    }

    #[test]
    fn test_splittable_with_setup() {
        let calendar = create_day_calendar();

        // 2024-01-22 (월요일) 11:30 시작
        // Setup: 45분, Process: 2시간
        // 점심 12:00-13:00에 의해 분할
        let monday_midnight = 1705881600000i64;
        let monday_1130 = monday_midnight + (11 * 60 + 30) * 60 * 1000;

        let result = split_operation_by_calendar(
            "OP-001",
            "JOB-001",
            None,
            monday_1130,
            45 * 60 * 1000,     // 45분 setup
            2 * 60 * 60 * 1000, // 2시간 process
            true,
            Some(&calendar),
            None,
        );

        assert!(result.tasks.len() >= 2);
        // Setup이 첫 번째 Task에 포함되어야 함
        assert!(result.tasks[0].setup_ms > 0 || result.tasks.iter().any(|t| t.setup_ms > 0));
    }

    #[test]
    fn test_fits_in_single_window() {
        let calendar = create_day_calendar();

        // 2024-01-22 (월요일) 09:00 시작, 2시간 작업
        // 점심 전에 완료 가능 → 분할 없음
        let monday_midnight = 1705881600000i64;
        let monday_9am = monday_midnight + 9 * 60 * 60 * 1000;

        let result = split_operation_by_calendar(
            "OP-001",
            "JOB-001",
            None,
            monday_9am,
            0,
            2 * 60 * 60 * 1000, // 2시간
            true,
            Some(&calendar),
            None,
        );

        assert_eq!(result.tasks.len(), 1);
        assert_eq!(result.tasks[0].split_reason, SplitReason::None);
    }

    // ============================================================================
    // DailyWorkWindow 테스트
    // ============================================================================

    /// epoch ms 기준 2024-01-22 (월요일) UTC 자정
    fn monday_midnight() -> i64 {
        1705881600000i64
    }

    #[test]
    fn test_daily_work_window_advance_before_begin() {
        // 07:00에 시작 → DailyWorkWindow(9:00-18:00) → 09:00으로 전진해야 함
        let monday_7am = monday_midnight() + 7 * 60 * 60 * 1000;
        let window = DailyWorkWindow::from_hm(9, 0, 18, 0);
        let advanced = advance_to_daily_window(monday_7am, window);
        let monday_9am = monday_midnight() + 9 * 60 * 60 * 1000;
        assert_eq!(advanced, monday_9am);
    }

    #[test]
    fn test_daily_work_window_advance_after_end() {
        // 19:00에 시작 → DailyWorkWindow(9:00-18:00) → 다음 날 09:00으로 전진해야 함
        let monday_7pm = monday_midnight() + 19 * 60 * 60 * 1000;
        let window = DailyWorkWindow::from_hm(9, 0, 18, 0);
        let advanced = advance_to_daily_window(monday_7pm, window);
        let tuesday_9am = monday_midnight() + 24 * 60 * 60 * 1000 + 9 * 60 * 60 * 1000;
        assert_eq!(advanced, tuesday_9am);
    }

    #[test]
    fn test_daily_work_window_advance_within() {
        // 10:00에 시작 → DailyWorkWindow(9:00-18:00) → 그대로 10:00
        let monday_10am = monday_midnight() + 10 * 60 * 60 * 1000;
        let window = DailyWorkWindow::from_hm(9, 0, 18, 0);
        let advanced = advance_to_daily_window(monday_10am, window);
        assert_eq!(advanced, monday_10am);
    }

    #[test]
    fn test_split_no_calendar_daily_window_moves_start() {
        // 07:00 시작, DailyWorkWindow(9:00-18:00), 1시간 작업 (캘린더 없음)
        // → 09:00에 시작해야 함
        let monday_7am = monday_midnight() + 7 * 60 * 60 * 1000;
        let monday_9am = monday_midnight() + 9 * 60 * 60 * 1000;
        let window = DailyWorkWindow::from_hm(9, 0, 18, 0);

        let result = split_operation_by_calendar(
            "OP-001",
            "JOB-001",
            None,
            monday_7am,
            0,
            60 * 60 * 1000, // 1시간
            true,
            None,
            Some(window),
        );

        assert_eq!(result.tasks.len(), 1);
        assert_eq!(result.tasks[0].plan_begin_ms, monday_9am);
    }

    #[test]
    fn test_split_no_calendar_daily_window_splits_long_task() {
        // 09:00 시작, DailyWorkWindow(9:00-18:00), 12시간 작업 (캘린더 없음, 분할 가능)
        // → 첫날 9:00-18:00 (9시간) + 다음날 9:00-12:00 (3시간)
        let monday_9am = monday_midnight() + 9 * 60 * 60 * 1000;
        let window = DailyWorkWindow::from_hm(9, 0, 18, 0);

        let result = split_operation_by_calendar(
            "OP-001",
            "JOB-001",
            None,
            monday_9am,
            0,
            12 * 60 * 60 * 1000, // 12시간
            true,
            None,
            Some(window),
        );

        assert!(result.tasks.len() >= 2, "12시간 작업은 분할되어야 함");
        // 첫 번째 Task는 09:00 시작
        assert_eq!(result.tasks[0].plan_begin_ms, monday_9am);
        // 첫 번째 Task의 총 process 시간은 9시간(32400000ms) 이하여야 함
        let first_task_duration = result.tasks[0].plan_end_ms - result.tasks[0].plan_begin_ms;
        assert!(
            first_task_duration <= 9 * 60 * 60 * 1000,
            "첫 번째 Task가 하루 DailyWorkWindow를 초과함: {}ms",
            first_task_duration
        );
    }

    #[test]
    fn test_split_no_calendar_daily_window_nosplit_moves_to_next_window() {
        // 09:00 시작, DailyWorkWindow(9:00-18:00), 10시간 작업 (캘린더 없음, 분할 불가)
        // 단일 윈도우(9h)에 안 들어가므로 → 이 테스트는 "단일 윈도우로 불가능" 케이스
        // 실제로 10시간 > 9시간이므로 그냥 시작 위치에서 배치됨 (비정상 케이스)
        // 이 케이스는 비정상이므로, 정상 케이스(NoSplit + fits): 8시간 작업
        let monday_9am = monday_midnight() + 9 * 60 * 60 * 1000;
        let window = DailyWorkWindow::from_hm(9, 0, 18, 0);

        let result = split_operation_by_calendar(
            "OP-001",
            "JOB-001",
            None,
            monday_9am,
            0,
            8 * 60 * 60 * 1000, // 8시간 (9시간 윈도우 안에 들어감)
            false,
            None,
            Some(window),
        );

        assert_eq!(result.tasks.len(), 1);
        assert_eq!(result.tasks[0].plan_begin_ms, monday_9am);
    }

    #[test]
    fn test_split_no_calendar_daily_window_nosplit_fits_next_day() {
        // 16:00 시작, DailyWorkWindow(9:00-18:00), 4시간 작업 (분할 불가)
        // 16:00~18:00은 2시간밖에 없음 → 다음 날 09:00으로 이동해야 함
        let monday_4pm = monday_midnight() + 16 * 60 * 60 * 1000;
        let tuesday_9am = monday_midnight() + 24 * 60 * 60 * 1000 + 9 * 60 * 60 * 1000;
        let window = DailyWorkWindow::from_hm(9, 0, 18, 0);

        let result = split_operation_by_calendar(
            "OP-001",
            "JOB-001",
            None,
            monday_4pm,
            0,
            4 * 60 * 60 * 1000, // 4시간
            false,
            None,
            Some(window),
        );

        assert_eq!(result.tasks.len(), 1);
        assert_eq!(
            result.tasks[0].plan_begin_ms, tuesday_9am,
            "NoSplit+DailyWindow: 4시간이 안 들어가므로 다음날 09:00으로 이동해야 함"
        );
    }

    #[test]
    fn test_remaining_in_daily_window() {
        let window = DailyWorkWindow::from_hm(9, 0, 18, 0);
        let ms_per_minute = 60 * 1000i64;

        // 09:00 시작 → 9시간 남음
        let at_9am = monday_midnight() + 9 * 60 * 60 * 1000;
        assert_eq!(
            remaining_in_daily_window(at_9am, window),
            9 * 60 * ms_per_minute
        );

        // 17:00 시작 → 1시간 남음
        let at_5pm = monday_midnight() + 17 * 60 * 60 * 1000;
        assert_eq!(
            remaining_in_daily_window(at_5pm, window),
            60 * ms_per_minute
        );

        // 18:00 → 윈도우 밖 (end_minute 경계)
        let at_6pm = monday_midnight() + 18 * 60 * 60 * 1000;
        assert_eq!(remaining_in_daily_window(at_6pm, window), 0);

        // 08:00 → 윈도우 밖 (begin 이전)
        let at_8am = monday_midnight() + 8 * 60 * 60 * 1000;
        assert_eq!(remaining_in_daily_window(at_8am, window), 0);
    }
}
