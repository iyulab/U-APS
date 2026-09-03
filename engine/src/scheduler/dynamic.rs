//! Dynamic Scheduler - 동적 재스케줄링 통합 인터페이스
//!
//! ProductionScheduler + TimeFence + Rescheduler + Pegging 통합

use crate::models::{Job, Resource, SetupMatrixCollection};
use crate::scheduler::{
    kpi::{KpiCalculator, ScheduleKpi},
    pegging::{PeggingEngine, PeggingResult},
    production::{ProductionConfig, ProductionScheduler},
    reschedule::{RescheduleResult, RescheduleStrategy, Rescheduler, ScheduleEvent},
    time_fence::{FenceViolation, TimeFenceChecker, TimeFenceConfig},
    Schedule,
};
use serde::{Deserialize, Serialize};

/// 동적 스케줄러 설정
#[derive(Debug, Clone)]
pub struct DynamicSchedulerConfig {
    /// 생산 스케줄러 설정
    pub production_config: ProductionConfig,
    /// Time Fence 설정
    pub fence_config: Option<TimeFenceConfig>,
    /// 자재 제약 활성화
    pub enable_material_constraints: bool,
    /// 기본 재스케줄 전략
    pub default_reschedule_strategy: RescheduleStrategy,
}

impl Default for DynamicSchedulerConfig {
    fn default() -> Self {
        Self {
            production_config: ProductionConfig::default(),
            fence_config: None,
            enable_material_constraints: false,
            default_reschedule_strategy: RescheduleStrategy::AOR,
        }
    }
}

/// 동적 스케줄링 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicScheduleResult {
    /// 스케줄
    pub schedule: Schedule,
    /// KPI
    pub kpi: ScheduleKpi,
    /// 페깅 결과 (자재 제약 시)
    pub pegging: Option<PeggingResult>,
    /// 자재 지연 공정
    pub material_delayed_operations: Vec<MaterialDelayedOperation>,
    /// 실행 시간 (ms)
    pub execution_time_ms: u128,
}

/// 자재 지연 공정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialDelayedOperation {
    /// 공정 ID
    pub operation_id: String,
    /// 원래 시작 시간
    pub original_start_ms: i64,
    /// 자재 가용 시간
    pub material_available_ms: i64,
    /// 지연 시간
    pub delay_ms: i64,
}

/// 재스케줄링 요청
#[derive(Debug, Clone)]
pub struct RescheduleRequest {
    /// 현재 스케줄
    pub current_schedule: Schedule,
    /// 이벤트
    pub event: ScheduleEvent,
    /// 전략 (None이면 자동 선택)
    pub strategy: Option<RescheduleStrategy>,
}

/// 통합 동적 스케줄러
#[derive(Debug, Clone)]
pub struct DynamicScheduler {
    config: DynamicSchedulerConfig,
    setup_matrices: SetupMatrixCollection,
    pegging_engine: Option<PeggingEngine>,
}

impl DynamicScheduler {
    /// 새 동적 스케줄러 생성
    pub fn new(config: DynamicSchedulerConfig) -> Self {
        Self {
            config,
            setup_matrices: SetupMatrixCollection::new(),
            pegging_engine: None,
        }
    }

    /// 기본 설정으로 생성
    pub fn default_scheduler() -> Self {
        Self::new(DynamicSchedulerConfig::default())
    }

    /// Setup Matrix 설정
    pub fn with_setup_matrices(mut self, matrices: SetupMatrixCollection) -> Self {
        self.setup_matrices = matrices;
        self
    }

    /// Time Fence 설정
    pub fn with_time_fence(mut self, config: TimeFenceConfig) -> Self {
        self.config.fence_config = Some(config);
        self
    }

    /// Pegging Engine 설정
    pub fn with_pegging_engine(mut self, engine: PeggingEngine) -> Self {
        self.pegging_engine = Some(engine);
        self.config.enable_material_constraints = true;
        self
    }

