//! Validation - 입력 데이터 검증
//!
//! 스케줄 요청 데이터의 유효성 검사

use crate::error::{ErrorCode, Result, UapsError};
use crate::models::{Job, Operation, Resource};
use std::collections::{HashMap, HashSet};

/// 검증 결과
#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    /// 에러 목록
    pub errors: Vec<UapsError>,
    /// 경고 목록
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn add_error(&mut self, error: UapsError) {
        self.errors.push(error);
    }

    pub fn add_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }

    /// Result로 변환
    pub fn into_result(self) -> Result<()> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors.into_iter().next().unwrap())
        }
    }
}

/// 입력 검증기
pub struct Validator;

impl Validator {
    /// 전체 스케줄 요청 검증
    pub fn validate_schedule_request(
        jobs: &[Job],
        operations: &[Operation],
        resources: &[Resource],
    ) -> ValidationResult {
        let mut result = ValidationResult::new();

        // 개별 검증
        Self::validate_jobs(jobs, &mut result);
        Self::validate_operations(operations, jobs, &mut result);
        Self::validate_resources(resources, &mut result);

        // 참조 무결성 검증
        Self::validate_references(jobs, operations, resources, &mut result);

        result
    }

    /// Job 검증
    pub fn validate_jobs(jobs: &[Job], result: &mut ValidationResult) {
        let mut ids = HashSet::new();

        for job in jobs {
            // ID 중복 검사
            if !ids.insert(&job.id) {
                result.add_error(
                    UapsError::new(
                        ErrorCode::DuplicateId,
                        format!("Duplicate Job ID: {}", job.id),
                    )
                    .with_entity(&job.id),
                );
            }

            // ID 유효성
            if job.id.is_empty() {
                result.add_error(UapsError::new(
                    ErrorCode::MissingRequiredField,
                    "Job ID cannot be empty",
                ));
            }

            // 우선순위 범위
            if job.priority < 0 {
                result.add_warning(format!(
                    "Job {} has negative priority: {}",
                    job.id, job.priority
                ));
            }

            // 납기일 유효성
            if let Some(due_date) = &job.due_date {
                if due_date.timestamp_millis() < 0 {
                    result.add_error(
                        UapsError::new(
                            ErrorCode::InvalidTimeRange,
                            format!("Job {} has negative due date", job.id),
                        )
                        .with_entity(&job.id),
                    );
                }
            }

            // 시작 시간 제약
            if let Some(earliest) = &job.earliest_start {
                if earliest.timestamp_millis() < 0 {
                    result.add_error(
                        UapsError::new(
                            ErrorCode::InvalidTimeRange,
                            format!("Job {} has negative earliest start", job.id),
                        )
                        .with_entity(&job.id),
                    );
                }
            }
        }
    }

    /// Operation 검증
    pub fn validate_operations(
        operations: &[Operation],
        jobs: &[Job],
        result: &mut ValidationResult,
    ) {
        let mut ids = HashSet::new();
        let job_ids: HashSet<&str> = jobs.iter().map(|j| j.id.as_str()).collect();

        for op in operations {
            // ID 중복 검사
            if !ids.insert(&op.id) {
                result.add_error(
                    UapsError::new(
                        ErrorCode::DuplicateId,
                        format!("Duplicate Operation ID: {}", op.id),
                    )
                    .with_entity(&op.id),
                );
            }

            // Job 참조 검사
            if !job_ids.contains(op.job_id.as_str()) {
                result.add_error(
                    UapsError::new(
                        ErrorCode::InvalidReference,
                        format!("Operation {} references unknown Job: {}", op.id, op.job_id),
                    )
                    .with_entity(&op.id),
                );
            }

            // 시간 유효성
            if op.time.process_ms < 0 {
                result.add_error(
                    UapsError::new(
                        ErrorCode::InvalidTimeRange,
                        format!("Operation {} has negative process time", op.id),
                    )
                    .with_entity(&op.id),
                );
            }

            if op.time.setup_ms < 0 {
                result.add_error(
                    UapsError::new(
                        ErrorCode::InvalidTimeRange,
                        format!("Operation {} has negative setup time", op.id),
                    )
                    .with_entity(&op.id),
                );
            }

            // 시퀀스 유효성
            if op.sequence < 0 {
                result.add_error(
                    UapsError::new(
                        ErrorCode::InvalidInput,
                        format!("Operation {} has negative sequence", op.id),
                    )
                    .with_entity(&op.id),
                );
            }

            // 자원 요구사항 검사
            if op.required_resources.is_empty() {
                result.add_warning(format!("Operation {} has no resource requirements", op.id));
            }

            // 분할 설정 검사 — 공정이나 그 작업이 분할을 허용할 때 쓰인다:
            // 조각 하나 이상, 조각 최소 · 조각당 추가 시간은 음수가 아님.
            let splittable =
                op.is_splittable || jobs.iter().any(|j| j.id == op.job_id && j.is_splittable);
            if let (true, Some(config)) = (splittable, &op.split_config) {
                if config.max_splits == 0
                    || config.min_split_size_ms < 0
                    || config.split_setup_overhead_ms < 0
                {
                    result.add_error(
                        UapsError::new(
                            ErrorCode::InvalidInput,
                            format!(
                                "Operation {} has invalid split config: max_splits must be at least 1, \
                                 min_split_size_ms and split_setup_overhead_ms not negative",
                                op.id
                            ),
                        )
                        .with_entity(&op.id),
                    );
                }
            }

            // 중첩 설정 검사 — 진행 비율은 0 ~ 1.
            if let Some(config) = &op.overlap_config {
                if !(0.0..=1.0).contains(&config.min_predecessor_completion) {
                    result.add_error(
                        UapsError::new(
                            ErrorCode::InvalidInput,
                            format!(
                                "Operation {} has invalid overlap config: min_predecessor_completion must be within 0 and 1",
                                op.id
                            ),
                        )
                        .with_entity(&op.id),
                    );
                }
            }
        }
    }

