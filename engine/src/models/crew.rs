//! Crew/Team - 작업 팀 관리
//!
//! 작업자 그룹(팀) 관리 및 팀 단위 작업 할당

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// 팀 유형
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TeamType {
    /// 고정 팀 (항상 같이 작업)
    Fixed,
    /// 유동 팀 (필요시 구성)
    Dynamic,
    /// 교대 팀 (교대 근무)
    Shift,
    /// 프로젝트 팀 (특정 기간)
    Project,
}

/// 팀 역할
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TeamRole {
    /// 팀장
    Leader,
    /// 부팀장
    SubLeader,
    /// 일반 팀원
    Member,
    /// 견습생
    Trainee,
    /// 전문가 (특수 작업 담당)
    Specialist,
}

impl TeamRole {
    /// 역할에 따른 필수 여부
    pub fn is_required(&self) -> bool {
        matches!(self, TeamRole::Leader | TeamRole::Member)
    }

    /// 역할 우선순위 (높을수록 중요)
    pub fn priority(&self) -> u8 {
        match self {
            TeamRole::Leader => 5,
            TeamRole::SubLeader => 4,
            TeamRole::Specialist => 3,
            TeamRole::Member => 2,
            TeamRole::Trainee => 1,
        }
    }
}

/// 팀 멤버
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    /// 작업자 ID
    pub worker_id: String,
    /// 팀 내 역할
    pub role: TeamRole,
    /// 대리 가능 여부
    pub can_substitute: bool,
    /// 우선순위 (팀 내)
    pub priority: i32,
}

impl TeamMember {
    /// 새 팀 멤버 생성
    pub fn new(worker_id: impl Into<String>, role: TeamRole) -> Self {
        Self {
            worker_id: worker_id.into(),
            role,
            can_substitute: false,
            priority: 0,
        }
    }

    /// 대리 가능 설정
    pub fn with_substitute(mut self, can: bool) -> Self {
        self.can_substitute = can;
        self
    }

