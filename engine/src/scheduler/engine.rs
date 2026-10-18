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
    dynamic::DynamicSchedulerConfig,
    kpi::{KpiCalculator, ScheduleKpi},
    pegging::PeggingEngine,
    production::ProductionConfig,
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
    /// 유전 알고리즘과 같은 탐색 — 예산은 `ProductionConfig.ga_params` (엔진 기본값)
    Production,
    /// 유전 알고리즘과 같은 탐색 — 예산은 `DynamicSchedulerConfig` 의 것.
    /// 페깅된 자재 가용 시각은 모든 알고리즘에서 릴리스 시각이다
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
        let mut schedule = self.run_scheduling(jobs, resources, start_time_ms)?;
        // The other algorithms do not read the whole request: say where their
        // schedule breaks it, judged as the default scheduler states it.
        if self.config.algorithm != SchedulingAlgorithm::Simple {
            let request = self.build_schedule_request(jobs, resources, start_time_ms);
            super::placement::report_breaks(&request, &mut schedule);
        }
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
    /// 그대로 존중). Production 은 탐색을 돌리므로 `ProductionConfig`가
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

    /// A search over the default scheduler's order within `params`' budget,
    /// or — for a request that splits or overlaps operations — this engine's
    /// own GA. The genetic algorithm, Production and Dynamic all run here:
    /// each differed from the others only in steps that left the schedule as
    /// the GA made it (Production's campaign order, which the GA re-sorted —
    /// the campaign planner is gone)
    /// or broke it (Dynamic's shift of each assignment to its material
    /// arrival, which now reaches every algorithm as a release time).
    fn run_ga(
        &mut self,
        jobs: &[Job],
        resources: &[Resource],
        start_time_ms: i64,
        params: GaParams,
    ) -> Result<Schedule> {
        // TODO(upstream: lot splitting and overlap on u-schedule —
        // ISSUE-u-aps-20261018-algorithm-paths-ignore-requirements):
        // only this engine's own GA splits an operation across
        // machines or overlaps it with the one before, and it holds
        // one machine per operation (no workers, calendars or
        // releases). Remove once u-schedule states both.
        let splits_or_overlaps = jobs
            .iter()
            .flat_map(|j| &j.operations)
            .any(|op| op.is_splittable || op.overlap_config.is_some());
        if !splits_or_overlaps {
            let request = self.build_schedule_request(jobs, resources, start_time_ms);
            return Ok(SimpleScheduler::new().search(&request, &params)?);
        }
        self.log(
            LogLevel::Warn,
            &format!(
                "{:?} splits or overlaps operations with the previous genetic algorithm, \
                 which holds one machine per operation: worker requirements, calendars \
                 and release times are not applied",
                self.config.algorithm
            ),
            None,
        );
        let operators = GeneticOperators::default();
        let scheduler = GaScheduler::new(params, operators)
            .with_setup_matrices(self.config.setup_matrices.clone())
            .with_site_transitions(self.config.site_transitions.clone());
        let result = scheduler.schedule(jobs, resources, start_time_ms);
        Ok(result.schedule)
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
                let request = self.build_schedule_request(jobs, resources, start_time_ms);
                Ok(SimpleScheduler::new().schedule(&request)?)
            }

            SchedulingAlgorithm::GeneticAlgorithm => {
                self.log(LogLevel::Debug, "Using GA scheduler", None);
                let params = self.effective_ga_params();
                self.run_ga(jobs, resources, start_time_ms, params)
            }

            SchedulingAlgorithm::Production => {
                self.log(LogLevel::Debug, "Using Production scheduler", None);
                let params = self.effective_production_config().ga_params;
                self.run_ga(jobs, resources, start_time_ms, params)
            }

            SchedulingAlgorithm::Dynamic => {
                self.log(LogLevel::Debug, "Using Dynamic scheduler", None);
                let mut params = self
                    .config
                    .dynamic_config
                    .as_ref()
                    .map(|c| c.production_config.ga_params.clone())
                    .unwrap_or_default();
                if params.time_limit_ms.is_none() {
                    params.time_limit_ms = self.config.timeout_ms.map(|ms| ms as i64);
                }
                self.run_ga(jobs, resources, start_time_ms, params)
            }

            SchedulingAlgorithm::CpSat => {
                self.log(LogLevel::Debug, "Using CP-SAT scheduler", None);
                let unstated = cp_unstated(jobs, resources, &self.config);
                if !unstated.is_empty() {
                    self.log(
                        LogLevel::Warn,
                        &format!(
                            "CpSat states each operation as its processing time on one machine; \
                             this request also has {}, which its schedule does not keep \
                             (reported as violations)",
                            unstated.join(", ")
                        ),
                        None,
                    );
                }
                let config = self.effective_cpsat_config();
                let scheduler = CpSatScheduler::new(config);
                let result = scheduler.schedule(jobs, resources, start_time_ms);
                Ok(result.schedule)
            }

            SchedulingAlgorithm::Auto => {
                let operation_count: usize = jobs.iter().map(|j| j.operations.len()).sum();
                let selected = select_algorithm(
                    operation_count,
                    resources.len(),
                    cp_unstated(jobs, resources, &self.config).is_empty(),
                );
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

                // Phase 1: CP-SAT, when its model states the whole request —
                // otherwise its schedule breaks the request and is never kept.
                let unstated = cp_unstated(jobs, resources, &self.config);
                if !unstated.is_empty() {
                    self.log(
                        LogLevel::Info,
                        &format!(
                            "CP-SAT phase skipped: the request has {}",
                            unstated.join(", ")
                        ),
                        None,
                    );
                    let params = self.effective_ga_params();
                    return self.run_ga(jobs, resources, start_time_ms, params);
                }
                let cpsat_config = self.effective_cpsat_config();
                let cpsat_scheduler = CpSatScheduler::new(cpsat_config);
                let cpsat_result = cpsat_scheduler.schedule(jobs, resources, start_time_ms);

                let initial_makespan = cpsat_result.schedule.makespan_ms;
                self.log(
                    LogLevel::Info,
                    &format!("CP-SAT phase complete: makespan={}ms", initial_makespan),
                    None,
                );

                // Phase 2: the GA
                let params = self.effective_ga_params();
                let ga_schedule = self.run_ga(jobs, resources, start_time_ms, params)?;

                let final_makespan = ga_schedule.makespan_ms;
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

                // The one that keeps more of the request, then the shorter.
                // CP-SAT reads one machine per operation and may find nothing
                // in its time: its schedule then breaks the request, and its
                // makespan of 0 is no reason to take it.
                let request = self.build_schedule_request(jobs, resources, start_time_ms);
                let score =
                    |s: &Schedule| (super::placement::breaks(&request, s).len(), s.makespan_ms);
                if score(&ga_schedule) <= score(&cpsat_result.schedule) {
                    Ok(ga_schedule)
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

/// What a request has that the CP-SAT model does not state — it states each
/// operation as its processing time on the first candidate of its first
/// requirement, in its job's list order, from the start time. Empty when the
/// model is the whole request (a plain job shop).
fn cp_unstated(jobs: &[Job], resources: &[Resource], config: &EngineConfig) -> Vec<&'static str> {
    use crate::ResourceType;
    let ops = || jobs.iter().flat_map(|j| &j.operations);
    let mut has = Vec::new();
    let mut note = |what: &'static str, yes: bool| {
        if yes {
            has.push(what);
        }
    };
    note(
        "alternative or several resources",
        ops().any(|o| {
            o.required_resources.len() > 1
                || o.required_resources.iter().any(|r| r.candidates.len() != 1)
        }),
    );
    note(
        "worker requirements",
        ops().any(|o| {
            o.required_resources
                .iter()
                .any(|r| r.resource_type == ResourceType::Worker)
        }),
    );
    note(
        "operations without resources",
        ops().any(|o| o.required_resources.is_empty()),
    );
    note(
        "set-up or wait times",
        ops().any(|o| o.time.setup_ms > 0 || o.time.wait_ms > 0),
    );
    note(
        "operations out of sequence order",
        jobs.iter().any(|j| {
            j.operations
                .windows(2)
                .any(|w| w[0].sequence > w[1].sequence)
        }),
    );
    note(
        "release times or time windows",
        jobs.iter().any(|j| j.earliest_start.is_some()) || ops().any(|o| o.time_window.is_some()),
    );
    note("dependencies", ops().any(|o| !o.dependencies.is_empty()));
    note(
        "daily work windows",
        ops().any(|o| o.daily_work_window.is_some()),
    );
    note(
        "splitting or overlap",
        ops().any(|o| o.is_splittable || o.overlap_config.is_some()),
    );
    note(
        "materials",
        ops().any(|o| !o.material_requirements.is_empty())
            || config.material_manager.is_some()
            || config.pegging_engine.is_some(),
    );
    note(
        "outsourcing",
        ops().any(|o| o.outsourcing_config.is_some()) || config.outsourcing_manager.is_some(),
    );
    note("crews", ops().any(|o| o.crew_id.is_some()));
    note(
        "target sites",
        jobs.iter().any(|j| j.target_site_id.is_some()),
    );
    note(
        "resource efficiencies or capacities",
        resources
            .iter()
            .any(|r| r.efficiency != 1.0 || r.capacity > 1),
    );
    note(
        "calendars",
        !config.calendars.is_empty() || resources.iter().any(|r| r.calendar_id.is_some()),
    );
    note("set-up matrices", !config.setup_matrices.is_empty());
    note("site transitions", !config.site_transitions.is_empty());
    note(
        "skill or certification matrices",
        config.skill_matrix.is_some()
            || config.certification_matrix.is_some()
            || config.worker_time_override.is_some(),
    );
    has
}

