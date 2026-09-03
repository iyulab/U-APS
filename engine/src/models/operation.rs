//! Operation - 작업 단위 (공정)

use super::material::MaterialRequirement;
use super::outsourcing::OutsourcingConfig;
use serde::{Deserialize, Serialize};

/// Operation Time Window - 공정 시간 제약 (Phase 11)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperationTimeWindow {
    /// Earliest start time (ms from epoch, optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub earliest_start_ms: Option<i64>,
    /// Latest start time (ms from epoch, optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_start_ms: Option<i64>,
    /// Earliest end time (ms from epoch, optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub earliest_end_ms: Option<i64>,
    /// Latest end time (ms from epoch, optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_end_ms: Option<i64>,
    /// Hard constraint (true) or soft constraint with penalty (false)
    #[serde(default = "default_hard_constraint")]
    pub is_hard: bool,
    /// Penalty per millisecond of violation (for soft constraints)
    #[serde(default)]
    pub penalty_per_ms: f64,
}

fn default_hard_constraint() -> bool {
    true
}

impl Default for OperationTimeWindow {
    fn default() -> Self {
        Self {
            earliest_start_ms: None,
            latest_start_ms: None,
            earliest_end_ms: None,
            latest_end_ms: None,
            is_hard: true,
            penalty_per_ms: 0.0,
        }
    }
}

impl OperationTimeWindow {
    /// Create a new time window with earliest start
    pub fn with_earliest_start(earliest_start_ms: i64) -> Self {
        Self {
            earliest_start_ms: Some(earliest_start_ms),
            ..Default::default()
        }
    }

    /// Create a new time window with latest end (deadline)
    pub fn with_deadline(latest_end_ms: i64) -> Self {
        Self {
            latest_end_ms: Some(latest_end_ms),
            ..Default::default()
        }
    }

    /// Create a bounded time window [earliest_start, latest_end]
    pub fn bounded(earliest_start_ms: i64, latest_end_ms: i64) -> Self {
        Self {
            earliest_start_ms: Some(earliest_start_ms),
            latest_end_ms: Some(latest_end_ms),
            ..Default::default()
        }
    }

    /// Make this a soft constraint with penalty
    pub fn soft(mut self, penalty_per_ms: f64) -> Self {
        self.is_hard = false;
        self.penalty_per_ms = penalty_per_ms;
        self
    }

    /// Validate if a proposed start/end time fits within the window
    /// Returns (valid, early_violation_ms, late_violation_ms)
    pub fn validate(&self, start_ms: i64, end_ms: i64) -> (bool, i64, i64) {
        let mut early_violation = 0i64;
        let mut late_violation = 0i64;

        if let Some(earliest_start) = self.earliest_start_ms {
            if start_ms < earliest_start {
                early_violation = earliest_start - start_ms;
            }
        }

        if let Some(latest_start) = self.latest_start_ms {
            if start_ms > latest_start {
                late_violation = late_violation.max(start_ms - latest_start);
            }
        }

        if let Some(earliest_end) = self.earliest_end_ms {
            if end_ms < earliest_end {
                early_violation = early_violation.max(earliest_end - end_ms);
            }
        }

        if let Some(latest_end) = self.latest_end_ms {
            if end_ms > latest_end {
                late_violation = late_violation.max(end_ms - latest_end);
            }
        }

        let valid = early_violation == 0 && late_violation == 0;
        (valid, early_violation, late_violation)
    }

    /// Calculate penalty for constraint violation
    pub fn calculate_penalty(&self, start_ms: i64, end_ms: i64) -> f64 {
        if self.is_hard {
            return 0.0; // Hard constraints don't have penalties, they just fail
        }

        let (_, early_violation, late_violation) = self.validate(start_ms, end_ms);
        (early_violation + late_violation) as f64 * self.penalty_per_ms
    }

    /// Check if any constraint is defined
    pub fn is_constrained(&self) -> bool {
        self.earliest_start_ms.is_some()
            || self.latest_start_ms.is_some()
            || self.earliest_end_ms.is_some()
            || self.latest_end_ms.is_some()
    }
}

