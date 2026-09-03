//! FFI - C# P/Invoke를 위한 외부 인터페이스

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

use crate::ga::GaParams;
use crate::scheduler::dispatching::DispatchingConfig;
use crate::scheduler::engine::{
    EngineConfig, PerformanceMetrics, SchedulingAlgorithm, SchedulingEngine,
    DEFAULT_ENGINE_TIMEOUT_MS,
};
use crate::scheduler::kpi::ScheduleKpi;
use crate::scheduler::pegging::PeggingEngine;
use crate::scheduler::reschedule::{
    RescheduleResult, RescheduleStrategy, Rescheduler, ScheduleEvent,
};
use crate::validation::Validator;
use crate::{
    Calendar, MaterialManager, SetupMatrixCollection, SiteTransitions, SkillMatrix,
    WorkerTimeOverride,
};
use crate::{CertificationMatrix, CrewManager, OutsourcingManager};
use crate::{Job, Resource, Schedule, ScheduleRequest, SimpleScheduler};

/// 스케줄링 요청 (JSON)
#[derive(serde::Deserialize)]
struct FfiScheduleRequest {
    jobs: Vec<Job>,
    resources: Vec<Resource>,
    start_time_ms: Option<i64>,
    // 알고리즘 설정 (Optional — 기본값: Simple)
    #[serde(default)]
    algorithm: Option<SchedulingAlgorithm>,
    #[serde(default)]
    ga_params: Option<GaParams>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    validate_input: Option<bool>,
    // 확장 필드 (Optional)
    #[serde(default)]
    setup_matrices: Option<SetupMatrixCollection>,
    #[serde(default)]
    calendars: Option<Vec<Calendar>>,
    #[serde(default)]
    skill_matrix: Option<SkillMatrix>,
    #[serde(default)]
    worker_time_override: Option<WorkerTimeOverride>,
    #[serde(default)]
    dispatching_config: Option<DispatchingConfig>,
    #[serde(default)]
    material_manager: Option<MaterialManager>,
    #[serde(default)]
    certification_matrix: Option<CertificationMatrix>,
    #[serde(default)]
    crew_manager: Option<CrewManager>,
    #[serde(default)]
    pegging_engine: Option<PeggingEngine>,
    #[serde(default)]
    outsourcing_manager: Option<OutsourcingManager>,
    #[serde(default)]
    site_transitions: Option<SiteTransitions>,
}

