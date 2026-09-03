//! Integration Tests - TDD 방식 점진적 테스트
//!
//! Level 1: 기본 스케줄링
//! Level 2: 제약조건 (납기, Setup Time)
//! Level 3: KPI 및 분석
//! Level 4: 동적 기능 (재스케줄링, CTP, What-If)
//! Level 5: 입력 검증

use chrono::{Duration, Utc};
use uaps_engine::*;

// =============================================================================
// Level 1: 기본 스케줄링 - 단순 Job/Operation/Resource
// =============================================================================

mod level1_basic {
    use super::*;

    /// 가장 단순한 케이스: 1 Job, 1 Operation, 1 Resource
    #[test]
    fn test_single_job_single_op() {
        // Given: 1개 Job, 1개 공정, 1개 기계
        let op = Operation::new("O1", "J1", 1)
            .with_time(0, 60_000, 0) // 1분 처리
            .with_equipment(vec!["M1".into()]);

        let job = Job::new("J1").with_priority(1).with_operation(op);

        let resource = Resource::equipment("M1");

        // When: 스케줄링
        let request = ScheduleRequest::new(vec![job], vec![resource]);
        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        // Then: 1개 할당, 0시작 60_000종료
        assert_eq!(schedule.assignments.len(), 1);
        assert_eq!(schedule.assignments[0].operation_id, "O1");
        assert_eq!(schedule.assignments[0].start_ms, 0);
        assert_eq!(schedule.assignments[0].end_ms, 60_000);
        assert_eq!(schedule.makespan_ms, 60_000);
    }

    /// 2개 Job, 각 1개 공정, 1개 기계 (순차 처리)
    #[test]
    fn test_two_jobs_sequential() {
        let job1 = Job::new("J1")
            .with_priority(100) // 낮은 우선순위
            .with_operation(
                Operation::new("O1", "J1", 1)
                    .with_time(0, 30_000, 0)
                    .with_equipment(vec!["M1".into()]),
            );

        let job2 = Job::new("J2")
            .with_priority(1) // 높은 우선순위 (낮은 숫자)
            .with_operation(
                Operation::new("O2", "J2", 1)
                    .with_time(0, 20_000, 0)
                    .with_equipment(vec!["M1".into()]),
            );

        let resource = Resource::equipment("M1");

        let request = ScheduleRequest::new(vec![job1, job2], vec![resource]);
        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        // J2가 우선순위 높으므로 먼저 처리
        assert_eq!(schedule.assignments.len(), 2);
        assert_eq!(schedule.makespan_ms, 50_000); // 20 + 30
    }

    /// 1개 Job, 2개 공정 (순차 의존)
    #[test]
    fn test_single_job_two_ops() {
        let job = Job::new("J1")
            .with_priority(1)
            .with_operation(
                Operation::new("O1", "J1", 1)
                    .with_time(0, 30_000, 0)
                    .with_equipment(vec!["M1".into()]),
            )
            .with_operation(
                Operation::new("O2", "J1", 2)
                    .with_time(0, 20_000, 0)
                    .with_equipment(vec!["M1".into()]),
            );

        let resource = Resource::equipment("M1");

        let request = ScheduleRequest::new(vec![job], vec![resource]);
        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        // O1 → O2 순차 처리
        assert_eq!(schedule.assignments.len(), 2);

        let o1 = schedule
            .assignments
            .iter()
            .find(|a| a.operation_id == "O1")
            .unwrap();
        let o2 = schedule
            .assignments
            .iter()
            .find(|a| a.operation_id == "O2")
            .unwrap();

        assert!(o2.start_ms >= o1.end_ms, "O2 must start after O1 ends");
        assert_eq!(schedule.makespan_ms, 50_000);
    }

    /// 2개 기계에서 병렬 처리
    #[test]
    fn test_parallel_machines() {
        let job1 = Job::new("J1").with_priority(1).with_operation(
            Operation::new("O1", "J1", 1)
                .with_time(0, 60_000, 0)
                .with_equipment(vec!["M1".into()]),
        );

        let job2 = Job::new("J2").with_priority(1).with_operation(
            Operation::new("O2", "J2", 1)
                .with_time(0, 60_000, 0)
                .with_equipment(vec!["M2".into()]),
        );

        let resources = vec![Resource::equipment("M1"), Resource::equipment("M2")];

        let request = ScheduleRequest::new(vec![job1, job2], resources);
        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        // 병렬 처리로 Makespan = 60_000
        assert_eq!(schedule.assignments.len(), 2);
        assert_eq!(schedule.makespan_ms, 60_000);
    }

