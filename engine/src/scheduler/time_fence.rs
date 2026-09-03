//! Time Fences - 계획 경계 관리
//!
//! Frozen, Slushy, Liquid 구역으로 스케줄 변경 제한 관리

use serde::{Deserialize, Serialize};

/// Time Fence 구역 유형
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FenceZone {
    /// 변경 불가 (실행 중, 자재 확정)
    Frozen,
    /// 제한적 변경 (승인 필요)
    Slushy,
    /// 자유 변경 가능
    Liquid,
}

impl FenceZone {
    /// 변경 가능 여부
    pub fn can_modify(&self) -> bool {
        !matches!(self, FenceZone::Frozen)
    }

    /// 승인 필요 여부
    pub fn requires_approval(&self) -> bool {
        matches!(self, FenceZone::Slushy)
    }
}

/// Time Fence 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeFenceConfig {
    /// Frozen Zone 종료 시점 (현재 시점 기준 ms)
    pub frozen_end_ms: i64,
    /// Slushy Zone 종료 시점 (현재 시점 기준 ms)
    pub slushy_end_ms: i64,
    /// 현재 시점 (ms)
    pub current_time_ms: i64,
}

impl TimeFenceConfig {
    /// 새 Time Fence 설정 생성
    pub fn new(current_time_ms: i64, frozen_hours: i64, slushy_hours: i64) -> Self {
        let hour_ms = 3_600_000;
        Self {
            frozen_end_ms: frozen_hours * hour_ms,
            slushy_end_ms: (frozen_hours + slushy_hours) * hour_ms,
            current_time_ms,
        }
    }

    /// 기본 설정 (Frozen: 24시간, Slushy: 48시간)
    pub fn default_with_time(current_time_ms: i64) -> Self {
        Self::new(current_time_ms, 24, 48)
    }

    /// 특정 시점의 Fence Zone 결정
    pub fn get_zone(&self, time_ms: i64) -> FenceZone {
        let relative = time_ms - self.current_time_ms;

        if relative < self.frozen_end_ms {
            FenceZone::Frozen
        } else if relative < self.slushy_end_ms {
            FenceZone::Slushy
        } else {
            FenceZone::Liquid
        }
    }

    /// 공정이 변경 가능한지 확인
    pub fn can_modify_operation(&self, start_ms: i64) -> bool {
        self.get_zone(start_ms).can_modify()
    }

    /// Frozen Zone 절대 시간
    pub fn frozen_boundary(&self) -> i64 {
        self.current_time_ms + self.frozen_end_ms
    }

    /// Slushy Zone 절대 시간
    pub fn slushy_boundary(&self) -> i64 {
        self.current_time_ms + self.slushy_end_ms
    }
}

/// Time Fence 위반
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FenceViolation {
    /// 공정 ID
    pub operation_id: String,
    /// 위반된 Zone
    pub zone: FenceZone,
    /// 위반 시점
    pub time_ms: i64,
    /// 위반 유형
    pub violation_type: FenceViolationType,
}

/// Fence 위반 유형
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FenceViolationType {
    /// Frozen Zone에서 변경 시도
    FrozenModification,
    /// 승인 없이 Slushy Zone 변경
    UnapprovedSlushyChange,
    /// 공정 삭제
    OperationDeleted,
    /// 공정 추가
    OperationAdded,
    /// 시간 변경
    TimeChanged { old_start: i64, new_start: i64 },
    /// 자원 변경
    ResourceChanged { old: String, new: String },
}

/// Time Fence 검사기
pub struct TimeFenceChecker {
    config: TimeFenceConfig,
}

impl TimeFenceChecker {
    pub fn new(config: TimeFenceConfig) -> Self {
        Self { config }
    }

    /// 스케줄 변경 검사
    pub fn check_schedule_change(
        &self,
        old_schedule: &crate::scheduler::Schedule,
        new_schedule: &crate::scheduler::Schedule,
    ) -> Vec<FenceViolation> {
        let mut violations = Vec::new();

        // 기존 공정의 변경 검사
        for old_assign in &old_schedule.assignments {
            let zone = self.config.get_zone(old_assign.start_ms);

            if let Some(new_assign) = new_schedule
                .assignments
                .iter()
                .find(|a| a.operation_id == old_assign.operation_id)
            {
                // 시간 변경 검사
                if old_assign.start_ms != new_assign.start_ms && !zone.can_modify() {
                    violations.push(FenceViolation {
                        operation_id: old_assign.operation_id.clone(),
                        zone,
                        time_ms: old_assign.start_ms,
                        violation_type: FenceViolationType::TimeChanged {
                            old_start: old_assign.start_ms,
                            new_start: new_assign.start_ms,
                        },
                    });
                }

                // 자원 변경 검사
                if old_assign.resource_id != new_assign.resource_id && !zone.can_modify() {
                    violations.push(FenceViolation {
                        operation_id: old_assign.operation_id.clone(),
                        zone,
                        time_ms: old_assign.start_ms,
                        violation_type: FenceViolationType::ResourceChanged {
                            old: old_assign.resource_id.clone(),
                            new: new_assign.resource_id.clone(),
                        },
                    });
                }
            } else {
                // 공정 삭제
                if !zone.can_modify() {
                    violations.push(FenceViolation {
                        operation_id: old_assign.operation_id.clone(),
                        zone,
                        time_ms: old_assign.start_ms,
                        violation_type: FenceViolationType::OperationDeleted,
                    });
                }
            }
        }

        violations
    }

    /// 단일 변경 검사
    pub fn check_single_change(
        &self,
        operation_id: &str,
        old_start: i64,
        new_start: i64,
    ) -> Option<FenceViolation> {
        let zone = self.config.get_zone(old_start);

        if !zone.can_modify() && old_start != new_start {
            Some(FenceViolation {
                operation_id: operation_id.to_string(),
                zone,
                time_ms: old_start,
                violation_type: FenceViolationType::TimeChanged {
                    old_start,
                    new_start,
                },
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fence_zone_determination() {
        let config = TimeFenceConfig::new(0, 24, 48);
        let hour = 3_600_000i64;

        // Frozen (0-24시간)
        assert_eq!(config.get_zone(0), FenceZone::Frozen);
        assert_eq!(config.get_zone(12 * hour), FenceZone::Frozen);

        // Slushy (24-72시간)
        assert_eq!(config.get_zone(30 * hour), FenceZone::Slushy);
        assert_eq!(config.get_zone(60 * hour), FenceZone::Slushy);

        // Liquid (72시간+)
        assert_eq!(config.get_zone(80 * hour), FenceZone::Liquid);
    }

    #[test]
    fn test_can_modify() {
        assert!(!FenceZone::Frozen.can_modify());
        assert!(FenceZone::Slushy.can_modify());
        assert!(FenceZone::Liquid.can_modify());
    }

    #[test]
    fn test_requires_approval() {
        assert!(!FenceZone::Frozen.requires_approval());
        assert!(FenceZone::Slushy.requires_approval());
        assert!(!FenceZone::Liquid.requires_approval());
    }

    #[test]
    fn test_boundaries() {
        let hour = 3_600_000i64;
        let config = TimeFenceConfig::new(100_000, 24, 48);

        assert_eq!(config.frozen_boundary(), 100_000 + 24 * hour);
        assert_eq!(config.slushy_boundary(), 100_000 + 72 * hour);
    }
}
