//! SimpleScheduler - 기본 스케줄러 (Phase 2)
//!
//! 단순한 FIFO 방식 스케줄링
//! - Job 우선순위 순으로 처리
//! - 각 Operation을 순차적으로 할당
//! - 자원 가용성 확인 후 가장 빠른 시점에 할당
//!
//! Phase 2.1 기능:
//! - ManLoadFactor: 작업자 부하계수 기반 동시 작업 지원
//! - RequiresOperator: 무인공정 지원
//! - LinkedResource: 로봇-설비 연동 자원 동시 점유

use std::collections::HashMap;

use super::pegging::PeggingEngine;
use super::{Assignment, Schedule, Violation};
use crate::{
    Calendar, CertificationMatrix, CrewManager, Job, MaterialManager, OutsourcingManager, Resource,
    ResourceType, SetupMatrixCollection, SiteTransitions, SkillMatrix, WorkerTimeOverride,
};
use chrono::{DateTime, Utc};

/// 작업자 부하 추적 (load_factor 기반 동시 작업 지원)
#[derive(Debug, Clone)]
struct WorkerLoadTracker {
    /// 작업자별 시간대별 부하 [(시작, 종료, 부하)]
    load_slots: HashMap<String, Vec<(i64, i64, f64)>>,
}

impl WorkerLoadTracker {
    fn new() -> Self {
        Self {
            load_slots: HashMap::new(),
        }
    }

    /// 특정 시간대에 작업자가 추가 부하를 수용할 수 있는지 확인
    fn can_accept_load(
        &self,
        worker_id: &str,
        start_ms: i64,
        end_ms: i64,
        load_factor: f64,
    ) -> bool {
        if let Some(slots) = self.load_slots.get(worker_id) {
            // 해당 시간대의 총 부하 계산
            let total_load: f64 = slots
                .iter()
                .filter(|(s, e, _)| *s < end_ms && *e > start_ms) // 겹치는 슬롯
                .map(|(_, _, load)| load)
                .sum();

            total_load + load_factor <= 1.0 + f64::EPSILON
        } else {
            true // 아직 할당된 작업 없음
        }
    }

    /// 작업자에게 부하 추가
    fn add_load(&mut self, worker_id: &str, start_ms: i64, end_ms: i64, load_factor: f64) {
        self.load_slots
            .entry(worker_id.to_string())
            .or_default()
            .push((start_ms, end_ms, load_factor));
    }