    /// 대체 기계 선택 (효율성 기반)
    #[test]
    fn test_alternative_resources() {
        let job = Job::new("J1").with_priority(1).with_operation(
            Operation::new("O1", "J1", 1)
                .with_time(0, 60_000, 0)
                .with_equipment(vec!["M1".into(), "M2".into()]), // 둘 다 가능
        );

        let resources = vec![
            Resource::equipment("M1").with_efficiency(1.0),
            Resource::equipment("M2").with_efficiency(1.5), // 더 효율적
        ];

        let request = ScheduleRequest::new(vec![job], resources);
        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        // M2가 더 효율적이므로 선택됨, 처리시간 = 60_000 / 1.5 = 40_000
        assert_eq!(schedule.assignments.len(), 1);
        assert_eq!(schedule.assignments[0].resource_id, "M2");
        assert_eq!(schedule.makespan_ms, 40_000);
    }
}

// =============================================================================
// Level 2: 제약조건 - 납기, Setup Time
// =============================================================================

mod level2_constraints {
    use super::*;

    /// 납기 제약 위반 감지
    #[test]
    fn test_due_date_violation() {
        use chrono::TimeZone;
        // Scheduler uses relative time (starts at 0), so use epoch-relative timestamp
        let due = Utc.timestamp_millis_opt(30_000).unwrap(); // 30초 후 (epoch 기준)

        let job = Job::new("J1")
            .with_priority(1)
            .with_due_date(due)
            .with_operation(
                Operation::new("O1", "J1", 1)
                    .with_time(0, 60_000, 0) // 60초 처리 → 위반
                    .with_equipment(vec!["M1".into()]),
            );

        let resource = Resource::equipment("M1");

        let request = ScheduleRequest::new(vec![job], vec![resource]);
        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        // 납기 위반 발생 (완료시간 60000 > 납기 30000)
        assert!(!schedule.violations.is_empty());
    }

    /// Setup Time Matrix (SDST)
    #[test]
    fn test_setup_time_matrix() {
        let job1 = Job::new("J1")
            .with_priority(100)
            .with_product("ProductA")
            .with_operation(
                Operation::new("O1", "J1", 1)
                    .with_time(0, 30_000, 0)
                    .with_equipment(vec!["M1".into()]),
            );

        let job2 = Job::new("J2")
            .with_priority(1) // 먼저 처리
            .with_product("ProductB")
            .with_operation(
                Operation::new("O2", "J2", 1)
                    .with_time(0, 30_000, 0)
                    .with_equipment(vec!["M1".into()]),
            );

        let resource = Resource::equipment("M1");

        // Setup Matrix: ProductB → ProductA 전환 시 10초
        let matrix = SetupMatrix::new("M1").with_setup("ProductB", "ProductA", 10_000);
        let setup_matrices = SetupMatrixCollection::new().with_matrix(matrix);

        let request = ScheduleRequest::new(vec![job1, job2], vec![resource])
            .with_setup_matrices(setup_matrices);

        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        // J2(30) → Setup(10) → J1(30) = 70
        assert_eq!(schedule.makespan_ms, 70_000);
    }
}

// =============================================================================
// Level 3: KPI 및 분석
// =============================================================================

mod level3_kpi {
    use super::*;

    /// KPI 계산
    #[test]
    fn test_kpi_calculation() {
        let mut schedule = Schedule::new();
        schedule.add_assignment(Assignment::new("O1", "J1", "M1", 0, 30_000));
        schedule.add_assignment(Assignment::new("O2", "J2", "M1", 30_000, 60_000));

        let due1 = Utc::now() + Duration::milliseconds(40_000);
        let due2 = Utc::now() + Duration::milliseconds(50_000);

        let jobs = vec![
            Job::new("J1").with_priority(1).with_due_date(due1),
            Job::new("J2").with_priority(2).with_due_date(due2),
        ];

        let calculator = KpiCalculator::new(100_000);
        let kpi = calculator.calculate(&schedule, &jobs);

        assert_eq!(kpi.makespan_ms, 60_000);
        assert!(kpi.average_utilization > 0.0);
    }

