//! Rescheduling - 동적 재스케줄링 전략
//!
//! Right-Shift, AOR, Total Regeneration 전략

use crate::scheduler::time_fence::{FenceZone, TimeFenceConfig};
use crate::scheduler::{Assignment, Schedule};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 재스케줄링 전략
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RescheduleStrategy {
    /// 단순 지연 전파 (영향 공정만 시프트)
    RightShift,
    /// 영향 공정만 재스케줄 (Affected Operation Rescheduling)
    AOR,
    /// 부분 재스케줄 (영향받는 자원/작업만)
    Partial,
    /// 전체 재스케줄
    Full,
    /// 전체 재생성 (Total Regeneration)
    TotalRegeneration,
}

impl std::fmt::Display for RescheduleStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RescheduleStrategy::RightShift => write!(f, "RightShift"),
            RescheduleStrategy::AOR => write!(f, "AOR"),
            RescheduleStrategy::Partial => write!(f, "Partial"),
            RescheduleStrategy::Full => write!(f, "Full"),
            RescheduleStrategy::TotalRegeneration => write!(f, "TotalRegeneration"),
        }
    }
}

/// 스케줄 변경 이벤트
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScheduleEvent {
    // =========================================================================
    // Phase 10.1: 기존 이벤트
    // =========================================================================
    /// 공정 지연
    OperationDelay { operation_id: String, delay_ms: i64 },
    /// 기계 고장
    MachineBreakdown {
        resource_id: String,
        start_ms: i64,
        duration_ms: i64,
    },
    /// 긴급 주문 삽입
    UrgentOrder { job_id: String, priority: i32 },
    /// 주문 취소
    OrderCancellation { job_id: String },
    /// 처리 시간 변경
    ProcessTimeChange {
        operation_id: String,
        new_duration_ms: i64,
    },

    // =========================================================================
    // Phase 10.2: 자재 관련 이벤트
    // =========================================================================
    /// 자재 부족 발생
    MaterialShortage {
        material_id: String,
        shortage_qty: f64,
        /// 영향받는 공정 ID 목록
        affected_operations: Vec<String>,
    },
    /// 자재 입고 지연
    MaterialDelay {
        material_id: String,
        /// 원래 입고 예정일 (ms)
        original_arrival_ms: i64,
        /// 새로운 입고 예정일 (ms)
        new_arrival_ms: i64,
    },
    /// 자재 긴급 입고
    MaterialArrival {
        material_id: String,
        quantity: f64,
        arrival_time_ms: i64,
    },

    // =========================================================================
    // Phase 10.3: 작업자 관련 이벤트
    // =========================================================================
    /// 작업자 불가 (병가, 휴가 등)
    WorkerUnavailable {
        worker_id: String,
        start_ms: i64,
        duration_ms: i64,
        /// 대체 작업자 ID (있으면)
        replacement_id: Option<String>,
    },
    /// 작업자 복귀
    WorkerAvailable {
        worker_id: String,
        available_from_ms: i64,
    },
    /// 작업자 기술 변경 (인증 획득/만료)
    WorkerSkillChange {
        worker_id: String,
        skill_id: String,
        /// true = 획득, false = 만료
        acquired: bool,
    },

    // =========================================================================
    // Phase 10.4: 품질 관련 이벤트
    // =========================================================================
    /// 품질 불량 발생 (재작업 필요)
    QualityDefect {
        operation_id: String,
        /// 재작업 필요 수량
        rework_qty: f64,
        /// 추가 소요 시간 (ms)
        additional_time_ms: i64,
    },
    /// 검사 지연
    InspectionDelay { operation_id: String, delay_ms: i64 },

    // =========================================================================
    // Phase 10.5: 설비 관련 이벤트
    // =========================================================================
    /// 예방 보전 스케줄
    MaintenanceScheduled {
        resource_id: String,
        start_ms: i64,
        duration_ms: i64,
    },
    /// 설비 효율 변경
    EfficiencyChange {
        resource_id: String,
        /// 새로운 효율 (0.0 ~ 2.0)
        new_efficiency: f64,
    },
    /// 설비 복구 완료 (고장 후)
    MachineRecovered {
        resource_id: String,
        recovered_at_ms: i64,
    },

    // =========================================================================
    // Phase 10.6: 주문 관련 이벤트
    // =========================================================================
    /// 납기 변경
    DueDateChange {
        job_id: String,
        /// 새로운 납기 (ms)
        new_due_date_ms: i64,
    },
    /// 수량 변경
    QuantityChange { job_id: String, new_quantity: i64 },
    /// 우선순위 변경
    PriorityChange { job_id: String, new_priority: i32 },
}

