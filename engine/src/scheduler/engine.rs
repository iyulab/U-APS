//! SchedulingEngine - 통합 스케줄링 엔진
//!
//! 프로덕션 환경을 위한 통합 인터페이스
//! - 모든 스케줄러 통합 (Simple, GA, Production, Dynamic, Analytics)
//! - 구조화된 로깅
//! - 성능 메트릭
//! - 에러 핸들링

use crate::cp::{CpSatConfig, CpSatScheduler};
use crate::error::Result;
use crate::ga::{GaParams, GaScheduler, GeneticOperators};
use crate::models::{
    Calendar, CertificationMatrix, CrewManager, Job, MaterialManager, OutsourcingManager, Resource,
    SetupMatrixCollection, SiteTransitions, SkillMatrix, WorkerTimeOverride,
};
use crate::scheduler::{
    analytics::{AnalyticsConfig, AnalyticsEngine, AnalyticsReport},
    dispatching::DispatchingConfig,
    dynamic::{DynamicScheduler, DynamicSchedulerConfig},
    kpi::{KpiCalculator, ScheduleKpi},
    pegging::PeggingEngine,
    production::{ProductionConfig, ProductionScheduler},
    simple::SimpleScheduler,
    Schedule, ScheduleRequest,
};
use crate::validation::{ValidationResult, Validator};
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// 엔진 레벨 기본 타임아웃(ms) — `EngineConfig::default()`와 FFI 경계(`ffi_request.timeout_ms`
/// 생략 시의 폴백) 양쪽이 이 상수 하나를 공유한다. 값이 갈리면 FFI/네이티브 두 진입점이
/// 서로 다른 기본 상한을 갖는 드리프트가 재발한다.
pub const DEFAULT_ENGINE_TIMEOUT_MS: u64 = 60_000;

/// 스케줄링 알고리즘
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchedulingAlgorithm {
    /// 우선순위 기반 단순 스케줄링
    Simple,
    /// 유전 알고리즘
    GeneticAlgorithm,
    /// 생산 스케줄러 (Setup Matrix, Campaign 등)
    Production,
    /// 동적 스케줄러 (Time Fence, Rescheduling, Pegging)
    Dynamic,
    /// CP-SAT 제약 프로그래밍 솔버
    CpSat,
    /// 자동 선택 (문제 크기/특성 기반)
    Auto,
    /// 하이브리드 (CP-SAT → GA)
    Hybrid,
}

/// 스케줄링 엔진 설정
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// 사용할 알고리즘
    pub algorithm: SchedulingAlgorithm,
    /// GA 파라미터 (GA 사용 시)
    pub ga_params: Option<GaParams>,
    /// 생산 스케줄러 설정
    pub production_config: Option<ProductionConfig>,
    /// 동적 스케줄러 설정
    pub dynamic_config: Option<DynamicSchedulerConfig>,
    /// CP-SAT 설정
    pub cpsat_config: Option<CpSatConfig>,
    /// Setup Matrix
    pub setup_matrices: SetupMatrixCollection,
    /// 사이트 간 이동 시간
    pub site_transitions: SiteTransitions,
    /// 운영 캘린더 목록
    pub calendars: Vec<Calendar>,
    /// 스킬 매트릭스 (작업자 숙련도)
    pub skill_matrix: Option<SkillMatrix>,
    /// 작업자별 시간 오버라이드
    pub worker_time_override: Option<WorkerTimeOverride>,
    /// 디스패칭 룰 설정
    pub dispatching_config: Option<DispatchingConfig>,
    /// 자재 관리자
    pub material_manager: Option<MaterialManager>,
    /// 인증 매트릭스 (작업자 자격)
    pub certification_matrix: Option<CertificationMatrix>,
    /// 팀 관리자
    pub crew_manager: Option<CrewManager>,
    /// Pegging 엔진
    pub pegging_engine: Option<PeggingEngine>,
    /// 외주 관리자
    pub outsourcing_manager: Option<OutsourcingManager>,
    /// 입력 검증 활성화
    pub validate_input: bool,
    /// 상세 로깅 활성화
    pub verbose_logging: bool,
    /// 타임아웃 (밀리초)
    pub timeout_ms: Option<u64>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            algorithm: SchedulingAlgorithm::Production,
            ga_params: None,
            production_config: None,
            dynamic_config: None,
            cpsat_config: None,
            setup_matrices: SetupMatrixCollection::new(),
            site_transitions: SiteTransitions::new(),
            calendars: Vec::new(),
            skill_matrix: None,
            worker_time_override: None,
            dispatching_config: None,
            material_manager: None,
            certification_matrix: None,
            crew_manager: None,
            pegging_engine: None,
            outsourcing_manager: None,
            validate_input: true,
            verbose_logging: false,
            timeout_ms: Some(DEFAULT_ENGINE_TIMEOUT_MS), // 1분 기본 타임아웃
        }
    }
}

/// 성능 메트릭
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// 전체 실행 시간 (ms)
    pub total_time_ms: u128,
    /// 검증 시간 (ms)
    pub validation_time_ms: u128,
    /// 스케줄링 시간 (ms)
    pub scheduling_time_ms: u128,
    /// KPI 계산 시간 (ms)
    pub kpi_time_ms: u128,
    /// 사용된 알고리즘
    pub algorithm: SchedulingAlgorithm,
    /// Job 수
    pub job_count: usize,
    /// Operation 수
    pub operation_count: usize,
    /// Resource 수
    pub resource_count: usize,
}

