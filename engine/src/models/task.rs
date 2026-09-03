//! Task - 공정작업 (Operation의 실제 스케줄링 단위)
//!
//! 계층 구조: Job → Operation → Task
//! - Operation: 정의/명세 단위
//! - Task: 실제 스케줄링 단위 (워킹캘린더에 의해 분할됨)

use serde::{Deserialize, Serialize};

/// Task 분할 이유
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum SplitReason {
    /// 휴게시간 시작으로 인한 분할
    BreakStart,
    /// 시프트 종료로 인한 분할
    ShiftEnd,
    /// 휴일로 인한 분할
    Holiday,
    /// 분할 없음 (단일 Task)
    #[default]
    None,
}

/// Task 상태 (Plan/Action)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum TaskStatus {
    /// 계획됨 (아직 실행 전)
    #[default]
    Plan,
    /// 실행 중
    InProgress,
    /// 완료됨 (실적 있음)
    Completed,
}

/// Task - 공정작업 (Operation의 실제 스케줄링 단위)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Task ID (예: "OP-001-T001", "OP-001-T002")
    pub id: String,

    /// 부모 Operation ID
    pub operation_id: String,

    /// 부모 Job ID
    pub job_id: String,

    /// 제품명 (Job에서 상속)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_name: Option<String>,

    // ========== 시간 정보 ==========
    /// 계획 시작 시간 (epoch ms)
    pub plan_begin_ms: i64,

    /// 계획 종료 시간 (epoch ms)
    pub plan_end_ms: i64,

    /// 실적 시작 시간 (epoch ms, nullable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_begin_ms: Option<i64>,

    /// 실적 종료 시간 (epoch ms, nullable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_end_ms: Option<i64>,

    // ========== 상태 정보 ==========
    /// Task 상태
    #[serde(default)]
    pub status: TaskStatus,

    /// 사용자 고정 여부 (재스케줄링 제외)
    #[serde(default)]
    pub is_fixed: bool,

    // ========== 분할 정보 ==========
    /// 분할 인덱스 (0, 1, 2...)
    pub split_index: u32,

    /// 총 분할 개수
    pub total_splits: u32,

    /// 분할 이유
    #[serde(default)]
    pub split_reason: SplitReason,

    // ========== 자원 정보 ==========
    /// 할당된 설비 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equipment_id: Option<String>,

    /// 할당된 작업자 ID 목록
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub worker_ids: Vec<String>,

    /// 필요 작업자 수 (0=무인공정)
    #[serde(default)]
    pub workers_required: u32,

    // ========== 공정 속성 ==========
    /// Setup 시간 (ms)
    #[serde(default)]
    pub setup_ms: i64,

    /// Process 시간 (ms) - 이 Task가 담당하는 공정 시간
    pub process_ms: i64,

    /// 원본 Operation이 분할 가능한지 여부
    #[serde(default)]
    pub is_splittable: bool,
}

impl Task {
    /// 새 Task 생성
    pub fn new(
        id: impl Into<String>,
        operation_id: impl Into<String>,
        job_id: impl Into<String>,
        plan_begin_ms: i64,
        plan_end_ms: i64,
    ) -> Self {
        Self {
            id: id.into(),
            operation_id: operation_id.into(),
            job_id: job_id.into(),
            product_name: None,
            plan_begin_ms,
            plan_end_ms,
            action_begin_ms: None,
            action_end_ms: None,
            status: TaskStatus::Plan,
            is_fixed: false,
            split_index: 0,
            total_splits: 1,
            split_reason: SplitReason::None,
            equipment_id: None,
            worker_ids: Vec::new(),
            workers_required: 0,
            setup_ms: 0,
            process_ms: plan_end_ms - plan_begin_ms,
            is_splittable: false,
        }
    }

    /// Operation에서 단일 Task 생성 (분할 없음)
    pub fn from_operation(
        operation_id: &str,
        job_id: &str,
        product_name: Option<&str>,
        plan_begin_ms: i64,
        plan_end_ms: i64,
        setup_ms: i64,
        is_splittable: bool,
    ) -> Self {
        let task_id = format!("{}-T001", operation_id);
        Self {
            id: task_id,
            operation_id: operation_id.to_string(),
            job_id: job_id.to_string(),
            product_name: product_name.map(|s| s.to_string()),
            plan_begin_ms,
            plan_end_ms,
            action_begin_ms: None,
            action_end_ms: None,
            status: TaskStatus::Plan,
            is_fixed: false,
            split_index: 0,
            total_splits: 1,
            split_reason: SplitReason::None,
            equipment_id: None,
            worker_ids: Vec::new(),
            workers_required: 0,
            setup_ms,
            process_ms: plan_end_ms - plan_begin_ms - setup_ms,
            is_splittable,
        }
    }