impl ScheduleEvent {
    /// 이벤트 심각도 평가 (0.0 ~ 1.0)
    pub fn severity(&self) -> f64 {
        match self {
            // 높은 심각도 (0.8+)
            Self::MachineBreakdown { .. } => 0.9,
            Self::MaterialShortage { .. } => 0.85,
            Self::QualityDefect { .. } => 0.8,
            Self::WorkerUnavailable {
                replacement_id: None,
                ..
            } => 0.8,

            // 중간 심각도 (0.5-0.8)
            Self::UrgentOrder { .. } => 0.7,
            Self::MaterialDelay { .. } => 0.65,
            Self::OperationDelay { .. } => 0.6,
            Self::MaintenanceScheduled { .. } => 0.55,
            Self::WorkerUnavailable {
                replacement_id: Some(_),
                ..
            } => 0.5,

            // 낮은 심각도 (0.3-0.5)
            Self::DueDateChange { .. } => 0.45,
            Self::ProcessTimeChange { .. } => 0.4,
            Self::QuantityChange { .. } => 0.4,
            Self::InspectionDelay { .. } => 0.35,
            Self::PriorityChange { .. } => 0.3,

            // 긍정적 이벤트 (낮은 심각도)
            Self::MaterialArrival { .. } => 0.2,
            Self::WorkerAvailable { .. } => 0.15,
            Self::MachineRecovered { .. } => 0.1,
            Self::EfficiencyChange { new_efficiency, .. } if *new_efficiency > 1.0 => 0.1,
            Self::EfficiencyChange { .. } => 0.4,

            // 기타
            Self::OrderCancellation { .. } => 0.3,
            Self::WorkerSkillChange { acquired: true, .. } => 0.15,
            Self::WorkerSkillChange {
                acquired: false, ..
            } => 0.5,
        }
    }

    /// 권장 재스케줄 전략
    pub fn recommended_strategy(&self) -> RescheduleStrategy {
        // 이벤트 유형별 특화 전략
        match self {
            // 자재 부족은 전체 재스케줄 필요
            ScheduleEvent::MaterialShortage { .. } => RescheduleStrategy::Full,
            // 장비 고장은 부분 재스케줄
            ScheduleEvent::MachineBreakdown { .. } => RescheduleStrategy::Partial,
            // 긴급 주문은 전체 재스케줄
            ScheduleEvent::UrgentOrder { .. } => RescheduleStrategy::Full,
            // 긍정적 이벤트는 단순 시프트
            ScheduleEvent::MaterialArrival { .. }
            | ScheduleEvent::WorkerAvailable { .. }
            | ScheduleEvent::MachineRecovered { .. }
            | ScheduleEvent::WorkerSkillChange { acquired: true, .. } => {
                RescheduleStrategy::RightShift
            }
            // 기타 이벤트는 심각도 기반
            _ => match self.severity() {
                s if s >= 0.8 => RescheduleStrategy::TotalRegeneration,
                s if s >= 0.6 => RescheduleStrategy::Partial,
                s if s >= 0.4 => RescheduleStrategy::AOR,
                _ => RescheduleStrategy::RightShift,
            },
        }
    }

    /// 영향받는 자원 ID 목록
    pub fn affected_resources(&self) -> Vec<String> {
        match self {
            Self::MachineBreakdown { resource_id, .. }
            | Self::MaintenanceScheduled { resource_id, .. }
            | Self::EfficiencyChange { resource_id, .. }
            | Self::MachineRecovered { resource_id, .. } => vec![resource_id.clone()],

            Self::WorkerUnavailable {
                worker_id,
                replacement_id,
                ..
            } => {
                let mut ids = vec![worker_id.clone()];
                if let Some(replacement) = replacement_id {
                    ids.push(replacement.clone());
                }
                ids
            }
            Self::WorkerAvailable { worker_id, .. } | Self::WorkerSkillChange { worker_id, .. } => {
                vec![worker_id.clone()]
            }

            _ => vec![],
        }
    }