/// 로그 레벨
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// 로그 엔트리
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// 타임스탬프 (ms)
    pub timestamp_ms: u128,
    /// 레벨
    pub level: String,
    /// 메시지
    pub message: String,
    /// 컨텍스트
    pub context: Option<String>,
}

/// 스케줄링 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineResult {
    /// 스케줄
    pub schedule: Schedule,
    /// KPI
    pub kpi: ScheduleKpi,
    /// 성능 메트릭
    pub metrics: PerformanceMetrics,
    /// 검증 경고
    pub warnings: Vec<String>,
    /// 로그
    pub logs: Vec<LogEntry>,
}

/// 통합 스케줄링 엔진
pub struct SchedulingEngine {
    config: EngineConfig,
    logs: Vec<LogEntry>,
    start_time: Option<Instant>,
}

impl SchedulingEngine {
    /// 새 엔진 생성
    pub fn new(config: EngineConfig) -> Self {
        Self {
            config,
            logs: Vec::new(),
            start_time: None,
        }
    }

    /// 기본 설정으로 생성
    pub fn default_engine() -> Self {
        Self::new(EngineConfig::default())
    }

    /// Simple 스케줄러로 생성
    pub fn simple() -> Self {
        Self::new(EngineConfig {
            algorithm: SchedulingAlgorithm::Simple,
            ..Default::default()
        })
    }

    /// GA 스케줄러로 생성
    pub fn genetic(params: GaParams) -> Self {
        Self::new(EngineConfig {
            algorithm: SchedulingAlgorithm::GeneticAlgorithm,
            ga_params: Some(params),
            ..Default::default()
        })
    }

    /// Production 스케줄러로 생성
    pub fn production(config: ProductionConfig) -> Self {
        Self::new(EngineConfig {
            algorithm: SchedulingAlgorithm::Production,
            production_config: Some(config),
            ..Default::default()
        })
    }

    /// Dynamic 스케줄러로 생성
    pub fn dynamic(config: DynamicSchedulerConfig) -> Self {
        Self::new(EngineConfig {
            algorithm: SchedulingAlgorithm::Dynamic,
            dynamic_config: Some(config),
            ..Default::default()
        })
    }

    /// CP-SAT 스케줄러로 생성
    pub fn cpsat(config: CpSatConfig) -> Self {
        Self::new(EngineConfig {
            algorithm: SchedulingAlgorithm::CpSat,
            cpsat_config: Some(config),
            ..Default::default()
        })
    }

    /// 자동 선택 스케줄러로 생성
    pub fn auto() -> Self {
        Self::new(EngineConfig {
            algorithm: SchedulingAlgorithm::Auto,
            ..Default::default()
        })
    }

    /// Hybrid 스케줄러 (CP-SAT → GA)
    pub fn hybrid() -> Self {
        Self::new(EngineConfig {
            algorithm: SchedulingAlgorithm::Hybrid,
            cpsat_config: Some(crate::cp::CpSatConfig {
                time_limit_ms: 10_000,
                ..Default::default()
            }),
            ga_params: Some(crate::ga::GaParams::balanced()),
            ..Default::default()
        })
    }

    /// Setup Matrix 설정
    pub fn with_setup_matrices(mut self, matrices: SetupMatrixCollection) -> Self {
        self.config.setup_matrices = matrices;
        self
    }

    /// 검증 비활성화
    pub fn without_validation(mut self) -> Self {
        self.config.validate_input = false;
        self
    }

    /// 상세 로깅 활성화
    pub fn with_verbose_logging(mut self) -> Self {
        self.config.verbose_logging = true;
        self
    }