    /// Action(실적)이 있는지 확인
    pub fn has_action(&self) -> bool {
        self.action_begin_ms.is_some() || self.action_end_ms.is_some()
    }

    /// 불변(immutable) 여부 - action이 있으면 불변
    pub fn is_immutable(&self) -> bool {
        self.has_action() || self.status == TaskStatus::Completed
    }

    /// Task 시간(duration) 계산 (ms)
    pub fn duration_ms(&self) -> i64 {
        self.plan_end_ms - self.plan_begin_ms
    }

    /// 실제 작업 시간(process만) - setup 제외
    pub fn actual_process_ms(&self) -> i64 {
        self.process_ms
    }

    // ========== Builder 메서드 ==========

    /// 제품명 설정
    pub fn with_product_name(mut self, product_name: impl Into<String>) -> Self {
        self.product_name = Some(product_name.into());
        self
    }

    /// 설비 할당
    pub fn with_equipment(mut self, equipment_id: impl Into<String>) -> Self {
        self.equipment_id = Some(equipment_id.into());
        self
    }

    /// 작업자 할당
    pub fn with_workers(mut self, worker_ids: Vec<String>, required: u32) -> Self {
        self.worker_ids = worker_ids;
        self.workers_required = required;
        self
    }

    /// 고정 상태 설정
    pub fn with_fixed(mut self, is_fixed: bool) -> Self {
        self.is_fixed = is_fixed;
        self
    }

    /// 분할 정보 설정
    pub fn with_split_info(mut self, index: u32, total: u32, reason: SplitReason) -> Self {
        self.split_index = index;
        self.total_splits = total;
        self.split_reason = reason;
        self
    }

    /// Setup 시간 설정
    pub fn with_setup(mut self, setup_ms: i64) -> Self {
        self.setup_ms = setup_ms;
        self
    }

    /// Process 시간 설정
    pub fn with_process(mut self, process_ms: i64) -> Self {
        self.process_ms = process_ms;
        self
    }

    /// 분할 가능 여부 설정
    pub fn with_splittable(mut self, is_splittable: bool) -> Self {
        self.is_splittable = is_splittable;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_creation() {
        let task = Task::new("OP-001-T001", "OP-001", "JOB-001", 0, 60000);

        assert_eq!(task.id, "OP-001-T001");
        assert_eq!(task.operation_id, "OP-001");
        assert_eq!(task.job_id, "JOB-001");
        assert_eq!(task.plan_begin_ms, 0);
        assert_eq!(task.plan_end_ms, 60000);
        assert_eq!(task.duration_ms(), 60000);
        assert!(!task.has_action());
        assert!(!task.is_immutable());
    }

    #[test]
    fn test_task_from_operation() {
        let task = Task::from_operation(
            "OP-001",
            "JOB-001",
            Some("Product-A"),
            0,
            70000,
            10000,
            true,
        );

        assert_eq!(task.id, "OP-001-T001");
        assert_eq!(task.product_name, Some("Product-A".to_string()));
        assert_eq!(task.setup_ms, 10000);
        assert_eq!(task.process_ms, 60000); // 70000 - 10000
        assert!(task.is_splittable);
    }

    #[test]
    fn test_task_immutability() {
        let mut task = Task::new("OP-001-T001", "OP-001", "JOB-001", 0, 60000);

        // Action 없으면 mutable
        assert!(!task.is_immutable());

        // Action 추가하면 immutable
        task.action_begin_ms = Some(0);
        task.action_end_ms = Some(55000);
        assert!(task.is_immutable());
    }

    #[test]
    fn test_task_builder() {
        let task = Task::new("OP-001-T001", "OP-001", "JOB-001", 0, 60000)
            .with_product_name("Widget-A")
            .with_equipment("EQP-001")
            .with_workers(vec!["W-001".to_string(), "W-002".to_string()], 2)
            .with_fixed(true)
            .with_split_info(0, 2, SplitReason::BreakStart)
            .with_setup(5000)
            .with_splittable(true);

        assert_eq!(task.product_name, Some("Widget-A".to_string()));
        assert_eq!(task.equipment_id, Some("EQP-001".to_string()));
        assert_eq!(task.worker_ids.len(), 2);
        assert_eq!(task.workers_required, 2);
        assert!(task.is_fixed);
        assert_eq!(task.split_index, 0);
        assert_eq!(task.total_splits, 2);
        assert_eq!(task.split_reason, SplitReason::BreakStart);
        assert_eq!(task.setup_ms, 5000);
        assert!(task.is_splittable);
    }

    #[test]
    fn test_split_reason_serialization() {
        // Ensure proper serialization
        let reason = SplitReason::ShiftEnd;
        let json = serde_json::to_string(&reason).unwrap();
        assert!(json.contains("ShiftEnd"));

        let deserialized: SplitReason = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, SplitReason::ShiftEnd);
    }
}