    /// 이벤트 분류
    pub fn category(&self) -> EventCategory {
        match self {
            Self::OperationDelay { .. }
            | Self::ProcessTimeChange { .. }
            | Self::InspectionDelay { .. }
            | Self::QualityDefect { .. } => EventCategory::Operation,

            Self::MachineBreakdown { .. }
            | Self::MaintenanceScheduled { .. }
            | Self::EfficiencyChange { .. }
            | Self::MachineRecovered { .. } => EventCategory::Equipment,

            Self::WorkerUnavailable { .. }
            | Self::WorkerAvailable { .. }
            | Self::WorkerSkillChange { .. } => EventCategory::Worker,

            Self::MaterialShortage { .. }
            | Self::MaterialDelay { .. }
            | Self::MaterialArrival { .. } => EventCategory::Material,

            Self::UrgentOrder { .. }
            | Self::OrderCancellation { .. }
            | Self::DueDateChange { .. }
            | Self::QuantityChange { .. }
            | Self::PriorityChange { .. } => EventCategory::Order,
        }
    }
}

/// 이벤트 카테고리
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventCategory {
    /// 공정/작업 관련
    Operation,
    /// 설비/기계 관련
    Equipment,
    /// 작업자 관련
    Worker,
    /// 자재 관련
    Material,
    /// 주문 관련
    Order,
}

/// 재스케줄링 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RescheduleResult {
    /// 새 스케줄
    pub schedule: Schedule,
    /// 변경된 공정 ID 목록
    pub affected_operations: Vec<String>,
    /// 적용된 전략
    pub strategy_used: RescheduleStrategy,
    /// 변경 통계
    pub stats: RescheduleStats,
}

/// 재스케줄링 통계
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RescheduleStats {
    /// 변경된 공정 수
    pub operations_changed: usize,
    /// 총 지연 시간 변화 (ms)
    pub total_delay_change_ms: i64,
    /// Makespan 변화 (ms)
    pub makespan_change_ms: i64,
}

/// 재스케줄러
pub struct Rescheduler {
    /// Time Fence 설정
    fence_config: Option<TimeFenceConfig>,
}

impl Rescheduler {
    pub fn new() -> Self {
        Self { fence_config: None }
    }

    pub fn with_fence(mut self, config: TimeFenceConfig) -> Self {
        self.fence_config = Some(config);
        self
    }

    /// Right-Shift: 지연 전파
    pub fn right_shift(&self, schedule: &Schedule, event: &ScheduleEvent) -> RescheduleResult {
        let mut new_schedule = schedule.clone();
        let mut affected = HashSet::new();

        match event {
            ScheduleEvent::OperationDelay {
                operation_id,
                delay_ms,
            } => {
                // 지연된 공정과 후속 공정들 시프트
                self.propagate_delay(&mut new_schedule, operation_id, *delay_ms, &mut affected);
            }
            ScheduleEvent::MachineBreakdown {
                resource_id,
                start_ms,
                duration_ms,
            } => {
                // 해당 기계의 영향받는 공정들 시프트
                self.shift_for_breakdown(
                    &mut new_schedule,
                    resource_id,
                    *start_ms,
                    *duration_ms,
                    &mut affected,
                );
            }
            _ => {}
        }

        let affected_vec: Vec<String> = affected.into_iter().collect();
        let stats = self.calculate_stats(schedule, &new_schedule, affected_vec.len());

        RescheduleResult {
            schedule: new_schedule,
            affected_operations: affected_vec,
            strategy_used: RescheduleStrategy::RightShift,
            stats,
        }
    }

    /// AOR: 영향 공정만 재스케줄
    pub fn aor(&self, schedule: &Schedule, event: &ScheduleEvent) -> RescheduleResult {
        let mut new_schedule = schedule.clone();
        let mut affected = HashSet::new();

        // 영향받는 공정 식별
        let affected_ops = self.identify_affected_operations(schedule, event);

        // Time Fence 확인 후 변경 가능한 공정만 재스케줄
        for op_id in &affected_ops {
            if self.can_reschedule_operation(&new_schedule, op_id) {
                affected.insert(op_id.clone());
            }
        }

        // 영향 공정들 재스케줄 (간단한 그리디)
        self.reschedule_affected(&mut new_schedule, &affected);

        let affected_vec: Vec<String> = affected.into_iter().collect();
        let stats = self.calculate_stats(schedule, &new_schedule, affected_vec.len());

        RescheduleResult {
            schedule: new_schedule,
            affected_operations: affected_vec,
            strategy_used: RescheduleStrategy::AOR,
            stats,
        }
    }