/// PERT 3-point estimation for operation duration (Phase 11)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PertDuration {
    /// Optimistic duration (best case, ms)
    pub optimistic_ms: i64,
    /// Most likely duration (ms)
    pub most_likely_ms: i64,
    /// Pessimistic duration (worst case, ms)
    pub pessimistic_ms: i64,
}

impl PertDuration {
    pub fn new(optimistic_ms: i64, most_likely_ms: i64, pessimistic_ms: i64) -> Self {
        Self {
            optimistic_ms,
            most_likely_ms,
            pessimistic_ms,
        }
    }

    /// Calculate PERT mean: (O + 4M + P) / 6
    pub fn mean_ms(&self) -> f64 {
        (self.optimistic_ms as f64 + 4.0 * self.most_likely_ms as f64 + self.pessimistic_ms as f64)
            / 6.0
    }

    /// Calculate PERT standard deviation: (P - O) / 6
    pub fn std_dev_ms(&self) -> f64 {
        (self.pessimistic_ms - self.optimistic_ms) as f64 / 6.0
    }

    /// Calculate PERT variance: ((P - O) / 6)^2
    pub fn variance_ms_sq(&self) -> f64 {
        let std_dev = self.std_dev_ms();
        std_dev * std_dev
    }

    /// Get duration at specific confidence level (using z-score)
    /// Common z-scores: 50% = 0.0, 85% = 1.0, 95% = 1.645, 99% = 2.326
    pub fn at_confidence(&self, z_score: f64) -> f64 {
        self.mean_ms() + z_score * self.std_dev_ms()
    }
}

/// 작업 시간 구조
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperationTime {
    /// 공정전 준비시간 (ms)
    pub setup_ms: i64,
    /// 실제 공정시간 (ms)
    pub process_ms: i64,
    /// 공정후 대기시간 (ms)
    pub wait_ms: i64,
}

/// 공정 분할 설정
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SplitConfig {
    /// 최소 분할 단위 (ms)
    pub min_split_size_ms: i64,
    /// 최대 분할 수
    pub max_splits: usize,
    /// 분할당 추가 Setup Time (ms)
    pub split_setup_overhead_ms: i64,
}

impl Default for SplitConfig {
    fn default() -> Self {
        Self {
            min_split_size_ms: 60_000, // 1분
            max_splits: 4,
            split_setup_overhead_ms: 0,
        }
    }
}

/// 공정 중첩 설정
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OverlapConfig {
    /// 선행 공정 최소 완료 비율 (0.0 ~ 1.0)
    pub min_predecessor_completion: f64,
    /// Transfer Batch 크기 (단위 수량)
    pub transfer_batch_size: i32,
}

impl Default for OverlapConfig {
    fn default() -> Self {
        Self {
            min_predecessor_completion: 0.5, // 50% 완료 후 시작 가능
            transfer_batch_size: 1,
        }
    }
}

/// 분할된 Sub-Operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SplitOperation {
    /// 원본 Operation ID
    pub parent_op_id: String,
    /// 분할 인덱스 (0-based)
    pub split_index: usize,
    /// 이 분할의 처리 시간 (ms)
    pub process_ms: i64,
    /// 할당된 기계 ID
    pub assigned_machine: Option<String>,
}

impl OperationTime {
    pub fn new(setup_ms: i64, process_ms: i64, wait_ms: i64) -> Self {
        Self {
            setup_ms,
            process_ms,
            wait_ms,
        }
    }

    /// 총 소요시간
    pub fn total_ms(&self) -> i64 {
        self.setup_ms + self.process_ms + self.wait_ms
    }
}

/// 자원 요구사항
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceRequirement {
    /// 자원 유형
    pub resource_type: ResourceType,
    /// 필요 수량
    pub quantity: i32,
    /// 가용 자원 후보 ID 목록
    pub candidates: Vec<String>,
    /// 인력 부하계수 (0.1~1.0, Worker 유형에만 적용)
    /// 1.0 = 전담, 0.5 = 2개 공정 동시 담당 가능
    #[serde(default = "default_load_factor")]
    pub load_factor: f64,
}