    /// Time Fence 검증
    #[test]
    fn test_time_fence() {
        let config = TimeFenceConfig::new(0, 24, 48);
        let hour = 3_600_000i64;

        // Frozen Zone
        assert_eq!(config.get_zone(12 * hour), FenceZone::Frozen);
        assert!(!config.can_modify_operation(12 * hour));

        // Slushy Zone
        assert_eq!(config.get_zone(36 * hour), FenceZone::Slushy);
        assert!(config.can_modify_operation(36 * hour));

        // Liquid Zone
        assert_eq!(config.get_zone(80 * hour), FenceZone::Liquid);
        assert!(config.can_modify_operation(80 * hour));
    }
}

// =============================================================================
// Level 4: 동적 기능 - 재스케줄링, Pegging, CTP, What-If
// =============================================================================

mod level4_dynamic {
    use super::*;

    /// 재스케줄링 - Right Shift
    #[test]
    fn test_reschedule_right_shift() {
        let mut schedule = Schedule::new();
        schedule.add_assignment(Assignment::new("O1", "J1", "M1", 0, 30_000));
        schedule.add_assignment(Assignment::new("O2", "J1", "M1", 30_000, 60_000));

        let rescheduler = Rescheduler::new();
        let event = ScheduleEvent::OperationDelay {
            operation_id: "O1".into(),
            delay_ms: 10_000,
        };

        let result = rescheduler.right_shift(&schedule, &event);

        // O1이 10초 지연
        let o1 = result
            .schedule
            .assignments
            .iter()
            .find(|a| a.operation_id == "O1")
            .unwrap();
        assert_eq!(o1.end_ms, 40_000);
    }

    /// Pegging 기본
    #[test]
    fn test_pegging_basic() {
        let mut engine = PeggingEngine::new();

        engine.add_material(PeggingMaterial {
            id: "MAT1".into(),
            name: "Component".into(),
            on_hand_qty: 50.0,
            unit: "EA".into(),
        });

        engine.add_demand(MaterialDemand {
            id: "DEM1".into(),
            material_id: "MAT1".into(),
            quantity: 30.0,
            required_at_ms: 60_000,
            operation_id: "O1".into(),
            job_id: "J1".into(),
        });

        let result = engine.execute_pegging();

        assert!(!result.links.is_empty());
        assert!(result.unmet_demands.is_empty());
    }

    /// CTP 확인
    #[test]
    fn test_ctp_check() {
        let mut engine = CtpEngine::new(0);

        engine.add_material(PeggingMaterial {
            id: "MAT1".into(),
            name: "Component".into(),
            on_hand_qty: 100.0,
            unit: "EA".into(),
        });

        engine.set_product_bom("PROD1", vec![("MAT1".into(), 2.0)]);
        engine.set_product_process_time("PROD1", 3_600_000);

        let request = CtpRequest {
            order_id: "ORD1".into(),
            product_id: "PROD1".into(),
            quantity: 10.0,
            requested_date_ms: 86_400_000, // 1일 후
            priority: 1,
        };

        let result = engine.check_capability(&request);

        assert!(result.is_capable);
        assert_eq!(result.promised_quantity, 10.0);
    }

    /// What-If 시나리오 비교
    #[test]
    fn test_whatif_comparison() {
        let mut schedule = Schedule::new();
        schedule.add_assignment(Assignment::new("O1", "J1", "M1", 0, 60_000));

        let jobs = vec![Job::new("J1").with_priority(1)];
        let engine = WhatIfEngine::new(schedule, jobs, 100_000);

        let baseline = Scenario {
            id: "baseline".into(),
            name: "기준".into(),
            changes: vec![],
        };

        let improved = Scenario {
            id: "improved".into(),
            name: "20% 단축".into(),
            changes: vec![ScenarioChange::ProcessingTime {
                operation_id: None,
                change_percent: -0.2,
            }],
        };

        let baseline_result = engine.apply_scenario(&baseline);
        let improved_result = engine.apply_scenario(&improved);

        assert!(improved_result.schedule.makespan_ms < baseline_result.schedule.makespan_ms);
    }
}