/// 문제 특성 기반 알고리즘 자동 선택 — CP-SAT 는 그 모델이 요청 전부를 말할 때만
/// (`cp_unstated` 가 빔), 아니면 같은 크기에서 탐색.
fn select_algorithm(
    operation_count: usize,
    resource_count: usize,
    cp_fits: bool,
) -> SchedulingAlgorithm {
    let complexity = operation_count * resource_count;

    if complexity <= 50 && cp_fits {
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
                .any(|l| l.level == "WARN" && l.message.contains("set-up matrices")),
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
        // 소규모 (complexity <= 50): CpSat, when its model is the whole request
        assert_eq!(select_algorithm(5, 5, true), SchedulingAlgorithm::CpSat);
        assert_eq!(select_algorithm(10, 5, true), SchedulingAlgorithm::CpSat);
        assert_eq!(
            select_algorithm(5, 5, false),
            SchedulingAlgorithm::Production
        );

        // 중규모 (50 < complexity <= 500): Production
        assert_eq!(
            select_algorithm(20, 10, true),
            SchedulingAlgorithm::Production
        );
        assert_eq!(
            select_algorithm(50, 10, true),
            SchedulingAlgorithm::Production
        );

        // 대규모 (complexity > 500): GA
        assert_eq!(
            select_algorithm(100, 10, true),
            SchedulingAlgorithm::GeneticAlgorithm
        );
    }
    #[test]
    fn cp_sat_is_chosen_or_tried_only_for_a_request_its_model_states() {
        let plain = vec![Job::new("J1").with_operation(
            Operation::new("O1", "J1", 1)
                .with_time(0, 3000, 0)
                .with_equipment(vec!["M1".to_string()]),
        )];
        let resources = vec![Resource::equipment("M1"), Resource::worker("W1")];
        assert!(cp_unstated(&plain, &resources[..1], &EngineConfig::default()).is_empty());
        let mut crewed = plain.clone();
        crewed[0].operations[0] = crewed[0].operations[0].clone().with_workers(1);
        assert_eq!(
            cp_unstated(&crewed, &resources, &EngineConfig::default()),
            ["alternative or several resources", "worker requirements"]
        );

        let quick = Some(GaParams {
            max_generations: 5,
            ..GaParams::default()
        });
        for (algorithm, expected) in [
            (
                SchedulingAlgorithm::Auto,
                "Auto-selected algorithm: Production",
            ),
            (SchedulingAlgorithm::Hybrid, "CP-SAT phase skipped"),
        ] {
            let mut engine = SchedulingEngine::new(EngineConfig {
                algorithm,
                ga_params: quick.clone(),
                production_config: Some(ProductionConfig {
                    ga_params: quick.clone().expect("set"),
                }),
                ..Default::default()
            });
            let result = engine.schedule(&crewed, &resources, 0).unwrap();
            assert!(
                result.logs.iter().any(|l| l.message.contains(expected)),
                "{algorithm:?}: {:?}",
                result.logs
            );
            assert!(result.schedule.violations.is_empty(), "{algorithm:?}");
        }
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
        // 회귀 가드: `EngineConfig.pegging_engine`가 한때 조용히 버려졌다 — 이제
        // 어댑터가 페깅 가용 시각을 모든 배치 알고리즘의 릴리스 시각으로 쓴다.
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

        // The pegged material arrives at 50 s: every algorithm that places
        // on the default scheduler waits for it (a release time).
        for algorithm in [
            SchedulingAlgorithm::Simple,
            SchedulingAlgorithm::GeneticAlgorithm,
            SchedulingAlgorithm::Dynamic,
        ] {
            let config = EngineConfig {
                algorithm,
                pegging_engine: Some(pegging_engine.clone()),
                ga_params: Some(GaParams {
                    max_generations: 5,
                    ..GaParams::default()
                }),
                ..Default::default()
            };
            let mut engine = SchedulingEngine::new(config);
            let result = engine.schedule(&jobs, &resources, 0).unwrap();
            let assignment = result
                .schedule
                .assignments
                .iter()
                .find(|a| a.operation_id == "J1-O1")
                .expect("J1-O1 should be assigned");
            assert_eq!(assignment.start_ms, 50_000, "{algorithm:?}");
            assert!(result.schedule.violations.is_empty(), "{algorithm:?}");
        }
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
    fn the_searching_algorithms_read_the_extension_fields_without_warning() {
        // The GA, Production and Dynamic search the default scheduler's
        // order, which reads the skill matrix: nothing to warn about.
        for algorithm in [
            SchedulingAlgorithm::GeneticAlgorithm,
            SchedulingAlgorithm::Production,
            SchedulingAlgorithm::Dynamic,
        ] {
            let (jobs, resources) = create_test_scenario();
            let config = EngineConfig {
                algorithm,
                skill_matrix: Some(SkillMatrix::new()),
                ga_params: Some(GaParams {
                    max_generations: 5,
                    ..GaParams::default()
                }),
                ..Default::default()
            };
            let mut engine = SchedulingEngine::new(config);
            let result = engine.schedule(&jobs, &resources, 0).unwrap();
            assert!(
                !result.logs.iter().any(|l| l.level == "WARN"),
                "{algorithm:?}: {:?}",
                result.logs
            );
        }
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
        // A splittable operation: the selected search warns that the
        // previous genetic algorithm takes the request.
        let (mut jobs, resources) = create_test_scenario();
        jobs[0].operations[0].is_splittable = true;
        let config = EngineConfig {
            algorithm: SchedulingAlgorithm::Auto,
            ga_params: Some(GaParams {
                max_generations: 5,
                ..GaParams::default()
            }),
            ..Default::default()
        };
        let mut engine = SchedulingEngine::new(config);
        let result = engine.schedule(&jobs, &resources, 0).unwrap();

        assert!(
            result
                .logs
                .iter()
                .any(|l| l.level == "WARN" && l.message.contains("splits or overlaps")),
            "Auto delegates to an inner engine — its warnings must surface in the outer result: {:?}",
            result.logs
        );
    }
}
