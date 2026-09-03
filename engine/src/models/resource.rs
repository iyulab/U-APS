//! Resource - 자원 (장비, 작업자)

use serde::{Deserialize, Serialize};

/// 자원 유형
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    Equipment,
    Worker,
}

/// 시간 슬롯
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeSlot {
    /// 시작 시간 (epoch ms)
    pub start_ms: i64,
    /// 종료 시간 (epoch ms)
    pub end_ms: i64,
}

impl TimeSlot {
    pub fn new(start_ms: i64, end_ms: i64) -> Self {
        Self { start_ms, end_ms }
    }

    /// 슬롯 길이 (ms)
    pub fn duration_ms(&self) -> i64 {
        self.end_ms - self.start_ms
    }

    /// 시간이 슬롯 내에 있는지 확인
    pub fn contains(&self, time_ms: i64) -> bool {
        time_ms >= self.start_ms && time_ms < self.end_ms
    }

    /// 두 슬롯이 겹치는지 확인
    pub fn overlaps(&self, other: &TimeSlot) -> bool {
        self.start_ms < other.end_ms && other.start_ms < self.end_ms
    }
}

/// Resource - 자원 (장비/작업자)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Resource {
    /// 자원 ID
    pub id: String,
    /// 자원 유형
    pub kind: ResourceKind,
    /// 수행 가능 작업 (capability)
    pub capabilities: Vec<String>,
    /// 동시 처리 가능 수
    pub capacity: i32,
    /// 효율 계수 (1.0 = 표준)
    pub efficiency: f64,
    /// 가용 시간 슬롯
    pub availability: Vec<TimeSlot>,
    /// 불가 시간 (PM, 휴무 등)
    pub unavailable: Vec<TimeSlot>,
    /// 연결된 자원 ID (로봇-설비 연동 등)
    /// 이 자원이 사용될 때 연결된 자원도 함께 점유됨
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_resource_id: Option<String>,
    /// 로봇 여부
    #[serde(default)]
    pub is_robot: bool,
    /// 운영 캘린더 ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_id: Option<String>,
    /// 소속 사이트/공장 ID (multi-site 스케줄링용)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site_id: Option<String>,
}

impl Resource {
    pub fn new(id: impl Into<String>, kind: ResourceKind) -> Self {
        Self {
            id: id.into(),
            kind,
            capabilities: Vec::new(),
            capacity: 1,
            efficiency: 1.0,
            availability: Vec::new(),
            unavailable: Vec::new(),
            linked_resource_id: None,
            is_robot: false,
            calendar_id: None,
            site_id: None,
        }
    }

    /// 장비 생성 헬퍼
    pub fn equipment(id: impl Into<String>) -> Self {
        Self::new(id, ResourceKind::Equipment)
    }

    /// 작업자 생성 헬퍼
    pub fn worker(id: impl Into<String>) -> Self {
        Self::new(id, ResourceKind::Worker)
    }

    /// Builder: capability 추가
    pub fn with_capability(mut self, capability: impl Into<String>) -> Self {
        self.capabilities.push(capability.into());
        self
    }

    /// Builder: 효율 설정
    pub fn with_efficiency(mut self, efficiency: f64) -> Self {
        self.efficiency = efficiency;
        self
    }

    /// Builder: capacity 설정
    pub fn with_capacity(mut self, capacity: i32) -> Self {
        self.capacity = capacity;
        self
    }

    /// Builder: 불가 시간 추가 (PM 일정 등)
    pub fn with_unavailable(mut self, slot: TimeSlot) -> Self {
        self.unavailable.push(slot);
        self
    }

    /// Builder: 연결 자원 설정 (로봇-설비 연동)
    pub fn with_linked_resource(mut self, resource_id: impl Into<String>) -> Self {
        self.linked_resource_id = Some(resource_id.into());
        self
    }

    /// Builder: 로봇 설정
    pub fn as_robot(mut self) -> Self {
        self.is_robot = true;
        self
    }

    /// Builder: 캘린더 설정
    pub fn with_calendar(mut self, calendar_id: impl Into<String>) -> Self {
        self.calendar_id = Some(calendar_id.into());
        self
    }

    /// Builder: 사이트/공장 설정
    pub fn with_site(mut self, site_id: impl Into<String>) -> Self {
        self.site_id = Some(site_id.into());
        self
    }

    /// 특정 시간에 가용한지 확인
    pub fn is_available_at(&self, time_ms: i64) -> bool {
        // 불가 시간에 포함되면 false
        for slot in &self.unavailable {
            if slot.contains(time_ms) {
                return false;
            }
        }
        true
    }

    /// capability 보유 여부 확인
    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c == capability)
    }

    /// 실제 작업 시간 계산 (효율 적용)
    pub fn adjusted_time_ms(&self, base_time_ms: i64) -> i64 {
        (base_time_ms as f64 / self.efficiency) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_slot() {
        let slot = TimeSlot::new(1000, 2000);

        assert_eq!(slot.duration_ms(), 1000);
        assert!(slot.contains(1500));
        assert!(!slot.contains(2000));
        assert!(!slot.contains(999));
    }

    #[test]
    fn test_time_slot_overlap() {
        let slot1 = TimeSlot::new(1000, 2000);
        let slot2 = TimeSlot::new(1500, 2500);
        let slot3 = TimeSlot::new(2000, 3000);

        assert!(slot1.overlaps(&slot2));
        assert!(!slot1.overlaps(&slot3)); // 경계는 겹치지 않음
    }

    #[test]
    fn test_resource_equipment() {
        let equip = Resource::equipment("EQP-CNC-001")
            .with_capability("milling")
            .with_capability("drilling")
            .with_efficiency(0.95);

        assert_eq!(equip.kind, ResourceKind::Equipment);
        assert!(equip.has_capability("milling"));
        assert!(!equip.has_capability("welding"));
    }

    #[test]
    fn test_resource_worker() {
        let worker = Resource::worker("EMP-001")
            .with_efficiency(1.2) // 숙련공
            .with_capability("cnc_operation");

        assert_eq!(worker.kind, ResourceKind::Worker);
        assert_eq!(worker.efficiency, 1.2);
    }

    #[test]
    fn test_resource_availability() {
        let pm_slot = TimeSlot::new(
            8 * 3600 * 1000,  // 8시
            10 * 3600 * 1000, // 10시
        );

        let equip = Resource::equipment("EQP-001").with_unavailable(pm_slot);

        assert!(!equip.is_available_at(9 * 3600 * 1000)); // PM 시간
        assert!(equip.is_available_at(11 * 3600 * 1000)); // PM 후
    }

    #[test]
    fn test_adjusted_time() {
        let skilled = Resource::worker("SKILLED").with_efficiency(1.5); // 50% 빠름
        let novice = Resource::worker("NOVICE").with_efficiency(0.8); // 20% 느림

        let base_time = 60000; // 1분

        assert_eq!(skilled.adjusted_time_ms(base_time), 40000); // 40초
        assert_eq!(novice.adjusted_time_ms(base_time), 75000); // 75초
    }

    #[test]
    fn test_resource_serialization() {
        let resource = Resource::equipment("EQP-001")
            .with_capability("milling")
            .with_capacity(2);

        let json = serde_json::to_string(&resource).unwrap();
        let deserialized: Resource = serde_json::from_str(&json).unwrap();

        assert_eq!(resource, deserialized);
    }
}