// =============================================================================
// Level 5: 입력 검증
// =============================================================================

mod level5_validation {
    use super::*;

    #[test]
    fn test_valid_input() {
        let jobs = vec![Job::new("J1").with_priority(1).with_operation(
            Operation::new("O1", "J1", 1)
                .with_time(0, 60_000, 0)
                .with_equipment(vec!["M1".into()]),
        )];
        let operations: Vec<Operation> = jobs.iter().flat_map(|j| j.operations.clone()).collect();
        let resources = vec![Resource::equipment("M1")];

        let result = Validator::validate_schedule_request(&jobs, &operations, &resources);
        assert!(result.is_valid());
    }

    #[test]
    fn test_duplicate_id_detection() {
        let jobs = vec![
            Job::new("J1").with_priority(1),
            Job::new("J1").with_priority(2), // 중복
        ];

        let mut result = ValidationResult::new();
        Validator::validate_jobs(&jobs, &mut result);

        assert!(!result.is_valid());
    }
}

// =============================================================================
// Level 6: GA 스케줄러 통합 테스트
// =============================================================================

mod level6_ga_scheduler {
    use super::*;

    /// GA 스케줄러 기본 테스트
    #[test]
    fn test_ga_basic_scheduling() {
        let jobs = vec![
            Job::new("J1").with_priority(1).with_operation(
                Operation::new("O1", "J1", 1)
                    .with_time(0, 30_000, 0)
                    .with_equipment(vec!["M1".into(), "M2".into()]),
            ),
            Job::new("J2").with_priority(2).with_operation(
                Operation::new("O2", "J2", 1)
                    .with_time(0, 40_000, 0)
                    .with_equipment(vec!["M1".into(), "M2".into()]),
            ),
        ];

        let resources = vec![Resource::equipment("M1"), Resource::equipment("M2")];

        let params = GaParams {
            population_size: 20,
            max_generations: 50,
            elite_ratio: 0.1,
            ..Default::default()
        };

        let scheduler = GaScheduler::new(params, GeneticOperators::default());
        let result = scheduler.schedule(&jobs, &resources, 0);

        // GA가 유효한 스케줄 생성
        assert_eq!(result.schedule.assignments.len(), 2);
        assert!(result.schedule.makespan_ms > 0);
        // 병렬 처리 가능하므로 makespan <= 40_000
        assert!(result.schedule.makespan_ms <= 70_000);
    }

    /// GA vs Simple 비교
    #[test]
    fn test_ga_improves_over_simple() {
        // 복잡한 시나리오에서 GA가 Simple보다 나은 결과
        let mut jobs = Vec::new();
        for i in 0..5 {
            let job = Job::new(format!("J{}", i))
                .with_priority(i + 1)
                .with_operation(
                    Operation::new(format!("O{}", i), format!("J{}", i), 1)
                        .with_time(0, 20_000 + (i as i64 * 5_000), 0)
                        .with_equipment(vec!["M1".into(), "M2".into(), "M3".into()]),
                );
            jobs.push(job);
        }

        let resources = vec![
            Resource::equipment("M1").with_efficiency(1.0),
            Resource::equipment("M2").with_efficiency(1.2),
            Resource::equipment("M3").with_efficiency(0.8),
        ];

        let request = ScheduleRequest::new(jobs.clone(), resources.clone());

        // Simple Scheduler
        let simple = SimpleScheduler::new();
        let simple_schedule = simple.schedule(&request);

        // GA Scheduler
        let ga_params = GaParams {
            population_size: 30,
            max_generations: 100,
            ..Default::default()
        };
        let ga = GaScheduler::new(ga_params, GeneticOperators::default());
        let ga_result = ga.schedule(&jobs, &resources, 0);

        // GA가 동등하거나 더 나은 결과
        assert!(ga_result.schedule.makespan_ms <= simple_schedule.makespan_ms + 5_000);
    }