    /// 우선순위 설정
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

/// 작업 팀
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Crew {
    /// 팀 ID
    pub id: String,
    /// 팀 이름
    pub name: String,
    /// 팀 유형
    pub team_type: TeamType,
    /// 팀 멤버 목록
    members: Vec<TeamMember>,
    /// 팀 효율 보정 (팀워크 효과)
    pub efficiency_modifier: f64,
    /// 최소 필요 인원
    pub min_members: usize,
    /// 최대 인원
    pub max_members: usize,
    /// 담당 공정 유형
    pub operation_types: Vec<String>,
    /// 활성화 여부
    pub is_active: bool,
    /// 운영 캘린더 ID (교대 근무 시간 정의)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_id: Option<String>,
}

impl Crew {
    /// 새 팀 생성
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            team_type: TeamType::Fixed,
            members: Vec::new(),
            efficiency_modifier: 1.0,
            min_members: 1,
            max_members: 10,
            operation_types: Vec::new(),
            is_active: true,
            calendar_id: None,
        }
    }

    /// 팀 유형 설정
    pub fn with_type(mut self, team_type: TeamType) -> Self {
        self.team_type = team_type;
        self
    }

    /// 캘린더 설정 (교대 근무 시간)
    pub fn with_calendar(mut self, calendar_id: impl Into<String>) -> Self {
        self.calendar_id = Some(calendar_id.into());
        self
    }

    /// 효율 보정 설정
    pub fn with_efficiency(mut self, modifier: f64) -> Self {
        self.efficiency_modifier = modifier.clamp(0.1, 2.0);
        self
    }

    /// 최소/최대 인원 설정
    pub fn with_capacity(mut self, min: usize, max: usize) -> Self {
        self.min_members = min;
        self.max_members = max;
        self
    }

    /// 담당 공정 유형 추가
    pub fn with_operation_type(mut self, op_type: impl Into<String>) -> Self {
        self.operation_types.push(op_type.into());
        self
    }

    /// 멤버 추가
    pub fn add_member(&mut self, member: TeamMember) -> Result<(), String> {
        if self.members.len() >= self.max_members {
            return Err(format!("Team {} is at maximum capacity", self.id));
        }
        if self.members.iter().any(|m| m.worker_id == member.worker_id) {
            return Err(format!(
                "Worker {} is already in team {}",
                member.worker_id, self.id
            ));
        }
        self.members.push(member);
        Ok(())
    }

    /// Builder 패턴: 멤버 추가
    pub fn with_member(mut self, member: TeamMember) -> Self {
        let _ = self.add_member(member);
        self
    }

    /// 간편 멤버 추가 (작업자 ID와 역할만)
    pub fn with_worker(self, worker_id: impl Into<String>, role: TeamRole) -> Self {
        self.with_member(TeamMember::new(worker_id, role))
    }

    /// 멤버 제거
    pub fn remove_member(&mut self, worker_id: &str) -> Option<TeamMember> {
        if let Some(pos) = self.members.iter().position(|m| m.worker_id == worker_id) {
            Some(self.members.remove(pos))
        } else {
            None
        }
    }

    /// 현재 멤버 수
    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// 모든 멤버 조회
    pub fn members(&self) -> &[TeamMember] {
        &self.members
    }

    /// 특정 역할의 멤버 조회
    pub fn members_by_role(&self, role: TeamRole) -> Vec<&TeamMember> {
        self.members.iter().filter(|m| m.role == role).collect()
    }

    /// 팀장 조회
    pub fn leader(&self) -> Option<&TeamMember> {
        self.members.iter().find(|m| m.role == TeamRole::Leader)
    }

    /// 팀이 작업 가능한 상태인지 확인 (최소 인원 충족)
    pub fn is_operational(&self) -> bool {
        self.is_active && self.members.len() >= self.min_members && self.leader().is_some()
    }

    /// 특정 공정 유형을 담당하는지 확인
    pub fn handles_operation_type(&self, op_type: &str) -> bool {
        self.operation_types.is_empty() || self.operation_types.iter().any(|t| t == op_type)
    }

    /// 멤버 ID 목록 반환
    pub fn member_ids(&self) -> Vec<&str> {
        self.members.iter().map(|m| m.worker_id.as_str()).collect()
    }

    /// 처리 시간 계산 (팀 효율 적용)
    pub fn calculate_process_time(&self, base_time_ms: i64) -> i64 {
        (base_time_ms as f64 / self.efficiency_modifier) as i64
    }
}

/// 교대 근무 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShiftSchedule {
    /// 교대 ID
    pub id: String,
    /// 교대 이름 (예: "주간", "야간", "야간2")
    pub name: String,
    /// 시작 시간 (일 시작 기준, ms)
    pub start_offset_ms: i64,
    /// 종료 시간 (일 시작 기준, ms)
    pub end_offset_ms: i64,
    /// 적용 요일 (0=일, 1=월, ..., 6=토)
    pub working_days: Vec<u8>,
    /// 인수인계 시간 (ms) - 교대 시작/종료 시 필요한 전환 시간
    #[serde(default)]
    pub handover_time_ms: i64,
}