    /// Total Regeneration: 전체 재스케줄
    pub fn total_regeneration(
        &self,
        schedule: &Schedule,
        _event: &ScheduleEvent,
    ) -> RescheduleResult {
        // Frozen Zone 공정은 유지
        let mut fixed_ops = Vec::new();
        let mut regenerate_ops = Vec::new();

        if let Some(ref fence) = self.fence_config {
            for assign in &schedule.assignments {
                if fence.get_zone(assign.start_ms) == FenceZone::Frozen {
                    fixed_ops.push(assign.clone());
                } else {
                    regenerate_ops.push(assign.operation_id.clone());
                }
            }
        } else {
            regenerate_ops = schedule
                .assignments
                .iter()
                .map(|a| a.operation_id.clone())
                .collect();
        }

        // Frozen 공정 유지 + 나머지 공정을 greedy forward-scheduling으로 재생성
        let mut new_schedule = Schedule::new();

        // Frozen 공정을 먼저 배치
        for op in &fixed_ops {
            new_schedule.add_assignment(op.clone());
        }

        // 리소스별 가용 시간 (frozen 공정 기준)
        let mut resource_available: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();
        for op in &fixed_ops {
            let entry = resource_available
                .entry(op.resource_id.clone())
                .or_insert(0);
            *entry = (*entry).max(op.end_ms);
        }

        // Job별 가용 시간 (frozen 공정 기준)
        let mut job_available: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();
        for op in &fixed_ops {
            let entry = job_available.entry(op.job_id.clone()).or_insert(0);
            *entry = (*entry).max(op.end_ms);
        }

        // 재생성 대상 공정들을 시작 시간 순으로 정렬하여 greedy 배치
        let mut regen_assignments: Vec<Assignment> = schedule
            .assignments
            .iter()
            .filter(|a| regenerate_ops.contains(&a.operation_id))
            .cloned()
            .collect();
        regen_assignments.sort_by_key(|a| a.start_ms);

        for assign in regen_assignments {
            let duration = assign.end_ms - assign.start_ms;
            let resource_ready = resource_available
                .get(&assign.resource_id)
                .copied()
                .unwrap_or(0);
            let job_ready = job_available.get(&assign.job_id).copied().unwrap_or(0);
            let start = resource_ready.max(job_ready);

            let mut new_assign = assign;
            new_assign.start_ms = start;
            new_assign.end_ms = start + duration;

            resource_available.insert(new_assign.resource_id.clone(), new_assign.end_ms);
            job_available.insert(new_assign.job_id.clone(), new_assign.end_ms);
            new_schedule.add_assignment(new_assign);
        }

        let stats = self.calculate_stats(schedule, &new_schedule, regenerate_ops.len());

        RescheduleResult {
            schedule: new_schedule,
            affected_operations: regenerate_ops,
            strategy_used: RescheduleStrategy::TotalRegeneration,
            stats,
        }
    }

    /// 전략 자동 선택
    pub fn auto_reschedule(&self, schedule: &Schedule, event: &ScheduleEvent) -> RescheduleResult {
        // 이벤트 영향도에 따라 전략 선택
        let affected_count = self.identify_affected_operations(schedule, event).len();
        let total_ops = schedule.assignments.len();

        let strategy = if affected_count == 0 {
            return RescheduleResult {
                schedule: schedule.clone(),
                affected_operations: vec![],
                strategy_used: RescheduleStrategy::RightShift,
                stats: RescheduleStats::default(),
            };
        } else if affected_count <= 3 {
            RescheduleStrategy::RightShift
        } else if affected_count as f64 / total_ops as f64 <= 0.3 {
            RescheduleStrategy::AOR
        } else {
            RescheduleStrategy::TotalRegeneration
        };

        match strategy {
            RescheduleStrategy::RightShift => self.right_shift(schedule, event),
            RescheduleStrategy::AOR => self.aor(schedule, event),
            RescheduleStrategy::Partial => self.aor(schedule, event), // Partial은 AOR과 유사하게 처리
            RescheduleStrategy::Full | RescheduleStrategy::TotalRegeneration => {
                self.total_regeneration(schedule, event)
            }
        }
    }

    // 내부 헬퍼 함수들

    fn propagate_delay(
        &self,
        schedule: &mut Schedule,
        operation_id: &str,
        delay_ms: i64,
        affected: &mut HashSet<String>,
    ) {
        // 지연된 공정 찾기
        let mut op_end_time = 0i64;
        for assign in &mut schedule.assignments {
            if assign.operation_id == operation_id {
                if self.can_reschedule_assignment(assign) {
                    assign.start_ms += delay_ms;
                    assign.end_ms += delay_ms;
                    op_end_time = assign.end_ms;
                    affected.insert(operation_id.to_string());
                }
                break;
            }
        }

        // 후속 공정들도 시프트 (같은 Job의 다음 공정들)
        // 간단한 구현: 시작 시간이 겹치는 공정들 시프트
        for assign in &mut schedule.assignments {
            if assign.operation_id != operation_id
                && assign.start_ms < op_end_time
                && assign.start_ms >= op_end_time - delay_ms
                && self.can_reschedule_assignment(assign)
            {
                assign.start_ms += delay_ms;
                assign.end_ms += delay_ms;
                affected.insert(assign.operation_id.clone());
            }
        }
    }

