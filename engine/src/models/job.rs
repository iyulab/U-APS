//! Job - 생산 오더 (스케줄링 대상)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::Operation;

/// Job - 스케줄링 대상 작업 단위
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Job {
    /// Job ID
    pub id: String,
    /// 제품명 (Setup Matrix에서 사용)
    pub product_name: Option<String>,
    /// 작업 목록
    pub operations: Vec<Operation>,
    /// 우선순위 (낮을수록 높은 우선순위)
    pub priority: i32,
    /// 납기
    pub due_date: Option<DateTime<Utc>>,
    /// 최조 시작 가능 시간
    pub earliest_start: Option<DateTime<Utc>>,
    /// 수량
    pub quantity: i32,
    /// 대상 사이트/공장 ID (multi-site 스케줄링용)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_site_id: Option<String>,
}

impl Job {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            product_name: None,
            operations: Vec::new(),
            priority: 100, // 기본 우선순위
            due_date: None,
            earliest_start: None,
            quantity: 1,
            target_site_id: None,
        }
    }

    /// Builder: 제품명 설정
    pub fn with_product(mut self, product_name: impl Into<String>) -> Self {
        self.product_name = Some(product_name.into());
        self
    }

    /// Builder: Operation 추가
    pub fn with_operation(mut self, operation: Operation) -> Self {
        self.operations.push(operation);
        self
    }

    /// Builder: 여러 Operation 추가
    pub fn with_operations(mut self, operations: Vec<Operation>) -> Self {
        self.operations.extend(operations);
        self
    }

    /// Builder: 우선순위 설정
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Builder: 납기 설정
    pub fn with_due_date(mut self, due_date: DateTime<Utc>) -> Self {
        self.due_date = Some(due_date);
        self
    }

    /// Builder: 수량 설정
    pub fn with_quantity(mut self, quantity: i32) -> Self {
        self.quantity = quantity;
        self
    }

    /// Builder: 대상 사이트/공장 설정
    pub fn with_site(mut self, site_id: impl Into<String>) -> Self {
        self.target_site_id = Some(site_id.into());
        self
    }

    /// 총 작업 시간 (모든 Operation 합계)
    pub fn total_time_ms(&self) -> i64 {
        self.operations.iter().map(|op| op.total_time_ms()).sum()
    }

    /// Operation 수
    pub fn operation_count(&self) -> usize {
        self.operations.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_creation() {
        let job = Job::new("JOB-001");

        assert_eq!(job.id, "JOB-001");
        assert_eq!(job.priority, 100);
        assert_eq!(job.quantity, 1);
        assert!(job.operations.is_empty());
    }

    #[test]
    fn test_job_with_operations() {
        let op1 = Operation::new("OP-001", "JOB-001", 1).with_time(1000, 2000, 500);
        let op2 = Operation::new("OP-002", "JOB-001", 2).with_time(500, 3000, 1000);

        let job = Job::new("JOB-001")
            .with_operation(op1)
            .with_operation(op2)
            .with_priority(1);

        assert_eq!(job.operation_count(), 2);
        assert_eq!(job.total_time_ms(), 8000); // 3500 + 4500
        assert_eq!(job.priority, 1);
    }

    #[test]
    fn test_job_serialization() {
        let op = Operation::new("OP-001", "JOB-001", 1).with_time(1000, 2000, 500);

        let job = Job::new("JOB-001").with_operation(op).with_quantity(100);

        let json = serde_json::to_string(&job).unwrap();
        let deserialized: Job = serde_json::from_str(&json).unwrap();

        assert_eq!(job, deserialized);
    }

    #[test]
    fn test_job_total_time() {
        let job = Job::new("JOB-001")
            .with_operation(
                Operation::new("OP-001", "JOB-001", 1).with_time(15 * 60000, 45 * 60000, 5 * 60000), // 65분
            )
            .with_operation(
                Operation::new("OP-002", "JOB-001", 2).with_time(10 * 60000, 30 * 60000, 0), // 40분
            );

        assert_eq!(job.total_time_ms(), 105 * 60000); // 105분
    }
}