impl ShiftSchedule {
    /// 새 교대 일정 생성
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            start_offset_ms: 0,
            end_offset_ms: 8 * 3_600_000,      // 기본 8시간
            working_days: vec![1, 2, 3, 4, 5], // 월-금
            handover_time_ms: 0,
        }
    }

    /// 시간 설정 (시:분 형식)
    pub fn with_hours(mut self, start_hour: u8, end_hour: u8) -> Self {
        self.start_offset_ms = start_hour as i64 * 3_600_000;
        self.end_offset_ms = end_hour as i64 * 3_600_000;
        self
    }

    /// 근무일 설정
    pub fn with_days(mut self, days: Vec<u8>) -> Self {
        self.working_days = days;
        self
    }

    /// 인수인계 시간 설정 (분 단위)
    pub fn with_handover_minutes(mut self, minutes: i64) -> Self {
        self.handover_time_ms = minutes * 60_000;
        self
    }

    /// 인수인계 시간 설정 (ms 단위)
    pub fn with_handover_ms(mut self, ms: i64) -> Self {
        self.handover_time_ms = ms;
        self
    }

    /// 교대 시간 길이 (ms)
    pub fn duration_ms(&self) -> i64 {
        if self.end_offset_ms > self.start_offset_ms {
            self.end_offset_ms - self.start_offset_ms
        } else {
            // 야간 교대 (자정을 넘어감)
            (24 * 3_600_000 - self.start_offset_ms) + self.end_offset_ms
        }
    }

    /// 실제 작업 가능 시간 (인수인계 시간 제외)
    pub fn effective_duration_ms(&self) -> i64 {
        (self.duration_ms() - self.handover_time_ms).max(0)
    }

    /// 실제 작업 시작 시간 (인수인계 후)
    pub fn effective_start_offset_ms(&self) -> i64 {
        self.start_offset_ms + self.handover_time_ms / 2
    }

    /// 실제 작업 종료 시간 (인수인계 전)
    pub fn effective_end_offset_ms(&self) -> i64 {
        if self.end_offset_ms > self.start_offset_ms {
            self.end_offset_ms - self.handover_time_ms / 2
        } else {
            // 야간 교대
            let adjusted = self.end_offset_ms - self.handover_time_ms / 2;
            if adjusted < 0 {
                24 * 3_600_000 + adjusted
            } else {
                adjusted
            }
        }
    }
}

/// 팀-교대 할당
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrewShiftAssignment {
    /// 팀 ID
    pub crew_id: String,
    /// 교대 ID
    pub shift_id: String,
    /// 유효 시작일 (timestamp ms)
    pub effective_from_ms: i64,
    /// 유효 종료일 (timestamp ms, None이면 무기한)
    pub effective_until_ms: Option<i64>,
}

/// 팀 관리자
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CrewManager {
    /// 팀 목록
    crews: HashMap<String, Crew>,
    /// 교대 일정 목록
    shifts: HashMap<String, ShiftSchedule>,
    /// 팀-교대 할당
    shift_assignments: Vec<CrewShiftAssignment>,
    /// 작업자-팀 매핑 (빠른 조회용)
    worker_to_crew: HashMap<String, String>,
}

impl CrewManager {
    /// 새 팀 관리자 생성
    pub fn new() -> Self {
        Self {
            crews: HashMap::new(),
            shifts: HashMap::new(),
            shift_assignments: Vec::new(),
            worker_to_crew: HashMap::new(),
        }
    }

    /// 팀 추가
    pub fn add_crew(&mut self, crew: Crew) {
        // 작업자-팀 매핑 업데이트
        for member in &crew.members {
            self.worker_to_crew
                .insert(member.worker_id.clone(), crew.id.clone());
        }
        self.crews.insert(crew.id.clone(), crew);
    }

    /// Builder 패턴: 팀 추가
    pub fn with_crew(mut self, crew: Crew) -> Self {
        self.add_crew(crew);
        self
    }

    /// 교대 일정 추가
    pub fn add_shift(&mut self, shift: ShiftSchedule) {
        self.shifts.insert(shift.id.clone(), shift);
    }

    /// Builder 패턴: 교대 일정 추가
    pub fn with_shift(mut self, shift: ShiftSchedule) -> Self {
        self.add_shift(shift);
        self
    }

    /// 팀-교대 할당
    pub fn assign_crew_to_shift(
        &mut self,
        crew_id: impl Into<String>,
        shift_id: impl Into<String>,
        effective_from_ms: i64,
    ) {
        self.shift_assignments.push(CrewShiftAssignment {
            crew_id: crew_id.into(),
            shift_id: shift_id.into(),
            effective_from_ms,
            effective_until_ms: None,
        });
    }

    /// 팀 조회
    pub fn get_crew(&self, crew_id: &str) -> Option<&Crew> {
        self.crews.get(crew_id)
    }

    /// 작업자가 속한 팀 조회
    pub fn get_worker_crew(&self, worker_id: &str) -> Option<&Crew> {
        self.worker_to_crew
            .get(worker_id)
            .and_then(|crew_id| self.crews.get(crew_id))
    }