    fn shift_for_breakdown(
        &self,
        schedule: &mut Schedule,
        resource_id: &str,
        start_ms: i64,
        duration_ms: i64,
        affected: &mut HashSet<String>,
    ) {
        let breakdown_end = start_ms + duration_ms;

        for assign in &mut schedule.assignments {
            if assign.resource_id == resource_id {
                // 고장 시간과 겹치는 공정
                if assign.start_ms < breakdown_end
                    && assign.end_ms > start_ms
                    && self.can_reschedule_assignment(assign)
                {
                    let shift = breakdown_end - assign.start_ms;
                    assign.start_ms += shift;
                    assign.end_ms += shift;
                    affected.insert(assign.operation_id.clone());
                }
            }
        }
    }

    fn identify_affected_operations(
        &self,
        schedule: &Schedule,
        event: &ScheduleEvent,
    ) -> Vec<String> {
        match event {
            ScheduleEvent::OperationDelay { operation_id, .. } => {
                // 지연된 공정과 관련 공정들
                vec![operation_id.clone()]
            }
            ScheduleEvent::MachineBreakdown {
                resource_id,
                start_ms,
                duration_ms,
            } => {
                let breakdown_end = start_ms + duration_ms;
                schedule
                    .assignments
                    .iter()
                    .filter(|a| {
                        a.resource_id == *resource_id
                            && a.start_ms < breakdown_end
                            && a.end_ms > *start_ms
                    })
                    .map(|a| a.operation_id.clone())
                    .collect()
            }
            ScheduleEvent::OrderCancellation { job_id } => schedule
                .assignments
                .iter()
                .filter(|a| a.job_id == *job_id)
                .map(|a| a.operation_id.clone())
                .collect(),
            ScheduleEvent::ProcessTimeChange { operation_id, .. } => {
                vec![operation_id.clone()]
            }
            ScheduleEvent::UrgentOrder { .. } => {
                // 긴급 주문은 전체에 영향
                schedule
                    .assignments
                    .iter()
                    .map(|a| a.operation_id.clone())
                    .collect()
            }
            ScheduleEvent::MaterialShortage {
                affected_operations,
                ..
            } => affected_operations.clone(),
            ScheduleEvent::WorkerUnavailable {
                worker_id,
                start_ms,
                duration_ms,
                ..
            } => {
                let unavail_end = start_ms + duration_ms;
                schedule
                    .assignments
                    .iter()
                    .filter(|a| {
                        a.resource_id == *worker_id
                            && a.start_ms < unavail_end
                            && a.end_ms > *start_ms
                    })
                    .map(|a| a.operation_id.clone())
                    .collect()
            }
            _ => vec![],
        }
    }

    fn can_reschedule_operation(&self, schedule: &Schedule, operation_id: &str) -> bool {
        if let Some(ref fence) = self.fence_config {
            if let Some(assign) = schedule
                .assignments
                .iter()
                .find(|a| a.operation_id == operation_id)
            {
                return fence.can_modify_operation(assign.start_ms);
            }
        }
        true
    }

    fn can_reschedule_assignment(&self, assign: &Assignment) -> bool {
        if let Some(ref fence) = self.fence_config {
            fence.can_modify_operation(assign.start_ms)
        } else {
            true
        }
    }

    fn reschedule_affected(&self, schedule: &mut Schedule, affected: &HashSet<String>) {
        // 영향받은 공정들의 리소스 충돌 해소 (greedy forward-shift)
        // 전략: 시작 시간 순으로 정렬 → 리소스별 가용 시간 추적 → 충돌 시 앞으로 이동

        // 리소스별 가용 시간 추적 (비영향 공정 기준)
        let mut resource_available: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();
        for assign in &schedule.assignments {
            if !affected.contains(&assign.operation_id) {
                let entry = resource_available
                    .entry(assign.resource_id.clone())
                    .or_insert(0);
                *entry = (*entry).max(assign.end_ms);
            }
        }

        // 영향받은 공정들을 시작 시간 순으로 정렬
        let mut affected_indices: Vec<usize> = schedule
            .assignments
            .iter()
            .enumerate()
            .filter(|(_, a)| affected.contains(&a.operation_id))
            .map(|(i, _)| i)
            .collect();
        affected_indices.sort_by_key(|&i| schedule.assignments[i].start_ms);

        // 각 영향 공정을 가장 빠른 가용 시점으로 이동
        for idx in affected_indices {
            let resource_id = schedule.assignments[idx].resource_id.clone();
            let duration = schedule.assignments[idx].end_ms - schedule.assignments[idx].start_ms;
            let current_start = schedule.assignments[idx].start_ms;

            let earliest = resource_available
                .get(&resource_id)
                .copied()
                .unwrap_or(0)
                .max(current_start);

            if earliest != current_start
                && self.can_reschedule_assignment(&schedule.assignments[idx])
            {
                schedule.assignments[idx].start_ms = earliest;
                schedule.assignments[idx].end_ms = earliest + duration;
            }

            // 가용 시간 업데이트
            let end = schedule.assignments[idx].end_ms;
            let entry = resource_available.entry(resource_id).or_insert(0);
            *entry = (*entry).max(end);
        }
    }

