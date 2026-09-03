//! Schedule - 스케줄링 결과

use crate::Task;
use serde::{Deserialize, Serialize};

/// Assignment - 작업 할당 결과
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Assignment {
    /// Operation ID
    pub operation_id: String,
    /// Job ID
    pub job_id: String,
    /// 할당된 자원 ID
    pub resource_id: String,
    /// 시작 시간 (epoch ms)
    pub start_ms: i64,
    /// 종료 시간 (epoch ms)
    pub end_ms: i64,
    /// 준비 시간 (epoch ms)
    pub setup_ms: i64,
    /// 사이트/공장 ID (multi-site 스케줄링용)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site_id: Option<String>,
}

impl Assignment {
    pub fn new(
        operation_id: impl Into<String>,
        job_id: impl Into<String>,
        resource_id: impl Into<String>,
        start_ms: i64,
        end_ms: i64,
    ) -> Self {
        Self {
            operation_id: operation_id.into(),
            job_id: job_id.into(),
            resource_id: resource_id.into(),
            start_ms,
            end_ms,
            setup_ms: 0,
            site_id: None,
        }
    }

    /// Setup 시간 포함 생성
    pub fn with_setup(
        operation_id: impl Into<String>,
        job_id: impl Into<String>,
        resource_id: impl Into<String>,
        start_ms: i64,
        end_ms: i64,
        setup_ms: i64,
    ) -> Self {
        Self {
            operation_id: operation_id.into(),
            job_id: job_id.into(),
            resource_id: resource_id.into(),
            start_ms,
            end_ms,
            setup_ms,
            site_id: None,
        }
    }

    /// 작업 시간 (ms)
    pub fn duration_ms(&self) -> i64 {
        self.end_ms - self.start_ms
    }
}

use super::Violation;

/// Schedule - 스케줄링 결과
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Schedule {
    /// 할당 결과 목록
    pub assignments: Vec<Assignment>,
    /// 총 소요 시간 (makespan)
    pub makespan_ms: i64,
    /// 제약조건 위반 목록
    pub violations: Vec<Violation>,
    /// Task 목록 (Operation을 Calendar 기반으로 분할한 작업 단위)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<Task>,
}

impl Schedule {
    pub fn new() -> Self {
        Self {
            assignments: Vec::new(),
            makespan_ms: 0,
            violations: Vec::new(),
            tasks: Vec::new(),
        }
    }

    /// 위반 추가
    pub fn add_violation(&mut self, violation: Violation) {
        self.violations.push(violation);
    }

    /// 납기 준수 여부
    pub fn is_on_time(&self) -> bool {
        self.violations.is_empty()
    }

    /// Assignment 추가
    pub fn add_assignment(&mut self, assignment: Assignment) {
        if assignment.end_ms > self.makespan_ms {
            self.makespan_ms = assignment.end_ms;
        }
        self.assignments.push(assignment);
    }

    /// Task 추가
    pub fn add_task(&mut self, task: Task) {
        if task.plan_end_ms > self.makespan_ms {
            self.makespan_ms = task.plan_end_ms;
        }
        self.tasks.push(task);
    }

    /// 여러 Task 추가
    pub fn add_tasks(&mut self, tasks: Vec<Task>) {
        for task in tasks {
            self.add_task(task);
        }
    }

    /// 특정 Operation의 Task 목록
    pub fn tasks_for_operation(&self, operation_id: &str) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|t| t.operation_id == operation_id)
            .collect()
    }

    /// 특정 자원의 Task 목록
    pub fn tasks_for_resource(&self, resource_id: &str) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|t| t.equipment_id.as_deref() == Some(resource_id))
            .collect()
    }

    /// 특정 자원의 할당 목록
    pub fn assignments_for_resource(&self, resource_id: &str) -> Vec<&Assignment> {
        self.assignments
            .iter()
            .filter(|a| a.resource_id == resource_id)
            .collect()
    }

    /// 특정 Operation의 할당
    pub fn assignment_for_operation(&self, operation_id: &str) -> Option<&Assignment> {
        self.assignments
            .iter()
            .find(|a| a.operation_id == operation_id)
    }

    /// 특정 사이트의 할당 목록
    pub fn assignments_for_site(&self, site_id: &str) -> Vec<&Assignment> {
        self.assignments
            .iter()
            .filter(|a| a.site_id.as_deref() == Some(site_id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assignment_creation() {
        let assignment = Assignment::new("OP-001", "JOB-001", "EQP-001", 0, 60000);

        assert_eq!(assignment.operation_id, "OP-001");
        assert_eq!(assignment.job_id, "JOB-001");
        assert_eq!(assignment.duration_ms(), 60000);
    }

    #[test]
    fn test_schedule_makespan() {
        let mut schedule = Schedule::new();

        schedule.add_assignment(Assignment::new("OP-001", "JOB-001", "EQP-001", 0, 30000));
        assert_eq!(schedule.makespan_ms, 30000);

        schedule.add_assignment(Assignment::new(
            "OP-002", "JOB-001", "EQP-001", 30000, 60000,
        ));
        assert_eq!(schedule.makespan_ms, 60000);

        // makespan은 최대값 유지
        schedule.add_assignment(Assignment::new("OP-003", "JOB-002", "EQP-002", 0, 45000));
        assert_eq!(schedule.makespan_ms, 60000);
    }

    #[test]
    fn test_schedule_queries() {
        let mut schedule = Schedule::new();

        schedule.add_assignment(Assignment::new("OP-001", "JOB-001", "EQP-001", 0, 30000));
        schedule.add_assignment(Assignment::new(
            "OP-002", "JOB-001", "EQP-001", 30000, 60000,
        ));
        schedule.add_assignment(Assignment::new("OP-003", "JOB-002", "EQP-002", 0, 45000));

        assert_eq!(schedule.assignments_for_resource("EQP-001").len(), 2);
        assert_eq!(schedule.assignments_for_resource("EQP-002").len(), 1);

        let op = schedule.assignment_for_operation("OP-002").unwrap();
        assert_eq!(op.start_ms, 30000);
    }
}