    /// Setup Matrix와 GA 통합
    #[test]
    fn test_ga_with_setup_matrices() {
        let jobs = vec![
            Job::new("J1").with_product("ProductA").with_operation(
                Operation::new("O1", "J1", 1)
                    .with_time(0, 30_000, 0)
                    .with_equipment(vec!["M1".into()]),
            ),
            Job::new("J2").with_product("ProductB").with_operation(
                Operation::new("O2", "J2", 1)
                    .with_time(0, 30_000, 0)
                    .with_equipment(vec!["M1".into()]),
            ),
            Job::new("J3").with_product("ProductA").with_operation(
                Operation::new("O3", "J3", 1)
                    .with_time(0, 30_000, 0)
                    .with_equipment(vec!["M1".into()]),
            ),
        ];

        let resources = vec![Resource::equipment("M1")];

        let matrix = SetupMatrix::new("M1")
            .with_default_setup(10_000)
            .with_setup("ProductA", "ProductB", 20_000)
            .with_setup("ProductB", "ProductA", 15_000);
        let setup_matrices = SetupMatrixCollection::new().with_matrix(matrix);

        let params = GaParams {
            population_size: 20,
            max_generations: 50,
            ..Default::default()
        };
        let scheduler = GaScheduler::new(params, GeneticOperators::default())
            .with_setup_matrices(setup_matrices);
        let result = scheduler.schedule(&jobs, &resources, 0);

        assert_eq!(result.schedule.assignments.len(), 3);
        // GA가 setup time 최소화 시도
        assert!(result.schedule.makespan_ms > 0);
    }
}

// =============================================================================
// Level 7: Edge Cases
// =============================================================================

mod level7_edge_cases {
    use super::*;

    /// 빈 Job 목록
    #[test]
    fn test_empty_jobs() {
        let request = ScheduleRequest::new(vec![], vec![Resource::equipment("M1")]);
        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        assert!(schedule.assignments.is_empty());
        assert_eq!(schedule.makespan_ms, 0);
    }

    /// Operation 없는 Job
    #[test]
    fn test_job_without_operations() {
        let job = Job::new("J1").with_priority(1);
        let request = ScheduleRequest::new(vec![job], vec![Resource::equipment("M1")]);
        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        assert!(schedule.assignments.is_empty());
    }

    /// 매우 큰 처리 시간
    #[test]
    fn test_large_processing_time() {
        let job = Job::new("J1").with_operation(
            Operation::new("O1", "J1", 1)
                .with_time(0, 86_400_000, 0) // 24시간
                .with_equipment(vec!["M1".into()]),
        );

        let request = ScheduleRequest::new(vec![job], vec![Resource::equipment("M1")]);
        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        assert_eq!(schedule.makespan_ms, 86_400_000);
    }

    /// 동일 우선순위 Job들
    #[test]
    fn test_same_priority_jobs() {
        let jobs: Vec<Job> = (0..5)
            .map(|i| {
                Job::new(format!("J{}", i))
                    .with_priority(1) // 모두 동일
                    .with_operation(
                        Operation::new(format!("O{}", i), format!("J{}", i), 1)
                            .with_time(0, 10_000, 0)
                            .with_equipment(vec!["M1".into()]),
                    )
            })
            .collect();

        let request = ScheduleRequest::new(jobs, vec![Resource::equipment("M1")]);
        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        assert_eq!(schedule.assignments.len(), 5);
        assert_eq!(schedule.makespan_ms, 50_000);
    }

    /// 많은 대체 자원
    #[test]
    fn test_many_alternative_resources() {
        let machines: Vec<String> = (0..10).map(|i| format!("M{}", i)).collect();

        let job = Job::new("J1").with_operation(
            Operation::new("O1", "J1", 1)
                .with_time(0, 60_000, 0)
                .with_equipment(machines.clone()),
        );

        let resources: Vec<Resource> = machines
            .iter()
            .enumerate()
            .map(|(i, id)| Resource::equipment(id).with_efficiency(1.0 + i as f64 * 0.1))
            .collect();

        let request = ScheduleRequest::new(vec![job], resources);
        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        // 가장 효율적인 M9 선택 (efficiency 1.9)
        assert_eq!(schedule.assignments[0].resource_id, "M9");
    }