    fn calculate_stats(
        &self,
        old: &Schedule,
        new: &Schedule,
        affected_count: usize,
    ) -> RescheduleStats {
        let old_makespan = old.assignments.iter().map(|a| a.end_ms).max().unwrap_or(0);
        let new_makespan = new.assignments.iter().map(|a| a.end_ms).max().unwrap_or(0);

        // 공정별 지연 변화 합산
        let old_times: std::collections::HashMap<&str, i64> = old
            .assignments
            .iter()
            .map(|a| (a.operation_id.as_str(), a.end_ms))
            .collect();
        let total_delay_change_ms: i64 = new
            .assignments
            .iter()
            .filter_map(|a| {
                old_times
                    .get(a.operation_id.as_str())
                    .map(|&old_end| a.end_ms - old_end)
            })
            .sum();

        RescheduleStats {
            operations_changed: affected_count,
            total_delay_change_ms,
            makespan_change_ms: new_makespan - old_makespan,
        }
    }
}

impl Default for Rescheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_schedule() -> Schedule {
        Schedule {
            assignments: vec![
                Assignment {
                    operation_id: "op1".into(),
                    job_id: "job1".into(),
                    resource_id: "m1".into(),
                    start_ms: 0,
                    end_ms: 10_000,
                    setup_ms: 0,
                    site_id: None,
                },
                Assignment {
                    operation_id: "op2".into(),
                    job_id: "job1".into(),
                    resource_id: "m1".into(),
                    start_ms: 10_000,
                    end_ms: 20_000,
                    setup_ms: 0,
                    site_id: None,
                },
                Assignment {
                    operation_id: "op3".into(),
                    job_id: "job2".into(),
                    resource_id: "m2".into(),
                    start_ms: 5_000,
                    end_ms: 15_000,
                    setup_ms: 0,
                    site_id: None,
                },
            ],
            makespan_ms: 20_000,
            violations: vec![],
            tasks: vec![],
        }
    }

    #[test]
    fn test_right_shift() {
        let schedule = create_test_schedule();
        let rescheduler = Rescheduler::new();

        let event = ScheduleEvent::OperationDelay {
            operation_id: "op1".into(),
            delay_ms: 5_000,
        };

        let result = rescheduler.right_shift(&schedule, &event);

        assert_eq!(result.strategy_used, RescheduleStrategy::RightShift);
        assert!(result.affected_operations.contains(&"op1".to_string()));
    }

    #[test]
    fn test_machine_breakdown() {
        let schedule = create_test_schedule();
        let rescheduler = Rescheduler::new();

        let event = ScheduleEvent::MachineBreakdown {
            resource_id: "m1".into(),
            start_ms: 5_000,
            duration_ms: 10_000,
        };

        let result = rescheduler.right_shift(&schedule, &event);
        assert!(!result.affected_operations.is_empty());
    }

    #[test]
    fn test_auto_strategy_selection() {
        let schedule = create_test_schedule();
        let rescheduler = Rescheduler::new();

        // 작은 변경 → RightShift
        let event = ScheduleEvent::OperationDelay {
            operation_id: "op1".into(),
            delay_ms: 1_000,
        };

        let result = rescheduler.auto_reschedule(&schedule, &event);
        assert_eq!(result.strategy_used, RescheduleStrategy::RightShift);
    }

    #[test]
    fn test_with_time_fence() {
        let schedule = create_test_schedule();
        let fence = TimeFenceConfig::new(0, 0, 0); // 모든 것이 Liquid
        let rescheduler = Rescheduler::new().with_fence(fence);

        let event = ScheduleEvent::OperationDelay {
            operation_id: "op1".into(),
            delay_ms: 5_000,
        };

        let result = rescheduler.right_shift(&schedule, &event);
        assert!(!result.affected_operations.is_empty());
    }

    #[test]
    fn test_event_severity() {
        // Critical severity events
        let material_shortage = ScheduleEvent::MaterialShortage {
            material_id: "mat1".into(),
            shortage_qty: 100.0,
            affected_operations: vec!["op1".into(), "op2".into()],
        };
        assert!(material_shortage.severity() > 0.8);

        let machine_breakdown = ScheduleEvent::MachineBreakdown {
            resource_id: "m1".into(),
            start_ms: 0,
            duration_ms: 10_000,
        };
        assert!(machine_breakdown.severity() > 0.8);

        // High severity events
        let quality_defect = ScheduleEvent::QualityDefect {
            operation_id: "op1".into(),
            rework_qty: 10.0,
            additional_time_ms: 5_000,
        };
        assert!(quality_defect.severity() >= 0.7 && quality_defect.severity() <= 0.85);

        // Medium severity events
        let due_date_change = ScheduleEvent::DueDateChange {
            job_id: "job1".into(),
            new_due_date_ms: 100_000,
        };
        assert!(due_date_change.severity() >= 0.4 && due_date_change.severity() <= 0.6);

        // Low severity events
        let material_arrival = ScheduleEvent::MaterialArrival {
            material_id: "mat1".into(),
            quantity: 100.0,
            arrival_time_ms: 50_000,
        };
        assert!(material_arrival.severity() <= 0.3);
    }

    #[test]
    fn test_event_recommended_strategy() {
        // Critical events should recommend Full reschedule
        let material_shortage = ScheduleEvent::MaterialShortage {
            material_id: "mat1".into(),
            shortage_qty: 100.0,
            affected_operations: vec!["op1".into()],
        };
        assert_eq!(
            material_shortage.recommended_strategy(),
            RescheduleStrategy::Full
        );

        // Machine breakdown should recommend Partial reschedule
        let machine_breakdown = ScheduleEvent::MachineBreakdown {
            resource_id: "m1".into(),
            start_ms: 0,
            duration_ms: 10_000,
        };
        assert_eq!(
            machine_breakdown.recommended_strategy(),
            RescheduleStrategy::Partial
        );

        // Operation delay (severity 0.6) should recommend Partial
        let operation_delay = ScheduleEvent::OperationDelay {
            operation_id: "op1".into(),
            delay_ms: 1_000,
        };
        assert_eq!(
            operation_delay.recommended_strategy(),
            RescheduleStrategy::Partial
        );

        // Positive event should recommend RightShift
        let machine_recovered = ScheduleEvent::MachineRecovered {
            resource_id: "m1".into(),
            recovered_at_ms: 5_000,
        };
        assert_eq!(
            machine_recovered.recommended_strategy(),
            RescheduleStrategy::RightShift
        );
    }

    #[test]
    fn test_event_affected_resources() {
        let machine_breakdown = ScheduleEvent::MachineBreakdown {
            resource_id: "m1".into(),
            start_ms: 0,
            duration_ms: 10_000,
        };
        assert_eq!(machine_breakdown.affected_resources(), vec!["m1"]);

        let worker_unavailable = ScheduleEvent::WorkerUnavailable {
            worker_id: "w1".into(),
            start_ms: 0,
            duration_ms: 10_000,
            replacement_id: Some("w2".into()),
        };
        let resources = worker_unavailable.affected_resources();
        assert!(resources.contains(&"w1".to_string()));
        assert!(resources.contains(&"w2".to_string()));

        let maintenance = ScheduleEvent::MaintenanceScheduled {
            resource_id: "m2".into(),
            start_ms: 0,
            duration_ms: 5_000,
        };
        assert_eq!(maintenance.affected_resources(), vec!["m2"]);
    }

    #[test]
    fn test_event_category() {
        let op_delay = ScheduleEvent::OperationDelay {
            operation_id: "op1".into(),
            delay_ms: 1_000,
        };
        assert_eq!(op_delay.category(), EventCategory::Operation);

        let machine_breakdown = ScheduleEvent::MachineBreakdown {
            resource_id: "m1".into(),
            start_ms: 0,
            duration_ms: 10_000,
        };
        assert_eq!(machine_breakdown.category(), EventCategory::Equipment);

        let worker_unavailable = ScheduleEvent::WorkerUnavailable {
            worker_id: "w1".into(),
            start_ms: 0,
            duration_ms: 10_000,
            replacement_id: None,
        };
        assert_eq!(worker_unavailable.category(), EventCategory::Worker);

        let material_shortage = ScheduleEvent::MaterialShortage {
            material_id: "mat1".into(),
            shortage_qty: 100.0,
            affected_operations: vec![],
        };
        assert_eq!(material_shortage.category(), EventCategory::Material);

        let due_date_change = ScheduleEvent::DueDateChange {
            job_id: "job1".into(),
            new_due_date_ms: 100_000,
        };
        assert_eq!(due_date_change.category(), EventCategory::Order);
    }

    #[test]
    fn test_material_events() {
        let schedule = create_test_schedule();
        let rescheduler = Rescheduler::new();

        // Material delay
        let material_delay = ScheduleEvent::MaterialDelay {
            material_id: "mat1".into(),
            original_arrival_ms: 0,
            new_arrival_ms: 10_000,
        };
        let result = rescheduler.auto_reschedule(&schedule, &material_delay);
        assert!(!result.strategy_used.to_string().is_empty());

        // Material arrival (positive event)
        let material_arrival = ScheduleEvent::MaterialArrival {
            material_id: "mat1".into(),
            quantity: 100.0,
            arrival_time_ms: 0,
        };
        assert!(material_arrival.severity() < 0.3);
    }

    #[test]
    fn test_worker_events() {
        // Worker unavailable with replacement
        let worker_unavailable = ScheduleEvent::WorkerUnavailable {
            worker_id: "w1".into(),
            start_ms: 0,
            duration_ms: 20_000,
            replacement_id: Some("w2".into()),
        };
        assert!(worker_unavailable.severity() < 0.8); // Has replacement, lower severity

        // Worker available (positive event)
        let worker_available = ScheduleEvent::WorkerAvailable {
            worker_id: "w1".into(),
            available_from_ms: 10_000,
        };
        assert!(worker_available.severity() < 0.3);

        // Skill change
        let skill_acquired = ScheduleEvent::WorkerSkillChange {
            worker_id: "w1".into(),
            skill_id: "welding".into(),
            acquired: true,
        };
        assert!(skill_acquired.severity() < 0.3);
    }

    #[test]
    fn test_quality_events() {
        let quality_defect = ScheduleEvent::QualityDefect {
            operation_id: "op1".into(),
            rework_qty: 50.0,
            additional_time_ms: 30_000,
        };
        assert!(quality_defect.severity() >= 0.7);
        assert_eq!(quality_defect.category(), EventCategory::Operation);

        let inspection_delay = ScheduleEvent::InspectionDelay {
            operation_id: "op2".into(),
            delay_ms: 5_000,
        };
        // InspectionDelay severity is 0.35 (lower than operation delay)
        assert!(inspection_delay.severity() >= 0.3 && inspection_delay.severity() <= 0.5);
    }

    #[test]
    fn test_equipment_events() {
        // Scheduled maintenance (lower severity than breakdown)
        let maintenance = ScheduleEvent::MaintenanceScheduled {
            resource_id: "m1".into(),
            start_ms: 50_000,
            duration_ms: 10_000,
        };
        assert!(maintenance.severity() < 0.7);

        // Efficiency change
        let efficiency_drop = ScheduleEvent::EfficiencyChange {
            resource_id: "m1".into(),
            new_efficiency: 0.7,
        };
        assert!(efficiency_drop.severity() >= 0.4);

        // Machine recovered (positive event)
        let recovered = ScheduleEvent::MachineRecovered {
            resource_id: "m1".into(),
            recovered_at_ms: 15_000,
        };
        assert!(recovered.severity() < 0.3);
        assert_eq!(
            recovered.recommended_strategy(),
            RescheduleStrategy::RightShift
        );
    }

    #[test]
    fn test_order_events() {
        let due_date_change = ScheduleEvent::DueDateChange {
            job_id: "job1".into(),
            new_due_date_ms: 50_000,
        };
        assert_eq!(due_date_change.category(), EventCategory::Order);
        // DueDateChange severity is 0.45
        assert!(due_date_change.severity() >= 0.4 && due_date_change.severity() <= 0.5);

        let quantity_change = ScheduleEvent::QuantityChange {
            job_id: "job1".into(),
            new_quantity: 200,
        };
        // QuantityChange severity is 0.4
        assert!(quantity_change.severity() >= 0.3 && quantity_change.severity() <= 0.5);

        let priority_change = ScheduleEvent::PriorityChange {
            job_id: "job1".into(),
            new_priority: 1, // Urgent
        };
        // PriorityChange severity is 0.3
        assert!(priority_change.severity() >= 0.2 && priority_change.severity() <= 0.4);
    }
}