/// 스케줄링 응답 (JSON)
#[derive(serde::Serialize, serde::Deserialize)]
struct FfiScheduleResponse {
    success: bool,
    schedule: Option<Schedule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    kpi: Option<ScheduleKpi>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metrics: Option<PerformanceMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    warnings: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// 검증 응답 (JSON)
#[derive(serde::Serialize, serde::Deserialize)]
struct FfiValidationResponse {
    valid: bool,
    errors: Vec<String>,
    warnings: Vec<String>,
}

/// 스케줄링 실행
///
/// algorithm 필드가 없거나 null이면 Simple 스케줄러 사용.
/// algorithm 필드 지정 시 SchedulingEngine을 통해 해당 알고리즘 실행.
///
/// # Arguments
/// * `request_json` - JSON 형식의 스케줄링 요청
/// * `result_ptr` - 결과 JSON 문자열 포인터 (호출자가 uaps_free_string으로 해제)
///
/// # Returns
/// * 0: 성공
/// * -1: JSON 파싱 에러
/// * -2: 스케줄링 에러
///
/// # Safety
///
/// `request_json` must be a valid pointer to a null-terminated UTF-8 string.
/// `result_ptr` must be a valid pointer to a `*mut c_char` that this function will write to.
/// The caller must free the result string with `uaps_free_string`.
#[no_mangle]
pub unsafe extern "C" fn uaps_schedule(
    request_json: *const c_char,
    result_ptr: *mut *mut c_char,
) -> i32 {
    // Null 체크
    if request_json.is_null() || result_ptr.is_null() {
        return -1;
    }

    // JSON 파싱
    let request_str = match CStr::from_ptr(request_json).to_str() {
        Ok(s) => s,
        Err(_) => {
            let response = FfiScheduleResponse {
                success: false,
                schedule: None,
                kpi: None,
                metrics: None,
                warnings: None,
                error: Some("Invalid UTF-8 in request".to_string()),
            };
            *result_ptr = response_to_ptr(&response);
            return -1;
        }
    };

    let ffi_request: FfiScheduleRequest = match serde_json::from_str(request_str) {
        Ok(r) => r,
        Err(e) => {
            let response = FfiScheduleResponse {
                success: false,
                schedule: None,
                kpi: None,
                metrics: None,
                warnings: None,
                error: Some(format!("JSON parse error: {}", e)),
            };
            *result_ptr = response_to_ptr(&response);
            return -1;
        }
    };

    // algorithm 필드 유무에 따라 분기
    match ffi_request.algorithm {
        Some(algorithm) => {
            // SchedulingEngine 경로: algorithm, KPI, metrics, validation 모두 지원
            let config = EngineConfig {
                algorithm,
                ga_params: ffi_request.ga_params,
                setup_matrices: ffi_request.setup_matrices.unwrap_or_default(),
                site_transitions: ffi_request.site_transitions.unwrap_or_default(),
                calendars: ffi_request.calendars.unwrap_or_default(),
                skill_matrix: ffi_request.skill_matrix,
                worker_time_override: ffi_request.worker_time_override,
                dispatching_config: ffi_request.dispatching_config,
                material_manager: ffi_request.material_manager,
                certification_matrix: ffi_request.certification_matrix,
                crew_manager: ffi_request.crew_manager,
                pegging_engine: ffi_request.pegging_engine,
                outsourcing_manager: ffi_request.outsourcing_manager,
                validate_input: ffi_request.validate_input.unwrap_or(true),
                // JSON이 timeout_ms를 생략하면 엔진 기본 타임아웃으로 폴백한다 — 그냥
                // 통과시키면 None이 되어 GA/Production/CP-SAT이 무제한 실행된다(SDK가
                // 아직 이 필드를 노출하지 않아 실제 FFI 호출은 거의 항상 생략 경로).
                timeout_ms: ffi_request.timeout_ms.or(Some(DEFAULT_ENGINE_TIMEOUT_MS)),
                ..Default::default()
            };

            let mut engine = SchedulingEngine::new(config);
            let start_time = ffi_request.start_time_ms.unwrap_or(0);

            match engine.schedule(&ffi_request.jobs, &ffi_request.resources, start_time) {
                Ok(result) => {
                    let response = FfiScheduleResponse {
                        success: true,
                        schedule: Some(result.schedule),
                        kpi: Some(result.kpi),
                        metrics: Some(result.metrics),
                        warnings: if result.warnings.is_empty() {
                            None
                        } else {
                            Some(result.warnings)
                        },
                        error: None,
                    };
                    *result_ptr = response_to_ptr(&response);
                    0
                }
                Err(e) => {
                    let response = FfiScheduleResponse {
                        success: false,
                        schedule: None,
                        kpi: None,
                        metrics: None,
                        warnings: None,
                        error: Some(format!("Scheduling error: {}", e)),
                    };
                    *result_ptr = response_to_ptr(&response);
                    -2
                }
            }
        }
        None => {
            // 레거시 경로: SimpleScheduler 직접 호출 (하위 호환성)
            let request = ScheduleRequest {
                jobs: ffi_request.jobs,
                resources: ffi_request.resources,
                start_time_ms: ffi_request.start_time_ms.unwrap_or(0),
                setup_matrices: ffi_request.setup_matrices.unwrap_or_default(),
                calendars: ffi_request.calendars.unwrap_or_default(),
                skill_matrix: ffi_request.skill_matrix,
                worker_time_override: ffi_request.worker_time_override,
                dispatching_config: ffi_request.dispatching_config,
                material_manager: ffi_request.material_manager,
                certification_matrix: ffi_request.certification_matrix,
                crew_manager: ffi_request.crew_manager,
                pegging_engine: ffi_request.pegging_engine,
                outsourcing_manager: ffi_request.outsourcing_manager,
                site_transitions: ffi_request.site_transitions,
            };

            let scheduler = SimpleScheduler::new();
            let schedule = scheduler.schedule(&request);

            let response = FfiScheduleResponse {
                success: true,
                schedule: Some(schedule),
                kpi: None,
                metrics: None,
                warnings: None,
                error: None,
            };

            *result_ptr = response_to_ptr(&response);
            0
        }
    }
}

/// 입력 검증 (스케줄링 없이)
///
/// # Safety
///
/// `request_json` must be a valid pointer to a null-terminated UTF-8 string.
/// `result_ptr` must be a valid pointer to a `*mut c_char` that this function will write to.
/// The caller must free the result string with `uaps_free_string`.
#[no_mangle]
pub unsafe extern "C" fn uaps_validate(
    request_json: *const c_char,
    result_ptr: *mut *mut c_char,
) -> i32 {
    if request_json.is_null() || result_ptr.is_null() {
        return -1;
    }

    let request_str = match CStr::from_ptr(request_json).to_str() {
        Ok(s) => s,
        Err(_) => {
            let response = FfiValidationResponse {
                valid: false,
                errors: vec!["Invalid UTF-8 in request".to_string()],
                warnings: Vec::new(),
            };
            *result_ptr = serialize_to_ptr(&response);
            return -1;
        }
    };

    let ffi_request: FfiScheduleRequest = match serde_json::from_str(request_str) {
        Ok(r) => r,
        Err(e) => {
            let response = FfiValidationResponse {
                valid: false,
                errors: vec![format!("JSON parse error: {}", e)],
                warnings: Vec::new(),
            };
            *result_ptr = serialize_to_ptr(&response);
            return -1;
        }
    };

    let operations: Vec<_> = ffi_request
        .jobs
        .iter()
        .flat_map(|j| j.operations.iter().cloned())
        .collect();

    let result = Validator::validate_schedule_request(
        &ffi_request.jobs,
        &operations,
        &ffi_request.resources,
    );

    let response = FfiValidationResponse {
        valid: result.is_valid(),
        errors: result
            .errors
            .into_iter()
            .map(|e| format!("{}", e))
            .collect(),
        warnings: result.warnings,
    };

    *result_ptr = serialize_to_ptr(&response);
    0
}

/// 리스케줄링 요청 (JSON)
#[derive(serde::Deserialize)]
struct FfiRescheduleRequest {
    /// 현재 스케줄
    schedule: Schedule,
    /// 이벤트
    event: ScheduleEvent,
    /// 전략 (None이면 auto_reschedule)
    #[serde(default)]
    strategy: Option<RescheduleStrategy>,
}

/// 리스케줄링 응답 (JSON)
#[derive(serde::Serialize, serde::Deserialize)]
struct FfiRescheduleResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<RescheduleResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// 이벤트 기반 리스케줄링
///
/// 기존 스케줄에 이벤트를 적용하여 새 스케줄을 생성.
/// strategy가 없으면 이벤트 유형에 따라 자동 선택.
///
/// # Safety
///
/// `request_json` must be a valid pointer to a null-terminated UTF-8 string.
/// `result_ptr` must be a valid pointer to a `*mut c_char` that this function will write to.
/// The caller must free the result string with `uaps_free_string`.
#[no_mangle]
pub unsafe extern "C" fn uaps_reschedule(
    request_json: *const c_char,
    result_ptr: *mut *mut c_char,
) -> i32 {
    if request_json.is_null() || result_ptr.is_null() {
        return -1;
    }

    let request_str = match CStr::from_ptr(request_json).to_str() {
        Ok(s) => s,
        Err(_) => {
            let response = FfiRescheduleResponse {
                success: false,
                result: None,
                error: Some("Invalid UTF-8 in request".to_string()),
            };
            *result_ptr = serialize_to_ptr(&response);
            return -1;
        }
    };

    let ffi_request: FfiRescheduleRequest = match serde_json::from_str(request_str) {
        Ok(r) => r,
        Err(e) => {
            let response = FfiRescheduleResponse {
                success: false,
                result: None,
                error: Some(format!("JSON parse error: {}", e)),
            };
            *result_ptr = serialize_to_ptr(&response);
            return -1;
        }
    };

    let rescheduler = Rescheduler::new();
    let reschedule_result = match ffi_request.strategy {
        Some(RescheduleStrategy::RightShift) => {
            rescheduler.right_shift(&ffi_request.schedule, &ffi_request.event)
        }
        Some(RescheduleStrategy::AOR) => rescheduler.aor(&ffi_request.schedule, &ffi_request.event),
        _ => rescheduler.auto_reschedule(&ffi_request.schedule, &ffi_request.event),
    };

    let response = FfiRescheduleResponse {
        success: true,
        result: Some(reschedule_result),
        error: None,
    };

    *result_ptr = serialize_to_ptr(&response);
    0
}

/// 문자열 메모리 해제
///
/// # Safety
///
/// `ptr` must have been allocated by this library (e.g., from `uaps_schedule`).
/// Must not be called more than once for the same pointer.
#[no_mangle]
pub unsafe extern "C" fn uaps_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        let _ = CString::from_raw(ptr);
    }
}