    /// 초기 스케줄링 (자재 제약 포함)
    pub fn schedule(
        &self,
        jobs: &[Job],
        resources: &[Resource],
        start_time_ms: i64,
    ) -> DynamicScheduleResult {
        let start = std::time::Instant::now();

        // 1. 기본 생산 스케줄링
        let production_scheduler = ProductionScheduler::new(self.config.production_config.clone())
            .with_setup_matrices(self.setup_matrices.clone());

        let production_result = production_scheduler.schedule(jobs, resources, start_time_ms);
        let mut schedule = production_result.schedule;

        // 2. 자재 제약 적용
        let (pegging_result, material_delays) = if self.config.enable_material_constraints {
            if let Some(ref engine) = self.pegging_engine {
                let (adjusted_schedule, delays) =
                    self.apply_material_constraints(&schedule, engine);
                schedule = adjusted_schedule;
                (Some(engine.execute_pegging()), delays)
            } else {
                (None, Vec::new())
            }
        } else {
            (None, Vec::new())
        };

        // 3. KPI 재계산
        let horizon_ms = schedule.makespan_ms.max(1);
        let kpi_calculator = KpiCalculator::new(horizon_ms);
        let kpi = kpi_calculator.calculate(&schedule, jobs);

        let execution_time_ms = start.elapsed().as_millis();

        DynamicScheduleResult {
            schedule,
            kpi,
            pegging: pegging_result,
            material_delayed_operations: material_delays,
            execution_time_ms,
        }
    }

    /// 재스케줄링
    pub fn reschedule(&self, request: RescheduleRequest) -> RescheduleResult {
        let rescheduler = if let Some(ref fence) = self.config.fence_config {
            Rescheduler::new().with_fence(fence.clone())
        } else {
            Rescheduler::new()
        };

        match request.strategy {
            Some(RescheduleStrategy::RightShift) => {
                rescheduler.right_shift(&request.current_schedule, &request.event)
            }
            Some(RescheduleStrategy::AOR) | Some(RescheduleStrategy::Partial) => {
                rescheduler.aor(&request.current_schedule, &request.event)
            }
            Some(RescheduleStrategy::TotalRegeneration) | Some(RescheduleStrategy::Full) => {
                rescheduler.total_regeneration(&request.current_schedule, &request.event)
            }
            None => rescheduler.auto_reschedule(&request.current_schedule, &request.event),
        }
    }

    /// 스케줄 변경 검증 (Time Fence)
    pub fn validate_schedule_change(
        &self,
        old_schedule: &Schedule,
        new_schedule: &Schedule,
    ) -> Vec<FenceViolation> {
        if let Some(ref fence) = self.config.fence_config {
            let checker = TimeFenceChecker::new(fence.clone());
            checker.check_schedule_change(old_schedule, new_schedule)
        } else {
            Vec::new()
        }
    }

    /// 단일 공정 변경 검증
    pub fn validate_operation_change(
        &self,
        operation_id: &str,
        old_start: i64,
        new_start: i64,
    ) -> Option<FenceViolation> {
        if let Some(ref fence) = self.config.fence_config {
            let checker = TimeFenceChecker::new(fence.clone());
            checker.check_single_change(operation_id, old_start, new_start)
        } else {
            None
        }
    }

    /// 자재 제약 적용
    fn apply_material_constraints(
        &self,
        schedule: &Schedule,
        engine: &PeggingEngine,
    ) -> (Schedule, Vec<MaterialDelayedOperation>) {
        let mut new_schedule = schedule.clone();
        let mut delays = Vec::new();

        // 각 공정별 자재 가용 시간 확인 및 조정
        for assign in &mut new_schedule.assignments {
            let material_available =
                engine.get_operation_material_availability(&assign.operation_id);

            if material_available > assign.start_ms {
                let delay = material_available - assign.start_ms;

                delays.push(MaterialDelayedOperation {
                    operation_id: assign.operation_id.clone(),
                    original_start_ms: assign.start_ms,
                    material_available_ms: material_available,
                    delay_ms: delay,
                });

                // 시작/종료 시간 조정
                let duration = assign.end_ms - assign.start_ms;
                assign.start_ms = material_available;
                assign.end_ms = material_available + duration;
            }
        }

        // Makespan 재계산
        new_schedule.makespan_ms = new_schedule
            .assignments
            .iter()
            .map(|a| a.end_ms)
            .max()
            .unwrap_or(0);

        (new_schedule, delays)
    }