    /// 타임아웃 설정
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.config.timeout_ms = Some(timeout_ms);
        self
    }

    /// 스케줄링 실행
    pub fn schedule(
        &mut self,
        jobs: &[Job],
        resources: &[Resource],
        start_time_ms: i64,
    ) -> Result<EngineResult> {
        let total_start = Instant::now();
        self.start_time = Some(total_start);
        self.logs.clear();

        self.log(LogLevel::Info, "Scheduling started", None);
        self.log(
            LogLevel::Info,
            &format!(
                "Input: {} jobs, {} resources, algorithm: {:?}",
                jobs.len(),
                resources.len(),
                self.config.algorithm
            ),
            None,
        );

        // 1. 입력 검증
        let validation_start = Instant::now();
        let validation_result = if self.config.validate_input {
            self.validate_input(jobs, resources)?
        } else {
            ValidationResult::new()
        };
        let validation_time = validation_start.elapsed().as_millis();

        // 2. 스케줄링 실행
        let scheduling_start = Instant::now();
        let schedule = self.run_scheduling(jobs, resources, start_time_ms)?;
        let scheduling_time = scheduling_start.elapsed().as_millis();

        self.log(
            LogLevel::Info,
            &format!(
                "Scheduling completed: {} assignments, makespan: {}ms",
                schedule.assignments.len(),
                schedule.makespan_ms
            ),
            None,
        );

        // 3. KPI 계산
        let kpi_start = Instant::now();
        let horizon_ms = schedule.makespan_ms.max(1);
        let kpi_calculator = KpiCalculator::new(horizon_ms);
        let kpi = kpi_calculator.calculate(&schedule, jobs);
        let kpi_time = kpi_start.elapsed().as_millis();

        // 4. 성능 메트릭 생성
        let total_time = total_start.elapsed().as_millis();
        let operation_count: usize = jobs.iter().map(|j| j.operations.len()).sum();

        let metrics = PerformanceMetrics {
            total_time_ms: total_time,
            validation_time_ms: validation_time,
            scheduling_time_ms: scheduling_time,
            kpi_time_ms: kpi_time,
            algorithm: self.config.algorithm,
            job_count: jobs.len(),
            operation_count,
            resource_count: resources.len(),
        };

        self.log(
            LogLevel::Info,
            &format!("Total execution time: {}ms", total_time),
            None,
        );

        Ok(EngineResult {
            schedule,
            kpi,
            metrics,
            warnings: validation_result.warnings,
            logs: self.logs.clone(),
        })
    }

    /// 스케줄링 및 분석
    pub fn schedule_with_analytics(
        &mut self,
        jobs: &[Job],
        resources: &[Resource],
        start_time_ms: i64,
    ) -> Result<(EngineResult, AnalyticsReport)> {
        let result = self.schedule(jobs, resources, start_time_ms)?;

        // Analytics 실행
        let analytics_engine = AnalyticsEngine::new(AnalyticsConfig {
            horizon_ms: result.schedule.makespan_ms.max(1),
            ..Default::default()
        })
        .with_schedule(result.schedule.clone(), jobs.to_vec());

        let report = analytics_engine.analyze();

        Ok((result, report))
    }

    // 내부 메서드

    fn validate_input(&mut self, jobs: &[Job], resources: &[Resource]) -> Result<ValidationResult> {
        self.log(LogLevel::Debug, "Validating input", None);

        // Operation 추출
        let operations: Vec<_> = jobs
            .iter()
            .flat_map(|j| j.operations.iter().cloned())
            .collect();

        let result = Validator::validate_schedule_request(jobs, &operations, resources);

        if !result.is_valid() {
            let error = result.errors.first().unwrap().clone();
            self.log(
                LogLevel::Error,
                &format!("Validation failed: {}", error),
                None,
            );
            return Err(error);
        }

        if !result.warnings.is_empty() {
            for warning in &result.warnings {
                self.log(LogLevel::Warn, warning, None);
            }
        }

        self.log(LogLevel::Debug, "Validation passed", None);
        Ok(result)
    }

    /// GA 파라미터 해석 — 호출자가 `ga_params`를 명시하지 않았다면 엔진 `timeout_ms`를
    /// GA의 `time_limit_ms`로 승계해 상한을 준다(명시된 `ga_params`는 그대로 존중).
    fn effective_ga_params(&self) -> GaParams {
        match &self.config.ga_params {
            Some(params) => params.clone(),
            None => GaParams {
                time_limit_ms: self.config.timeout_ms.map(|ms| ms as i64),
                ..GaParams::default()
            },
        }
    }

    /// CP-SAT 설정 해석 — 호출자가 `cpsat_config`를 명시하지 않았다면 엔진 `timeout_ms`를
    /// CP-SAT의 `time_limit_ms`로 승계해 상한을 준다(명시된 `cpsat_config`는 그대로 존중).
    fn effective_cpsat_config(&self) -> CpSatConfig {
        match &self.config.cpsat_config {
            Some(config) => config.clone(),
            None => {
                let mut config = CpSatConfig::default();
                if let Some(ms) = self.config.timeout_ms {
                    config.time_limit_ms = ms as i64;
                }
                config
            }
        }
    }

    /// Production 설정 해석 — 호출자가 `production_config`를 명시하지 않았다면 엔진
    /// `timeout_ms`를 내부 GA의 `time_limit_ms`로 승계한다(명시된 `production_config`는
    /// 그대로 존중). `ProductionScheduler`는 내부적으로 GA를 돌리므로 `ProductionConfig`가
    /// 기본값일 때(`ga_params: GaParams::default()`, `time_limit_ms: None`) 이 승계가
    /// 없으면 `SchedulingAlgorithm::Production`(엔진 기본 알고리즘)이 엔진 타임아웃과
    /// 무관하게 무제한 실행된다.
    fn effective_production_config(&self) -> ProductionConfig {
        match &self.config.production_config {
            Some(config) => config.clone(),
            None => {
                let mut config = ProductionConfig::default();
                if config.ga_params.time_limit_ms.is_none() {
                    config.ga_params.time_limit_ms = self.config.timeout_ms.map(|ms| ms as i64);
                }
                config
            }
        }
    }

    /// 선택된 알고리즘이 지원하지 않는 확장 필드가 채워져 있으면 경고 로그를 남긴다.
    /// 채워둔 값이 조용히 무시되는 것과, 로그로 드러나는 무시는 호출자 입장에서
    /// 완전히 다른 신뢰도를 준다 — 스케줄링 결과 자체는 바꾸지 않는다.
    fn warn_unsupported_extensions(&mut self, unsupported: &[(&str, bool)]) {
        let ignored: Vec<&str> = unsupported
            .iter()
            .filter(|(_, is_set)| *is_set)
            .map(|(name, _)| *name)
            .collect();
        if ignored.is_empty() {
            return;
        }
        let algorithm = self.config.algorithm;
        self.log(
            LogLevel::Warn,
            &format!(
                "{:?} does not use: {} (value set on EngineConfig but ignored by this algorithm)",
                algorithm,
                ignored.join(", ")
            ),
            None,
        );
    }

    fn run_scheduling(
        &mut self,
        jobs: &[Job],
        resources: &[Resource],
        start_time_ms: i64,
    ) -> Result<Schedule> {
        match self.config.algorithm {
            SchedulingAlgorithm::Simple => {
                self.log(LogLevel::Debug, "Using Simple scheduler", None);
                let scheduler = SimpleScheduler::new();
                let request = self.build_schedule_request(jobs, resources, start_time_ms);
                Ok(scheduler.schedule(&request))
            }

            SchedulingAlgorithm::GeneticAlgorithm => {
                self.log(LogLevel::Debug, "Using GA scheduler", None);
                self.warn_unsupported_extensions(&[
                    ("calendars", !self.config.calendars.is_empty()),
                    ("skill_matrix", self.config.skill_matrix.is_some()),
                    (
                        "worker_time_override",
                        self.config.worker_time_override.is_some(),
                    ),
                    (
                        "dispatching_config",
                        self.config.dispatching_config.is_some(),
                    ),
                    ("material_manager", self.config.material_manager.is_some()),
                    (
                        "certification_matrix",
                        self.config.certification_matrix.is_some(),
                    ),
                    ("crew_manager", self.config.crew_manager.is_some()),
                    ("pegging_engine", self.config.pegging_engine.is_some()),
                    (
                        "outsourcing_manager",
                        self.config.outsourcing_manager.is_some(),
                    ),
                ]);
                let params = self.effective_ga_params();
                let operators = GeneticOperators::default();
                let scheduler = GaScheduler::new(params, operators)
                    .with_setup_matrices(self.config.setup_matrices.clone())
                    .with_site_transitions(self.config.site_transitions.clone());
                let result = scheduler.schedule(jobs, resources, start_time_ms);
                Ok(result.schedule)
            }

            SchedulingAlgorithm::Production => {
                self.log(LogLevel::Debug, "Using Production scheduler", None);
                self.warn_unsupported_extensions(&[
                    ("calendars", !self.config.calendars.is_empty()),
                    ("skill_matrix", self.config.skill_matrix.is_some()),
                    (
                        "worker_time_override",
                        self.config.worker_time_override.is_some(),
                    ),
                    (
                        "dispatching_config",
                        self.config.dispatching_config.is_some(),
                    ),
                    ("material_manager", self.config.material_manager.is_some()),
                    (
                        "certification_matrix",
                        self.config.certification_matrix.is_some(),
                    ),
                    ("crew_manager", self.config.crew_manager.is_some()),
                    ("pegging_engine", self.config.pegging_engine.is_some()),
                    (
                        "outsourcing_manager",
                        self.config.outsourcing_manager.is_some(),
                    ),
                ]);
                let config = self.effective_production_config();
                let scheduler = ProductionScheduler::new(config)
                    .with_setup_matrices(self.config.setup_matrices.clone())
                    .with_site_transitions(self.config.site_transitions.clone());
                let result = scheduler.schedule(jobs, resources, start_time_ms);
                Ok(result.schedule)
            }

            SchedulingAlgorithm::Dynamic => {
                self.log(LogLevel::Debug, "Using Dynamic scheduler", None);
                self.warn_unsupported_extensions(&[
                    ("calendars", !self.config.calendars.is_empty()),
                    ("skill_matrix", self.config.skill_matrix.is_some()),
                    (
                        "worker_time_override",
                        self.config.worker_time_override.is_some(),
                    ),
                    (
                        "dispatching_config",
                        self.config.dispatching_config.is_some(),
                    ),
                    ("material_manager", self.config.material_manager.is_some()),
                    (
                        "certification_matrix",
                        self.config.certification_matrix.is_some(),
                    ),
                    ("crew_manager", self.config.crew_manager.is_some()),
                    (
                        "outsourcing_manager",
                        self.config.outsourcing_manager.is_some(),
                    ),
                ]);
                let config = self.config.dynamic_config.clone().unwrap_or_default();
                let mut scheduler = DynamicScheduler::new(config)
                    .with_setup_matrices(self.config.setup_matrices.clone());
                if let Some(engine) = self.config.pegging_engine.clone() {
                    scheduler = scheduler.with_pegging_engine(engine);
                }
                let result = scheduler.schedule(jobs, resources, start_time_ms);
                Ok(result.schedule)
            }

            SchedulingAlgorithm::CpSat => {
                self.log(LogLevel::Debug, "Using CP-SAT scheduler", None);
                self.warn_unsupported_extensions(&[
                    ("setup_matrices", !self.config.setup_matrices.is_empty()),
                    ("site_transitions", !self.config.site_transitions.is_empty()),
                    ("calendars", !self.config.calendars.is_empty()),
                    ("skill_matrix", self.config.skill_matrix.is_some()),
                    (
                        "worker_time_override",
                        self.config.worker_time_override.is_some(),
                    ),
                    (
                        "dispatching_config",
                        self.config.dispatching_config.is_some(),
                    ),
                    ("material_manager", self.config.material_manager.is_some()),
                    (
                        "certification_matrix",
                        self.config.certification_matrix.is_some(),
                    ),
                    ("crew_manager", self.config.crew_manager.is_some()),
                    ("pegging_engine", self.config.pegging_engine.is_some()),
                    (
                        "outsourcing_manager",
                        self.config.outsourcing_manager.is_some(),
                    ),
                ]);
                let config = self.effective_cpsat_config();
                let scheduler = CpSatScheduler::new(config);
                let result = scheduler.schedule(jobs, resources, start_time_ms);
                Ok(result.schedule)
            }

            SchedulingAlgorithm::Auto => {
                let operation_count: usize = jobs.iter().map(|j| j.operations.len()).sum();
                let selected = select_algorithm(operation_count, resources.len());
                self.log(
                    LogLevel::Info,
                    &format!(
                        "Auto-selected algorithm: {:?} (ops={}, resources={})",
                        selected,
                        operation_count,
                        resources.len()
                    ),
                    None,
                );

                let mut auto_config = self.config.clone();
                auto_config.algorithm = selected;
                auto_config.validate_input = false;
                let mut engine = SchedulingEngine::new(auto_config);
                let result = engine.run_scheduling(jobs, resources, start_time_ms);
                // `engine`은 이 분기 전용 임시 인스턴스라 그 안에서 남긴 로그(선택된
                // 알고리즘의 "Using X scheduler"/미지원 필드 경고 등)는 병합하지 않으면
                // 스코프를 벗어나며 사라진다 — 호출자에게 아무 로그도 안 보이는 결과가 됨.
                self.logs.extend(engine.logs);
                result
            }

            SchedulingAlgorithm::Hybrid => {
                self.log(LogLevel::Info, "Hybrid scheduling: CP-SAT → GA", None);
                self.warn_unsupported_extensions(&[
                    ("calendars", !self.config.calendars.is_empty()),
                    ("skill_matrix", self.config.skill_matrix.is_some()),
                    (
                        "worker_time_override",
                        self.config.worker_time_override.is_some(),
                    ),
                    (
                        "dispatching_config",
                        self.config.dispatching_config.is_some(),
                    ),
                    ("material_manager", self.config.material_manager.is_some()),
                    (
                        "certification_matrix",
                        self.config.certification_matrix.is_some(),
                    ),
                    ("crew_manager", self.config.crew_manager.is_some()),
                    ("pegging_engine", self.config.pegging_engine.is_some()),
                    (
                        "outsourcing_manager",
                        self.config.outsourcing_manager.is_some(),
                    ),
                ]);

                // Phase 1: CP-SAT with short time limit
                let cpsat_config = self.effective_cpsat_config();
                let cpsat_scheduler = CpSatScheduler::new(cpsat_config);
                let cpsat_result = cpsat_scheduler.schedule(jobs, resources, start_time_ms);

                let initial_makespan = cpsat_result.schedule.makespan_ms;
                self.log(
                    LogLevel::Info,
                    &format!("CP-SAT phase complete: makespan={}ms", initial_makespan),
                    None,
                );

                // Phase 2: GA refinement
                let ga_params = self.effective_ga_params();
                let ga_scheduler = GaScheduler::new(ga_params, GeneticOperators::default())
                    .with_setup_matrices(self.config.setup_matrices.clone())
                    .with_site_transitions(self.config.site_transitions.clone());
                let ga_result = ga_scheduler.schedule(jobs, resources, start_time_ms);

                let final_makespan = ga_result.schedule.makespan_ms;
                self.log(
                    LogLevel::Info,
                    &format!(
                        "GA phase complete: makespan={}ms (improvement: {}%)",
                        final_makespan,
                        if initial_makespan > 0 {
                            ((initial_makespan - final_makespan) as f64 / initial_makespan as f64
                                * 100.0) as i32
                        } else {
                            0
                        }
                    ),
                    None,
                );

                // Return better result
                if ga_result.schedule.makespan_ms <= cpsat_result.schedule.makespan_ms {
                    Ok(ga_result.schedule)
                } else {
                    Ok(cpsat_result.schedule)
                }
            }
        }
    }

    /// Builds a ScheduleRequest from EngineConfig, forwarding all extension fields.
    fn build_schedule_request(
        &self,
        jobs: &[Job],
        resources: &[Resource],
        start_time_ms: i64,
    ) -> ScheduleRequest {
        ScheduleRequest {
            jobs: jobs.to_vec(),
            resources: resources.to_vec(),
            start_time_ms,
            setup_matrices: self.config.setup_matrices.clone(),
            calendars: self.config.calendars.clone(),
            skill_matrix: self.config.skill_matrix.clone(),
            worker_time_override: self.config.worker_time_override.clone(),
            dispatching_config: self.config.dispatching_config.clone(),
            material_manager: self.config.material_manager.clone(),
            certification_matrix: self.config.certification_matrix.clone(),
            crew_manager: self.config.crew_manager.clone(),
            pegging_engine: self.config.pegging_engine.clone(),
            outsourcing_manager: self.config.outsourcing_manager.clone(),
            site_transitions: Some(self.config.site_transitions.clone()),
        }
    }

    fn log(&mut self, level: LogLevel, message: &str, context: Option<&str>) {
        if !self.config.verbose_logging && matches!(level, LogLevel::Debug) {
            return;
        }

        let timestamp = self
            .start_time
            .map(|s| s.elapsed().as_millis())
            .unwrap_or(0);

        let level_str = match level {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        };

        self.logs.push(LogEntry {
            timestamp_ms: timestamp,
            level: level_str.to_string(),
            message: message.to_string(),
            context: context.map(|s| s.to_string()),
        });
    }
}