    /// Resource 검증
    pub fn validate_resources(resources: &[Resource], result: &mut ValidationResult) {
        let mut ids = HashSet::new();

        for resource in resources {
            // ID 중복 검사
            if !ids.insert(&resource.id) {
                result.add_error(
                    UapsError::new(
                        ErrorCode::DuplicateId,
                        format!("Duplicate Resource ID: {}", resource.id),
                    )
                    .with_entity(&resource.id),
                );
            }

            // 효율성 유효성
            if resource.efficiency <= 0.0 {
                result.add_error(
                    UapsError::new(
                        ErrorCode::InvalidInput,
                        format!("Resource {} has non-positive efficiency", resource.id),
                    )
                    .with_entity(&resource.id),
                );
            }

            if resource.efficiency > 2.0 {
                result.add_warning(format!(
                    "Resource {} has unusually high efficiency: {:.2}",
                    resource.id, resource.efficiency
                ));
            }
        }
    }

    /// 참조 무결성 검증
    pub fn validate_references(
        jobs: &[Job],
        operations: &[Operation],
        resources: &[Resource],
        result: &mut ValidationResult,
    ) {
        let resource_ids: HashSet<&str> = resources.iter().map(|r| r.id.as_str()).collect();
        let operation_ids: HashSet<&str> = operations.iter().map(|o| o.id.as_str()).collect();

        for op in operations {
            for dependency in &op.dependencies {
                if !operation_ids.contains(dependency.as_str()) {
                    result.add_error(
                        UapsError::new(
                            ErrorCode::InvalidReference,
                            format!(
                                "Operation {} depends on unknown operation: {}",
                                op.id, dependency
                            ),
                        )
                        .with_entity(&op.id),
                    );
                }
            }
            for req in &op.required_resources {
                for candidate in &req.candidates {
                    if !resource_ids.contains(candidate.as_str()) {
                        result.add_error(
                            UapsError::new(
                                ErrorCode::InvalidReference,
                                format!(
                                    "Operation {} references unknown resource: {}",
                                    op.id, candidate
                                ),
                            )
                            .with_entity(&op.id),
                        );
                    }
                }
            }
        }

        if let Some(waiting) = dependency_loop(jobs) {
            result.add_error(
                UapsError::new(
                    ErrorCode::InvalidInput,
                    format!(
                        "Operations {} wait on each other through their dependencies and job order, so none of them can start",
                        waiting.join(", ")
                    ),
                )
                .with_entity(&waiting[0]),
            );
        }
    }
}

