//! Error - 통합 에러 처리
//!
//! U-APS 엔진의 모든 에러 타입 정의

use serde::{Deserialize, Serialize};
use std::fmt;

/// U-APS 결과 타입
pub type Result<T> = std::result::Result<T, UapsError>;

/// U-APS 에러
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UapsError {
    /// 에러 코드
    pub code: ErrorCode,
    /// 에러 메시지
    pub message: String,
    /// 상세 정보
    pub details: Option<String>,
    /// 관련 엔티티 ID
    pub entity_id: Option<String>,
}

impl UapsError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
            entity_id: None,
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    pub fn with_entity(mut self, entity_id: impl Into<String>) -> Self {
        self.entity_id = Some(entity_id.into());
        self
    }
}

impl fmt::Display for UapsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)?;
        if let Some(ref details) = self.details {
            write!(f, " - {}", details)?;
        }
        Ok(())
    }
}

impl std::error::Error for UapsError {}

/// 에러 코드
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    // 입력 검증 에러 (1xxx)
    InvalidInput = 1000,
    MissingRequiredField = 1001,
    InvalidTimeRange = 1002,
    InvalidQuantity = 1003,
    DuplicateId = 1004,
    InvalidReference = 1005,

    // 스케줄링 에러 (2xxx)
    SchedulingFailed = 2000,
    NoFeasibleSolution = 2001,
    ConstraintViolation = 2002,
    ResourceNotFound = 2003,
    OperationNotFound = 2004,
    JobNotFound = 2005,

    // 자원 에러 (3xxx)
    ResourceUnavailable = 3000,
    CapacityExceeded = 3001,
    ResourceConflict = 3002,

    // 자재 에러 (4xxx)
    MaterialShortage = 4000,
    MaterialNotFound = 4001,
    SupplyDelayed = 4002,

    // 시스템 에러 (5xxx)
    InternalError = 5000,
    SerializationError = 5001,
    FfiError = 5002,
    Timeout = 5003,

    // GA 에러 (6xxx)
    GaConvergenceFailed = 6000,
    InvalidChromosome = 6001,
    PopulationEmpty = 6002,

    // CP 에러 (7xxx)
    CpModelInvalid = 7000,
    CpInfeasible = 7001,
    CpTimeout = 7002,
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "E{:04}", *self as u32)
    }
}

// 편의 함수들

/// 입력 검증 에러 생성
pub fn validation_error(message: impl Into<String>) -> UapsError {
    UapsError::new(ErrorCode::InvalidInput, message)
}

/// 필수 필드 누락 에러
pub fn missing_field(field: &str) -> UapsError {
    UapsError::new(
        ErrorCode::MissingRequiredField,
        format!("Required field missing: {}", field),
    )
}

/// 중복 ID 에러
pub fn duplicate_id(entity_type: &str, id: &str) -> UapsError {
    UapsError::new(
        ErrorCode::DuplicateId,
        format!("Duplicate {} ID: {}", entity_type, id),
    )
    .with_entity(id)
}

/// 참조 에러
pub fn invalid_reference(entity_type: &str, id: &str) -> UapsError {
    UapsError::new(
        ErrorCode::InvalidReference,
        format!("{} not found: {}", entity_type, id),
    )
    .with_entity(id)
}

/// 스케줄링 실패 에러
pub fn scheduling_failed(reason: impl Into<String>) -> UapsError {
    UapsError::new(ErrorCode::SchedulingFailed, reason)
}

/// 자원 부족 에러
pub fn resource_unavailable(resource_id: &str) -> UapsError {
    UapsError::new(
        ErrorCode::ResourceUnavailable,
        format!("Resource unavailable: {}", resource_id),
    )
    .with_entity(resource_id)
}

/// 자재 부족 에러
pub fn material_shortage(material_id: &str, shortage: f64) -> UapsError {
    UapsError::new(
        ErrorCode::MaterialShortage,
        format!("Material shortage: {} (qty: {:.2})", material_id, shortage),
    )
    .with_entity(material_id)
}

/// 내부 에러
pub fn internal_error(message: impl Into<String>) -> UapsError {
    UapsError::new(ErrorCode::InternalError, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = UapsError::new(ErrorCode::InvalidInput, "Test error")
            .with_details("Additional info")
            .with_entity("entity-123");

        assert_eq!(err.code, ErrorCode::InvalidInput);
        assert_eq!(err.message, "Test error");
        assert_eq!(err.details, Some("Additional info".into()));
        assert_eq!(err.entity_id, Some("entity-123".into()));
    }

    #[test]
    fn test_error_display() {
        let err = validation_error("Invalid time range");
        let display = format!("{}", err);
        assert!(display.contains("E1000"));
        assert!(display.contains("Invalid time range"));
    }

    #[test]
    fn test_convenience_functions() {
        let err = missing_field("due_date");
        assert_eq!(err.code, ErrorCode::MissingRequiredField);

        let err = duplicate_id("Job", "job-1");
        assert_eq!(err.code, ErrorCode::DuplicateId);
        assert_eq!(err.entity_id, Some("job-1".into()));
    }

    #[test]
    fn test_error_code_display() {
        assert_eq!(format!("{}", ErrorCode::InvalidInput), "E1000");
        assert_eq!(format!("{}", ErrorCode::SchedulingFailed), "E2000");
    }
}