fn default_load_factor() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResourceType {
    Equipment,
    Worker,
}

/// 일일 작업시간 제약 — 특정 공정이 하루 중 허용되는 시간대
/// 예: begin_minute=540(09:00), end_minute=1080(18:00)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct DailyWorkWindow {
    /// 작업 시작 가능 시각 (자정부터 분)
    pub begin_minute: i32,
    /// 작업 종료 시각 (자정부터 분, begin보다 작으면 자정 넘김)
    pub end_minute: i32,
}

impl DailyWorkWindow {
    pub fn new(begin_minute: i32, end_minute: i32) -> Self {
        Self {
            begin_minute,
            end_minute,
        }
    }

    pub fn from_hm(begin_h: i32, begin_m: i32, end_h: i32, end_m: i32) -> Self {
        Self {
            begin_minute: begin_h * 60 + begin_m,
            end_minute: end_h * 60 + end_m,
        }
    }

    /// 특정 시각(분)이 윈도우 내에 있는지 확인
    pub fn contains_minute(&self, minute_of_day: i32) -> bool {
        if self.end_minute > self.begin_minute {
            minute_of_day >= self.begin_minute && minute_of_day < self.end_minute
        } else {
            // 자정을 넘는 경우
            minute_of_day >= self.begin_minute || minute_of_day < self.end_minute
        }
    }
}

/// Operation - 단일 공정 단계
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Operation {
    /// 작업 ID
    pub id: String,
    /// 소속 Job ID
    pub job_id: String,
    /// 공정 순서
    pub sequence: i32,
    /// 작업 유형 (스킬 매트릭스 매핑용)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_type: Option<String>,
    /// 작업 시간
    pub time: OperationTime,
    /// 필요 자원
    pub required_resources: Vec<ResourceRequirement>,
    /// 필요 자재
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub material_requirements: Vec<MaterialRequirement>,
    /// 분할 가능 여부
    pub is_splittable: bool,
    /// 병렬 처리 가능 여부
    pub allow_parallel: bool,
    /// 분할 설정
    pub split_config: Option<SplitConfig>,
    /// 중첩 설정
    pub overlap_config: Option<OverlapConfig>,
    /// 작업자 필요 여부 (false = 무인공정)
    #[serde(default = "default_requires_operator")]
    pub requires_operator: bool,
    /// 필요 팀/조 ID (조 단위 할당 필요 시)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crew_id: Option<String>,
    /// 외주 공정 설정
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outsourcing_config: Option<OutsourcingConfig>,
    /// 시간 윈도우 제약 (Phase 11)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_window: Option<OperationTimeWindow>,
    /// PERT 3-point estimation (Phase 11)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pert_duration: Option<PertDuration>,
    /// 일일 작업시간 제약 (MES BeginWorkTime/EndWorkTime 대응)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_work_window: Option<DailyWorkWindow>,
}

fn default_requires_operator() -> bool {
    true
}

impl Operation {
    pub fn new(id: impl Into<String>, job_id: impl Into<String>, sequence: i32) -> Self {
        Self {
            id: id.into(),
            job_id: job_id.into(),
            sequence,
            operation_type: None,
            time: OperationTime::new(0, 0, 0),
            required_resources: Vec::new(),
            material_requirements: Vec::new(),
            is_splittable: false,
            allow_parallel: false,
            split_config: None,
            overlap_config: None,
            requires_operator: true,
            crew_id: None,
            outsourcing_config: None,
            time_window: None,
            pert_duration: None,
            daily_work_window: None,
        }
    }

    /// Builder: 작업 유형 설정 (스킬 매트릭스 매핑용)
    pub fn with_operation_type(mut self, operation_type: impl Into<String>) -> Self {
        self.operation_type = Some(operation_type.into());
        self
    }

    /// Builder: 무인공정 설정
    pub fn unmanned(mut self) -> Self {
        self.requires_operator = false;
        self
    }