/// 문제 특성 기반 알고리즘 자동 선택
fn select_algorithm(operation_count: usize, resource_count: usize) -> SchedulingAlgorithm {
    let complexity = operation_count * resource_count;

    if complexity <= 50 {
        SchedulingAlgorithm::CpSat
    } else if complexity <= 500 {
        SchedulingAlgorithm::Production
    } else {
        SchedulingAlgorithm::GeneticAlgorithm
    }
}

impl Default for SchedulingEngine {
    fn default() -> Self {
        Self::default_engine()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Operation, SetupMatrix};

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
            Job::new("J3")
                .with_product("PRODUCT-A")
                .with_priority(3)
                .with_operation(
                    Operation::new("J3-O1", "J3", 1)
                        .with_time(0, 20_000, 0)
                        .with_equipment(vec!["M1".to_string()]),
                ),
        ];

        let resources = vec![
            Resource::equipment("M1").with_efficiency(1.0),
            Resource::equipment("M2").with_efficiency(0.9),
        ];

        (jobs, resources)
    }

    #[test]
    fn test_simple_scheduling() {
        let (jobs, resources) = create_test_scenario();
        let mut engine = SchedulingEngine::simple();

        let result = engine.schedule(&jobs, &resources, 0).unwrap();

        assert!(!result.schedule.assignments.is_empty());
        assert!(result.kpi.makespan_ms > 0);
        assert_eq!(result.metrics.job_count, 3);
    }

    #[test]
    fn test_production_scheduling() {
        let (jobs, resources) = create_test_scenario();
        let mut engine = SchedulingEngine::default();

        let result = engine.schedule(&jobs, &resources, 0).unwrap();

        assert!(!result.schedule.assignments.is_empty());
        assert_eq!(result.metrics.algorithm, SchedulingAlgorithm::Production);
    }

    #[test]
    fn test_with_analytics() {
        let (jobs, resources) = create_test_scenario();
        let mut engine = SchedulingEngine::default();

        let (result, report) = engine
            .schedule_with_analytics(&jobs, &resources, 0)
            .unwrap();

        assert!(!result.schedule.assignments.is_empty());
        assert!(report.kpi.makespan_ms > 0);
    }

    #[test]
    fn test_verbose_logging() {
        let (jobs, resources) = create_test_scenario();
        let mut engine = SchedulingEngine::default().with_verbose_logging();

        let result = engine.schedule(&jobs, &resources, 0).unwrap();

        // DEBUG 로그도 포함
        assert!(result.logs.len() > 3);
        assert!(result.logs.iter().any(|l| l.level == "DEBUG"));
    }

    #[test]
    fn test_without_validation() {
        let (jobs, resources) = create_test_scenario();
        let mut engine = SchedulingEngine::default().without_validation();

        let result = engine.schedule(&jobs, &resources, 0).unwrap();

        assert_eq!(result.metrics.validation_time_ms, 0);
    }

    #[test]
    fn test_validation_failure() {
        let jobs = vec![
            Job::new(""), // Invalid: empty ID
        ];
        let resources = vec![Resource::equipment("M1")];

        let mut engine = SchedulingEngine::default();
        let result = engine.schedule(&jobs, &resources, 0);

        assert!(result.is_err());
    }

    #[test]
    fn test_ga_scheduling() {
        let (jobs, resources) = create_test_scenario();
        let params = GaParams {
            population_size: 10,
            max_generations: 5,
            ..Default::default()
        };
        let mut engine = SchedulingEngine::genetic(params);

        let result = engine.schedule(&jobs, &resources, 0).unwrap();

        assert!(!result.schedule.assignments.is_empty());
        assert_eq!(
            result.metrics.algorithm,
            SchedulingAlgorithm::GeneticAlgorithm
        );
    }

    #[test]
    fn test_dynamic_scheduling() {
        let (jobs, resources) = create_test_scenario();
        let mut engine = SchedulingEngine::dynamic(DynamicSchedulerConfig::default());

        let result = engine.schedule(&jobs, &resources, 0).unwrap();

        assert!(!result.schedule.assignments.is_empty());
        assert_eq!(result.metrics.algorithm, SchedulingAlgorithm::Dynamic);
    }

    #[test]
    fn test_performance_metrics() {
        let (jobs, resources) = create_test_scenario();
        let mut engine = SchedulingEngine::default();

        let result = engine.schedule(&jobs, &resources, 0).unwrap();

        let metrics = &result.metrics;
        assert!(metrics.total_time_ms >= metrics.scheduling_time_ms);
        assert_eq!(metrics.operation_count, 3);
        assert_eq!(metrics.resource_count, 2);
    }

    #[test]
    fn test_cpsat_scheduling() {
        let (jobs, resources) = create_test_scenario();
        let mut engine = SchedulingEngine::cpsat(crate::cp::CpSatConfig::default());

        let result = engine.schedule(&jobs, &resources, 0).unwrap();

        assert!(!result.schedule.assignments.is_empty());
        assert_eq!(result.metrics.algorithm, SchedulingAlgorithm::CpSat);
    }

    #[test]
    fn test_cpsat_algorithm_warns_when_setup_matrices_set() {
        // 회귀 가드(C193): `SetupMatrixCollection::is_empty()` 신설로 CP-SAT 분기의
        // "설정됨-그러나-무시됨" 경고가 setup_matrices도 감지하는지 확인.
        let (jobs, resources) = create_test_scenario();
        let matrices = SetupMatrixCollection::new().with_matrix(SetupMatrix::new("EQP-001"));
        let mut engine = SchedulingEngine::cpsat(crate::cp::CpSatConfig::default())
            .with_setup_matrices(matrices);
        let result = engine.schedule(&jobs, &resources, 0).unwrap();

        assert!(
            result
                .logs
                .iter()
                .any(|l| l.level == "WARN" && l.message.contains("setup_matrices")),
            "expected a WARN log naming setup_matrices as unsupported, got: {:?}",
            result.logs
        );
    }

    #[test]
    fn test_auto_selection() {
        // 소규모 문제: CpSat 선택됨
        let (jobs, resources) = create_test_scenario();
        let mut engine = SchedulingEngine::auto();

        let result = engine.schedule(&jobs, &resources, 0).unwrap();

        assert!(!result.schedule.assignments.is_empty());
        // Auto는 내부적으로 다른 알고리즘을 선택하므로 Auto가 아닌 선택된 알고리즘이 표시됨
        assert!(result
            .logs
            .iter()
            .any(|l| l.message.contains("Auto-selected")));
    }

    #[test]
    fn test_select_algorithm_heuristic() {
        // 소규모 (complexity <= 50): CpSat
        assert_eq!(select_algorithm(5, 5), SchedulingAlgorithm::CpSat);
        assert_eq!(select_algorithm(10, 5), SchedulingAlgorithm::CpSat);

        // 중규모 (50 < complexity <= 500): Production
        assert_eq!(select_algorithm(20, 10), SchedulingAlgorithm::Production);
        assert_eq!(select_algorithm(50, 10), SchedulingAlgorithm::Production);

        // 대규모 (complexity > 500): GA
        assert_eq!(
            select_algorithm(100, 10),
            SchedulingAlgorithm::GeneticAlgorithm
        );
    }
    #[test]
    fn test_hybrid_scheduling() {
        let jobs = vec![
            Job::new("J1").with_operation(
                Operation::new("O1", "J1", 1)
                    .with_time(0, 3000, 0)
                    .with_equipment(vec!["M1".to_string()]),
            ),
            Job::new("J2").with_operation(
                Operation::new("O2", "J2", 1)
                    .with_time(0, 2000, 0)
                    .with_equipment(vec!["M1".to_string()]),
            ),
        ];

        let resources = vec![Resource::equipment("M1")];

        let mut engine = SchedulingEngine::hybrid();
        let result = engine.schedule(&jobs, &resources, 0);

        assert!(result.is_ok());
        let schedule = result.unwrap();
        assert!(schedule.schedule.makespan_ms > 0);
    }

    // 회귀 가드: `timeout_ms`는 한때 저장만 되고 어디서도 읽히지 않는 죽은 필드였다
    // (GA/Production/CP-SAT/Hybrid가 무제한 실행). `effective_*` 헬퍼가 명시적 하위
    // 설정이 없을 때만 엔진 타임아웃을 승계하고, 명시된 설정은 그대로 존중하는지 고정한다.

    #[test]
    fn test_effective_ga_params_falls_back_to_engine_timeout() {
        let config = EngineConfig {
            algorithm: SchedulingAlgorithm::GeneticAlgorithm,
            timeout_ms: Some(5_000),
            ga_params: None,
            ..Default::default()
        };
        let engine = SchedulingEngine::new(config);

        assert_eq!(engine.effective_ga_params().time_limit_ms, Some(5_000));
    }

    #[test]
    fn test_effective_ga_params_respects_explicit_override() {
        let config = EngineConfig {
            algorithm: SchedulingAlgorithm::GeneticAlgorithm,
            timeout_ms: Some(5_000),
            ga_params: Some(GaParams {
                time_limit_ms: Some(999),
                ..GaParams::default()
            }),
            ..Default::default()
        };
        let engine = SchedulingEngine::new(config);

        assert_eq!(engine.effective_ga_params().time_limit_ms, Some(999));
    }

    #[test]
    fn test_effective_ga_params_no_engine_timeout_stays_unbounded() {
        let config = EngineConfig {
            algorithm: SchedulingAlgorithm::GeneticAlgorithm,
            timeout_ms: None,
            ga_params: None,
            ..Default::default()
        };
        let engine = SchedulingEngine::new(config);

        assert_eq!(engine.effective_ga_params().time_limit_ms, None);
    }

    #[test]
    fn test_effective_cpsat_config_falls_back_to_engine_timeout() {
        let config = EngineConfig {
            algorithm: SchedulingAlgorithm::CpSat,
            timeout_ms: Some(5_000),
            cpsat_config: None,
            ..Default::default()
        };
        let engine = SchedulingEngine::new(config);

        assert_eq!(engine.effective_cpsat_config().time_limit_ms, 5_000);
    }

    #[test]
    fn test_effective_cpsat_config_respects_explicit_override() {
        let config = EngineConfig {
            algorithm: SchedulingAlgorithm::CpSat,
            timeout_ms: Some(5_000),
            cpsat_config: Some(crate::cp::CpSatConfig {
                time_limit_ms: 999,
                ..crate::cp::CpSatConfig::default()
            }),
            ..Default::default()
        };
        let engine = SchedulingEngine::new(config);

        assert_eq!(engine.effective_cpsat_config().time_limit_ms, 999);
    }

    #[test]
    fn test_effective_production_config_falls_back_to_engine_timeout() {
        let config = EngineConfig {
            algorithm: SchedulingAlgorithm::Production,
            timeout_ms: Some(5_000),
            production_config: None,
            ..Default::default()
        };
        let engine = SchedulingEngine::new(config);

        assert_eq!(
            engine.effective_production_config().ga_params.time_limit_ms,
            Some(5_000)
        );
    }

    #[test]
    fn test_effective_production_config_respects_explicit_override() {
        let config = EngineConfig {
            algorithm: SchedulingAlgorithm::Production,
            timeout_ms: Some(5_000),
            production_config: Some(ProductionConfig {
                ga_params: GaParams {
                    time_limit_ms: Some(999),
                    ..GaParams::default()
                },
                ..ProductionConfig::default()
            }),
            ..Default::default()
        };
        let engine = SchedulingEngine::new(config);

        assert_eq!(
            engine.effective_production_config().ga_params.time_limit_ms,
            Some(999)
        );
    }

    #[test]
    fn test_dynamic_algorithm_forwards_pegging_engine() {
        // 회귀 가드: `EngineConfig.pegging_engine`가 `SchedulingAlgorithm::Dynamic`
        // 경로에서 한때 조용히 버려지고 있었다 — `DynamicScheduler`는 자재 페깅을
        // 지원하지만 `run_scheduling`이 `.with_pegging_engine()`을 호출하지 않았음.
        use crate::scheduler::pegging::{
            MaterialDemand, MaterialSupply, PeggingMaterial, SupplyType,
        };

        let jobs = vec![Job::new("J1").with_operation(
            Operation::new("J1-O1", "J1", 1)
                .with_time(0, 30_000, 0)
                .with_equipment(vec!["M1".to_string()]),
        )];
        let resources = vec![Resource::equipment("M1")];

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
            available_at_ms: 50_000, // 자재가 50초 후에나 도착
            supply_type: SupplyType::PurchaseOrder,
        });
        pegging_engine.add_demand(MaterialDemand {
            id: "dem1".into(),
            material_id: "mat1".into(),
            quantity: 10.0,
            required_at_ms: 0,
            operation_id: "J1-O1".into(),
            job_id: "J1".into(),
        });

        let config = EngineConfig {
            algorithm: SchedulingAlgorithm::Dynamic,
            pegging_engine: Some(pegging_engine),
            ..Default::default()
        };
        let mut engine = SchedulingEngine::new(config);
        let result = engine.schedule(&jobs, &resources, 0).unwrap();

        // 자재가 50초 후에나 도착하므로, 페깅이 실제로 적용됐다면 배정 시작시각이
        // 0ms일 수 없다 — 배선이 빠져 있었다면 이 검증 없이 0ms로 배정됐을 것.
        let assignment = result
            .schedule
            .assignments
            .iter()
            .find(|a| a.operation_id == "J1-O1")
            .expect("J1-O1 should be assigned");
        assert!(
            assignment.start_ms >= 50_000,
            "material-constrained operation should be delayed until supply arrives, got start_ms={}",
            assignment.start_ms
        );
    }

    #[test]
    fn test_default_engine_config_has_bounded_timeout() {
        // 문서화된 "1분 기본 타임아웃"이 실제로 존재하는지 고정 — 이 값이 None이 되는
        // 순간 위 폴백 체인 전체가 조용히 무력화된다.
        assert_eq!(
            EngineConfig::default().timeout_ms,
            Some(DEFAULT_ENGINE_TIMEOUT_MS)
        );
    }

    // 회귀 가드: "설정됨-그러나-무시됨" 확장 필드 경고 로그.

    #[test]
    fn test_ga_algorithm_warns_when_unsupported_field_set() {
        let (jobs, resources) = create_test_scenario();
        let config = EngineConfig {
            algorithm: SchedulingAlgorithm::GeneticAlgorithm,
            skill_matrix: Some(SkillMatrix::new()),
            ..Default::default()
        };
        let mut engine = SchedulingEngine::new(config);
        let result = engine.schedule(&jobs, &resources, 0).unwrap();

        assert!(
            result
                .logs
                .iter()
                .any(|l| l.level == "WARN" && l.message.contains("skill_matrix")),
            "expected a WARN log naming skill_matrix as unsupported, got: {:?}",
            result.logs
        );
    }

    #[test]
    fn test_ga_algorithm_no_warning_when_nothing_unsupported_set() {
        let (jobs, resources) = create_test_scenario();
        let config = EngineConfig {
            algorithm: SchedulingAlgorithm::GeneticAlgorithm,
            ..Default::default()
        };
        let mut engine = SchedulingEngine::new(config);
        let result = engine.schedule(&jobs, &resources, 0).unwrap();

        assert!(
            !result
                .logs
                .iter()
                .any(|l| l.message.contains("does not use")),
            "no extension fields were set, so no such warning should fire: {:?}",
            result.logs
        );
    }

    #[test]
    fn test_auto_merges_inner_engine_logs() {
        // 회귀 가드: `Auto`는 내부적으로 새 `SchedulingEngine`을 만들어 위임한다 — 그
        // 내부 엔진이 남긴 로그(예: 이 경고)를 병합하지 않으면 호출자에게는 아무
        // 로그도 보이지 않는다.
        let (jobs, resources) = create_test_scenario();
        let config = EngineConfig {
            algorithm: SchedulingAlgorithm::Auto,
            skill_matrix: Some(SkillMatrix::new()),
            ..Default::default()
        };
        let mut engine = SchedulingEngine::new(config);
        let result = engine.schedule(&jobs, &resources, 0).unwrap();

        assert!(
            result
                .logs
                .iter()
                .any(|l| l.level == "WARN" && l.message.contains("skill_matrix")),
            "Auto delegates to an inner engine — its warnings must surface in the outer result: {:?}",
            result.logs
        );
    }
}
