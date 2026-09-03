//! Integration Tests - 전체 시나리오 테스트

use chrono::{TimeZone, Utc};
use uaps_engine::*;

/// 시나리오: 단일 제품 생산 스케줄링
#[test]
fn test_single_product_scheduling() {
    // Given: 알루미늄 프레임 생산 오더
    let job = Job::new("PO-2501-001")
        .with_priority(1)
        .with_quantity(500)
        .with_due_date(Utc.timestamp_millis_opt(7200000).unwrap()) // 2시간 후
        .with_operation(
            Operation::new("OP-010", "PO-2501-001", 10)
                .with_time(
                    10 * 60 * 1000, // setup 10분
                    15 * 60 * 1000, // process 15분
                    0,              // wait 0
                )
                .with_equipment(vec!["EQP-CUT-001".to_string()]),
        )
        .with_operation(
            Operation::new("OP-020", "PO-2501-001", 20)
                .with_time(
                    15 * 60 * 1000, // setup 15분
                    45 * 60 * 1000, // process 45분
                    5 * 60 * 1000,  // wait 5분
                )
                .with_equipment(vec!["EQP-CNC-001".to_string()]),
        );

    let resources = vec![
        Resource::equipment("EQP-CUT-001").with_capability("cutting"),
        Resource::equipment("EQP-CNC-001").with_capability("milling"),
    ];

    let request = ScheduleRequest::new(vec![job], resources);
    let scheduler = SimpleScheduler::new();

    // When
    let schedule = scheduler.schedule(&request);

    // Then
    assert_eq!(schedule.assignments.len(), 2);

    // 총 시간: 25분 + 65분 = 90분 = 5,400,000ms
    assert_eq!(schedule.makespan_ms, 90 * 60 * 1000);

    // 납기 준수 확인 (90분 < 120분)
    assert!(schedule.is_on_time());

    // 공정 순서 확인
    let op1 = schedule.assignment_for_operation("OP-010").unwrap();
    let op2 = schedule.assignment_for_operation("OP-020").unwrap();
    assert!(op1.end_ms <= op2.start_ms);
}

/// 시나리오: 다중 오더 우선순위 스케줄링
#[test]
fn test_multi_order_priority_scheduling() {
    // Given: 3개 오더 (긴급, 일반, 낮음)
    let urgent = Job::new("URGENT-001").with_priority(1).with_operation(
        Operation::new("OP-U1", "URGENT-001", 1)
            .with_time(0, 30 * 60 * 1000, 0) // 30분
            .with_equipment(vec!["EQP-001".to_string()]),
    );

    let normal = Job::new("NORMAL-001").with_priority(5).with_operation(
        Operation::new("OP-N1", "NORMAL-001", 1)
            .with_time(0, 20 * 60 * 1000, 0) // 20분
            .with_equipment(vec!["EQP-001".to_string()]),
    );

    let low = Job::new("LOW-001").with_priority(10).with_operation(
        Operation::new("OP-L1", "LOW-001", 1)
            .with_time(0, 15 * 60 * 1000, 0) // 15분
            .with_equipment(vec!["EQP-001".to_string()]),
    );

    let resource = Resource::equipment("EQP-001");

    // 의도적으로 순서 섞기
    let request = ScheduleRequest::new(vec![low, normal, urgent], vec![resource]);
    let scheduler = SimpleScheduler::new();

    // When
    let schedule = scheduler.schedule(&request);

    // Then: 우선순위대로 스케줄링
    let op_u = schedule.assignment_for_operation("OP-U1").unwrap();
    let op_n = schedule.assignment_for_operation("OP-N1").unwrap();
    let op_l = schedule.assignment_for_operation("OP-L1").unwrap();

    assert_eq!(op_u.start_ms, 0);
    assert_eq!(op_n.start_ms, 30 * 60 * 1000);
    assert_eq!(op_l.start_ms, 50 * 60 * 1000);
}

/// 시나리오: 병렬 자원 활용
#[test]
fn test_parallel_resource_utilization() {
    // Given: 2개 Job, 2개 장비
    let job1 = Job::new("JOB-001").with_priority(1).with_operation(
        Operation::new("OP-001", "JOB-001", 1)
            .with_time(0, 60 * 60 * 1000, 0) // 60분
            .with_equipment(vec!["EQP-001".to_string(), "EQP-002".to_string()]),
    );

    let job2 = Job::new("JOB-002").with_priority(1).with_operation(
        Operation::new("OP-002", "JOB-002", 1)
            .with_time(0, 60 * 60 * 1000, 0) // 60분
            .with_equipment(vec!["EQP-001".to_string(), "EQP-002".to_string()]),
    );

    let resources = vec![
        Resource::equipment("EQP-001"),
        Resource::equipment("EQP-002"),
    ];

    let request = ScheduleRequest::new(vec![job1, job2], resources);
    let scheduler = SimpleScheduler::new();

    // When
    let schedule = scheduler.schedule(&request);

    // Then: 병렬 실행으로 60분 완료 (순차면 120분)
    assert_eq!(schedule.makespan_ms, 60 * 60 * 1000);

    let op1 = schedule.assignment_for_operation("OP-001").unwrap();
    let op2 = schedule.assignment_for_operation("OP-002").unwrap();

    // 서로 다른 장비에 할당
    assert_ne!(op1.resource_id, op2.resource_id);
    // 동시 시작
    assert_eq!(op1.start_ms, op2.start_ms);
}