/// 엔진 버전 반환
#[no_mangle]
pub extern "C" fn uaps_version() -> *const c_char {
    static VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "\0");
    VERSION.as_ptr() as *const c_char
}

/// Serialize 가능한 응답을 C 문자열 포인터로 변환
fn response_to_ptr(response: &FfiScheduleResponse) -> *mut c_char {
    let json = serde_json::to_string(response)
        .unwrap_or_else(|_| r#"{"success":false,"error":"Serialization failed"}"#.to_string());

    CString::new(json)
        .map(|s| s.into_raw())
        .unwrap_or(ptr::null_mut())
}

/// 임의의 Serialize 타입을 C 문자열 포인터로 변환
fn serialize_to_ptr<T: serde::Serialize>(value: &T) -> *mut c_char {
    let json = serde_json::to_string(value)
        .unwrap_or_else(|_| r#"{"error":"Serialization failed"}"#.to_string());

    CString::new(json)
        .map(|s| s.into_raw())
        .unwrap_or(ptr::null_mut())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_ffi_schedule() {
        let request = r#"{
            "jobs": [{
                "id": "JOB-001",
                "operations": [{
                    "id": "OP-001",
                    "job_id": "JOB-001",
                    "sequence": 1,
                    "time": {"setup_ms": 0, "process_ms": 30000, "wait_ms": 0},
                    "required_resources": [{
                        "resource_type": "Equipment",
                        "quantity": 1,
                        "candidates": ["EQP-001"]
                    }],
                    "is_splittable": false,
                    "allow_parallel": false
                }],
                "priority": 1,
                "due_date": null,
                "earliest_start": null,
                "quantity": 1
            }],
            "resources": [{
                "id": "EQP-001",
                "kind": "Equipment",
                "capabilities": [],
                "capacity": 1,
                "efficiency": 1.0,
                "availability": [],
                "unavailable": []
            }],
            "start_time_ms": 0
        }"#;

        let request_cstr = CString::new(request).unwrap();
        let mut result_ptr: *mut c_char = ptr::null_mut();

        let code = unsafe { uaps_schedule(request_cstr.as_ptr(), &mut result_ptr) };

        assert_eq!(code, 0);
        assert!(!result_ptr.is_null());

        let result_str = unsafe { CStr::from_ptr(result_ptr).to_str().unwrap() };
        let response: FfiScheduleResponse = serde_json::from_str(result_str).unwrap();

        assert!(response.success);
        assert!(response.schedule.is_some());

        let schedule = response.schedule.unwrap();
        assert_eq!(schedule.assignments.len(), 1);
        assert_eq!(schedule.makespan_ms, 30000);

        unsafe { uaps_free_string(result_ptr) };
    }

    #[test]
    fn test_ffi_schedule_with_algorithm() {
        let request = r#"{
            "jobs": [{
                "id": "JOB-001",
                "operations": [{
                    "id": "OP-001",
                    "job_id": "JOB-001",
                    "sequence": 1,
                    "time": {"setup_ms": 0, "process_ms": 30000, "wait_ms": 0},
                    "required_resources": [{
                        "resource_type": "Equipment",
                        "quantity": 1,
                        "candidates": ["EQP-001"]
                    }],
                    "is_splittable": false,
                    "allow_parallel": false
                }],
                "priority": 1,
                "due_date": null,
                "earliest_start": null,
                "quantity": 1
            }],
            "resources": [{
                "id": "EQP-001",
                "kind": "Equipment",
                "capabilities": [],
                "capacity": 1,
                "efficiency": 1.0,
                "availability": [],
                "unavailable": []
            }],
            "start_time_ms": 0,
            "algorithm": "Simple"
        }"#;

        let request_cstr = CString::new(request).unwrap();
        let mut result_ptr: *mut c_char = ptr::null_mut();

        let code = unsafe { uaps_schedule(request_cstr.as_ptr(), &mut result_ptr) };

        assert_eq!(code, 0);
        assert!(!result_ptr.is_null());

        let result_str = unsafe { CStr::from_ptr(result_ptr).to_str().unwrap() };
        let response: FfiScheduleResponse = serde_json::from_str(result_str).unwrap();

        assert!(response.success);
        assert!(response.schedule.is_some());
        // SchedulingEngine 경로에서는 KPI와 metrics 포함
        assert!(response.kpi.is_some());
        assert!(response.metrics.is_some());

        let metrics = response.metrics.unwrap();
        assert_eq!(metrics.job_count, 1);
        assert_eq!(metrics.resource_count, 1);

        unsafe { uaps_free_string(result_ptr) };
    }

    #[test]
    fn test_ffi_schedule_with_ga() {
        let request = r#"{
            "jobs": [{
                "id": "JOB-001",
                "operations": [{
                    "id": "OP-001",
                    "job_id": "JOB-001",
                    "sequence": 1,
                    "time": {"setup_ms": 0, "process_ms": 30000, "wait_ms": 0},
                    "required_resources": [{
                        "resource_type": "Equipment",
                        "quantity": 1,
                        "candidates": ["EQP-001"]
                    }],
                    "is_splittable": false,
                    "allow_parallel": false
                }],
                "priority": 1,
                "quantity": 1
            }],
            "resources": [{
                "id": "EQP-001",
                "kind": "Equipment",
                "capabilities": [],
                "capacity": 1,
                "efficiency": 1.0,
                "availability": [],
                "unavailable": []
            }],
            "start_time_ms": 0,
            "algorithm": "GeneticAlgorithm",
            "ga_params": {
                "population_size": 10,
                "max_generations": 5,
                "elite_ratio": 0.1,
                "tournament_size": 3,
                "convergence_generations": 30,
                "convergence_threshold": 0.001
            }
        }"#;

        let request_cstr = CString::new(request).unwrap();
        let mut result_ptr: *mut c_char = ptr::null_mut();

        let code = unsafe { uaps_schedule(request_cstr.as_ptr(), &mut result_ptr) };

        assert_eq!(code, 0);

        let result_str = unsafe { CStr::from_ptr(result_ptr).to_str().unwrap() };
        let response: FfiScheduleResponse = serde_json::from_str(result_str).unwrap();

        assert!(response.success);
        assert!(response.schedule.is_some());
        assert!(response.kpi.is_some());
        assert!(response.metrics.is_some());

        let metrics = response.metrics.unwrap();
        assert_eq!(metrics.algorithm, SchedulingAlgorithm::GeneticAlgorithm);

        unsafe { uaps_free_string(result_ptr) };
    }

    #[test]
    fn test_ffi_invalid_json() {
        let request = CString::new("invalid json").unwrap();
        let mut result_ptr: *mut c_char = ptr::null_mut();

        let code = unsafe { uaps_schedule(request.as_ptr(), &mut result_ptr) };

        assert_eq!(code, -1);
        assert!(!result_ptr.is_null());

        let result_str = unsafe { CStr::from_ptr(result_ptr).to_str().unwrap() };
        let response: FfiScheduleResponse = serde_json::from_str(result_str).unwrap();

        assert!(!response.success);
        assert!(response.error.is_some());

        unsafe { uaps_free_string(result_ptr) };
    }

    #[test]
    fn test_ffi_version() {
        let version_ptr = uaps_version();
        let version = unsafe { CStr::from_ptr(version_ptr).to_str().unwrap() };
        assert_eq!(version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_ffi_validate_valid_input() {
        let request = r#"{
            "jobs": [{
                "id": "JOB-001",
                "operations": [{
                    "id": "OP-001",
                    "job_id": "JOB-001",
                    "sequence": 1,
                    "time": {"setup_ms": 0, "process_ms": 30000, "wait_ms": 0},
                    "required_resources": [{
                        "resource_type": "Equipment",
                        "quantity": 1,
                        "candidates": ["EQP-001"]
                    }],
                    "is_splittable": false,
                    "allow_parallel": false
                }],
                "priority": 1,
                "quantity": 1
            }],
            "resources": [{
                "id": "EQP-001",
                "kind": "Equipment",
                "capabilities": [],
                "capacity": 1,
                "efficiency": 1.0,
                "availability": [],
                "unavailable": []
            }]
        }"#;

        let request_cstr = CString::new(request).unwrap();
        let mut result_ptr: *mut c_char = ptr::null_mut();

        let code = unsafe { uaps_validate(request_cstr.as_ptr(), &mut result_ptr) };

        assert_eq!(code, 0);
        assert!(!result_ptr.is_null());

        let result_str = unsafe { CStr::from_ptr(result_ptr).to_str().unwrap() };
        let response: FfiValidationResponse = serde_json::from_str(result_str).unwrap();

        assert!(response.valid);
        assert!(response.errors.is_empty());

        unsafe { uaps_free_string(result_ptr) };
    }

    #[test]
    fn test_ffi_validate_duplicate_ids() {
        let request = r#"{
            "jobs": [{
                "id": "JOB-001",
                "operations": [{
                    "id": "OP-001",
                    "job_id": "JOB-001",
                    "sequence": 1,
                    "time": {"setup_ms": 0, "process_ms": 30000, "wait_ms": 0},
                    "required_resources": [{
                        "resource_type": "Equipment",
                        "quantity": 1,
                        "candidates": ["EQP-001"]
                    }],
                    "is_splittable": false,
                    "allow_parallel": false
                }],
                "priority": 1,
                "quantity": 1
            },
            {
                "id": "JOB-001",
                "operations": [{
                    "id": "OP-002",
                    "job_id": "JOB-001",
                    "sequence": 1,
                    "time": {"setup_ms": 0, "process_ms": 20000, "wait_ms": 0},
                    "required_resources": [{
                        "resource_type": "Equipment",
                        "quantity": 1,
                        "candidates": ["EQP-001"]
                    }],
                    "is_splittable": false,
                    "allow_parallel": false
                }],
                "priority": 1,
                "quantity": 1
            }],
            "resources": [{
                "id": "EQP-001",
                "kind": "Equipment",
                "capabilities": [],
                "capacity": 1,
                "efficiency": 1.0,
                "availability": [],
                "unavailable": []
            }]
        }"#;

        let request_cstr = CString::new(request).unwrap();
        let mut result_ptr: *mut c_char = ptr::null_mut();

        let code = unsafe { uaps_validate(request_cstr.as_ptr(), &mut result_ptr) };

        assert_eq!(code, 0);
        assert!(!result_ptr.is_null());

        let result_str = unsafe { CStr::from_ptr(result_ptr).to_str().unwrap() };
        let response: FfiValidationResponse = serde_json::from_str(result_str).unwrap();

        assert!(!response.valid);
        assert!(!response.errors.is_empty());

        unsafe { uaps_free_string(result_ptr) };
    }

    #[test]
    fn test_ffi_reschedule_right_shift() {
        // 먼저 스케줄 생성
        let request = r#"{
            "jobs": [{
                "id": "JOB-001",
                "operations": [{
                    "id": "OP-001",
                    "job_id": "JOB-001",
                    "sequence": 1,
                    "time": {"setup_ms": 0, "process_ms": 30000, "wait_ms": 0},
                    "required_resources": [{"resource_type": "Equipment", "quantity": 1, "candidates": ["EQP-001"]}],
                    "is_splittable": false,
                    "allow_parallel": false
                },
                {
                    "id": "OP-002",
                    "job_id": "JOB-001",
                    "sequence": 2,
                    "time": {"setup_ms": 0, "process_ms": 20000, "wait_ms": 0},
                    "required_resources": [{"resource_type": "Equipment", "quantity": 1, "candidates": ["EQP-001"]}],
                    "is_splittable": false,
                    "allow_parallel": false
                }],
                "priority": 1,
                "quantity": 1
            }],
            "resources": [{"id": "EQP-001", "kind": "Equipment", "capabilities": [], "capacity": 1, "efficiency": 1.0, "availability": [], "unavailable": []}],
            "start_time_ms": 0
        }"#;

        let request_cstr = CString::new(request).unwrap();
        let mut result_ptr: *mut c_char = ptr::null_mut();
        let code = unsafe { uaps_schedule(request_cstr.as_ptr(), &mut result_ptr) };
        assert_eq!(code, 0);

        let result_str = unsafe { CStr::from_ptr(result_ptr).to_str().unwrap() };
        let response: FfiScheduleResponse = serde_json::from_str(result_str).unwrap();
        let schedule = response.schedule.unwrap();

        // 리스케줄 요청: OP-001에 5초 지연
        let schedule_json = serde_json::to_string(&schedule).unwrap();
        let reschedule_request = format!(
            r#"{{"schedule":{schedule_json},"event":{{"OperationDelay":{{"operation_id":"OP-001","delay_ms":5000}}}},"strategy":"RightShift"}}"#
        );

        let reschedule_cstr = CString::new(reschedule_request).unwrap();
        let mut reschedule_ptr: *mut c_char = ptr::null_mut();
        let code2 = unsafe { uaps_reschedule(reschedule_cstr.as_ptr(), &mut reschedule_ptr) };
        assert_eq!(code2, 0);

        let reschedule_str = unsafe { CStr::from_ptr(reschedule_ptr).to_str().unwrap() };
        let reschedule_response: FfiRescheduleResponse =
            serde_json::from_str(reschedule_str).unwrap();

        assert!(reschedule_response.success);
        assert!(reschedule_response.result.is_some());

        let result = reschedule_response.result.unwrap();
        assert_eq!(result.strategy_used, RescheduleStrategy::RightShift);
        // 리스케줄이 수행되어야 함
        assert!(
            !result.affected_operations.is_empty()
                || result.schedule.makespan_ms >= schedule.makespan_ms
        );

        unsafe {
            uaps_free_string(result_ptr);
            uaps_free_string(reschedule_ptr);
        };
    }

    #[test]
    fn test_ffi_reschedule_auto() {
        let schedule = Schedule::new();
        let schedule_json = serde_json::to_string(&schedule).unwrap();

        let request = format!(
            r#"{{"schedule":{schedule_json},"event":{{"OperationDelay":{{"operation_id":"OP-001","delay_ms":5000}}}}}}"#
        );

        let request_cstr = CString::new(request).unwrap();
        let mut result_ptr: *mut c_char = ptr::null_mut();
        let code = unsafe { uaps_reschedule(request_cstr.as_ptr(), &mut result_ptr) };
        assert_eq!(code, 0);

        let result_str = unsafe { CStr::from_ptr(result_ptr).to_str().unwrap() };
        let response: FfiRescheduleResponse = serde_json::from_str(result_str).unwrap();

        assert!(response.success);
        assert!(response.result.is_some());

        unsafe { uaps_free_string(result_ptr) };
    }

    #[test]
    fn test_ffi_engine_path_with_setup_matrix() {
        // SchedulingEngine 경로 (algorithm 지정) + setup_matrices가 정상 전달되는지 확인
        let request = r#"{
            "jobs": [{
                "id": "JOB-001",
                "product_name": "A",
                "operations": [{
                    "id": "OP-001",
                    "job_id": "JOB-001",
                    "sequence": 1,
                    "time": {"setup_ms": 0, "process_ms": 30000, "wait_ms": 0},
                    "required_resources": [{"resource_type": "Equipment", "quantity": 1, "candidates": ["EQP-001"]}],
                    "is_splittable": false,
                    "allow_parallel": false
                }],
                "priority": 1,
                "quantity": 1
            },
            {
                "id": "JOB-002",
                "product_name": "B",
                "operations": [{
                    "id": "OP-002",
                    "job_id": "JOB-002",
                    "sequence": 1,
                    "time": {"setup_ms": 0, "process_ms": 20000, "wait_ms": 0},
                    "required_resources": [{"resource_type": "Equipment", "quantity": 1, "candidates": ["EQP-001"]}],
                    "is_splittable": false,
                    "allow_parallel": false
                }],
                "priority": 2,
                "quantity": 1
            }],
            "resources": [{"id": "EQP-001", "kind": "Equipment", "capabilities": [], "capacity": 1, "efficiency": 1.0, "availability": [], "unavailable": []}],
            "start_time_ms": 0,
            "algorithm": "Simple",
            "setup_matrices": {
                "matrices": {
                    "EQP-001": {
                        "resource_id": "EQP-001",
                        "default_setup_ms": 0,
                        "entries": [
                            {"from_product": "A", "to_product": "B", "setup_ms": 10000},
                            {"from_product": "B", "to_product": "A", "setup_ms": 5000}
                        ]
                    }
                }
            }
        }"#;

        let request_cstr = CString::new(request).unwrap();
        let mut result_ptr: *mut c_char = ptr::null_mut();
        let code = unsafe { uaps_schedule(request_cstr.as_ptr(), &mut result_ptr) };

        assert_eq!(code, 0);

        let result_str = unsafe { CStr::from_ptr(result_ptr).to_str().unwrap() };
        let response: FfiScheduleResponse = serde_json::from_str(result_str).unwrap();

        assert!(response.success);
        assert!(response.schedule.is_some());
        assert!(response.kpi.is_some()); // SchedulingEngine 경로 → KPI 포함

        let schedule = response.schedule.unwrap();
        // 2 jobs, 1 equipment → sequential, A→B setup 10s 포함
        // makespan >= 30000 + 10000(setup) + 20000 = 60000
        assert!(
            schedule.makespan_ms >= 60_000,
            "Expected makespan >= 60000 with setup time, got {}",
            schedule.makespan_ms
        );

        unsafe { uaps_free_string(result_ptr) };
    }

    #[test]
    fn test_ffi_engine_path_with_site_transitions() {
        // SchedulingEngine 경로 + site_transitions 전달 확인
        let request = r#"{
            "jobs": [{
                "id": "JOB-001",
                "operations": [{
                    "id": "OP-001",
                    "job_id": "JOB-001",
                    "sequence": 1,
                    "time": {"setup_ms": 0, "process_ms": 10000, "wait_ms": 0},
                    "required_resources": [{"resource_type": "Equipment", "quantity": 1, "candidates": ["EQP-A"]}],
                    "is_splittable": false,
                    "allow_parallel": false
                },
                {
                    "id": "OP-002",
                    "job_id": "JOB-001",
                    "sequence": 2,
                    "time": {"setup_ms": 0, "process_ms": 10000, "wait_ms": 0},
                    "required_resources": [{"resource_type": "Equipment", "quantity": 1, "candidates": ["EQP-B"]}],
                    "is_splittable": false,
                    "allow_parallel": false
                }],
                "priority": 1,
                "quantity": 1
            }],
            "resources": [
                {"id": "EQP-A", "kind": "Equipment", "capabilities": [], "capacity": 1, "efficiency": 1.0, "availability": [], "unavailable": [], "site_id": "SITE-1"},
                {"id": "EQP-B", "kind": "Equipment", "capabilities": [], "capacity": 1, "efficiency": 1.0, "availability": [], "unavailable": [], "site_id": "SITE-2"}
            ],
            "start_time_ms": 0,
            "algorithm": "Simple",
            "site_transitions": {
                "transitions": {
                    "SITE-1\u2192SITE-2": 15000,
                    "SITE-2\u2192SITE-1": 15000
                }
            }
        }"#;

        let request_cstr = CString::new(request).unwrap();
        let mut result_ptr: *mut c_char = ptr::null_mut();
        let code = unsafe { uaps_schedule(request_cstr.as_ptr(), &mut result_ptr) };

        assert_eq!(code, 0);

        let result_str = unsafe { CStr::from_ptr(result_ptr).to_str().unwrap() };
        let response: FfiScheduleResponse = serde_json::from_str(result_str).unwrap();

        assert!(response.success);
        let schedule = response.schedule.unwrap();
        // OP-001: 10s on SITE-1, then 15s transit, then OP-002: 10s on SITE-2
        // makespan >= 10000 + 15000 + 10000 = 35000
        assert!(
            schedule.makespan_ms >= 35_000,
            "Expected makespan >= 35000 with site transition, got {}",
            schedule.makespan_ms
        );

        unsafe { uaps_free_string(result_ptr) };
    }

    // --- FFI JSON schema regression guards (Cycle 116) ---
    //
    // These tests pin the Rust FFI wire format to the **C# SDK binding contract**,
    // the source of truth being the DTOs in
    //   consumers/u-aps/sdk/UAPS.SDK/Client/SchedulerClient.cs
    // which deserialize with `JsonNamingPolicy.SnakeCaseLower`:
    //   ScheduleResponseDto { success, schedule, error }
    //   ScheduleDto         { assignments, makespan_ms, violations, tasks }
    //   AssignmentDto       { operation_id, resource_id, start_ms, end_ms }
    // The C# DTO deliberately does NOT read kpi/metrics/warnings — those are
    // Rust-only extras (System.Text.Json ignores unknown members).
    //
    // Asserting the field names/types the C# DTOs declare (not a Rust echo) means
    // a rename or type change on either side breaks the test — the same class of
    // guard as the u-nesting rotation array-vs-scalar incident.

    const GUARD_REQUEST: &str = r#"{
        "jobs": [{
            "id": "JOB-001",
            "operations": [{
                "id": "OP-001", "job_id": "JOB-001", "sequence": 1,
                "time": {"setup_ms": 0, "process_ms": 30000, "wait_ms": 0},
                "required_resources": [{"resource_type": "Equipment", "quantity": 1, "candidates": ["EQP-001"]}],
                "is_splittable": false, "allow_parallel": false
            }],
            "priority": 1, "quantity": 1
        }],
        "resources": [{
            "id": "EQP-001", "kind": "Equipment", "capabilities": [],
            "capacity": 1, "efficiency": 1.0, "availability": [], "unavailable": []
        }],
        "start_time_ms": 0
    }"#;

    #[test]
    fn schema_guard_response_matches_csharp_dto_contract() {
        let request_cstr = CString::new(GUARD_REQUEST).unwrap();
        let mut result_ptr: *mut c_char = ptr::null_mut();
        let code = unsafe { uaps_schedule(request_cstr.as_ptr(), &mut result_ptr) };
        assert_eq!(code, 0);

        let result_str = unsafe { CStr::from_ptr(result_ptr).to_str().unwrap() };
        let json: serde_json::Value = serde_json::from_str(result_str).unwrap();

        // ScheduleResponseDto contract.
        assert!(json["success"].is_boolean(), "ScheduleResponseDto.success");
        assert!(json["schedule"].is_object(), "ScheduleResponseDto.schedule");

        // ScheduleDto contract.
        let s = &json["schedule"];
        assert!(s["assignments"].is_array(), "ScheduleDto.assignments");
        assert!(s["makespan_ms"].is_number(), "ScheduleDto.makespan_ms");
        assert!(s["violations"].is_array(), "ScheduleDto.violations");
        // `tasks` is skip-if-empty on the Rust side; C# defaults it to []. So it
        // is array-or-absent, never a non-array.
        assert!(
            s.get("tasks").map(|t| t.is_array()).unwrap_or(true),
            "ScheduleDto.tasks must be an array when present"
        );

        // AssignmentDto contract.
        let a = &s["assignments"][0];
        assert!(a["operation_id"].is_string(), "AssignmentDto.operation_id");
        assert!(a["resource_id"].is_string(), "AssignmentDto.resource_id");
        assert!(a["start_ms"].is_number(), "AssignmentDto.start_ms");
        assert!(a["end_ms"].is_number(), "AssignmentDto.end_ms");

        unsafe { uaps_free_string(result_ptr) };
    }

    #[test]
    fn schema_guard_request_accepts_csharp_dto_contract() {
        // The Rust FfiScheduleRequest must accept the snake_case field names the
        // C# SDK request DTO emits (jobs, resources, start_time_ms, algorithm,
        // ga_params). A rename of a required field on the Rust side would reject
        // every C# request — this guards that boundary.
        let request = r#"{
            "jobs": [{
                "id": "JOB-001",
                "operations": [{
                    "id": "OP-001", "job_id": "JOB-001", "sequence": 1,
                    "time": {"setup_ms": 0, "process_ms": 30000, "wait_ms": 0},
                    "required_resources": [{"resource_type": "Equipment", "quantity": 1, "candidates": ["EQP-001"]}],
                    "is_splittable": false, "allow_parallel": false
                }],
                "priority": 1, "quantity": 1
            }],
            "resources": [{
                "id": "EQP-001", "kind": "Equipment", "capabilities": [],
                "capacity": 1, "efficiency": 1.0, "availability": [], "unavailable": []
            }],
            "start_time_ms": 0,
            "algorithm": "GeneticAlgorithm",
            "ga_params": {
                "population_size": 10, "max_generations": 5, "elite_ratio": 0.1,
                "tournament_size": 3, "convergence_generations": 30, "convergence_threshold": 0.001
            }
        }"#;
        let request_cstr = CString::new(request).unwrap();
        let mut result_ptr: *mut c_char = ptr::null_mut();
        let code = unsafe { uaps_schedule(request_cstr.as_ptr(), &mut result_ptr) };
        // Code 0 means the request deserialized and scheduled — the C# request
        // contract is accepted (a JSON parse failure would return -1).
        assert_eq!(code, 0, "Rust must accept the C# SDK request DTO contract");

        let result_str = unsafe { CStr::from_ptr(result_ptr).to_str().unwrap() };
        let json: serde_json::Value = serde_json::from_str(result_str).unwrap();
        assert!(json["success"].as_bool().unwrap_or(false));

        unsafe { uaps_free_string(result_ptr) };
    }
}