    /// 모든 팀 조회
    pub fn all_crews(&self) -> Vec<&Crew> {
        self.crews.values().collect()
    }

    /// 운영 가능한 팀 조회
    pub fn operational_crews(&self) -> Vec<&Crew> {
        self.crews.values().filter(|c| c.is_operational()).collect()
    }

    /// 특정 공정 유형을 담당하는 팀 조회
    pub fn crews_for_operation(&self, op_type: &str) -> Vec<&Crew> {
        self.crews
            .values()
            .filter(|c| c.is_operational() && c.handles_operation_type(op_type))
            .collect()
    }

    /// 특정 시간에 근무하는 팀 조회
    pub fn crews_on_shift(&self, time_ms: i64) -> Vec<&Crew> {
        // 타임스탬프에서 요일 계산 (Unix epoch: 1970-01-01은 목요일=4)
        // 0=일, 1=월, 2=화, 3=수, 4=목, 5=금, 6=토
        let ms_per_day = 24 * 3_600_000i64;
        let days_since_epoch = time_ms / ms_per_day;
        let day_of_week = ((days_since_epoch + 4) % 7) as u8; // 목요일 기준 보정

        // 일 시작 기준 오프셋 (ms)
        let time_in_day_ms = time_ms % ms_per_day;

        // 교대 할당이 있는 팀 찾기
        let mut crews_in_shift: Vec<&Crew> = Vec::new();

        for assignment in &self.shift_assignments {
            // 유효 기간 확인
            if time_ms < assignment.effective_from_ms {
                continue;
            }
            if let Some(until) = assignment.effective_until_ms {
                if time_ms > until {
                    continue;
                }
            }

            // 교대 일정 확인
            if let Some(shift) = self.shifts.get(&assignment.shift_id) {
                // 근무일 확인
                if !shift.working_days.contains(&day_of_week) {
                    continue;
                }

                // 시간대 확인
                let in_time_range = if shift.end_offset_ms > shift.start_offset_ms {
                    // 일반 교대 (같은 날 종료)
                    time_in_day_ms >= shift.start_offset_ms && time_in_day_ms < shift.end_offset_ms
                } else {
                    // 야간 교대 (자정을 넘어감)
                    time_in_day_ms >= shift.start_offset_ms || time_in_day_ms < shift.end_offset_ms
                };

                if in_time_range {
                    if let Some(crew) = self.crews.get(&assignment.crew_id) {
                        if crew.is_operational() {
                            crews_in_shift.push(crew);
                        }
                    }
                }
            }
        }

        // 교대 할당이 없는 팀은 항상 근무 가능 (TeamType::Fixed)
        for crew in self.crews.values() {
            if crew.is_operational()
                && crew.team_type == TeamType::Fixed
                && !crews_in_shift.iter().any(|c| c.id == crew.id)
            {
                // 이미 교대 할당으로 추가되지 않은 Fixed 팀만 추가
                let has_shift_assignment =
                    self.shift_assignments.iter().any(|a| a.crew_id == crew.id);
                if !has_shift_assignment {
                    crews_in_shift.push(crew);
                }
            }
        }

        crews_in_shift
    }

    /// 팀 가용성 확인 (모든 멤버 가용)
    pub fn is_crew_available(&self, crew_id: &str, available_workers: &HashSet<String>) -> bool {
        if let Some(crew) = self.get_crew(crew_id) {
            // 최소 인원이 가용해야 함
            let available_count = crew
                .members
                .iter()
                .filter(|m| available_workers.contains(&m.worker_id))
                .count();

            // 팀장이 가용해야 함
            let leader_available = crew
                .leader()
                .is_some_and(|l| available_workers.contains(&l.worker_id));

            available_count >= crew.min_members && leader_available
        } else {
            false
        }
    }