/// 시나리오: 납기 위반 감지
#[test]
fn test_due_date_violation_detection() {
    // Given: 납기가 촉박한 오더
    let tight_deadline = Job::new("JOB-TIGHT")
        .with_due_date(Utc.timestamp_millis_opt(30 * 60 * 1000).unwrap()) // 30분
        .with_operation(
            Operation::new("OP-001", "JOB-TIGHT", 1)
                .with_time(0, 45 * 60 * 1000, 0) // 45분 소요
                .with_equipment(vec!["EQP-001".to_string()]),
        );

    let resource = Resource::equipment("EQP-001");

    let request = ScheduleRequest::new(vec![tight_deadline], vec![resource]);
    let scheduler = SimpleScheduler::new();

    // When
    let schedule = scheduler.schedule(&request);

    // Then: 15분 지연
    assert!(!schedule.is_on_time());
    assert_eq!(schedule.violations.len(), 1);

    let violation = &schedule.violations[0];
    assert_eq!(violation.target_id, "JOB-TIGHT");
    assert_eq!(violation.amount, 15.0 * 60.0 * 1000.0);
}

/// 시나리오: 연속 공정 스케줄링
#[test]
fn test_sequential_operations() {
    // Given: 5개 연속 공정
    let job = Job::new("JOB-SEQ")
        .with_operation(
            Operation::new("OP-010", "JOB-SEQ", 10)
                .with_time(5000, 10000, 0)
                .with_equipment(vec!["EQP-001".to_string()]),
        )
        .with_operation(
            Operation::new("OP-020", "JOB-SEQ", 20)
                .with_time(0, 15000, 0)
                .with_equipment(vec!["EQP-002".to_string()]),
        )
        .with_operation(
            Operation::new("OP-030", "JOB-SEQ", 30)
                .with_time(5000, 20000, 5000)
                .with_equipment(vec!["EQP-001".to_string()]),
        )
        .with_operation(
            Operation::new("OP-040", "JOB-SEQ", 40)
                .with_time(0, 10000, 0)
                .with_equipment(vec!["EQP-002".to_string()]),
        )
        .with_operation(
            Operation::new("OP-050", "JOB-SEQ", 50)
                .with_time(0, 5000, 0)
                .with_equipment(vec!["EQP-001".to_string()]),
        );

    let resources = vec![
        Resource::equipment("EQP-001"),
        Resource::equipment("EQP-002"),
    ];

    let request = ScheduleRequest::new(vec![job], resources);
    let scheduler = SimpleScheduler::new();

    // When
    let schedule = scheduler.schedule(&request);

    // Then
    assert_eq!(schedule.assignments.len(), 5);

    // 공정 순서 확인
    for i in 0..4 {
        let seq = (i + 1) * 10;
        let next_seq = (i + 2) * 10;
        let op_id = format!("OP-{:03}", seq);
        let next_op_id = format!("OP-{:03}", next_seq);

        let current = schedule.assignment_for_operation(&op_id).unwrap();
        let next = schedule.assignment_for_operation(&next_op_id).unwrap();

        assert!(
            current.end_ms <= next.start_ms,
            "Operation {} should finish before {} starts",
            op_id,
            next_op_id
        );
    }
}

/// 시나리오: 자원 효율성 테스트
#[test]
fn test_resource_efficiency() {
    let skilled = Resource::worker("SKILLED").with_efficiency(1.5); // 50% 빠름

    let novice = Resource::worker("NOVICE").with_efficiency(0.8); // 20% 느림

    let base_time = 60 * 60 * 1000; // 1시간

    // 숙련공: 40분
    assert_eq!(skilled.adjusted_time_ms(base_time), 40 * 60 * 1000);

    // 신입: 75분
    assert_eq!(novice.adjusted_time_ms(base_time), 75 * 60 * 1000);
}

/// 시나리오: 캘린더 작업 시간 계산
#[test]
fn test_calendar_working_hours() {
    // 기본 주간 캘린더
    let cal = Calendar::default_day();

    // 9시간 근무 - 1시간 점심 = 8시간 = 480분
    assert_eq!(cal.working_minutes_per_day(), 480);

    // 24시간 캘린더 (교대 + 휴식)
    let cal_24h = Calendar::new("CAL-24H", "24시간")
        .with_shift(Shift::day_shift())
        .with_shift(Shift::night_shift())
        .with_break(BreakTime::lunch())
        .with_break(BreakTime::new(Time::new(0, 0), Time::new(0, 30))); // 야간 휴식

    // 18시간 - 90분 = 990분
    assert_eq!(cal_24h.working_minutes_per_day(), 990);
}