    /// 이벤트 기반 자동 재스케줄링
    pub fn handle_event(
        &self,
        current_schedule: &Schedule,
        event: ScheduleEvent,
        jobs: &[Job],
    ) -> DynamicScheduleResult {
        let start = std::time::Instant::now();

        // 1. 재스케줄링 수행
        let reschedule_result = self.reschedule(RescheduleRequest {
            current_schedule: current_schedule.clone(),
            event,
            strategy: None, // 자동 선택
        });

        let schedule = reschedule_result.schedule;

        // 2. KPI 계산
        let horizon_ms = schedule.makespan_ms.max(1);
        let kpi_calculator = KpiCalculator::new(horizon_ms);
        let kpi = kpi_calculator.calculate(&schedule, jobs);

        let execution_time_ms = start.elapsed().as_millis();

        DynamicScheduleResult {
            schedule,
            kpi,
            pegging: None,
            material_delayed_operations: Vec::new(),
            execution_time_ms,
        }
    }
}

impl Default for DynamicScheduler {
    fn default() -> Self {
        Self::default_scheduler()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Operation;
    use crate::{MaterialDemand, MaterialSupply, PeggingMaterial};

    fn create_test_scenario() -> (Vec<Job>, Vec<Resource>) {
        let jobs = vec![
            Job::new("J1")
                .with_product("PRODUCT-A")
                .with_priority(1)
                .with_operation(
                    Operation::new("J1-O1", "J1", 1)
                        .with_time(0, 30_000, 0)
                        .with_equipment(vec!["M1".to_string(), "M2".to_string()]),
                ),
            Job::new("J2")
                .with_product("PRODUCT-B")
                .with_priority(2)
                .with_operation(
                    Operation::new("J2-O1", "J2", 1)
                        .with_time(0, 25_000, 0)
                        .with_equipment(vec!["M1".to_string(), "M2".to_string()]),
                ),
        ];

        let resources = vec![
            Resource::equipment("M1").with_efficiency(1.0),
            Resource::equipment("M2").with_efficiency(0.9),
        ];

        (jobs, resources)
    }

    #[test]
    fn test_basic_dynamic_scheduling() {
        let (jobs, resources) = create_test_scenario();

        let scheduler = DynamicScheduler::default();
        let result = scheduler.schedule(&jobs, &resources, 0);

        assert!(!result.schedule.assignments.is_empty());
        assert!(result.kpi.makespan_ms > 0);
    }

    #[test]
    fn test_with_time_fence() {
        let (jobs, resources) = create_test_scenario();

        let fence = TimeFenceConfig::new(0, 24, 48);
        let scheduler = DynamicScheduler::default().with_time_fence(fence);

        let result = scheduler.schedule(&jobs, &resources, 0);
        assert!(!result.schedule.assignments.is_empty());
    }

    #[test]
    fn test_reschedule_operation_delay() {
        let (jobs, resources) = create_test_scenario();

        let scheduler = DynamicScheduler::default();
        let initial_result = scheduler.schedule(&jobs, &resources, 0);

        let event = ScheduleEvent::OperationDelay {
            operation_id: "J1-O1".to_string(),
            delay_ms: 5_000,
        };

        let reschedule_result = scheduler.reschedule(RescheduleRequest {
            current_schedule: initial_result.schedule,
            event,
            strategy: Some(RescheduleStrategy::RightShift),
        });

        assert_eq!(
            reschedule_result.strategy_used,
            RescheduleStrategy::RightShift
        );
    }

    #[test]
    fn test_reschedule_machine_breakdown() {
        let (jobs, resources) = create_test_scenario();

        let scheduler = DynamicScheduler::default();
        let initial_result = scheduler.schedule(&jobs, &resources, 0);

        let event = ScheduleEvent::MachineBreakdown {
            resource_id: "M1".to_string(),
            start_ms: 10_000,
            duration_ms: 20_000,
        };

        let reschedule_result = scheduler.reschedule(RescheduleRequest {
            current_schedule: initial_result.schedule,
            event,
            strategy: None, // 자동 선택
        });

        assert!(reschedule_result.stats.makespan_change_ms >= 0);
    }

    #[test]
    fn test_validate_frozen_zone_change() {
        let fence = TimeFenceConfig::new(0, 24, 48);
        let scheduler = DynamicScheduler::default().with_time_fence(fence);

        // Frozen zone (0-24시간)에서 변경 시도
        let hour = 3_600_000i64;
        let violation = scheduler.validate_operation_change("op1", 12 * hour, 20 * hour);

        assert!(violation.is_some());
    }

    #[test]
    fn test_liquid_zone_change_allowed() {
        let fence = TimeFenceConfig::new(0, 24, 48);
        let scheduler = DynamicScheduler::default().with_time_fence(fence);

        // Liquid zone (72시간+)에서 변경
        let hour = 3_600_000i64;
        let violation = scheduler.validate_operation_change("op1", 80 * hour, 90 * hour);

        assert!(violation.is_none());
    }

    #[test]
    fn test_with_material_constraints() {
        let (jobs, resources) = create_test_scenario();

        // 자재 엔진 설정
        let mut pegging_engine = PeggingEngine::new();
        pegging_engine.add_material(PeggingMaterial {
            id: "mat1".into(),
            name: "Component A".into(),
            on_hand_qty: 0.0,
            unit: "EA".into(),
        });

        pegging_engine.add_supply(MaterialSupply {
            id: "sup1".into(),
            material_id: "mat1".into(),
            quantity: 100.0,
            available_at_ms: 50_000, // 50초 후 가용
            supply_type: crate::scheduler::pegging::SupplyType::PurchaseOrder,
        });

        pegging_engine.add_demand(MaterialDemand {
            id: "dem1".into(),
            material_id: "mat1".into(),
            quantity: 10.0,
            required_at_ms: 0,
            operation_id: "J1-O1".into(),
            job_id: "J1".into(),
        });

        let scheduler = DynamicScheduler::default().with_pegging_engine(pegging_engine);

        let result = scheduler.schedule(&jobs, &resources, 0);

        // 자재 지연이 적용되어야 함
        assert!(result.pegging.is_some());
    }

    #[test]
    fn test_handle_event() {
        let (jobs, resources) = create_test_scenario();

        let scheduler = DynamicScheduler::default();
        let initial_result = scheduler.schedule(&jobs, &resources, 0);

        let event = ScheduleEvent::ProcessTimeChange {
            operation_id: "J1-O1".to_string(),
            new_duration_ms: 40_000,
        };

        let result = scheduler.handle_event(&initial_result.schedule, event, &jobs);
        assert!(result.kpi.makespan_ms > 0);
    }

    #[test]
    fn test_auto_reschedule_strategy_selection() {
        let (jobs, resources) = create_test_scenario();

        let scheduler = DynamicScheduler::default();
        let initial_result = scheduler.schedule(&jobs, &resources, 0);

        // 작은 변경 → RightShift
        let small_event = ScheduleEvent::OperationDelay {
            operation_id: "J1-O1".to_string(),
            delay_ms: 1_000,
        };

        let result = scheduler.reschedule(RescheduleRequest {
            current_schedule: initial_result.schedule,
            event: small_event,
            strategy: None,
        });

        // 자동 선택된 전략 확인
        assert!(matches!(
            result.strategy_used,
            RescheduleStrategy::RightShift | RescheduleStrategy::AOR
        ));
    }
}
