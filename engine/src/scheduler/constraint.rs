//! Constraint - 제약조건 시스템

use serde::{Deserialize, Serialize};

/// 제약조건 유형
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConstraintType {
    /// 납기 제약
    DueDate,
    /// 최조 시작 시간
    EarliestStart,
    /// 선후행 관계
    Precedence,
    /// 자원 용량
    Capacity,
    /// 자재 가용성
    MaterialAvailability,
    /// 자재 부족
    MaterialShortage,
    /// 안전재고 침범
    SafetyStockViolation,
}

/// 제약조건
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    pub id: String,
    pub constraint_type: ConstraintType,
    pub target_id: String,
    pub value: ConstraintValue,
    /// 필수 제약 여부 (false면 soft constraint)
    pub is_hard: bool,
    /// 위반시 페널티 가중치
    pub penalty_weight: f64,
}

/// 제약조건 값
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConstraintValue {
    TimeMs(i64),
    StringRef(String),
    Number(f64),
}

impl Constraint {
    /// 납기 제약 생성
    pub fn due_date(job_id: impl Into<String>, due_date_ms: i64) -> Self {
        let job_id_str = job_id.into();
        Self {
            id: format!("DUE-{}", job_id_str),
            constraint_type: ConstraintType::DueDate,
            target_id: job_id_str,
            value: ConstraintValue::TimeMs(due_date_ms),
            is_hard: false, // 납기는 기본적으로 soft
            penalty_weight: 1.0,
        }
    }

    /// 최조 시작 시간 제약
    pub fn earliest_start(target_id: impl Into<String>, time_ms: i64) -> Self {
        let target_id_str = target_id.into();
        Self {
            id: format!("ES-{}", target_id_str),
            constraint_type: ConstraintType::EarliestStart,
            target_id: target_id_str,
            value: ConstraintValue::TimeMs(time_ms),
            is_hard: true,
            penalty_weight: 1.0,
        }
    }

    /// 선후행 관계 제약
    pub fn precedence(successor_id: impl Into<String>, predecessor_id: impl Into<String>) -> Self {
        let succ = successor_id.into();
        let pred = predecessor_id.into();
        Self {
            id: format!("PREC-{}-{}", pred, succ),
            constraint_type: ConstraintType::Precedence,
            target_id: succ,
            value: ConstraintValue::StringRef(pred),
            is_hard: true,
            penalty_weight: 1.0,
        }
    }
}

/// 제약조건 위반 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    pub constraint_id: String,
    pub constraint_type: ConstraintType,
    pub target_id: String,
    /// 위반 정도 (예: 납기 지연 시간)
    pub amount: f64,
    pub message: String,
}

impl Violation {
    pub fn due_date_violation(job_id: &str, delay_ms: i64) -> Self {
        Self {
            constraint_id: format!("DUE-{}", job_id),
            constraint_type: ConstraintType::DueDate,
            target_id: job_id.to_string(),
            amount: delay_ms as f64,
            message: format!("Job {} is late by {}ms", job_id, delay_ms),
        }
    }

    /// 자재 가용성 위반 (자재 가용일 미도래)
    pub fn material_availability_violation(
        operation_id: &str,
        material_id: &str,
        delay_ms: i64,
    ) -> Self {
        Self {
            constraint_id: format!("MAT-AVAIL-{}-{}", operation_id, material_id),
            constraint_type: ConstraintType::MaterialAvailability,
            target_id: operation_id.to_string(),
            amount: delay_ms as f64,
            message: format!(
                "Operation {} delayed by {}ms waiting for material {}",
                operation_id, delay_ms, material_id
            ),
        }
    }

    /// 자재 부족 위반 (재고 부족)
    pub fn material_shortage_violation(
        operation_id: &str,
        material_id: &str,
        required: f64,
        available: f64,
    ) -> Self {
        let shortage = required - available;
        Self {
            constraint_id: format!("MAT-SHORT-{}-{}", operation_id, material_id),
            constraint_type: ConstraintType::MaterialShortage,
            target_id: operation_id.to_string(),
            amount: shortage,
            message: format!(
                "Operation {} requires {} of material {}, but only {} available (shortage: {})",
                operation_id, required, material_id, available, shortage
            ),
        }
    }

    /// 안전재고 침범 위반 (soft warning)
    pub fn safety_stock_violation(
        material_id: &str,
        current_stock: f64,
        safety_stock: f64,
        after_consumption: f64,
    ) -> Self {
        Self {
            constraint_id: format!("SAFETY-{}", material_id),
            constraint_type: ConstraintType::SafetyStockViolation,
            target_id: material_id.to_string(),
            amount: safety_stock - after_consumption,
            message: format!(
                "Material {} will drop below safety stock: current={}, safety={}, after_consumption={}",
                material_id, current_stock, safety_stock, after_consumption
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_due_date_constraint() {
        let constraint = Constraint::due_date("JOB-001", 1000000);

        assert_eq!(constraint.constraint_type, ConstraintType::DueDate);
        assert_eq!(constraint.target_id, "JOB-001");
        assert!(!constraint.is_hard);
    }

    #[test]
    fn test_earliest_start_constraint() {
        let constraint = Constraint::earliest_start("OP-001", 5000);

        assert_eq!(constraint.constraint_type, ConstraintType::EarliestStart);
        assert!(constraint.is_hard);
    }

    #[test]
    fn test_precedence_constraint() {
        let constraint = Constraint::precedence("OP-002", "OP-001");

        assert_eq!(constraint.constraint_type, ConstraintType::Precedence);
        assert_eq!(constraint.target_id, "OP-002");

        if let ConstraintValue::StringRef(pred) = constraint.value {
            assert_eq!(pred, "OP-001");
        } else {
            panic!("Expected StringRef");
        }
    }

    #[test]
    fn test_violation() {
        let violation = Violation::due_date_violation("JOB-001", 60000);

        assert_eq!(violation.target_id, "JOB-001");
        assert_eq!(violation.amount, 60000.0);
    }
}