    /// Builder: 필요 조/팀 설정 (조 단위 일괄 할당)
    pub fn with_crew(mut self, crew_id: impl Into<String>) -> Self {
        self.crew_id = Some(crew_id.into());
        self
    }

    /// Builder: 외주 공정 설정
    pub fn with_outsourcing(mut self, config: OutsourcingConfig) -> Self {
        self.outsourcing_config = Some(config);
        self
    }

    /// Builder: 시간 윈도우 제약 설정 (Phase 11)
    pub fn with_time_window(mut self, time_window: OperationTimeWindow) -> Self {
        self.time_window = Some(time_window);
        self
    }

    /// Builder: 최소 시작 시간 설정 (Phase 11)
    pub fn with_earliest_start(mut self, earliest_start_ms: i64) -> Self {
        self.time_window = Some(OperationTimeWindow::with_earliest_start(earliest_start_ms));
        self
    }

    /// Builder: 마감 시간 설정 (Phase 11)
    pub fn with_deadline(mut self, latest_end_ms: i64) -> Self {
        self.time_window = Some(OperationTimeWindow::with_deadline(latest_end_ms));
        self
    }

    /// Builder: 시간 범위 제약 설정 (Phase 11)
    pub fn with_time_bounds(mut self, earliest_start_ms: i64, latest_end_ms: i64) -> Self {
        self.time_window = Some(OperationTimeWindow::bounded(
            earliest_start_ms,
            latest_end_ms,
        ));
        self
    }

    /// Builder: PERT 3-point estimation 설정 (Phase 11)
    pub fn with_pert(
        mut self,
        optimistic_ms: i64,
        most_likely_ms: i64,
        pessimistic_ms: i64,
    ) -> Self {
        self.pert_duration = Some(PertDuration::new(
            optimistic_ms,
            most_likely_ms,
            pessimistic_ms,
        ));
        self
    }

    /// Builder: 일일 작업시간 제약 설정
    pub fn with_daily_work_window(mut self, window: DailyWorkWindow) -> Self {
        self.daily_work_window = Some(window);
        self
    }

    /// Check if this operation is outsourced
    pub fn is_outsourced(&self) -> bool {
        self.outsourcing_config.is_some()
    }

    /// Check if this operation has time window constraints (Phase 11)
    pub fn has_time_window(&self) -> bool {
        self.time_window
            .as_ref()
            .is_some_and(|tw| tw.is_constrained())
    }

    /// Check if this operation has PERT estimation (Phase 11)
    pub fn has_pert(&self) -> bool {
        self.pert_duration.is_some()
    }

    /// Get effective process time (PERT mean or fixed) (Phase 11)
    pub fn effective_process_ms(&self) -> i64 {
        if let Some(ref pert) = self.pert_duration {
            pert.mean_ms().round() as i64
        } else {
            self.time.process_ms
        }
    }

    /// Get process time at confidence level (Phase 11)
    /// z_scores: 0.0=50%, 1.0=85%, 1.645=95%, 2.326=99%
    pub fn process_ms_at_confidence(&self, z_score: f64) -> i64 {
        if let Some(ref pert) = self.pert_duration {
            pert.at_confidence(z_score).round() as i64
        } else {
            self.time.process_ms
        }
    }

    /// Builder: 분할 가능 설정
    pub fn with_splitting(mut self, config: SplitConfig) -> Self {
        self.is_splittable = true;
        self.split_config = Some(config);
        self
    }

    /// Builder: 중첩 가능 설정
    pub fn with_overlapping(mut self, config: OverlapConfig) -> Self {
        self.allow_parallel = true;
        self.overlap_config = Some(config);
        self
    }