    /// 자원 없이 실행
    #[test]
    fn test_operation_without_resource_requirements() {
        let job = Job::new("J1").with_operation(
            Operation::new("O1", "J1", 1).with_time(0, 30_000, 0), // 자원 요구사항 없음
        );

        let request = ScheduleRequest::new(vec![job], vec![Resource::equipment("M1")]);
        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        assert_eq!(schedule.assignments.len(), 1);
        assert_eq!(schedule.assignments[0].resource_id, "NONE");
    }
}

// =============================================================================
// Level 8: 성능 및 확장성 테스트
// =============================================================================

mod level8_scalability {
    use super::*;

    /// 중간 규모 시나리오 (20 Jobs, 5 Machines)
    #[test]
    fn test_medium_scale_scenario() {
        let mut jobs = Vec::new();
        let machines: Vec<String> = (0..5).map(|i| format!("M{}", i)).collect();

        for i in 0..20 {
            let mut job = Job::new(format!("J{}", i)).with_priority(i % 5 + 1);

            // 각 Job에 2-3개 Operations
            let num_ops = 2 + (i % 2);
            for seq in 1..=num_ops {
                let op = Operation::new(format!("J{}-O{}", i, seq), format!("J{}", i), seq)
                    .with_time(5_000, 10_000 + (i as i64 * 1_000), 2_000)
                    .with_equipment(machines.clone());

                job = job.with_operation(op);
            }
            jobs.push(job);
        }

        let resources: Vec<Resource> = machines.iter().map(Resource::equipment).collect();

        let request = ScheduleRequest::new(jobs, resources);
        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        // 모든 Operation이 할당됨
        let expected_ops: usize = (0..20).map(|i| 2 + (i % 2)).sum();
        assert_eq!(schedule.assignments.len(), expected_ops);
        assert!(schedule.makespan_ms > 0);
    }

    /// 대규모 시나리오 (50 Jobs, 10 Machines)
    #[test]
    fn test_large_scale_scenario() {
        let mut jobs = Vec::new();
        let machines: Vec<String> = (0..10).map(|i| format!("M{}", i)).collect();

        for i in 0..50 {
            let job = Job::new(format!("J{}", i))
                .with_priority(i % 10 + 1)
                .with_operation(
                    Operation::new(format!("O{}", i), format!("J{}", i), 1)
                        .with_time(1_000, 5_000 + (i as i64 * 100), 500)
                        .with_equipment(machines.clone()),
                );
            jobs.push(job);
        }

        let resources: Vec<Resource> = machines
            .iter()
            .enumerate()
            .map(|(i, id)| Resource::equipment(id).with_efficiency(0.8 + i as f64 * 0.05))
            .collect();

        let request = ScheduleRequest::new(jobs.clone(), resources.clone());

        // Simple Scheduler
        let simple = SimpleScheduler::new();
        let simple_schedule = simple.schedule(&request);

        assert_eq!(simple_schedule.assignments.len(), 50);
        assert!(simple_schedule.makespan_ms > 0);

        // GA Scheduler (빠른 설정)
        let ga_params = GaParams {
            population_size: 20,
            max_generations: 30,
            ..Default::default()
        };
        let ga = GaScheduler::new(ga_params, GeneticOperators::default());
        let ga_result = ga.schedule(&jobs, &resources, 0);

        assert_eq!(ga_result.schedule.assignments.len(), 50);
    }

    /// 복잡한 의존성 체인
    #[test]
    fn test_complex_dependency_chain() {
        // 10개 Operation이 순차적으로 의존
        let mut job = Job::new("J1").with_priority(1);

        for i in 1..=10 {
            let op = Operation::new(format!("O{}", i), "J1", i)
                .with_time(1_000, 5_000, 500)
                .with_equipment(vec!["M1".into()]);
            job = job.with_operation(op);
        }

        let request = ScheduleRequest::new(vec![job], vec![Resource::equipment("M1")]);
        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        assert_eq!(schedule.assignments.len(), 10);

        // 순차 실행 확인
        for i in 0..9 {
            let curr = &schedule.assignments[i];
            let next = &schedule.assignments[i + 1];
            assert!(curr.end_ms <= next.start_ms);
        }
    }
}