    /// 최적 팀 선택 (공정 유형, 가용 작업자 기준)
    pub fn best_crew_for_operation(
        &self,
        op_type: &str,
        available_workers: &HashSet<String>,
    ) -> Option<&Crew> {
        self.crews_for_operation(op_type)
            .into_iter()
            .filter(|c| self.is_crew_available(&c.id, available_workers))
            .max_by(|a, b| {
                // 효율 높은 팀 우선
                a.efficiency_modifier
                    .partial_cmp(&b.efficiency_modifier)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// 팀 효율 조회
    pub fn get_crew_efficiency(&self, crew_id: &str) -> f64 {
        self.get_crew(crew_id)
            .map_or(1.0, |c| c.efficiency_modifier)
    }

    /// 특정 작업자의 팀 역할 조회
    pub fn get_worker_role(&self, worker_id: &str) -> Option<TeamRole> {
        self.get_worker_crew(worker_id)
            .and_then(|crew| crew.members.iter().find(|m| m.worker_id == worker_id))
            .map(|m| m.role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crew_creation() {
        let crew = Crew::new("TEAM-001", "Assembly Team A")
            .with_type(TeamType::Fixed)
            .with_efficiency(1.2)
            .with_capacity(3, 8)
            .with_operation_type("assembly")
            .with_worker("W1", TeamRole::Leader)
            .with_worker("W2", TeamRole::Member)
            .with_worker("W3", TeamRole::Member);

        assert_eq!(crew.id, "TEAM-001");
        assert_eq!(crew.member_count(), 3);
        assert!(crew.is_operational());
        assert!(crew.handles_operation_type("assembly"));
        assert!(!crew.handles_operation_type("welding"));
    }

    #[test]
    fn test_crew_not_operational() {
        // 팀장 없음
        let crew_no_leader = Crew::new("TEAM-001", "No Leader")
            .with_capacity(2, 5)
            .with_worker("W1", TeamRole::Member)
            .with_worker("W2", TeamRole::Member);
        assert!(!crew_no_leader.is_operational());

        // 최소 인원 미달
        let crew_understaffed = Crew::new("TEAM-002", "Understaffed")
            .with_capacity(3, 5)
            .with_worker("W1", TeamRole::Leader)
            .with_worker("W2", TeamRole::Member);
        assert!(!crew_understaffed.is_operational());
    }

    #[test]
    fn test_crew_members_by_role() {
        let crew = Crew::new("TEAM-001", "Mixed Team")
            .with_worker("W1", TeamRole::Leader)
            .with_worker("W2", TeamRole::SubLeader)
            .with_worker("W3", TeamRole::Member)
            .with_worker("W4", TeamRole::Member)
            .with_worker("W5", TeamRole::Trainee);

        assert_eq!(crew.members_by_role(TeamRole::Leader).len(), 1);
        assert_eq!(crew.members_by_role(TeamRole::Member).len(), 2);
        assert_eq!(crew.members_by_role(TeamRole::Trainee).len(), 1);
    }

    #[test]
    fn test_crew_process_time() {
        let crew = Crew::new("TEAM-001", "Fast Team")
            .with_efficiency(1.5) // 50% 빠름
            .with_worker("W1", TeamRole::Leader)
            .with_worker("W2", TeamRole::Member);

        let base_time = 60_000; // 60초
        let team_time = crew.calculate_process_time(base_time);
        assert_eq!(team_time, 40_000); // 40초
    }

    #[test]
    fn test_shift_schedule() {
        // 주간 교대 (08:00 - 16:00)
        let day_shift = ShiftSchedule::new("DAY", "Day Shift")
            .with_hours(8, 16)
            .with_days(vec![1, 2, 3, 4, 5]);

        assert_eq!(day_shift.duration_ms(), 8 * 3_600_000);

        // 야간 교대 (22:00 - 06:00)
        let night_shift = ShiftSchedule::new("NIGHT", "Night Shift")
            .with_hours(22, 6)
            .with_days(vec![1, 2, 3, 4, 5]);

        assert_eq!(night_shift.duration_ms(), 8 * 3_600_000);
    }

    #[test]
    fn test_crew_manager_basic() {
        let mut manager = CrewManager::new();

        let team_a = Crew::new("TEAM-A", "Team A")
            .with_efficiency(1.2)
            .with_operation_type("assembly")
            .with_worker("W1", TeamRole::Leader)
            .with_worker("W2", TeamRole::Member)
            .with_worker("W3", TeamRole::Member);

        let team_b = Crew::new("TEAM-B", "Team B")
            .with_efficiency(1.0)
            .with_operation_type("welding")
            .with_worker("W4", TeamRole::Leader)
            .with_worker("W5", TeamRole::Member);

        manager.add_crew(team_a);
        manager.add_crew(team_b);

        assert_eq!(manager.all_crews().len(), 2);
        assert_eq!(manager.crews_for_operation("assembly").len(), 1);
        assert_eq!(manager.crews_for_operation("welding").len(), 1);
    }

    #[test]
    fn test_crew_manager_worker_lookup() {
        let manager = CrewManager::new().with_crew(
            Crew::new("TEAM-001", "Team 1")
                .with_worker("W1", TeamRole::Leader)
                .with_worker("W2", TeamRole::Member),
        );

        let crew = manager.get_worker_crew("W1");
        assert!(crew.is_some());
        assert_eq!(crew.unwrap().id, "TEAM-001");

        let role = manager.get_worker_role("W1");
        assert_eq!(role, Some(TeamRole::Leader));
    }

    #[test]
    fn test_crew_availability() {
        let manager = CrewManager::new().with_crew(
            Crew::new("TEAM-001", "Team 1")
                .with_capacity(2, 5)
                .with_worker("W1", TeamRole::Leader)
                .with_worker("W2", TeamRole::Member)
                .with_worker("W3", TeamRole::Member),
        );

        // 모든 멤버 가용
        let all_available: HashSet<String> =
            ["W1", "W2", "W3"].iter().map(|s| s.to_string()).collect();
        assert!(manager.is_crew_available("TEAM-001", &all_available));

        // 팀장 없음 → 불가
        let no_leader: HashSet<String> = ["W2", "W3"].iter().map(|s| s.to_string()).collect();
        assert!(!manager.is_crew_available("TEAM-001", &no_leader));

        // 최소 인원 미달 → 불가
        let too_few: HashSet<String> = ["W1"].iter().map(|s| s.to_string()).collect();
        assert!(!manager.is_crew_available("TEAM-001", &too_few));
    }

    #[test]
    fn test_best_crew_selection() {
        let manager = CrewManager::new()
            .with_crew(
                Crew::new("TEAM-A", "Team A")
                    .with_efficiency(1.0)
                    .with_capacity(2, 5)
                    .with_operation_type("assembly")
                    .with_worker("W1", TeamRole::Leader)
                    .with_worker("W2", TeamRole::Member),
            )
            .with_crew(
                Crew::new("TEAM-B", "Team B")
                    .with_efficiency(1.5) // 더 효율적
                    .with_capacity(2, 5)
                    .with_operation_type("assembly")
                    .with_worker("W3", TeamRole::Leader)
                    .with_worker("W4", TeamRole::Member),
            );

        let available: HashSet<String> = ["W1", "W2", "W3", "W4"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let best = manager.best_crew_for_operation("assembly", &available);
        assert!(best.is_some());
        assert_eq!(best.unwrap().id, "TEAM-B"); // 효율 높은 팀
    }

    #[test]
    fn test_team_role_properties() {
        assert!(TeamRole::Leader.is_required());
        assert!(TeamRole::Member.is_required());
        assert!(!TeamRole::Trainee.is_required());

        assert!(TeamRole::Leader.priority() > TeamRole::Member.priority());
        assert!(TeamRole::Specialist.priority() > TeamRole::Trainee.priority());
    }

    #[test]
    fn test_crew_all_operation_types() {
        // 공정 유형이 비어있으면 모든 공정 담당 가능
        let universal_crew = Crew::new("TEAM-001", "Universal Team")
            .with_worker("W1", TeamRole::Leader)
            .with_worker("W2", TeamRole::Member);

        assert!(universal_crew.handles_operation_type("assembly"));
        assert!(universal_crew.handles_operation_type("welding"));
        assert!(universal_crew.handles_operation_type("any_type"));
    }
}