    /// 공정 분할 생성
    pub fn create_splits(&self, num_splits: usize) -> Vec<SplitOperation> {
        if !self.is_splittable || num_splits <= 1 {
            return vec![SplitOperation {
                parent_op_id: self.id.clone(),
                split_index: 0,
                process_ms: self.time.process_ms,
                assigned_machine: None,
            }];
        }

        let default_config = SplitConfig::default();
        let config = self.split_config.as_ref().unwrap_or(&default_config);
        let actual_splits = num_splits.min(config.max_splits);

        // 처리 시간을 균등 분할
        let base_time = self.time.process_ms / actual_splits as i64;
        let remainder = self.time.process_ms % actual_splits as i64;

        (0..actual_splits)
            .map(|i| {
                let extra = if i < remainder as usize { 1 } else { 0 };
                SplitOperation {
                    parent_op_id: self.id.clone(),
                    split_index: i,
                    process_ms: base_time + extra,
                    assigned_machine: None,
                }
            })
            .collect()
    }

    /// 분할 가능한 최대 수 계산
    pub fn max_possible_splits(&self) -> usize {
        if !self.is_splittable {
            return 1;
        }

        let default_config = SplitConfig::default();
        let config = self.split_config.as_ref().unwrap_or(&default_config);
        let time_based_max = (self.time.process_ms / config.min_split_size_ms) as usize;
        time_based_max.min(config.max_splits).max(1)
    }

    /// Builder: 시간 설정
    pub fn with_time(mut self, setup_ms: i64, process_ms: i64, wait_ms: i64) -> Self {
        self.time = OperationTime::new(setup_ms, process_ms, wait_ms);
        self
    }

    /// Builder: 장비 요구사항 추가
    pub fn with_equipment(mut self, equipment_ids: Vec<String>) -> Self {
        self.required_resources.push(ResourceRequirement {
            resource_type: ResourceType::Equipment,
            quantity: 1,
            candidates: equipment_ids,
            load_factor: 1.0,
        });
        self
    }

    /// Builder: 작업자 요구사항 추가
    pub fn with_workers(mut self, count: i32) -> Self {
        self.required_resources.push(ResourceRequirement {
            resource_type: ResourceType::Worker,
            quantity: count,
            candidates: Vec::new(),
            load_factor: 1.0,
        });
        self
    }

    /// Builder: 작업자 요구사항 추가 (부하계수 포함)
    pub fn with_workers_load(mut self, count: i32, load_factor: f64) -> Self {
        self.required_resources.push(ResourceRequirement {
            resource_type: ResourceType::Worker,
            quantity: count,
            candidates: Vec::new(),
            load_factor: load_factor.clamp(0.1, 1.0),
        });
        self
    }

    /// Builder: 자재 요구사항 추가
    pub fn with_material(mut self, requirement: MaterialRequirement) -> Self {
        self.material_requirements.push(requirement);
        self
    }

    /// Builder: 자재 요구사항 추가 (간편)
    pub fn with_material_simple(mut self, material_id: impl Into<String>, quantity: f64) -> Self {
        self.material_requirements
            .push(MaterialRequirement::new(material_id, quantity));
        self
    }

    /// 총 소요시간
    pub fn total_time_ms(&self) -> i64 {
        self.time.total_ms()
    }