    /// 특정 시간 이후로 작업자가 부하를 수용할 수 있는 가장 빠른 시간 찾기
    fn find_earliest_available(
        &self,
        worker_id: &str,
        after_ms: i64,
        duration_ms: i64,
        load_factor: f64,
    ) -> i64 {
        let mut candidate_start = after_ms;

        // 최대 100회 반복으로 가용 시간 찾기
        for _ in 0..100 {
            let candidate_end = candidate_start + duration_ms;
            if self.can_accept_load(worker_id, candidate_start, candidate_end, load_factor) {
                return candidate_start;
            }

            // 겹치는 슬롯 중 가장 빨리 끝나는 시간으로 이동
            if let Some(slots) = self.load_slots.get(worker_id) {
                let next_end = slots
                    .iter()
                    .filter(|(s, e, _)| *s < candidate_end && *e > candidate_start)
                    .map(|(_, e, _)| *e)
                    .min();

                if let Some(end) = next_end {
                    candidate_start = end;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        candidate_start
    }
}

/// 스케줄링 요청
#[derive(Debug, Clone, Default)]
pub struct ScheduleRequest {
    pub jobs: Vec<Job>,
    pub resources: Vec<Resource>,
    pub start_time_ms: i64,
    /// 순서 의존 준비 시간 행렬
    pub setup_matrices: SetupMatrixCollection,
    /// 운영 캘린더 목록
    pub calendars: Vec<Calendar>,
    /// 스킬 매트릭스 (작업자 숙련도)
    pub skill_matrix: Option<SkillMatrix>,
    /// 작업자별 시간 오버라이드 (우선순위: Override > SkillMatrix > 기본값)
    pub worker_time_override: Option<WorkerTimeOverride>,
    /// 디스패칭 룰 설정 (기본: Priority)
    pub dispatching_config: Option<super::dispatching::DispatchingConfig>,
    /// 자재 관리자 (자재 가용성 검사)
    pub material_manager: Option<MaterialManager>,
    /// 인증 매트릭스 (작업자 자격)
    pub certification_matrix: Option<CertificationMatrix>,
    /// 팀 관리자 (Crew/Team)
    pub crew_manager: Option<CrewManager>,
    /// Pegging 엔진 (고급 자재 수급 관리)
    pub pegging_engine: Option<PeggingEngine>,
    /// 외주 관리자 (Outsourcing)
    pub outsourcing_manager: Option<OutsourcingManager>,
    /// 사이트 간 이동 시간 (multi-site 스케줄링용)
    pub site_transitions: Option<SiteTransitions>,
}

impl ScheduleRequest {
    pub fn new(jobs: Vec<Job>, resources: Vec<Resource>) -> Self {
        Self {
            jobs,
            resources,
            start_time_ms: 0,
            setup_matrices: SetupMatrixCollection::new(),
            calendars: Vec::new(),
            skill_matrix: None,
            worker_time_override: None,
            dispatching_config: None,
            material_manager: None,
            certification_matrix: None,
            crew_manager: None,
            pegging_engine: None,
            outsourcing_manager: None,
            site_transitions: None,
        }
    }

    pub fn with_start_time(mut self, start_time_ms: i64) -> Self {
        self.start_time_ms = start_time_ms;
        self
    }

    /// Builder: Setup Matrix 설정
    pub fn with_setup_matrices(mut self, matrices: SetupMatrixCollection) -> Self {
        self.setup_matrices = matrices;
        self
    }

    /// Builder: 캘린더 설정
    pub fn with_calendars(mut self, calendars: Vec<Calendar>) -> Self {
        self.calendars = calendars;
        self
    }

    /// Builder: 단일 캘린더 추가
    pub fn with_calendar(mut self, calendar: Calendar) -> Self {
        self.calendars.push(calendar);
        self
    }

    /// Builder: 스킬 매트릭스 설정
    pub fn with_skill_matrix(mut self, skill_matrix: SkillMatrix) -> Self {
        self.skill_matrix = Some(skill_matrix);
        self
    }

    /// Builder: 작업자 시간 오버라이드 설정
    pub fn with_worker_time_override(mut self, override_matrix: WorkerTimeOverride) -> Self {
        self.worker_time_override = Some(override_matrix);
        self
    }

    /// Builder: 디스패칭 룰 설정
    pub fn with_dispatching_config(
        mut self,
        config: super::dispatching::DispatchingConfig,
    ) -> Self {
        self.dispatching_config = Some(config);
        self
    }

    /// Builder: 단일 디스패칭 룰 설정 (편의 메서드)
    pub fn with_dispatching_rule(mut self, rule: super::dispatching::DispatchingRuleName) -> Self {
        self.dispatching_config = Some(super::dispatching::DispatchingConfig::new(rule));
        self
    }

    /// Builder: 자재 관리자 설정
    pub fn with_material_manager(mut self, manager: MaterialManager) -> Self {
        self.material_manager = Some(manager);
        self
    }

    /// Builder: 인증 매트릭스 설정
    pub fn with_certification_matrix(mut self, matrix: CertificationMatrix) -> Self {
        self.certification_matrix = Some(matrix);
        self
    }

    /// Builder: 팀 관리자 설정
    pub fn with_crew_manager(mut self, manager: CrewManager) -> Self {
        self.crew_manager = Some(manager);
        self
    }

    /// Builder: Pegging 엔진 설정 (고급 자재 수급 관리)
    pub fn with_pegging_engine(mut self, engine: PeggingEngine) -> Self {
        self.pegging_engine = Some(engine);
        self
    }

    /// Builder: 외주 관리자 설정 (Outsourcing)
    pub fn with_outsourcing_manager(mut self, manager: OutsourcingManager) -> Self {
        self.outsourcing_manager = Some(manager);
        self
    }

    /// Builder: 사이트 간 이동 시간 설정 (multi-site 스케줄링)
    pub fn with_site_transitions(mut self, transitions: SiteTransitions) -> Self {
        self.site_transitions = Some(transitions);
        self
    }
}

/// SimpleScheduler - 기본 FIFO 스케줄러
pub struct SimpleScheduler;

impl SimpleScheduler {
    pub fn new() -> Self {
        Self
    }

    /// 스케줄링 실행
    pub fn schedule(&self, request: &ScheduleRequest) -> Schedule {
        let mut schedule = Schedule::new();

        // 자원별 다음 가용 시간 추적
        let mut resource_next_available: HashMap<String, i64> = HashMap::new();
        // 자원별 마지막 처리 제품 추적 (Setup Matrix용)
        let mut resource_last_product: HashMap<String, String> = HashMap::new();
        // 작업자 부하 추적 (load_factor 기반 동시 작업 지원)
        let mut worker_load_tracker = WorkerLoadTracker::new();
        // 연결 자원 맵 구축 (linked_resource_id → 원본 자원)
        let linked_resource_map: HashMap<String, String> = request
            .resources
            .iter()
            .filter_map(|r| {
                r.linked_resource_id
                    .as_ref()
                    .map(|linked| (r.id.clone(), linked.clone()))
            })
            .collect();
        // 캘린더 맵 구축 (calendar_id → Calendar)
        let calendar_map: HashMap<&str, &Calendar> = request
            .calendars
            .iter()
            .map(|c| (c.id.as_str(), c))
            .collect();
        // 자원별 캘린더 맵 (resource_id → Calendar)
        let resource_calendar_map: HashMap<&str, &Calendar> = request
            .resources
            .iter()
            .filter_map(|r| {
                r.calendar_id.as_ref().and_then(|cal_id| {
                    calendar_map
                        .get(cal_id.as_str())
                        .map(|cal| (r.id.as_str(), *cal))
                })
            })
            .collect();

        // 자원별 사이트 맵 (resource_id → site_id)
        let resource_site_map: HashMap<&str, &str> = request
            .resources
            .iter()
            .filter_map(|r| r.site_id.as_ref().map(|s| (r.id.as_str(), s.as_str())))
            .collect();

        for resource in &request.resources {
            resource_next_available.insert(resource.id.clone(), request.start_time_ms);
        }

        // Job 정렬: dispatching_config가 있으면 해당 룰 사용, 없으면 priority 기반
        let sorted_jobs: Vec<Job> = if let Some(config) = &request.dispatching_config {
            let sorted_indices =
                super::dispatching::sort_jobs_by_rule(&request.jobs, config, request.start_time_ms);
            sorted_indices
                .iter()
                .map(|&i| request.jobs[i].clone())
                .collect()
        } else {
            // 기본: priority 순 (낮은 값 = 높은 우선순위)
            let mut jobs = request.jobs.clone();
            jobs.sort_by_key(|j| j.priority);
            jobs
        };

        // 각 Job 처리
        for job in &sorted_jobs {
            // Job 내 현재 시점 (이전 Operation 종료 후)
            let mut job_current_time = request.start_time_ms;
            // 이전 Operation이 수행된 사이트 (inter-site transition time 계산용)
            let mut job_last_site: Option<String> = None;

            // Operation을 순서대로 처리
            let mut operations = job.operations.clone();
            operations.sort_by_key(|op| op.sequence);

            for operation in &operations {
                // Multi-Resource: 모든 자원 요구사항 처리
                let mut allocated_resources: Vec<String> = Vec::new();
                let mut latest_available = job_current_time;
                let mut total_setup_time = 0i64;
                let mut primary_efficiency = 1.0f64;
                // 할당된 작업자 ID 추적 (스킬 기반 시간 조정용)
                let mut assigned_worker_id: Option<String> = None;

                // 0. 자재 제약 검사 (MaterialManager 설정된 경우)
                if let Some(ref material_manager) = request.material_manager {
                    if operation.has_material_requirements() {
                        // 현재 시점 기준 자재 가용성 검사
                        let check_time = DateTime::from_timestamp_millis(latest_available)
                            .unwrap_or_else(Utc::now);
                        let availability_result = material_manager
                            .check_availability(&operation.material_requirements, check_time);

                        if !availability_result.is_available {
                            for shortage in &availability_result.shortages {
                                // 자재 가용일 미도래로 인한 지연
                                if let Some(avail_date) = shortage.available_date {
                                    let avail_ms = avail_date.timestamp_millis();
                                    if avail_ms > latest_available {
                                        let delay = avail_ms - latest_available;
                                        schedule.add_violation(
                                            Violation::material_availability_violation(
                                                &operation.id,
                                                &shortage.material_id,
                                                delay,
                                            ),
                                        );
                                        // 자재 가용일까지 시작 시간 지연
                                        latest_available = avail_ms;
                                    }
                                } else {
                                    // 재고 부족 (가용일과 무관)
                                    schedule.add_violation(Violation::material_shortage_violation(
                                        &operation.id,
                                        &shortage.material_id,
                                        shortage.required,
                                        shortage.available,
                                    ));
                                }
                            }
                        }
                    }
                }

                // 0.5. 외주 공정 처리 (Outsourcing)
                if operation.is_outsourced() {
                    if let Some(ref outsourcing_config) = operation.outsourcing_config {
                        // 외주 리드타임 계산
                        let lead_time = if let Some(ref mgr) = request.outsourcing_manager {
                            outsourcing_config.lead_time_ms.unwrap_or_else(|| {
                                mgr.get_provider(&outsourcing_config.provider_id)
                                    .map(|p| p.default_lead_time_ms)
                                    .unwrap_or(operation.time.process_ms)
                            })
                        } else {
                            outsourcing_config
                                .lead_time_ms
                                .unwrap_or(operation.time.process_ms)
                        };

                        // 외주 시작 시간 (현재 가용 시간 기준)
                        let outsourcing_start = latest_available;
                        let outsourcing_end = outsourcing_start + lead_time;

                        // 외주 assignment 생성 (resource_id = provider_id)
                        let outsourcing_assignment = Assignment {
                            operation_id: operation.id.clone(),
                            job_id: job.id.clone(),
                            resource_id: format!("OUTSOURCE:{}", outsourcing_config.provider_id),
                            start_ms: outsourcing_start,
                            end_ms: outsourcing_end,
                            setup_ms: 0,
                            site_id: None,
                        };
                        schedule.add_assignment(outsourcing_assignment);

                        // Job 현재 시간 업데이트 (외주 완료 후)
                        job_current_time = outsourcing_end;

                        // 외주 공정은 내부 자원 할당 생략
                        continue;
                    }
                }

                // 1. 장비 자원 처리 (Setup Time 및 효율성 적용)
                if let Some(equipment_req) = operation
                    .required_resources
                    .iter()
                    .find(|r| r.resource_type == ResourceType::Equipment)
                {
                    // Site 필터링: target_site_id가 지정되면 해당 사이트 리소스만 허용
                    let site_filtered_candidates: Vec<String> =
                        if let Some(ref target_site) = job.target_site_id {
                            equipment_req
                                .candidates
                                .iter()
                                .filter(|c| {
                                    // 리소스에 site_id가 없으면 허용 (하위 호환)
                                    resource_site_map
                                        .get(c.as_str())
                                        .is_none_or(|s| *s == target_site.as_str())
                                })
                                .cloned()
                                .collect()
                        } else {
                            equipment_req.candidates.clone()
                        };
                    let (resource_id, available_time, setup_time, efficiency) = self
                        .find_earliest_resource_with_setup(
                            &site_filtered_candidates,
                            &request.resources,
                            &resource_next_available,
                            &resource_last_product,
                            &request.setup_matrices,
                            job.product_name.as_deref(),
                        );

                    if let Some(res_id) = resource_id {
                        // Inter-site transition time 적용
                        if let Some(ref transitions) = request.site_transitions {
                            let current_site = resource_site_map.get(res_id.as_str()).copied();
                            let transition_time = transitions
                                .get_transition_time(job_last_site.as_deref(), current_site);
                            latest_available =
                                latest_available.max(job_current_time + transition_time);
                        }
                        allocated_resources.push(res_id);
                        latest_available = latest_available.max(available_time);
                        total_setup_time = setup_time;
                        primary_efficiency = efficiency;
                    }
                }

                // 2. 팀 기반 할당 (crew_id 설정된 경우)
                let mut crew_allocated = false;
                if let Some(ref crew_id) = operation.crew_id {
                    if let Some(ref crew_manager) = request.crew_manager {
                        if let Some(crew) = crew_manager.get_crew(crew_id) {
                            if crew.is_operational() {
                                // crew의 모든 멤버를 할당
                                for member in crew.members() {
                                    if !allocated_resources.contains(&member.worker_id) {
                                        allocated_resources.push(member.worker_id.clone());
                                        // 작업자 가용 시간 확인
                                        let worker_available = resource_next_available
                                            .get(&member.worker_id)
                                            .copied()
                                            .unwrap_or(0);
                                        latest_available = latest_available.max(worker_available);
                                    }
                                    if assigned_worker_id.is_none() {
                                        assigned_worker_id = Some(member.worker_id.clone());
                                    }
                                }
                                // 팀 효율 보정 적용
                                primary_efficiency *= crew.efficiency_modifier;
                                crew_allocated = true;
                            }
                        }
                    }
                }

                // 작업자 할당 추적 (Task 생성 시 사용)
                let mut worker_allocations: Vec<(String, f64)> = Vec::new();

                // 3. 작업자 자원 처리 (requires_operator 체크, 팀 미할당 시)
                if !crew_allocated && operation.requires_operator {
                    for worker_req in operation
                        .required_resources
                        .iter()
                        .filter(|r| r.resource_type == ResourceType::Worker)
                    {
                        let load_factor = worker_req.load_factor;
                        // 필요한 작업자 수만큼 할당
                        let mut assigned_workers = 0;

                        // 후보가 있으면 후보에서 선택
                        let process_duration = operation.time.process_ms
                            + operation.time.setup_ms
                            + operation.time.wait_ms;
                        // 작업 유형 (스킬 매트릭스 조회용)
                        let op_type_for_candidates =
                            operation.operation_type.as_deref().unwrap_or(&operation.id);
                        if !worker_req.candidates.is_empty() {
                            for candidate_id in &worker_req.candidates {
                                if assigned_workers >= worker_req.quantity {
                                    break;
                                }
                                // 스킬 매트릭스 검증 (설정된 경우)
                                if let Some(ref skill_matrix) = request.skill_matrix {
                                    if !skill_matrix
                                        .can_perform(candidate_id, op_type_for_candidates)
                                    {
                                        continue;
                                    }
                                }
                                // 인증 매트릭스 검증 (설정된 경우)
                                if let Some(ref cert_matrix) = request.certification_matrix {
                                    if !cert_matrix.is_certified(
                                        candidate_id,
                                        op_type_for_candidates,
                                        request.start_time_ms,
                                    ) {
                                        continue;
                                    }
                                }
                                if request.resources.iter().any(|r| r.id == *candidate_id) {
                                    // load_factor 기반 가용 시간 계산
                                    let available = if load_factor < 1.0 {
                                        // 부하 기반 스케줄링: 시작 시간부터 가용 시간 찾기
                                        worker_load_tracker.find_earliest_available(
                                            candidate_id,
                                            latest_available,
                                            process_duration,
                                            load_factor,
                                        )
                                    } else {
                                        // 전담 스케줄링: 다음 가용 시간 사용
                                        resource_next_available
                                            .get(candidate_id)
                                            .copied()
                                            .unwrap_or(0)
                                    };
                                    latest_available = latest_available.max(available);
                                    allocated_resources.push(candidate_id.clone());
                                    worker_allocations.push((candidate_id.clone(), load_factor));
                                    // 첫 번째 할당 작업자 기록 (스킬 기반 시간 조정용)
                                    if assigned_worker_id.is_none() {
                                        assigned_worker_id = Some(candidate_id.clone());
                                    }
                                    assigned_workers += 1;
                                }
                            }
                        } else {
                            // 후보가 없으면 Worker 타입 자원에서 선택
                            // 작업 유형 (스킬 매트릭스 조회용)
                            let op_type =
                                operation.operation_type.as_deref().unwrap_or(&operation.id);
                            let mut workers: Vec<_> = request
                                .resources
                                .iter()
                                .filter(|r| r.kind == crate::ResourceKind::Worker)
                                // 스킬 매트릭스 기반 필터링 (설정된 경우)
                                .filter(|r| {
                                    if let Some(ref skill_matrix) = request.skill_matrix {
                                        skill_matrix.can_perform(&r.id, op_type)
                                    } else {
                                        true
                                    }
                                })
                                // 인증 매트릭스 기반 필터링 (설정된 경우)
                                .filter(|r| {
                                    if let Some(ref cert_matrix) = request.certification_matrix {
                                        cert_matrix.is_certified(
                                            &r.id,
                                            op_type,
                                            request.start_time_ms,
                                        )
                                    } else {
                                        true
                                    }
                                })
                                .map(|r| {
                                    // load_factor < 1.0이면 load_tracker 기반 스케줄링
                                    // load_factor == 1.0이면 기존 방식 (전담)
                                    let avail = if load_factor < 1.0 {
                                        // 부하 기반 스케줄링: 시작 시간부터 가용 시간 찾기
                                        worker_load_tracker.find_earliest_available(
                                            &r.id,
                                            latest_available,
                                            process_duration,
                                            load_factor,
                                        )
                                    } else {
                                        // 전담 스케줄링: 다음 가용 시간 사용
                                        resource_next_available.get(&r.id).copied().unwrap_or(0)
                                    };
                                    (r.id.clone(), avail, load_factor)
                                })
                                .collect();

                            // 가용 시간 순 정렬
                            workers.sort_by_key(|(_, avail, _)| *avail);

                            for (worker_id, avail, lf) in workers {
                                if assigned_workers >= worker_req.quantity {
                                    break;
                                }
                                if !allocated_resources.contains(&worker_id) {
                                    latest_available = latest_available.max(avail);
                                    allocated_resources.push(worker_id.clone());
                                    worker_allocations.push((worker_id.clone(), lf));
                                    // 첫 번째 할당 작업자 기록 (스킬 기반 시간 조정용)
                                    if assigned_worker_id.is_none() {
                                        assigned_worker_id = Some(worker_id);
                                    }
                                    assigned_workers += 1;
                                }
                            }
                        }
                    }
                }

                // 3. 연결 자원 처리 (로봇-설비 연동)
                let mut linked_resources: Vec<String> = Vec::new();
                for res_id in &allocated_resources {
                    if let Some(linked_id) = linked_resource_map.get(res_id) {
                        // 연결 자원의 가용 시간도 고려
                        let linked_available =
                            resource_next_available.get(linked_id).copied().unwrap_or(0);
                        latest_available = latest_available.max(linked_available);
                        linked_resources.push(linked_id.clone());
                    }
                }

                // 4. 할당 생성
                if !allocated_resources.is_empty() {
                    let primary_resource = &allocated_resources[0];

                    // DailyWorkWindow 적용: 가용 시간을 윈도우 내 유효 시각으로 전진
                    let latest_available = if let Some(window) = operation.daily_work_window {
                        super::task_splitter::advance_to_daily_window(latest_available, window)
                    } else {
                        latest_available
                    };

                    // 캘린더 제약 적용: 시작 시간을 근무 시간으로 조정
                    let start_time = if let Some(calendar) =
                        resource_calendar_map.get(primary_resource.as_str())
                    {
                        let cal_start = calendar.next_working_time(latest_available);
                        // 캘린더 전진 후 DailyWorkWindow 재확인
                        if let Some(window) = operation.daily_work_window {
                            super::task_splitter::advance_to_daily_window(cal_start, window)
                        } else {
                            cal_start
                        }
                    } else {
                        latest_available
                    };

                    // 효율성 적용: 처리 시간 = base_time / efficiency
                    let mut adjusted_process_time =
                        (operation.time.process_ms as f64 / primary_efficiency) as i64;
                    let mut adjusted_setup_time = operation.time.setup_ms;
                    let mut adjusted_wait_time = operation.time.wait_ms;

                    // 시간 오버라이드/조정 우선순위:
                    // 1. WorkerTimeOverride (명시적 값) - 최우선
                    // 2. SkillMatrix (효율 기반 조정)
                    // 3. CertificationMatrix (인증 기반 조정)
                    // 4. 기본값
                    if let Some(ref worker_id) = assigned_worker_id {
                        let op_type = operation.operation_type.as_deref().unwrap_or(&operation.id);

                        // 1. WorkerTimeOverride 적용 (최우선)
                        let mut override_applied = false;
                        if let Some(ref override_matrix) = request.worker_time_override {
                            if override_matrix.has_override(worker_id, op_type) {
                                let (setup, process, wait) = override_matrix.calculate_times(
                                    worker_id,
                                    op_type,
                                    adjusted_setup_time,
                                    adjusted_process_time,
                                    adjusted_wait_time,
                                );
                                adjusted_setup_time = setup;
                                adjusted_process_time = process;
                                adjusted_wait_time = wait;
                                override_applied = true;
                            }
                        }

                        // 2, 3. SkillMatrix/CertificationMatrix는 override가 없을 때만 적용
                        if !override_applied {
                            if let Some(ref skill_matrix) = request.skill_matrix {
                                if let Some(skill_adjusted) = skill_matrix.calculate_process_time(
                                    worker_id,
                                    op_type,
                                    adjusted_process_time,
                                ) {
                                    adjusted_process_time = skill_adjusted;
                                }
                            }
                            // 인증 매트릭스 기반 처리 시간 보정 (인증 레벨에 따른 효율)
                            if let Some(ref cert_matrix) = request.certification_matrix {
                                if let Some(cert_adjusted) = cert_matrix.calculate_process_time(
                                    worker_id,
                                    op_type,
                                    adjusted_process_time,
                                    request.start_time_ms,
                                ) {
                                    adjusted_process_time = cert_adjusted;
                                }
                            }
                        }
                    }

                    // Setup time: Operation 기본 setup + Matrix 기반 setup (둘 중 큰 값 사용)
                    let effective_setup = adjusted_setup_time.max(total_setup_time);
                    let process_time = adjusted_process_time + adjusted_wait_time;
                    let total_duration = effective_setup + process_time;

                    // Task 생성 (Calendar + DailyWorkWindow 기반 분할) — Assignment의 end_time도 여기서 결정
                    let calendar = resource_calendar_map
                        .get(primary_resource.as_str())
                        .copied();
                    let split_result = super::task_splitter::split_operation_by_calendar(
                        &operation.id,
                        &job.id,
                        job.product_name.as_deref(),
                        start_time,
                        effective_setup,
                        process_time,
                        operation.is_splittable,
                        calendar,
                        operation.daily_work_window,
                    );

                    // 실제 종료 시간: DailyWorkWindow가 있으면 splitter의 final_end를 사용
                    // (윈도우 경계에서 다음 날로 넘어갈 수 있으므로 단순 calendar.calculate_end_time 불가)
                    let end_time = if operation.daily_work_window.is_some() {
                        split_result.final_end_ms
                    } else if let Some(cal) = calendar {
                        cal.calculate_end_time(start_time, total_duration)
                    } else {
                        start_time + total_duration
                    };

                    // Assignment 생성 - 모든 할당된 자원에 대해 생성
                    for res_id in &allocated_resources {
                        let mut assignment = Assignment::with_setup(
                            &operation.id,
                            &job.id,
                            res_id,
                            start_time,
                            end_time,
                            effective_setup,
                        );
                        assignment.site_id = resource_site_map
                            .get(res_id.as_str())
                            .map(|s| s.to_string());
                        schedule.add_assignment(assignment);
                    }

                    // Task에 자원 정보 설정 후 추가
                    for mut task in split_result.tasks {
                        if !allocated_resources.is_empty() {
                            task = task.with_equipment(allocated_resources[0].clone());
                        }
                        // 작업자 정보 추가
                        let worker_ids: Vec<String> = worker_allocations
                            .iter()
                            .map(|(id, _)| id.clone())
                            .collect();
                        if !worker_ids.is_empty() {
                            task = task.with_workers(
                                worker_ids,
                                operation
                                    .required_resources
                                    .iter()
                                    .filter(|r| r.resource_type == ResourceType::Worker)
                                    .map(|r| r.quantity as u32)
                                    .next()
                                    .unwrap_or(0),
                            );
                        }
                        schedule.add_task(task);
                    }

                    // 모든 할당된 자원의 가용 시간 업데이트
                    for res_id in &allocated_resources {
                        // 작업자의 경우 load_factor 기반 부하 추적
                        if let Some(resource) = request.resources.iter().find(|r| r.id == *res_id) {
                            if resource.kind == crate::ResourceKind::Worker {
                                // 해당 작업자의 load_factor 찾기
                                let load_factor = operation
                                    .required_resources
                                    .iter()
                                    .filter(|r| r.resource_type == ResourceType::Worker)
                                    .map(|r| r.load_factor)
                                    .next()
                                    .unwrap_or(1.0);
                                worker_load_tracker.add_load(
                                    res_id,
                                    start_time,
                                    end_time,
                                    load_factor,
                                );

                                // load_factor < 1.0이면 resource_next_available 업데이트 건너뛰기
                                // (load_tracker가 동시 작업을 관리)
                                if load_factor < 1.0 {
                                    continue;
                                }
                            }
                        }
                        resource_next_available.insert(res_id.clone(), end_time);
                    }

                    // 연결 자원의 가용 시간도 업데이트 (동시 점유)
                    for linked_id in &linked_resources {
                        resource_next_available.insert(linked_id.clone(), end_time);
                    }

                    // 마지막 처리 제품 업데이트 (장비에만)
                    if let Some(product) = &job.product_name {
                        if let Some(equipment_id) = allocated_resources.first() {
                            if request.resources.iter().any(|r| {
                                r.id == *equipment_id && r.kind == crate::ResourceKind::Equipment
                            }) {
                                resource_last_product.insert(equipment_id.clone(), product.clone());
                            }
                        }
                    }

                    job_current_time = end_time;

                    // 이전 Operation 사이트 업데이트 (inter-site transition 추적)
                    if let Some(primary) = allocated_resources.first() {
                        job_last_site = resource_site_map
                            .get(primary.as_str())
                            .map(|s| s.to_string());
                    }
                } else {
                    // 자원 요구사항 없으면 시간만 진행 (Calendar + DailyWorkWindow 적용)
                    let mut start_time = job_current_time;

                    // DailyWorkWindow 적용
                    if let Some(window) = operation.daily_work_window {
                        start_time =
                            super::task_splitter::advance_to_daily_window(start_time, window);
                    }

                    // Default Calendar 적용 (첫 번째 Calendar 사용)
                    let default_calendar = request.calendars.first();
                    if let Some(calendar) = default_calendar {
                        start_time = calendar.next_working_time(start_time);
                        // 캘린더 전진 후 DailyWorkWindow 재확인
                        if let Some(window) = operation.daily_work_window {
                            start_time =
                                super::task_splitter::advance_to_daily_window(start_time, window);
                        }
                    }

                    let process_time = operation.time.process_ms + operation.time.wait_ms;

                    // Calendar + DailyWorkWindow 기반 분할
                    let split_result = super::task_splitter::split_operation_by_calendar(
                        &operation.id,
                        &job.id,
                        job.product_name.as_deref(),
                        start_time,
                        operation.time.setup_ms,
                        process_time,
                        operation.is_splittable,
                        default_calendar,
                        operation.daily_work_window,
                    );

                    let end_time = split_result.final_end_ms;

                    schedule.add_assignment(Assignment::new(
                        &operation.id,
                        &job.id,
                        "NONE",
                        start_time,
                        end_time,
                    ));

                    for task in split_result.tasks {
                        schedule.add_task(task);
                    }

                    job_current_time = end_time;
                }
            }
        }

        // 납기 위반 체크
        self.check_due_dates(&mut schedule, &sorted_jobs);

        schedule
    }

    /// 납기 위반 체크
    fn check_due_dates(&self, schedule: &mut Schedule, jobs: &[Job]) {
        for job in jobs {
            if let Some(due_date) = job.due_date {
                let due_ms = due_date.timestamp_millis();

                // Job의 마지막 Operation 종료 시간 찾기
                let job_end_time = job
                    .operations
                    .iter()
                    .filter_map(|op| schedule.assignment_for_operation(&op.id).map(|a| a.end_ms))
                    .max()
                    .unwrap_or(0);

                if job_end_time > due_ms {
                    let delay = job_end_time - due_ms;
                    schedule.add_violation(Violation::due_date_violation(&job.id, delay));
                }
            }
        }
    }

    /// 가장 빨리 가용한 자원 찾기
    #[allow(dead_code)]
    fn find_earliest_resource(
        &self,
        candidates: &[String],
        resources: &[Resource],
        next_available: &HashMap<String, i64>,
    ) -> (Option<String>, i64) {
        let mut earliest_id: Option<String> = None;
        let mut earliest_time = i64::MAX;

        for candidate_id in candidates {
            // 자원이 존재하는지 확인
            if resources.iter().any(|r| r.id == *candidate_id) {
                let available = next_available.get(candidate_id).copied().unwrap_or(0);

                if available < earliest_time {
                    earliest_time = available;
                    earliest_id = Some(candidate_id.clone());
                }
            }
        }

        (earliest_id, earliest_time)
    }

    /// Setup Time과 효율성을 고려하여 최적의 자원 찾기
    ///
    /// 선택 기준:
    /// 1. 완료 시간이 가장 빠른 자원 선택
    /// 2. 완료 시간이 동일하면 효율성이 높은 자원 선택
    fn find_earliest_resource_with_setup(
        &self,
        candidates: &[String],
        resources: &[Resource],
        next_available: &HashMap<String, i64>,
        last_product: &HashMap<String, String>,
        setup_matrices: &crate::SetupMatrixCollection,
        current_product: Option<&str>,
    ) -> (Option<String>, i64, i64, f64) {
        let mut best_id: Option<String> = None;
        let mut best_available_time = i64::MAX;
        let mut best_setup_time = 0i64;
        let mut best_efficiency = 0.0f64;
        let mut best_completion = i64::MAX;

        for candidate_id in candidates {
            // 자원 찾기
            if let Some(resource) = resources.iter().find(|r| r.id == *candidate_id) {
                let available = next_available.get(candidate_id).copied().unwrap_or(0);

                // 해당 자원의 마지막 제품
                let prev_product = last_product.get(candidate_id).map(|s| s.as_str());

                // Setup time 계산
                let setup_time = if let Some(curr) = current_product {
                    setup_matrices.get_setup_time(candidate_id, prev_product, curr)
                } else {
                    0
                };

                // 총 완료 시간 = 가용 시간 + 준비 시간 (효율은 처리 시간에만 적용)
                let completion_time = available + setup_time;

                // 선택 기준: 완료 시간이 빠르거나, 같으면 효율이 높은 자원
                let is_better = completion_time < best_completion
                    || (completion_time == best_completion
                        && resource.efficiency > best_efficiency);

                if is_better {
                    best_available_time = available;
                    best_setup_time = setup_time;
                    best_efficiency = resource.efficiency;
                    best_completion = completion_time;
                    best_id = Some(candidate_id.clone());
                }
            }
        }

        (
            best_id,
            best_available_time,
            best_setup_time,
            best_efficiency,
        )
    }
}

impl Default for SimpleScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Operation;

    #[test]
    fn test_single_job_single_resource() {
        // Given: 1개 Job, 2개 Operation, 1개 장비
        let job = Job::new("JOB-001")
            .with_operation(
                Operation::new("OP-001", "JOB-001", 1)
                    .with_time(0, 30000, 0) // 30초
                    .with_equipment(vec!["EQP-001".to_string()]),
            )
            .with_operation(
                Operation::new("OP-002", "JOB-001", 2)
                    .with_time(0, 45000, 0) // 45초
                    .with_equipment(vec!["EQP-001".to_string()]),
            );

        let resource = Resource::equipment("EQP-001");

        let request = ScheduleRequest::new(vec![job], vec![resource]);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then
        assert_eq!(schedule.assignments.len(), 2);
        assert_eq!(schedule.makespan_ms, 75000); // 30초 + 45초

        let op1 = schedule.assignment_for_operation("OP-001").unwrap();
        let op2 = schedule.assignment_for_operation("OP-002").unwrap();

        assert_eq!(op1.start_ms, 0);
        assert_eq!(op1.end_ms, 30000);
        assert_eq!(op2.start_ms, 30000); // OP-001 종료 후 시작
        assert_eq!(op2.end_ms, 75000);
    }

    #[test]
    fn test_multiple_jobs_priority() {
        // Given: 2개 Job (우선순위 다름), 1개 장비
        let job_low = Job::new("JOB-LOW").with_priority(10).with_operation(
            Operation::new("OP-LOW", "JOB-LOW", 1)
                .with_time(0, 20000, 0)
                .with_equipment(vec!["EQP-001".to_string()]),
        );

        let job_high = Job::new("JOB-HIGH")
            .with_priority(1) // 높은 우선순위
            .with_operation(
                Operation::new("OP-HIGH", "JOB-HIGH", 1)
                    .with_time(0, 30000, 0)
                    .with_equipment(vec!["EQP-001".to_string()]),
            );

        let resource = Resource::equipment("EQP-001");

        let request = ScheduleRequest::new(vec![job_low, job_high], vec![resource]);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 우선순위 높은 JOB-HIGH가 먼저
        let op_high = schedule.assignment_for_operation("OP-HIGH").unwrap();
        let op_low = schedule.assignment_for_operation("OP-LOW").unwrap();

        assert_eq!(op_high.start_ms, 0);
        assert_eq!(op_low.start_ms, 30000);
    }

    #[test]
    fn test_parallel_resources() {
        // Given: 2개 Job, 2개 장비 (병렬 처리 가능)
        let job1 = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 30000, 0)
                .with_equipment(vec!["EQP-001".to_string(), "EQP-002".to_string()]),
        );

        let job2 = Job::new("JOB-002").with_operation(
            Operation::new("OP-002", "JOB-002", 1)
                .with_time(0, 30000, 0)
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

        // Then: 병렬 처리로 makespan = 30초
        assert_eq!(schedule.makespan_ms, 30000);

        let op1 = schedule.assignment_for_operation("OP-001").unwrap();
        let op2 = schedule.assignment_for_operation("OP-002").unwrap();

        // 서로 다른 장비에 할당
        assert_ne!(op1.resource_id, op2.resource_id);
    }

    #[test]
    fn test_operation_with_setup_and_wait() {
        // Given: setup time과 wait time 포함
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(
                    10000, // setup 10초
                    30000, // process 30초
                    5000,  // wait 5초
                )
                .with_equipment(vec!["EQP-001".to_string()]),
        );

        let resource = Resource::equipment("EQP-001");

        let request = ScheduleRequest::new(vec![job], vec![resource]);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.duration_ms(), 45000); // 10 + 30 + 5
    }

    #[test]
    fn test_due_date_on_time() {
        use chrono::{TimeZone, Utc};

        // Given: 납기 충분
        let due_date = Utc.timestamp_millis_opt(100000).unwrap();
        let job = Job::new("JOB-001").with_due_date(due_date).with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 30000, 0)
                .with_equipment(vec!["EQP-001".to_string()]),
        );

        let resource = Resource::equipment("EQP-001");

        let request = ScheduleRequest::new(vec![job], vec![resource]);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 납기 준수
        assert!(schedule.is_on_time());
        assert!(schedule.violations.is_empty());
    }

    #[test]
    fn test_due_date_violation() {
        use chrono::{TimeZone, Utc};

        // Given: 납기 부족 (20초 납기, 30초 작업)
        let due_date = Utc.timestamp_millis_opt(20000).unwrap();
        let job = Job::new("JOB-001").with_due_date(due_date).with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 30000, 0)
                .with_equipment(vec!["EQP-001".to_string()]),
        );

        let resource = Resource::equipment("EQP-001");

        let request = ScheduleRequest::new(vec![job], vec![resource]);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 10초 지연
        assert!(!schedule.is_on_time());
        assert_eq!(schedule.violations.len(), 1);
        assert_eq!(schedule.violations[0].amount, 10000.0);
    }

    #[test]
    fn test_multiple_jobs_due_dates() {
        use chrono::{TimeZone, Utc};

        // Given: 2개 Job, 1개만 납기 위반
        let job1 = Job::new("JOB-001")
            .with_due_date(Utc.timestamp_millis_opt(50000).unwrap())
            .with_priority(1)
            .with_operation(
                Operation::new("OP-001", "JOB-001", 1)
                    .with_time(0, 30000, 0)
                    .with_equipment(vec!["EQP-001".to_string()]),
            );

        let job2 = Job::new("JOB-002")
            .with_due_date(Utc.timestamp_millis_opt(40000).unwrap()) // 빠듯한 납기
            .with_priority(2)
            .with_operation(
                Operation::new("OP-002", "JOB-002", 1)
                    .with_time(0, 20000, 0)
                    .with_equipment(vec!["EQP-001".to_string()]),
            );

        let resource = Resource::equipment("EQP-001");

        let request = ScheduleRequest::new(vec![job1, job2], vec![resource]);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: JOB-002만 납기 위반 (30000 + 20000 = 50000 > 40000)
        assert_eq!(schedule.violations.len(), 1);
        assert_eq!(schedule.violations[0].target_id, "JOB-002");
    }

    #[test]
    fn test_setup_matrix_basic() {
        use crate::{SetupMatrix, SetupMatrixCollection};

        // Given: 제품 전환 시 준비 시간이 다름
        // ProductA → ProductB: 20초
        // ProductB → ProductA: 15초
        // 동일 제품: 0초
        let matrix = SetupMatrix::new("EQP-001")
            .with_default_setup(10000) // 기본 10초
            .with_setup("ProductA", "ProductB", 20000)
            .with_setup("ProductB", "ProductA", 15000);

        let setup_matrices = SetupMatrixCollection::new().with_matrix(matrix);

        // Job 생성: ProductA → ProductB → ProductA
        let job1 = Job::new("JOB-001").with_product("ProductA").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 30000, 0) // 공정 30초
                .with_equipment(vec!["EQP-001".to_string()]),
        );

        let job2 = Job::new("JOB-002").with_product("ProductB").with_operation(
            Operation::new("OP-002", "JOB-002", 1)
                .with_time(0, 20000, 0) // 공정 20초
                .with_equipment(vec!["EQP-001".to_string()]),
        );

        let job3 = Job::new("JOB-003").with_product("ProductA").with_operation(
            Operation::new("OP-003", "JOB-003", 1)
                .with_time(0, 25000, 0) // 공정 25초
                .with_equipment(vec!["EQP-001".to_string()]),
        );

        let resource = Resource::equipment("EQP-001");

        let request = ScheduleRequest::new(vec![job1, job2, job3], vec![resource])
            .with_setup_matrices(setup_matrices);

        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then
        let op1 = schedule.assignment_for_operation("OP-001").unwrap();
        let op2 = schedule.assignment_for_operation("OP-002").unwrap();
        let op3 = schedule.assignment_for_operation("OP-003").unwrap();

        // OP-001: 첫 작업이므로 기본 준비 시간 10초 + 공정 30초 = 40초
        assert_eq!(op1.start_ms, 0);
        assert_eq!(op1.end_ms, 40000);

        // OP-002: A→B 준비 시간 20초 + 공정 20초 = 40초
        assert_eq!(op2.start_ms, 40000);
        assert_eq!(op2.end_ms, 80000);

        // OP-003: B→A 준비 시간 15초 + 공정 25초 = 40초
        assert_eq!(op3.start_ms, 80000);
        assert_eq!(op3.end_ms, 120000);

        // 총 makespan: 120초
        assert_eq!(schedule.makespan_ms, 120000);
    }

    #[test]
    fn test_setup_matrix_same_product() {
        use crate::{SetupMatrix, SetupMatrixCollection};

        // Given: 동일 제품 연속 생산 시 준비 시간 0
        let matrix = SetupMatrix::new("EQP-001").with_default_setup(30000);

        let setup_matrices = SetupMatrixCollection::new().with_matrix(matrix);

        let job1 = Job::new("JOB-001").with_product("ProductA").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 20000, 0)
                .with_equipment(vec!["EQP-001".to_string()]),
        );

        let job2 = Job::new("JOB-002")
            .with_product("ProductA") // 동일 제품
            .with_operation(
                Operation::new("OP-002", "JOB-002", 1)
                    .with_time(0, 20000, 0)
                    .with_equipment(vec!["EQP-001".to_string()]),
            );

        let resource = Resource::equipment("EQP-001");

        let request = ScheduleRequest::new(vec![job1, job2], vec![resource])
            .with_setup_matrices(setup_matrices);

        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then
        let op1 = schedule.assignment_for_operation("OP-001").unwrap();
        let op2 = schedule.assignment_for_operation("OP-002").unwrap();

        // OP-001: 기본 준비 30초 + 공정 20초 = 50초
        assert_eq!(op1.end_ms, 50000);

        // OP-002: 동일 제품이므로 준비 0초 + 공정 20초 = 20초
        assert_eq!(op2.start_ms, 50000);
        assert_eq!(op2.end_ms, 70000);

        assert_eq!(schedule.makespan_ms, 70000);
    }

    #[test]
    fn test_multi_resource_equipment_and_worker() {
        // Given: 설비 + 작업자 동시 필요
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 30000, 0)
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers(1), // 작업자 1명 필요
        );

        let resources = vec![Resource::equipment("EQP-001"), Resource::worker("WRK-001")];

        let request = ScheduleRequest::new(vec![job], resources);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 둘 다 가용할 때 시작
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.start_ms, 0);
        assert_eq!(op.end_ms, 30000);
    }

    #[test]
    fn test_multi_resource_wait_for_worker() {
        // Given: 작업자가 나중에 가용
        let job1 = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 20000, 0)
                .with_workers(1),
        );

        let job2 = Job::new("JOB-002").with_operation(
            Operation::new("OP-002", "JOB-002", 1)
                .with_time(0, 30000, 0)
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers(1), // 같은 작업자 필요
        );

        let resources = vec![Resource::equipment("EQP-001"), Resource::worker("WRK-001")];

        let request = ScheduleRequest::new(vec![job1, job2], resources);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then
        let op1 = schedule.assignment_for_operation("OP-001").unwrap();
        let op2 = schedule.assignment_for_operation("OP-002").unwrap();

        // OP-001: 작업자만 사용, 0~20초
        assert_eq!(op1.start_ms, 0);
        assert_eq!(op1.end_ms, 20000);

        // OP-002: 설비는 0부터 가용하지만, 작업자가 20초까지 사용 중
        // 따라서 20초부터 시작
        assert_eq!(op2.start_ms, 20000);
        assert_eq!(op2.end_ms, 50000);
    }

    #[test]
    fn test_multi_resource_multiple_workers() {
        // Given: 작업자 2명 동시 필요
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 30000, 0)
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers(2),
        );

        let resources = vec![
            Resource::equipment("EQP-001"),
            Resource::worker("WRK-001"),
            Resource::worker("WRK-002"),
        ];

        let request = ScheduleRequest::new(vec![job], resources);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.start_ms, 0);
        assert_eq!(op.end_ms, 30000);
    }

    #[test]
    fn test_alternative_resource_prefer_higher_efficiency() {
        // Given: 두 장비가 동시에 가용, 효율이 다름
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 60000, 0) // 60초
                .with_equipment(vec!["EQP-001".to_string(), "EQP-002".to_string()]),
        );

        let resources = vec![
            Resource::equipment("EQP-001").with_efficiency(1.0), // 표준
            Resource::equipment("EQP-002").with_efficiency(1.5), // 50% 빠름
        ];

        let request = ScheduleRequest::new(vec![job], resources);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 효율이 높은 EQP-002 선택
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.resource_id, "EQP-002");
        // 60000 / 1.5 = 40000ms
        assert_eq!(op.end_ms, 40000);
    }

    #[test]
    fn test_alternative_resource_efficiency_adjusts_time() {
        // Given: 효율성이 다른 장비들
        let job1 = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 60000, 0)
                .with_equipment(vec!["EQP-FAST".to_string()]),
        );

        let job2 = Job::new("JOB-002").with_operation(
            Operation::new("OP-002", "JOB-002", 1)
                .with_time(0, 60000, 0)
                .with_equipment(vec!["EQP-SLOW".to_string()]),
        );

        let resources = vec![
            Resource::equipment("EQP-FAST").with_efficiency(2.0), // 2배 빠름
            Resource::equipment("EQP-SLOW").with_efficiency(0.5), // 2배 느림
        ];

        let request = ScheduleRequest::new(vec![job1, job2], resources);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then
        let op1 = schedule.assignment_for_operation("OP-001").unwrap();
        let op2 = schedule.assignment_for_operation("OP-002").unwrap();

        // EQP-FAST: 60000 / 2.0 = 30000ms
        assert_eq!(op1.end_ms, 30000);
        // EQP-SLOW: 60000 / 0.5 = 120000ms
        assert_eq!(op2.end_ms, 120000);
    }

    #[test]
    fn test_alternative_resource_earlier_availability_wins() {
        // Given: EQP-002가 효율 높지만, EQP-001이 먼저 가용
        let job1 = Job::new("JOB-001").with_priority(1).with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 30000, 0)
                .with_equipment(vec!["EQP-002".to_string()]), // 효율 높은 장비 점유
        );

        let job2 = Job::new("JOB-002").with_priority(2).with_operation(
            Operation::new("OP-002", "JOB-002", 1)
                .with_time(0, 60000, 0)
                .with_equipment(vec!["EQP-001".to_string(), "EQP-002".to_string()]),
        );

        let resources = vec![
            Resource::equipment("EQP-001").with_efficiency(1.0),
            Resource::equipment("EQP-002").with_efficiency(1.5),
        ];

        let request = ScheduleRequest::new(vec![job1, job2], resources);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: EQP-001이 먼저 가용하므로 선택
        let op2 = schedule.assignment_for_operation("OP-002").unwrap();
        assert_eq!(op2.resource_id, "EQP-001");
        assert_eq!(op2.start_ms, 0);
        // 60000 / 1.0 = 60000ms
        assert_eq!(op2.end_ms, 60000);
    }

    // ===== Phase 2.1 Tests: ManLoadFactor, RequiresOperator, LinkedResource =====

    #[test]
    fn test_unmanned_operation_no_worker_required() {
        // Given: 무인공정 (requires_operator = false)
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 30000, 0)
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers(1) // 작업자 요구사항 있지만
                .unmanned(), // 무인공정으로 설정
        );

        let resources = vec![Resource::equipment("EQP-001"), Resource::worker("WRK-001")];

        let request = ScheduleRequest::new(vec![job], resources);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 무인공정이므로 작업자 가용 상태와 무관하게 즉시 시작
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.start_ms, 0);
        assert_eq!(op.end_ms, 30000);
        // 장비만 할당됨 (작업자 할당 안됨)
        assert_eq!(op.resource_id, "EQP-001");
    }

    #[test]
    fn test_unmanned_operation_parallel_jobs() {
        // Given: 2개 Job - 1개는 무인, 1개는 유인
        // 작업자가 1명이므로 유인공정은 대기해야 함
        let job_unmanned = Job::new("JOB-UNMANNED").with_priority(2).with_operation(
            Operation::new("OP-UNMANNED", "JOB-UNMANNED", 1)
                .with_time(0, 30000, 0)
                .with_equipment(vec!["EQP-001".to_string()])
                .unmanned(),
        );

        let job_manned = Job::new("JOB-MANNED").with_priority(1).with_operation(
            Operation::new("OP-MANNED", "JOB-MANNED", 1)
                .with_time(0, 20000, 0)
                .with_equipment(vec!["EQP-002".to_string()])
                .with_workers(1),
        );

        let resources = vec![
            Resource::equipment("EQP-001"),
            Resource::equipment("EQP-002"),
            Resource::worker("WRK-001"),
        ];

        let request = ScheduleRequest::new(vec![job_unmanned, job_manned], resources);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 두 작업 모두 0에서 시작 가능 (무인공정은 작업자 불필요)
        let op_manned = schedule.assignment_for_operation("OP-MANNED").unwrap();
        let op_unmanned = schedule.assignment_for_operation("OP-UNMANNED").unwrap();

        assert_eq!(op_manned.start_ms, 0);
        assert_eq!(op_unmanned.start_ms, 0); // 무인이므로 병렬 가능
    }

    #[test]
    fn test_worker_load_factor_concurrent_tasks() {
        // Given: 2개 Job, 1명 작업자 (load_factor 0.5씩이면 동시 처리 가능)
        let job1 = Job::new("JOB-001").with_priority(1).with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 30000, 0)
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers_load(1, 0.5), // 50% 부하
        );

        let job2 = Job::new("JOB-002").with_priority(2).with_operation(
            Operation::new("OP-002", "JOB-002", 1)
                .with_time(0, 30000, 0)
                .with_equipment(vec!["EQP-002".to_string()])
                .with_workers_load(1, 0.5), // 50% 부하
        );

        let resources = vec![
            Resource::equipment("EQP-001"),
            Resource::equipment("EQP-002"),
            Resource::worker("WRK-001"),
        ];

        let request = ScheduleRequest::new(vec![job1, job2], resources);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 0.5 + 0.5 = 1.0이므로 동시 작업 가능
        let op1 = schedule.assignment_for_operation("OP-001").unwrap();
        let op2 = schedule.assignment_for_operation("OP-002").unwrap();

        assert_eq!(op1.start_ms, 0);
        assert_eq!(op2.start_ms, 0); // 동시 시작 가능
        assert_eq!(schedule.makespan_ms, 30000); // 병렬 처리로 30초
    }

    #[test]
    fn test_worker_load_factor_sequential_when_overloaded() {
        // Given: 2개 Job, 1명 작업자 (load_factor 합이 1.0 초과하면 순차)
        let job1 = Job::new("JOB-001").with_priority(1).with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 30000, 0)
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers_load(1, 0.7), // 70% 부하
        );

        let job2 = Job::new("JOB-002").with_priority(2).with_operation(
            Operation::new("OP-002", "JOB-002", 1)
                .with_time(0, 30000, 0)
                .with_equipment(vec!["EQP-002".to_string()])
                .with_workers_load(1, 0.5), // 50% 부하
        );

        let resources = vec![
            Resource::equipment("EQP-001"),
            Resource::equipment("EQP-002"),
            Resource::worker("WRK-001"),
        ];

        let request = ScheduleRequest::new(vec![job1, job2], resources);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 0.7 + 0.5 = 1.2 > 1.0이므로 순차 처리
        let op1 = schedule.assignment_for_operation("OP-001").unwrap();
        let op2 = schedule.assignment_for_operation("OP-002").unwrap();

        assert_eq!(op1.start_ms, 0);
        // JOB-002는 JOB-001 종료 후 시작
        assert!(op2.start_ms >= op1.end_ms);
    }

    #[test]
    fn test_linked_resource_robot_equipment_cooccupation() {
        // Given: 로봇이 설비에 연결됨 (로봇 사용 시 설비도 점유)
        let job1 = Job::new("JOB-001").with_priority(1).with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 30000, 0)
                .with_equipment(vec!["ROBOT-001".to_string()]), // 로봇 사용
        );

        let job2 = Job::new("JOB-002").with_priority(2).with_operation(
            Operation::new("OP-002", "JOB-002", 1)
                .with_time(0, 20000, 0)
                .with_equipment(vec!["EQP-001".to_string()]), // 설비 직접 사용
        );

        let resources = vec![
            Resource::equipment("ROBOT-001")
                .as_robot()
                .with_linked_resource("EQP-001"), // 로봇이 EQP-001에 연결
            Resource::equipment("EQP-001"),
        ];

        let request = ScheduleRequest::new(vec![job1, job2], resources);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 로봇 사용 시 EQP-001도 점유되므로 JOB-002는 대기
        let op1 = schedule.assignment_for_operation("OP-001").unwrap();
        let op2 = schedule.assignment_for_operation("OP-002").unwrap();

        assert_eq!(op1.start_ms, 0);
        assert_eq!(op1.end_ms, 30000);
        // EQP-001이 로봇과 함께 점유되었으므로 로봇 작업 후 시작
        assert_eq!(op2.start_ms, 30000);
        assert_eq!(op2.end_ms, 50000);
    }

    #[test]
    fn test_linked_resource_bidirectional() {
        // Given: 로봇 사용 시 설비 점유, 설비 사용 시 로봇 점유 안됨
        let job1 = Job::new("JOB-001").with_priority(1).with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 30000, 0)
                .with_equipment(vec!["EQP-001".to_string()]), // 설비만 사용
        );

        let job2 = Job::new("JOB-002").with_priority(2).with_operation(
            Operation::new("OP-002", "JOB-002", 1)
                .with_time(0, 20000, 0)
                .with_equipment(vec!["ROBOT-001".to_string()]), // 로봇 사용
        );

        let resources = vec![
            Resource::equipment("ROBOT-001")
                .as_robot()
                .with_linked_resource("EQP-001"),
            Resource::equipment("EQP-001"),
        ];

        let request = ScheduleRequest::new(vec![job1, job2], resources);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then
        let op1 = schedule.assignment_for_operation("OP-001").unwrap();
        let op2 = schedule.assignment_for_operation("OP-002").unwrap();

        // JOB-001은 EQP-001만 사용 (로봇 점유 안함)
        assert_eq!(op1.start_ms, 0);
        // JOB-002는 로봇 사용 - EQP-001도 필요하므로 대기
        assert_eq!(op2.start_ms, 30000); // EQP-001이 해제된 후 시작
    }

    #[test]
    fn test_worker_load_tracker_basic() {
        // WorkerLoadTracker 단위 테스트
        let mut tracker = WorkerLoadTracker::new();

        // 0~30초 구간에 0.5 부하 추가
        tracker.add_load("WRK-001", 0, 30000, 0.5);

        // 같은 구간에 0.4 부하 추가 가능 (합계 0.9)
        assert!(tracker.can_accept_load("WRK-001", 0, 30000, 0.4));

        // 같은 구간에 0.6 부하 추가 불가 (합계 1.1)
        assert!(!tracker.can_accept_load("WRK-001", 0, 30000, 0.6));

        // 겹치지 않는 구간에는 어떤 부하든 가능
        assert!(tracker.can_accept_load("WRK-001", 30000, 60000, 1.0));
    }

    // ===== Phase 2.2 Tests: Calendar Constraints =====

    #[test]
    fn test_calendar_no_calendar_24_7() {
        // Given: 캘린더 없음 = 24/7 운영
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 60000, 0) // 1분
                .with_equipment(vec!["EQP-001".to_string()]),
        );

        let resource = Resource::equipment("EQP-001");
        // 캘린더 없이 요청

        let request = ScheduleRequest::new(vec![job], vec![resource]);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 캘린더 없으면 즉시 시작, 단순 덧셈 종료
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.start_ms, 0);
        assert_eq!(op.end_ms, 60000);
    }

    #[test]
    fn test_calendar_start_during_working_hours() {
        use crate::Calendar;

        // Given: 월요일 10시 시작, 주간 근무 캘린더
        let calendar = Calendar::default_day(); // 08:00-17:00, Mon-Fri, 점심 12:00-13:00

        // 2024-01-22 (월요일) 10:00 UTC
        let monday_10am = 1705881600000i64 + 10 * 60 * 60 * 1000;

        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 60 * 60 * 1000, 0) // 1시간
                .with_equipment(vec!["EQP-001".to_string()]),
        );

        let resource = Resource::equipment("EQP-001").with_calendar("CAL-DAY");

        let request = ScheduleRequest::new(vec![job], vec![resource])
            .with_start_time(monday_10am)
            .with_calendar(calendar);

        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 10시 시작, 11시 종료 (근무 시간 내)
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.start_ms, monday_10am);
        // 1시간 작업 = 60분 = 3600초
        assert_eq!(op.end_ms, monday_10am + 60 * 60 * 1000);
    }

    #[test]
    fn test_calendar_start_before_shift() {
        use crate::Calendar;

        // Given: 월요일 06시 시작 요청, 08시부터 근무
        let calendar = Calendar::default_day();

        // 2024-01-22 (월요일) 00:00 UTC
        let monday_midnight = 1705881600000i64;
        let monday_6am = monday_midnight + 6 * 60 * 60 * 1000;
        let monday_8am = monday_midnight + 8 * 60 * 60 * 1000;

        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 30 * 60 * 1000, 0) // 30분
                .with_equipment(vec!["EQP-001".to_string()]),
        );

        let resource = Resource::equipment("EQP-001").with_calendar("CAL-DAY");

        let request = ScheduleRequest::new(vec![job], vec![resource])
            .with_start_time(monday_6am)
            .with_calendar(calendar);

        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 06시 요청했지만 08시부터 시작
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.start_ms, monday_8am);
        // 30분 후 종료
        assert_eq!(op.end_ms, monday_8am + 30 * 60 * 1000);
    }

    #[test]
    fn test_calendar_work_spans_lunch_break() {
        use crate::Calendar;

        // Given: 11시 시작, 3시간 작업 (점심시간 걸침)
        let calendar = Calendar::default_day();

        // 2024-01-22 (월요일) 00:00 UTC
        let monday_midnight = 1705881600000i64;
        let monday_11am = monday_midnight + 11 * 60 * 60 * 1000;

        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 3 * 60 * 60 * 1000, 0) // 3시간 = 180분
                .with_equipment(vec!["EQP-001".to_string()]),
        );

        let resource = Resource::equipment("EQP-001").with_calendar("CAL-DAY");

        let request = ScheduleRequest::new(vec![job], vec![resource])
            .with_start_time(monday_11am)
            .with_calendar(calendar);

        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 11시 시작, 점심(12-13시) 제외하고 3시간 후 = 15시 종료
        // 11:00-12:00 = 1시간, 13:00-15:00 = 2시간, 총 3시간
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.start_ms, monday_11am);
        // 15시 = monday_midnight + 15시간
        let monday_3pm = monday_midnight + 15 * 60 * 60 * 1000;
        assert_eq!(op.end_ms, monday_3pm);
    }

    #[test]
    fn test_calendar_work_spans_weekend() {
        use crate::Calendar;

        // Given: 금요일 16시 시작, 2시간 작업 (주말 걸침)
        let calendar = Calendar::default_day();

        // 2024-01-19 (금요일) 00:00 UTC = 1705622400000
        let friday_midnight = 1705622400000i64;
        let friday_4pm = friday_midnight + 16 * 60 * 60 * 1000;

        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 2 * 60 * 60 * 1000, 0) // 2시간
                .with_equipment(vec!["EQP-001".to_string()]),
        );

        let resource = Resource::equipment("EQP-001").with_calendar("CAL-DAY");

        let request = ScheduleRequest::new(vec![job], vec![resource])
            .with_start_time(friday_4pm)
            .with_calendar(calendar);

        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 금요일 16:00 시작
        // 16:00-17:00 = 1시간 (금요일 종료)
        // 주말 건너뛰고 월요일 08:00-09:00 = 1시간
        // 총 2시간 후 = 월요일 09:00 종료
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.start_ms, friday_4pm);

        // 월요일 09:00 = 금요일 + 3일 + 9시간 - 16시간 = 금요일 + 3일 - 7시간
        // 금요일 00:00 + 3일 = 월요일 00:00 = 1705622400000 + 3*24*60*60*1000 = 1705881600000
        let monday_9am = 1705881600000i64 + 9 * 60 * 60 * 1000;
        assert_eq!(op.end_ms, monday_9am);
    }

    #[test]
    fn test_calendar_with_holiday() {
        use crate::Calendar;

        // Given: 월요일이 휴일인 경우
        // 2024-01-22 (월요일) 00:00 UTC = 1705881600000
        let monday_midnight = 1705881600000i64;
        let monday_10am = monday_midnight + 10 * 60 * 60 * 1000;

        // 월요일을 휴일로 설정
        let calendar = Calendar::default_day().with_holiday(monday_midnight);

        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 60 * 60 * 1000, 0) // 1시간
                .with_equipment(vec!["EQP-001".to_string()]),
        );

        let resource = Resource::equipment("EQP-001").with_calendar("CAL-DAY");

        let request = ScheduleRequest::new(vec![job], vec![resource])
            .with_start_time(monday_10am)
            .with_calendar(calendar);

        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 월요일 휴일이므로 화요일 08시부터 시작
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        // 화요일 08:00 = 월요일 + 1일 + 8시간
        let tuesday_8am = monday_midnight + 24 * 60 * 60 * 1000 + 8 * 60 * 60 * 1000;
        assert_eq!(op.start_ms, tuesday_8am);
        // 1시간 후 종료
        assert_eq!(op.end_ms, tuesday_8am + 60 * 60 * 1000);
    }

    // ===== Phase 3 Tests: SkillMatrix Integration =====

    #[test]
    fn test_skill_no_skill_matrix() {
        // Given: SkillMatrix 없이 기본 스케줄링
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 60000, 0) // 1분
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers(1),
        );

        let resource_eqp = Resource::equipment("EQP-001");
        let resource_wrk = Resource::worker("WRK-001");

        let request = ScheduleRequest::new(vec![job], vec![resource_eqp, resource_wrk]);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 기본 처리 시간 그대로 (1분)
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.start_ms, 0);
        assert_eq!(op.end_ms, 60000);
    }

    #[test]
    fn test_skill_proficiency_adjusts_time() {
        use crate::SkillMatrix;

        // Given: 숙련도 2.0인 작업자 (처리 시간 절반)
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_operation_type("welding") // 작업 유형 지정
                .with_time(0, 60000, 0) // 1분 기본 처리 시간
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers(1),
        );

        let resource_eqp = Resource::equipment("EQP-001");
        let resource_wrk = Resource::worker("WRK-001");

        // 숙련도 2.0 = 처리 시간이 절반
        let skill_matrix = SkillMatrix::new().with_skill("WRK-001", "welding", 2.0);

        let request = ScheduleRequest::new(vec![job], vec![resource_eqp, resource_wrk])
            .with_skill_matrix(skill_matrix);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 60000ms / 2.0 = 30000ms
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.start_ms, 0);
        assert_eq!(op.end_ms, 30000);
    }

    #[test]
    fn test_skill_low_proficiency_increases_time() {
        use crate::SkillMatrix;

        // Given: 숙련도 0.5인 작업자 (처리 시간 2배)
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_operation_type("assembly")
                .with_time(0, 60000, 0) // 1분
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers(1),
        );

        let resource_eqp = Resource::equipment("EQP-001");
        let resource_wrk = Resource::worker("WRK-001");

        let skill_matrix = SkillMatrix::new().with_skill("WRK-001", "assembly", 0.5);

        let request = ScheduleRequest::new(vec![job], vec![resource_eqp, resource_wrk])
            .with_skill_matrix(skill_matrix);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 60000ms / 0.5 = 120000ms
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.start_ms, 0);
        assert_eq!(op.end_ms, 120000);
    }

    #[test]
    fn test_skill_worker_cannot_perform() {
        use crate::SkillMatrix;

        // Given: 작업자가 해당 작업을 수행할 수 없는 경우 (숙련도 0)
        // 후보 없이 자동 선택
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_operation_type("welding")
                .with_time(0, 60000, 0)
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers(1), // 후보 없음 - 자동 선택
        );

        let resource_eqp = Resource::equipment("EQP-001");
        let resource_wrk1 = Resource::worker("WRK-001"); // welding 불가
        let resource_wrk2 = Resource::worker("WRK-002"); // welding 가능

        // WRK-001은 welding 숙련도 0, WRK-002는 1.0
        let skill_matrix = SkillMatrix::new()
            .with_skill("WRK-001", "welding", 0.0)
            .with_skill("WRK-002", "welding", 1.0);

        let request =
            ScheduleRequest::new(vec![job], vec![resource_eqp, resource_wrk1, resource_wrk2])
                .with_skill_matrix(skill_matrix);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: WRK-001 제외, WRK-002 할당
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        // WRK-002가 할당되어 정상 처리
        assert_eq!(op.end_ms - op.start_ms, 60000);
    }

    #[test]
    fn test_skill_fallback_to_operation_id() {
        use crate::SkillMatrix;

        // Given: operation_type 없이 operation.id를 키로 사용
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-WELDING", "JOB-001", 1)
                // operation_type 없음 - id가 키로 사용됨
                .with_time(0, 60000, 0)
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers(1),
        );

        let resource_eqp = Resource::equipment("EQP-001");
        let resource_wrk = Resource::worker("WRK-001");

        // operation ID를 키로 사용
        let skill_matrix = SkillMatrix::new().with_skill("WRK-001", "OP-WELDING", 2.0);

        let request = ScheduleRequest::new(vec![job], vec![resource_eqp, resource_wrk])
            .with_skill_matrix(skill_matrix);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 60000ms / 2.0 = 30000ms
        let op = schedule.assignment_for_operation("OP-WELDING").unwrap();
        assert_eq!(op.end_ms - op.start_ms, 30000);
    }

    #[test]
    fn test_skill_auto_filter_unqualified_workers() {
        use crate::SkillMatrix;

        // Given: 여러 작업자 중 스킬 있는 작업자만 자동 선택
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_operation_type("machining")
                .with_time(0, 60000, 0)
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers(1), // 후보 목록 없이 자동 선택
        );

        let resource_eqp = Resource::equipment("EQP-001");
        let resource_wrk1 = Resource::worker("WRK-001");
        let resource_wrk2 = Resource::worker("WRK-002");

        // WRK-001은 machining 불가, WRK-002만 가능
        let skill_matrix = SkillMatrix::new()
            .with_skill("WRK-001", "machining", 0.0)
            .with_skill("WRK-002", "machining", 1.5);

        let request =
            ScheduleRequest::new(vec![job], vec![resource_eqp, resource_wrk1, resource_wrk2])
                .with_skill_matrix(skill_matrix);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: WRK-002 할당, 숙련도 1.5 적용
        // 60000ms / 1.5 = 40000ms
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.end_ms - op.start_ms, 40000);
    }

    #[test]
    fn test_material_constraint_availability_date() {
        use crate::{Material, MaterialManager, MaterialRequirement};
        use chrono::TimeZone;

        // Given: 자재 가용일이 미래인 경우
        let future_time = Utc.with_ymd_and_hms(2025, 1, 1, 12, 0, 0).unwrap();
        let start_time = Utc.with_ymd_and_hms(2025, 1, 1, 8, 0, 0).unwrap();

        let material_req = MaterialRequirement::new("MAT-001", 10.0).with_availability(future_time);

        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 60000, 0) // 1분
                .with_equipment(vec!["EQP-001".to_string()])
                .with_material(material_req),
        );

        let resource = Resource::equipment("EQP-001");

        let mut material_manager = MaterialManager::new();
        material_manager.add_material(Material::new("MAT-001", "Steel Plate").with_stock(100.0));

        let request = ScheduleRequest::new(vec![job], vec![resource])
            .with_start_time(start_time.timestamp_millis())
            .with_material_manager(material_manager);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 자재 가용일까지 지연되어 시작
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        // 시작 시간이 자재 가용일 이후여야 함
        assert!(op.start_ms >= future_time.timestamp_millis());
        // 위반이 기록되어야 함
        assert!(!schedule.violations.is_empty());
        let violation = &schedule.violations[0];
        assert_eq!(
            violation.constraint_type,
            super::super::ConstraintType::MaterialAvailability
        );
    }

    #[test]
    fn test_material_constraint_shortage() {
        use crate::{Material, MaterialManager, MaterialRequirement};
        use chrono::TimeZone;

        // Given: 재고가 부족한 경우
        let start_time = Utc.with_ymd_and_hms(2025, 1, 1, 8, 0, 0).unwrap();

        let material_req = MaterialRequirement::new("MAT-001", 50.0); // 50개 필요

        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 60000, 0)
                .with_equipment(vec!["EQP-001".to_string()])
                .with_material(material_req),
        );

        let resource = Resource::equipment("EQP-001");

        let mut material_manager = MaterialManager::new();
        material_manager.add_material(
            Material::new("MAT-001", "Steel Plate")
                .with_stock(30.0) // 30개만 있음
                .with_safety_stock(10.0), // 안전재고 10개 → 가용 20개
        );

        let request = ScheduleRequest::new(vec![job], vec![resource])
            .with_start_time(start_time.timestamp_millis())
            .with_material_manager(material_manager);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 재고 부족 위반 기록
        assert!(!schedule.violations.is_empty());
        let violation = &schedule.violations[0];
        assert_eq!(
            violation.constraint_type,
            super::super::ConstraintType::MaterialShortage
        );
        assert_eq!(violation.amount, 30.0); // 50 - 20 = 30 부족
    }

    #[test]
    fn test_material_constraint_sufficient_stock() {
        use crate::{Material, MaterialManager, MaterialRequirement};
        use chrono::TimeZone;

        // Given: 재고가 충분한 경우
        let start_time = Utc.with_ymd_and_hms(2025, 1, 1, 8, 0, 0).unwrap();

        let material_req = MaterialRequirement::new("MAT-001", 20.0);

        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 60000, 0)
                .with_equipment(vec!["EQP-001".to_string()])
                .with_material(material_req),
        );

        let resource = Resource::equipment("EQP-001");

        let mut material_manager = MaterialManager::new();
        material_manager.add_material(Material::new("MAT-001", "Steel Plate").with_stock(100.0));

        let request = ScheduleRequest::new(vec![job], vec![resource])
            .with_start_time(start_time.timestamp_millis())
            .with_material_manager(material_manager);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 위반 없음
        assert!(schedule.violations.is_empty());
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.start_ms, start_time.timestamp_millis());
    }

    #[test]
    fn test_certification_filters_uncertified_workers() {
        use crate::{CertificationLevel, CertificationMatrix};

        // Given: 인증된 작업자만 작업 가능
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_operation_type("welding")
                .with_time(0, 60000, 0)
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers(1),
        );

        let resource_eqp = Resource::equipment("EQP-001");
        let resource_wrk1 = Resource::worker("WRK-001");
        let resource_wrk2 = Resource::worker("WRK-002");

        let now = 1_000_000_000i64;
        let day_ms = 24 * 3_600_000;

        // WRK-001은 인증 없음, WRK-002는 Standard 인증
        let cert_matrix = CertificationMatrix::new().with_operation_cert(
            "WRK-002",
            "welding",
            CertificationLevel::Standard,
            now - day_ms,
        );

        let request =
            ScheduleRequest::new(vec![job], vec![resource_eqp, resource_wrk1, resource_wrk2])
                .with_start_time(now)
                .with_certification_matrix(cert_matrix);

        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 작업 스케줄됨 (WRK-002가 할당됨)
        assert!(!schedule.assignments.is_empty());
    }

    #[test]
    fn test_certification_adjusts_process_time() {
        use crate::{CertificationLevel, CertificationMatrix, SkillMatrix};

        // Given: Master 인증 (효율 1.3)
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_operation_type("machining")
                .with_time(0, 60000, 0)
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers(1),
        );

        let resource_eqp = Resource::equipment("EQP-001");
        let resource_wrk = Resource::worker("WRK-001");

        let now = 1_000_000_000i64;
        let day_ms = 24 * 3_600_000;

        // 스킬 1.0 (기본), 인증 Master (1.3)
        let skill_matrix = SkillMatrix::new().with_skill("WRK-001", "machining", 1.0);

        let cert_matrix = CertificationMatrix::new().with_operation_cert(
            "WRK-001",
            "machining",
            CertificationLevel::Master,
            now - day_ms,
        );

        let request = ScheduleRequest::new(vec![job], vec![resource_eqp, resource_wrk])
            .with_start_time(now)
            .with_skill_matrix(skill_matrix)
            .with_certification_matrix(cert_matrix);

        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 60000 / 1.0 (스킬) / 1.3 (인증) ≈ 46153ms
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        let duration = op.end_ms - op.start_ms;
        assert!(duration < 50000, "Expected < 50000ms, got {}", duration);
    }

    #[test]
    fn test_certification_expired_worker_filtered() {
        use crate::{Certification, CertificationLevel, CertificationMatrix, CertificationTarget};

        // Given: 만료된 인증
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_operation_type("welding")
                .with_time(0, 60000, 0)
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers(1),
        );

        let resource_eqp = Resource::equipment("EQP-001");
        let resource_wrk1 = Resource::worker("WRK-001");
        let resource_wrk2 = Resource::worker("WRK-002");

        let now = 1_000_000_000i64;
        let day_ms = 24 * 3_600_000;

        // WRK-001은 만료된 인증, WRK-002는 유효한 인증
        let mut cert_matrix = CertificationMatrix::new();

        let expired_cert = Certification::new(
            "CERT-001",
            "WRK-001",
            "welding",
            CertificationTarget::OperationType,
            CertificationLevel::Advanced,
            now - day_ms * 100,
        )
        .with_expiry(now - day_ms * 10); // 10일 전 만료
        cert_matrix.add_certification(expired_cert);

        let valid_cert = Certification::new(
            "CERT-002",
            "WRK-002",
            "welding",
            CertificationTarget::OperationType,
            CertificationLevel::Standard,
            now - day_ms * 30,
        )
        .with_expiry(now + day_ms * 30); // 30일 후 만료
        cert_matrix.add_certification(valid_cert);

        let request =
            ScheduleRequest::new(vec![job], vec![resource_eqp, resource_wrk1, resource_wrk2])
                .with_start_time(now)
                .with_certification_matrix(cert_matrix);

        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: WRK-002 할당 (WRK-001은 인증 만료로 제외)
        assert!(!schedule.assignments.is_empty());
    }

    #[test]
    fn test_crew_manager_integration() {
        use crate::{Crew, CrewManager, TeamRole};

        // Given: 팀 관리자 설정
        let crew_manager = CrewManager::new().with_crew(
            Crew::new("TEAM-A", "Assembly Team A")
                .with_efficiency(1.2)
                .with_operation_type("assembly")
                .with_worker("WRK-001", TeamRole::Leader)
                .with_worker("WRK-002", TeamRole::Member),
        );

        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_operation_type("assembly")
                .with_time(0, 60000, 0)
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers(1),
        );

        let resource_eqp = Resource::equipment("EQP-001");
        let resource_wrk1 = Resource::worker("WRK-001");
        let resource_wrk2 = Resource::worker("WRK-002");

        let request =
            ScheduleRequest::new(vec![job], vec![resource_eqp, resource_wrk1, resource_wrk2])
                .with_crew_manager(crew_manager);

        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 스케줄 생성됨
        assert!(!schedule.assignments.is_empty());
    }

    #[test]
    fn test_crew_based_allocation() {
        use crate::{Crew, CrewManager, TeamRole};

        // Given: 팀 설정 (WRK-001 리더, WRK-002 멤버)
        let crew_manager = CrewManager::new().with_crew(
            Crew::new("TEAM-A", "Assembly Team A")
                .with_efficiency(1.5) // 50% 효율 향상
                .with_capacity(2, 5)
                .with_worker("WRK-001", TeamRole::Leader)
                .with_worker("WRK-002", TeamRole::Member),
        );

        // Operation에 crew_id 설정
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 60000, 0) // 60초
                .with_equipment(vec!["EQP-001".to_string()])
                .with_crew("TEAM-A"), // 팀 기반 할당 요청
        );

        let resource_eqp = Resource::equipment("EQP-001");
        let resource_wrk1 = Resource::worker("WRK-001");
        let resource_wrk2 = Resource::worker("WRK-002");

        let request =
            ScheduleRequest::new(vec![job], vec![resource_eqp, resource_wrk1, resource_wrk2])
                .with_crew_manager(crew_manager);

        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: 스케줄 생성됨
        assert!(!schedule.assignments.is_empty());

        // 팀 멤버들이 모두 할당되었는지 확인 (각 자원별로 Assignment 생성됨)
        let op_assignments: Vec<_> = schedule
            .assignments
            .iter()
            .filter(|a| a.operation_id == "OP-001")
            .collect();

        assert!(
            op_assignments.iter().any(|a| a.resource_id == "WRK-001"),
            "WRK-001 should be assigned"
        );
        assert!(
            op_assignments.iter().any(|a| a.resource_id == "WRK-002"),
            "WRK-002 should be assigned"
        );

        // 팀 효율(1.5)이 적용되어 처리 시간이 단축되었는지 확인
        // 60000ms / 1.5 = 40000ms
        let equipment_assignment = op_assignments
            .iter()
            .find(|a| a.resource_id == "EQP-001")
            .unwrap();
        let duration = equipment_assignment.end_ms - equipment_assignment.start_ms;
        assert_eq!(
            duration, 40000,
            "Team efficiency should reduce process time"
        );
    }

    #[test]
    fn test_crew_shift_schedule() {
        use crate::ShiftSchedule;

        // Given: 교대 근무 일정 설정
        let day_shift = ShiftSchedule::new("DAY", "Day Shift")
            .with_hours(8, 16)
            .with_days(vec![1, 2, 3, 4, 5]) // 월-금
            .with_handover_minutes(15); // 15분 인수인계

        let night_shift = ShiftSchedule::new("NIGHT", "Night Shift")
            .with_hours(22, 6) // 야간 (자정을 넘어감)
            .with_days(vec![1, 2, 3, 4, 5])
            .with_handover_minutes(15);

        // Then: 교대 시간 검증
        assert_eq!(day_shift.duration_ms(), 8 * 3_600_000);
        assert_eq!(night_shift.duration_ms(), 8 * 3_600_000);

        // 실제 작업 가능 시간 (인수인계 제외)
        assert_eq!(
            day_shift.effective_duration_ms(),
            8 * 3_600_000 - 15 * 60_000
        );
        assert_eq!(
            night_shift.effective_duration_ms(),
            8 * 3_600_000 - 15 * 60_000
        );
    }

    #[test]
    fn test_crews_on_shift() {
        use crate::{Crew, CrewManager, ShiftSchedule, TeamRole, TeamType};

        // Given: 교대 팀 설정
        let mut manager = CrewManager::new();

        // 주간 팀
        let day_team = Crew::new("DAY-TEAM", "Day Team")
            .with_type(TeamType::Shift)
            .with_worker("W1", TeamRole::Leader)
            .with_worker("W2", TeamRole::Member);
        manager.add_crew(day_team);

        // 야간 팀
        let night_team = Crew::new("NIGHT-TEAM", "Night Team")
            .with_type(TeamType::Shift)
            .with_worker("W3", TeamRole::Leader)
            .with_worker("W4", TeamRole::Member);
        manager.add_crew(night_team);

        // 고정 팀 (교대 할당 없음)
        let fixed_team = Crew::new("FIXED-TEAM", "Fixed Team")
            .with_type(TeamType::Fixed)
            .with_worker("W5", TeamRole::Leader)
            .with_worker("W6", TeamRole::Member);
        manager.add_crew(fixed_team);

        // 교대 일정 추가
        manager.add_shift(
            ShiftSchedule::new("DAY", "Day Shift")
                .with_hours(8, 16)
                .with_days(vec![1, 2, 3, 4, 5]),
        );
        manager.add_shift(
            ShiftSchedule::new("NIGHT", "Night Shift")
                .with_hours(22, 6)
                .with_days(vec![1, 2, 3, 4, 5]),
        );

        // 교대 할당
        // 월요일 00:00:00 UTC = 1970-01-05 (목요일 다음 월요일)
        let monday_8am = 4 * 24 * 3_600_000 + 8 * 3_600_000; // 1970-01-05 08:00 UTC (월요일)
        manager.assign_crew_to_shift("DAY-TEAM", "DAY", 0);
        manager.assign_crew_to_shift("NIGHT-TEAM", "NIGHT", 0);

        // When: 주간 시간에 근무하는 팀 조회
        let crews = manager.crews_on_shift(monday_8am);

        // Then: 고정 팀은 항상 포함, 주간 팀도 포함
        assert!(
            crews.iter().any(|c| c.id == "FIXED-TEAM"),
            "Fixed team should always be available"
        );
        assert!(
            crews.iter().any(|c| c.id == "DAY-TEAM"),
            "Day team should be on shift at 8am"
        );
        assert!(
            !crews.iter().any(|c| c.id == "NIGHT-TEAM"),
            "Night team should not be on shift at 8am"
        );
    }

    // ========== WorkerTimeOverride Tests ==========

    #[test]
    fn test_worker_time_override_process_time() {
        use crate::WorkerTimeOverride;

        // Given: Worker with explicit process time override
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_operation_type("welding")
                .with_time(0, 60000, 0) // 1 minute base
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers(1),
        );

        let resource_eqp = Resource::equipment("EQP-001");
        let resource_wrk = Resource::worker("WRK-001");

        // Override: WRK-001 does welding in 20 seconds (explicit)
        let override_matrix =
            WorkerTimeOverride::new().with_process_time("WRK-001", "welding", 20000);

        let request = ScheduleRequest::new(vec![job], vec![resource_eqp, resource_wrk])
            .with_worker_time_override(override_matrix);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: Should use override time (20000ms), not default (60000ms)
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.start_ms, 0);
        assert_eq!(op.end_ms, 20000);
    }

    #[test]
    fn test_worker_time_override_full_time() {
        use crate::WorkerTimeOverride;

        // Given: Full time override (setup + process + wait)
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_operation_type("assembly")
                .with_time(10000, 60000, 5000) // setup=10s, process=1m, wait=5s
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers(1),
        );

        let resource_eqp = Resource::equipment("EQP-001");
        let resource_wrk = Resource::worker("WRK-001");

        // Override: explicit times - setup=5s, process=30s, wait=2s
        let override_matrix =
            WorkerTimeOverride::new().with_full_time("WRK-001", "assembly", 5000, 30000, 2000);

        let request = ScheduleRequest::new(vec![job], vec![resource_eqp, resource_wrk])
            .with_worker_time_override(override_matrix);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: Total duration = setup(5000) + process(30000) + wait(2000) = 37000ms
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.start_ms, 0);
        assert_eq!(op.end_ms, 37000);
    }

    #[test]
    fn test_worker_time_override_priority_over_skill_matrix() {
        use crate::{SkillMatrix, WorkerTimeOverride};

        // Given: Both override and skill matrix defined
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_operation_type("machining")
                .with_time(0, 60000, 0) // 1 minute base
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers(1),
        );

        let resource_eqp = Resource::equipment("EQP-001");
        let resource_wrk = Resource::worker("WRK-001");

        // Skill matrix: 2.0 efficiency would make it 30s
        let skill_matrix = SkillMatrix::new().with_skill("WRK-001", "machining", 2.0);

        // Override: explicit 45s (should win over skill matrix)
        let override_matrix =
            WorkerTimeOverride::new().with_process_time("WRK-001", "machining", 45000);

        let request = ScheduleRequest::new(vec![job], vec![resource_eqp, resource_wrk])
            .with_skill_matrix(skill_matrix)
            .with_worker_time_override(override_matrix);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: Override (45000ms) wins over skill-adjusted (30000ms)
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.start_ms, 0);
        assert_eq!(op.end_ms, 45000);
    }

    #[test]
    fn test_worker_time_override_no_override_uses_skill_matrix() {
        use crate::{SkillMatrix, WorkerTimeOverride};

        // Given: Override for different worker, skill matrix applies
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_operation_type("welding")
                .with_time(0, 60000, 0) // 1 minute base
                .with_equipment(vec!["EQP-001".to_string()])
                .with_workers(1),
        );

        let resource_eqp = Resource::equipment("EQP-001");
        let resource_wrk = Resource::worker("WRK-001");

        // Skill matrix for WRK-001
        let skill_matrix = SkillMatrix::new().with_skill("WRK-001", "welding", 2.0);

        // Override for different worker (WRK-002), not WRK-001
        let override_matrix =
            WorkerTimeOverride::new().with_process_time("WRK-002", "welding", 45000);

        let request = ScheduleRequest::new(vec![job], vec![resource_eqp, resource_wrk])
            .with_skill_matrix(skill_matrix)
            .with_worker_time_override(override_matrix);
        let scheduler = SimpleScheduler::new();

        // When
        let schedule = scheduler.schedule(&request);

        // Then: No override for WRK-001, so skill matrix applies (60000/2.0 = 30000ms)
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.start_ms, 0);
        assert_eq!(op.end_ms, 30000);
    }

    // ===== Multi-Site Tests =====

    #[test]
    fn test_multi_site_basic_filtering() {
        // Given: 2개 사이트, 각 사이트에 장비 1대, Job은 SITE-A 지정
        let job = Job::new("JOB-001").with_site("SITE-A").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 30000, 0)
                .with_equipment(vec!["EQP-A1".to_string(), "EQP-B1".to_string()]),
        );

        let resources = vec![
            Resource::equipment("EQP-A1").with_site("SITE-A"),
            Resource::equipment("EQP-B1").with_site("SITE-B"),
        ];

        let request = ScheduleRequest::new(vec![job], resources);
        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        // Then: SITE-A 장비만 할당됨
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.resource_id, "EQP-A1");
        assert_eq!(op.site_id.as_deref(), Some("SITE-A"));
    }

    #[test]
    fn test_multi_site_no_constraint() {
        // Given: Job에 site 미지정 → 가장 빠른 리소스 할당
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 30000, 0)
                .with_equipment(vec!["EQP-A1".to_string(), "EQP-B1".to_string()]),
        );

        let resources = vec![
            Resource::equipment("EQP-A1").with_site("SITE-A"),
            Resource::equipment("EQP-B1").with_site("SITE-B"),
        ];

        let request = ScheduleRequest::new(vec![job], resources);
        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        // Then: 둘 다 가용하므로 첫 번째(EQP-A1) 할당
        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert!(op.site_id.is_some());
    }

    #[test]
    fn test_multi_site_transition_time() {
        // Given: 2개 사이트, 사이트 간 이동 시간 1시간
        // Job의 OP-001은 SITE-A, OP-002는 SITE-B에서 수행
        let job = Job::new("JOB-001")
            .with_operation(
                Operation::new("OP-001", "JOB-001", 1)
                    .with_time(0, 30000, 0)
                    .with_equipment(vec!["EQP-A1".to_string()]),
            )
            .with_operation(
                Operation::new("OP-002", "JOB-001", 2)
                    .with_time(0, 30000, 0)
                    .with_equipment(vec!["EQP-B1".to_string()]),
            );

        let resources = vec![
            Resource::equipment("EQP-A1").with_site("SITE-A"),
            Resource::equipment("EQP-B1").with_site("SITE-B"),
        ];

        let mut transitions = crate::SiteTransitions::new();
        transitions.add_bidirectional("SITE-A", "SITE-B", 3_600_000); // 1시간

        let request = ScheduleRequest::new(vec![job], resources).with_site_transitions(transitions);
        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        // Then: OP-002는 OP-001 종료 + 이동시간 이후 시작
        let op1 = schedule.assignment_for_operation("OP-001").unwrap();
        let op2 = schedule.assignment_for_operation("OP-002").unwrap();
        assert_eq!(op1.end_ms, 30000);
        // OP-002 시작 = max(op1.end + transition, resource_available) = 30000 + 3600000
        assert_eq!(op2.start_ms, 3_630_000);
        assert_eq!(op2.site_id.as_deref(), Some("SITE-B"));
    }

    #[test]
    fn test_multi_site_same_site_no_transition() {
        // Given: 같은 사이트 내 이동은 transition 없음
        let job = Job::new("JOB-001")
            .with_operation(
                Operation::new("OP-001", "JOB-001", 1)
                    .with_time(0, 30000, 0)
                    .with_equipment(vec!["EQP-A1".to_string()]),
            )
            .with_operation(
                Operation::new("OP-002", "JOB-001", 2)
                    .with_time(0, 30000, 0)
                    .with_equipment(vec!["EQP-A2".to_string()]),
            );

        let resources = vec![
            Resource::equipment("EQP-A1").with_site("SITE-A"),
            Resource::equipment("EQP-A2").with_site("SITE-A"),
        ];

        let mut transitions = crate::SiteTransitions::new();
        transitions.add_bidirectional("SITE-A", "SITE-B", 3_600_000);

        let request = ScheduleRequest::new(vec![job], resources).with_site_transitions(transitions);
        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        // Then: 같은 사이트이므로 transition 없음
        let op1 = schedule.assignment_for_operation("OP-001").unwrap();
        let op2 = schedule.assignment_for_operation("OP-002").unwrap();
        assert_eq!(op1.end_ms, 30000);
        assert_eq!(op2.start_ms, 30000); // 바로 시작
    }

    #[test]
    fn test_multi_site_backward_compatible() {
        // Given: site_id 없는 기존 데이터 → 정상 동작
        let job = Job::new("JOB-001").with_operation(
            Operation::new("OP-001", "JOB-001", 1)
                .with_time(0, 30000, 0)
                .with_equipment(vec!["EQP-001".to_string()]),
        );

        let resources = vec![Resource::equipment("EQP-001")]; // site_id 없음

        let request = ScheduleRequest::new(vec![job], resources);
        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        let op = schedule.assignment_for_operation("OP-001").unwrap();
        assert_eq!(op.resource_id, "EQP-001");
        assert!(op.site_id.is_none()); // site_id 없음
    }

    #[test]
    fn test_schedule_assignments_for_site() {
        let mut schedule = Schedule::new();
        schedule.add_assignment({
            let mut a = Assignment::new("OP-001", "JOB-001", "EQP-A1", 0, 30000);
            a.site_id = Some("SITE-A".to_string());
            a
        });
        schedule.add_assignment({
            let mut a = Assignment::new("OP-002", "JOB-001", "EQP-B1", 30000, 60000);
            a.site_id = Some("SITE-B".to_string());
            a
        });

        assert_eq!(schedule.assignments_for_site("SITE-A").len(), 1);
        assert_eq!(schedule.assignments_for_site("SITE-B").len(), 1);
        assert_eq!(schedule.assignments_for_site("SITE-C").len(), 0);
    }
}