/// The operations a loop of waits keeps from ever starting — each waits for
/// the operations it names in `dependencies` and for the one before it in its
/// job (`sequence` order) — or `None` when every operation can start.
/// Dependencies on unknown operations are left to the reference check.
///
/// Kahn (1962): repeatedly take an operation nothing it waits for is left
/// before; what remains is on a loop or after one. O(operations + waits).
fn dependency_loop(jobs: &[Job]) -> Option<Vec<String>> {
    let mut waits_for: HashMap<&str, Vec<&str>> = HashMap::new();
    for job in jobs {
        let mut ordered: Vec<&Operation> = job.operations.iter().collect();
        ordered.sort_by_key(|o| o.sequence);
        for (k, op) in ordered.iter().enumerate() {
            let entry = waits_for.entry(op.id.as_str()).or_default();
            if k > 0 {
                entry.push(ordered[k - 1].id.as_str());
            }
            entry.extend(op.dependencies.iter().map(String::as_str));
        }
    }
    let mut left: HashMap<&str, usize> = HashMap::new();
    let mut successors: HashMap<&str, Vec<&str>> = HashMap::new();
    for (&op, preds) in &waits_for {
        let known: Vec<&str> = preds
            .iter()
            .copied()
            .filter(|p| waits_for.contains_key(p))
            .collect();
        left.insert(op, known.len());
        for p in known {
            successors.entry(p).or_default().push(op);
        }
    }
    let mut free: Vec<&str> = left
        .iter()
        .filter(|(_, &n)| n == 0)
        .map(|(&op, _)| op)
        .collect();
    while let Some(op) = free.pop() {
        left.remove(op);
        for &next in successors.get(op).into_iter().flatten() {
            if let Some(n) = left.get_mut(next) {
                *n -= 1;
                if *n == 0 {
                    free.push(next);
                }
            }
        }
    }
    if left.is_empty() {
        return None;
    }
    let mut waiting: Vec<String> = left.into_keys().map(str::to_string).collect();
    waiting.sort();
    Some(waiting)
}

/// 시간 범위 검증
pub fn validate_time_range(start: i64, end: i64) -> Result<()> {
    if start < 0 || end < 0 {
        return Err(UapsError::new(
            ErrorCode::InvalidTimeRange,
            "Time values cannot be negative",
        ));
    }
    if start > end {
        return Err(UapsError::new(
            ErrorCode::InvalidTimeRange,
            format!("Start time ({}) is after end time ({})", start, end),
        ));
    }
    Ok(())
}

/// 수량 검증
pub fn validate_quantity(qty: f64, field: &str) -> Result<()> {
    if qty < 0.0 {
        return Err(UapsError::new(
            ErrorCode::InvalidQuantity,
            format!("{} cannot be negative: {}", field, qty),
        ));
    }
    if qty.is_nan() || qty.is_infinite() {
        return Err(UapsError::new(
            ErrorCode::InvalidQuantity,
            format!("{} must be a valid number", field),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Resource;

    #[test]
    fn test_valid_input() {
        let jobs = vec![Job::new("job1").with_priority(1)];
        let ops = vec![Operation::new("op1", "job1", 1)
            .with_time(1000, 5000, 0)
            .with_equipment(vec!["m1".into()])];
        let resources = vec![Resource::equipment("m1")];

        let result = Validator::validate_schedule_request(&jobs, &ops, &resources);
        assert!(result.is_valid());
    }

    #[test]
    fn test_duplicate_job_id() {
        let jobs = vec![
            Job::new("job1").with_priority(1),
            Job::new("job1").with_priority(2), // 중복
        ];

        let mut result = ValidationResult::new();
        Validator::validate_jobs(&jobs, &mut result);

        assert!(!result.is_valid());
        assert!(result
            .errors
            .iter()
            .any(|e| e.code == ErrorCode::DuplicateId));
    }

    #[test]
    fn test_invalid_reference() {
        let jobs = vec![Job::new("job1").with_priority(1)];
        let ops = vec![Operation::new("op1", "job2", 1) // 존재하지 않는 job
            .with_time(1000, 5000, 0)];

        let mut result = ValidationResult::new();
        Validator::validate_operations(&ops, &jobs, &mut result);

        assert!(!result.is_valid());
        assert!(result
            .errors
            .iter()
            .any(|e| e.code == ErrorCode::InvalidReference));
    }

    #[test]
    fn test_negative_time() {
        let jobs = vec![Job::new("job1").with_priority(1)];
        let ops = vec![
            Operation::new("op1", "job1", 1).with_time(-100, 5000, 0), // 음수 시간
        ];

        let mut result = ValidationResult::new();
        Validator::validate_operations(&ops, &jobs, &mut result);

        assert!(!result.is_valid());
    }

    #[test]
    fn test_time_range_validation() {
        assert!(validate_time_range(0, 100).is_ok());
        assert!(validate_time_range(100, 100).is_ok());
        assert!(validate_time_range(100, 50).is_err());
        assert!(validate_time_range(-1, 100).is_err());
    }

    #[test]
    fn test_quantity_validation() {
        assert!(validate_quantity(10.0, "qty").is_ok());
        assert!(validate_quantity(0.0, "qty").is_ok());
        assert!(validate_quantity(-1.0, "qty").is_err());
        assert!(validate_quantity(f64::NAN, "qty").is_err());
    }
}