    /// 자재 요구사항 존재 여부
    pub fn has_material_requirements(&self) -> bool {
        !self.material_requirements.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_time_total() {
        let time = OperationTime::new(
            15 * 60 * 1000, // 15분
            45 * 60 * 1000, // 45분
            5 * 60 * 1000,  // 5분
        );

        assert_eq!(time.total_ms(), 65 * 60 * 1000); // 65분
    }

    #[test]
    fn test_operation_builder() {
        let op = Operation::new("OP-001", "JOB-001", 1)
            .with_time(15 * 60 * 1000, 45 * 60 * 1000, 5 * 60 * 1000)
            .with_equipment(vec!["EQP-CNC-001".to_string()]);

        assert_eq!(op.id, "OP-001");
        assert_eq!(op.sequence, 1);
        assert_eq!(op.total_time_ms(), 65 * 60 * 1000);
        assert_eq!(op.required_resources.len(), 1);
    }

    #[test]
    fn test_operation_serialization() {
        let op = Operation::new("OP-001", "JOB-001", 1).with_time(1000, 2000, 500);

        let json = serde_json::to_string(&op).unwrap();
        let deserialized: Operation = serde_json::from_str(&json).unwrap();

        assert_eq!(op, deserialized);
    }

    #[test]
    fn test_operation_splitting() {
        let config = SplitConfig {
            min_split_size_ms: 10_000,
            max_splits: 4,
            split_setup_overhead_ms: 1000,
        };

        let op = Operation::new("OP-001", "JOB-001", 1)
            .with_time(0, 100_000, 0) // 100초 처리시간
            .with_splitting(config);

        // 3개로 분할
        let splits = op.create_splits(3);
        assert_eq!(splits.len(), 3);

        // 총 처리시간 확인
        let total: i64 = splits.iter().map(|s| s.process_ms).sum();
        assert_eq!(total, 100_000);

        // 각 분할 확인
        assert_eq!(splits[0].split_index, 0);
        assert_eq!(splits[1].split_index, 1);
        assert_eq!(splits[2].split_index, 2);
    }

    #[test]
    fn test_max_possible_splits() {
        let config = SplitConfig {
            min_split_size_ms: 30_000, // 30초 최소
            max_splits: 5,
            split_setup_overhead_ms: 0,
        };

        let op = Operation::new("OP-001", "JOB-001", 1)
            .with_time(0, 100_000, 0) // 100초
            .with_splitting(config);

        // 100초 / 30초 = 3, max_splits 5 → 3
        assert_eq!(op.max_possible_splits(), 3);
    }

    #[test]
    fn test_non_splittable_operation() {
        let op = Operation::new("OP-001", "JOB-001", 1).with_time(0, 100_000, 0);

        // 분할 불가능
        assert_eq!(op.max_possible_splits(), 1);

        let splits = op.create_splits(3);
        assert_eq!(splits.len(), 1);
        assert_eq!(splits[0].process_ms, 100_000);
    }

    #[test]
    fn test_operation_overlapping() {
        let config = OverlapConfig {
            min_predecessor_completion: 0.3,
            transfer_batch_size: 10,
        };

        let op = Operation::new("OP-001", "JOB-001", 1)
            .with_time(0, 60_000, 0)
            .with_overlapping(config);

        assert!(op.allow_parallel);
        assert!(op.overlap_config.is_some());
        assert_eq!(op.overlap_config.unwrap().min_predecessor_completion, 0.3);
    }

    // Phase 11: Time Window Tests
    #[test]
    fn test_time_window_basic() {
        let tw = OperationTimeWindow::bounded(1000, 5000);

        // Valid case
        let (valid, early, late) = tw.validate(1500, 4500);
        assert!(valid);
        assert_eq!(early, 0);
        assert_eq!(late, 0);

        // Early start violation
        let (valid, early, late) = tw.validate(500, 4500);
        assert!(!valid);
        assert_eq!(early, 500); // 1000 - 500
        assert_eq!(late, 0);

        // Late end violation
        let (valid, early, late) = tw.validate(1500, 6000);
        assert!(!valid);
        assert_eq!(early, 0);
        assert_eq!(late, 1000); // 6000 - 5000
    }

    #[test]
    fn test_time_window_soft_penalty() {
        let tw = OperationTimeWindow::bounded(1000, 5000).soft(0.5);

        // No violation = no penalty
        let penalty = tw.calculate_penalty(1500, 4500);
        assert_eq!(penalty, 0.0);

        // Violation = penalty
        let penalty = tw.calculate_penalty(1500, 6000);
        assert_eq!(penalty, 500.0); // 1000ms * 0.5
    }

    #[test]
    fn test_operation_with_time_window() {
        let op = Operation::new("OP-001", "JOB-001", 1)
            .with_time(1000, 30000, 500)
            .with_time_bounds(0, 60000);

        assert!(op.has_time_window());
        let tw = op.time_window.as_ref().unwrap();
        assert_eq!(tw.earliest_start_ms, Some(0));
        assert_eq!(tw.latest_end_ms, Some(60000));
    }

    #[test]
    fn test_operation_with_deadline() {
        let op = Operation::new("OP-001", "JOB-001", 1)
            .with_time(1000, 30000, 500)
            .with_deadline(100000);

        assert!(op.has_time_window());
        let tw = op.time_window.as_ref().unwrap();
        assert_eq!(tw.latest_end_ms, Some(100000));
    }

    // Phase 11: PERT Tests
    #[test]
    fn test_pert_duration_calculation() {
        let pert = PertDuration::new(60000, 90000, 180000);

        // Mean = (60000 + 4*90000 + 180000) / 6 = 600000 / 6 = 100000
        assert_eq!(pert.mean_ms(), 100000.0);

        // Std dev = (180000 - 60000) / 6 = 120000 / 6 = 20000
        assert_eq!(pert.std_dev_ms(), 20000.0);

        // Variance = 20000^2 = 400000000
        assert_eq!(pert.variance_ms_sq(), 400000000.0);
    }

    #[test]
    fn test_pert_confidence_levels() {
        let pert = PertDuration::new(60000, 90000, 180000);

        // 50% confidence (z=0) = mean
        assert_eq!(pert.at_confidence(0.0), 100000.0);

        // 85% confidence (z=1) = mean + std_dev
        assert_eq!(pert.at_confidence(1.0), 120000.0);

        // 95% confidence (z=1.645) = mean + 1.645*std_dev
        let expected_95 = 100000.0 + 1.645 * 20000.0;
        assert!((pert.at_confidence(1.645) - expected_95).abs() < 0.1);
    }

    #[test]
    fn test_operation_with_pert() {
        let op = Operation::new("OP-001", "JOB-001", 1)
            .with_time(1000, 90000, 500) // Base process time
            .with_pert(60000, 90000, 180000); // PERT estimation

        assert!(op.has_pert());

        // Effective process time should use PERT mean
        assert_eq!(op.effective_process_ms(), 100000);

        // At 85% confidence
        assert_eq!(op.process_ms_at_confidence(1.0), 120000);
    }

    #[test]
    fn test_operation_without_pert_uses_fixed() {
        let op = Operation::new("OP-001", "JOB-001", 1).with_time(1000, 90000, 500);

        assert!(!op.has_pert());

        // Without PERT, use fixed time
        assert_eq!(op.effective_process_ms(), 90000);
        assert_eq!(op.process_ms_at_confidence(1.0), 90000);
    }

    #[test]
    fn test_time_window_serialization() {
        let op = Operation::new("OP-001", "JOB-001", 1)
            .with_time(1000, 30000, 500)
            .with_time_bounds(0, 60000)
            .with_pert(20000, 30000, 50000);

        let json = serde_json::to_string(&op).unwrap();
        let deserialized: Operation = serde_json::from_str(&json).unwrap();

        assert_eq!(op.time_window, deserialized.time_window);
        assert_eq!(op.pert_duration, deserialized.pert_duration);
    }

    #[test]
    fn test_daily_work_window() {
        let window = DailyWorkWindow::from_hm(9, 0, 18, 0);
        assert!(window.contains_minute(540)); // 09:00
        assert!(window.contains_minute(600)); // 10:00
        assert!(!window.contains_minute(480)); // 08:00
        assert!(!window.contains_minute(1080)); // 18:00

        // 자정 넘김 (22:00~06:00)
        let night = DailyWorkWindow::from_hm(22, 0, 6, 0);
        assert!(night.contains_minute(23 * 60)); // 23:00
        assert!(night.contains_minute(3 * 60)); // 03:00
        assert!(!night.contains_minute(12 * 60)); // 12:00
    }

    #[test]
    fn test_operation_with_daily_work_window() {
        let op = Operation::new("OP-1", "J-1", 1)
            .with_daily_work_window(DailyWorkWindow::from_hm(9, 0, 18, 0));
        assert!(op.daily_work_window.is_some());
        let dw = op.daily_work_window.unwrap();
        assert_eq!(dw.begin_minute, 540);
        assert_eq!(dw.end_minute, 1080);
    }
}
