//! Skill Matrix and Worker Time Override
//!
//! 작업자별 공정 유형에 대한 숙련도 및 시간 오버라이드 관리
//!
//! 시간 결정 우선순위:
//! 1. WorkerTimeOverride (작업자별 명시적 시간)
//! 2. SkillMatrix 효율 조정
//! 3. 기본 공정 시간

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 스킬 레벨
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum SkillLevel {
    /// 수행 불가
    None,
    /// 초급 (효율 50%)
    Beginner,
    /// 중급 (효율 75%)
    Intermediate,
    /// 고급 (효율 100%)
    Advanced,
    /// 전문가 (효율 120%)
    Expert,
}

impl SkillLevel {
    /// 효율성 값 반환 (0.0 ~ 1.2)
    pub fn efficiency(&self) -> f64 {
        match self {
            SkillLevel::None => 0.0,
            SkillLevel::Beginner => 0.5,
            SkillLevel::Intermediate => 0.75,
            SkillLevel::Advanced => 1.0,
            SkillLevel::Expert => 1.2,
        }
    }

    /// 숙련도 값에서 스킬 레벨 생성
    pub fn from_proficiency(proficiency: f64) -> Self {
        if proficiency <= 0.0 {
            SkillLevel::None
        } else if proficiency < 0.6 {
            SkillLevel::Beginner
        } else if proficiency < 0.9 {
            SkillLevel::Intermediate
        } else if proficiency < 1.1 {
            SkillLevel::Advanced
        } else {
            SkillLevel::Expert
        }
    }
}

/// 스킬 매트릭스
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillMatrix {
    /// (작업자 ID, 공정 유형) → 숙련도
    proficiencies: HashMap<(String, String), f64>,
    /// 기본 숙련도 (매핑 없을 때)
    default_proficiency: f64,
}

impl SkillMatrix {
    /// 새 스킬 매트릭스 생성
    pub fn new() -> Self {
        Self {
            proficiencies: HashMap::new(),
            default_proficiency: 1.0,
        }
    }

    /// 기본 숙련도 설정
    pub fn with_default(mut self, proficiency: f64) -> Self {
        self.default_proficiency = proficiency.clamp(0.0, 2.0);
        self
    }

    /// 숙련도 추가
    pub fn add_skill(
        &mut self,
        worker_id: impl Into<String>,
        operation_type: impl Into<String>,
        proficiency: f64,
    ) {
        let key = (worker_id.into(), operation_type.into());
        self.proficiencies.insert(key, proficiency.clamp(0.0, 2.0));
    }

    /// Builder 패턴: 숙련도 추가
    pub fn with_skill(
        mut self,
        worker_id: impl Into<String>,
        operation_type: impl Into<String>,
        proficiency: f64,
    ) -> Self {
        self.add_skill(worker_id, operation_type, proficiency);
        self
    }

    /// 숙련도 조회
    pub fn get_proficiency(&self, worker_id: &str, operation_type: &str) -> f64 {
        let key = (worker_id.to_string(), operation_type.to_string());
        *self
            .proficiencies
            .get(&key)
            .unwrap_or(&self.default_proficiency)
    }

    /// 스킬 레벨 조회
    pub fn get_skill_level(&self, worker_id: &str, operation_type: &str) -> SkillLevel {
        let proficiency = self.get_proficiency(worker_id, operation_type);
        SkillLevel::from_proficiency(proficiency)
    }

    /// 작업자가 특정 공정을 수행할 수 있는지 확인
    pub fn can_perform(&self, worker_id: &str, operation_type: &str) -> bool {
        self.get_proficiency(worker_id, operation_type) > 0.0
    }

    /// 특정 공정을 수행할 수 있는 작업자 목록 반환
    pub fn qualified_workers(&self, operation_type: &str) -> Vec<(String, f64)> {
        self.proficiencies
            .iter()
            .filter(|((_, op_type), prof)| op_type == operation_type && **prof > 0.0)
            .map(|((worker_id, _), prof)| (worker_id.clone(), *prof))
            .collect()
    }

    /// 특정 작업자가 수행할 수 있는 공정 유형 목록 반환
    pub fn worker_skills(&self, worker_id: &str) -> Vec<(String, f64)> {
        self.proficiencies
            .iter()
            .filter(|((wid, _), prof)| wid == worker_id && **prof > 0.0)
            .map(|((_, op_type), prof)| (op_type.clone(), *prof))
            .collect()
    }

    /// 처리 시간 계산 (숙련도 적용)
    pub fn calculate_process_time(
        &self,
        worker_id: &str,
        operation_type: &str,
        base_time_ms: i64,
    ) -> Option<i64> {
        let proficiency = self.get_proficiency(worker_id, operation_type);
        if proficiency <= 0.0 {
            return None; // 수행 불가
        }
        Some((base_time_ms as f64 / proficiency) as i64)
    }

    /// 최적 작업자 선택 (가장 높은 숙련도)
    pub fn best_worker_for(&self, operation_type: &str) -> Option<(String, f64)> {
        self.qualified_workers(operation_type)
            .into_iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    }
}

/// 작업자별 시간 오버라이드 항목
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeOverrideEntry {
    /// Setup 시간 오버라이드 (ms, None이면 기본값 사용)
    pub setup_ms: Option<i64>,
    /// Process 시간 오버라이드 (ms, None이면 기본값 사용)
    pub process_ms: Option<i64>,
    /// Wait 시간 오버라이드 (ms, None이면 기본값 사용)
    pub wait_ms: Option<i64>,
}

impl TimeOverrideEntry {
    /// 새 시간 오버라이드 항목 생성
    pub fn new() -> Self {
        Self {
            setup_ms: None,
            process_ms: None,
            wait_ms: None,
        }
    }

    /// Setup 시간 설정
    pub fn with_setup(mut self, setup_ms: i64) -> Self {
        self.setup_ms = Some(setup_ms);
        self
    }

    /// Process 시간 설정
    pub fn with_process(mut self, process_ms: i64) -> Self {
        self.process_ms = Some(process_ms);
        self
    }

    /// Wait 시간 설정
    pub fn with_wait(mut self, wait_ms: i64) -> Self {
        self.wait_ms = Some(wait_ms);
        self
    }

    /// 전체 시간 설정 (setup, process, wait)
    pub fn full(setup_ms: i64, process_ms: i64, wait_ms: i64) -> Self {
        Self {
            setup_ms: Some(setup_ms),
            process_ms: Some(process_ms),
            wait_ms: Some(wait_ms),
        }
    }

    /// Process 시간만 설정 (가장 일반적인 케이스)
    pub fn process_only(process_ms: i64) -> Self {
        Self {
            setup_ms: None,
            process_ms: Some(process_ms),
            wait_ms: None,
        }
    }
}

impl Default for TimeOverrideEntry {
    fn default() -> Self {
        Self::new()
    }
}

/// 작업자별 시간 오버라이드 매트릭스
///
/// 특정 작업자가 특정 공정 유형을 수행할 때 적용되는 시간 오버라이드.
/// SkillMatrix의 효율 조정보다 높은 우선순위로 적용됨.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkerTimeOverride {
    /// (작업자 ID, 공정 유형) → 시간 오버라이드
    overrides: HashMap<(String, String), TimeOverrideEntry>,
}

impl WorkerTimeOverride {
    /// 새 오버라이드 매트릭스 생성
    pub fn new() -> Self {
        Self {
            overrides: HashMap::new(),
        }
    }

    /// 오버라이드 추가
    pub fn add_override(
        &mut self,
        worker_id: impl Into<String>,
        operation_type: impl Into<String>,
        entry: TimeOverrideEntry,
    ) {
        let key = (worker_id.into(), operation_type.into());
        self.overrides.insert(key, entry);
    }

    /// Builder 패턴: 오버라이드 추가
    pub fn with_override(
        mut self,
        worker_id: impl Into<String>,
        operation_type: impl Into<String>,
        entry: TimeOverrideEntry,
    ) -> Self {
        self.add_override(worker_id, operation_type, entry);
        self
    }

    /// 간편 추가: 전체 시간 오버라이드
    pub fn with_full_time(
        self,
        worker_id: impl Into<String>,
        operation_type: impl Into<String>,
        setup_ms: i64,
        process_ms: i64,
        wait_ms: i64,
    ) -> Self {
        self.with_override(
            worker_id,
            operation_type,
            TimeOverrideEntry::full(setup_ms, process_ms, wait_ms),
        )
    }

    /// 간편 추가: Process 시간만 오버라이드
    pub fn with_process_time(
        self,
        worker_id: impl Into<String>,
        operation_type: impl Into<String>,
        process_ms: i64,
    ) -> Self {
        self.with_override(
            worker_id,
            operation_type,
            TimeOverrideEntry::process_only(process_ms),
        )
    }

    /// 오버라이드 조회
    pub fn get_override(
        &self,
        worker_id: &str,
        operation_type: &str,
    ) -> Option<&TimeOverrideEntry> {
        let key = (worker_id.to_string(), operation_type.to_string());
        self.overrides.get(&key)
    }

    /// 오버라이드가 있는지 확인
    pub fn has_override(&self, worker_id: &str, operation_type: &str) -> bool {
        self.get_override(worker_id, operation_type).is_some()
    }

    /// Setup 시간 조회 (오버라이드 있으면 반환, 없으면 None)
    pub fn get_setup_time(&self, worker_id: &str, operation_type: &str) -> Option<i64> {
        self.get_override(worker_id, operation_type)
            .and_then(|e| e.setup_ms)
    }

    /// Process 시간 조회 (오버라이드 있으면 반환, 없으면 None)
    pub fn get_process_time(&self, worker_id: &str, operation_type: &str) -> Option<i64> {
        self.get_override(worker_id, operation_type)
            .and_then(|e| e.process_ms)
    }

    /// Wait 시간 조회 (오버라이드 있으면 반환, 없으면 None)
    pub fn get_wait_time(&self, worker_id: &str, operation_type: &str) -> Option<i64> {
        self.get_override(worker_id, operation_type)
            .and_then(|e| e.wait_ms)
    }

    /// 시간 계산 (오버라이드 > 기본값)
    /// 각 시간 요소에 대해 오버라이드가 있으면 그 값을, 없으면 기본값 사용
    pub fn calculate_times(
        &self,
        worker_id: &str,
        operation_type: &str,
        default_setup: i64,
        default_process: i64,
        default_wait: i64,
    ) -> (i64, i64, i64) {
        if let Some(entry) = self.get_override(worker_id, operation_type) {
            (
                entry.setup_ms.unwrap_or(default_setup),
                entry.process_ms.unwrap_or(default_process),
                entry.wait_ms.unwrap_or(default_wait),
            )
        } else {
            (default_setup, default_process, default_wait)
        }
    }

    /// 특정 작업자의 모든 오버라이드 조회
    pub fn worker_overrides(&self, worker_id: &str) -> Vec<(&str, &TimeOverrideEntry)> {
        self.overrides
            .iter()
            .filter(|((wid, _), _)| wid == worker_id)
            .map(|((_, op_type), entry)| (op_type.as_str(), entry))
            .collect()
    }

    /// 특정 공정 유형에 대한 모든 작업자 오버라이드 조회
    pub fn operation_overrides(&self, operation_type: &str) -> Vec<(&str, &TimeOverrideEntry)> {
        self.overrides
            .iter()
            .filter(|((_, op), _)| op == operation_type)
            .map(|((worker_id, _), entry)| (worker_id.as_str(), entry))
            .collect()
    }

    /// 오버라이드 수
    pub fn len(&self) -> usize {
        self.overrides.len()
    }

    /// 비어있는지 확인
    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty()
    }
}

/// 학습 곡선 효과 (경험에 따른 숙련도 증가)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningCurve {
    /// 초기 숙련도
    pub initial_proficiency: f64,
    /// 최대 숙련도
    pub max_proficiency: f64,
    /// 학습률 (0.0 ~ 1.0)
    pub learning_rate: f64,
    /// 현재 경험 (작업 횟수)
    pub experience: u32,
}

/// 자격 인증 레벨
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum CertificationLevel {
    /// 미인증
    None,
    /// 기본 인증 (Basic)
    Basic,
    /// 표준 인증 (Standard)
    Standard,
    /// 고급 인증 (Advanced)
    Advanced,
    /// 마스터 인증 (Master)
    Master,
}

impl CertificationLevel {
    /// 인증 레벨에 따른 효율 보정
    pub fn efficiency_modifier(&self) -> f64 {
        match self {
            CertificationLevel::None => 0.0,
            CertificationLevel::Basic => 0.7,
            CertificationLevel::Standard => 1.0,
            CertificationLevel::Advanced => 1.15,
            CertificationLevel::Master => 1.3,
        }
    }
}

/// 자격 인증 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Certification {
    /// 인증 ID
    pub id: String,
    /// 작업자 ID
    pub worker_id: String,
    /// 대상 (설비 ID 또는 공정 유형)
    pub target_id: String,
    /// 대상 유형 (Machine 또는 OperationType)
    pub target_type: CertificationTarget,
    /// 인증 레벨
    pub level: CertificationLevel,
    /// 발급일 (timestamp ms)
    pub issued_at_ms: i64,
    /// 만료일 (timestamp ms, None이면 무기한)
    pub expires_at_ms: Option<i64>,
    /// 인증 기관
    pub issuer: Option<String>,
    /// 비고
    pub notes: Option<String>,
}

/// 인증 대상 유형
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CertificationTarget {
    /// 설비 자격
    Machine,
    /// 공정 유형 자격
    OperationType,
    /// 안전 자격 (특수 작업)
    Safety,
}

impl Certification {
    /// 새 인증 생성
    pub fn new(
        id: impl Into<String>,
        worker_id: impl Into<String>,
        target_id: impl Into<String>,
        target_type: CertificationTarget,
        level: CertificationLevel,
        issued_at_ms: i64,
    ) -> Self {
        Self {
            id: id.into(),
            worker_id: worker_id.into(),
            target_id: target_id.into(),
            target_type,
            level,
            issued_at_ms,
            expires_at_ms: None,
            issuer: None,
            notes: None,
        }
    }

    /// 만료일 설정
    pub fn with_expiry(mut self, expires_at_ms: i64) -> Self {
        self.expires_at_ms = Some(expires_at_ms);
        self
    }

    /// 인증 기관 설정
    pub fn with_issuer(mut self, issuer: impl Into<String>) -> Self {
        self.issuer = Some(issuer.into());
        self
    }

    /// 비고 설정
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }

    /// 인증이 유효한지 확인 (현재 시점 기준)
    pub fn is_valid(&self, current_time_ms: i64) -> bool {
        self.level != CertificationLevel::None
            && self.expires_at_ms.is_none_or(|exp| exp > current_time_ms)
    }

    /// 인증 만료까지 남은 시간 (ms)
    pub fn time_until_expiry(&self, current_time_ms: i64) -> Option<i64> {
        self.expires_at_ms.map(|exp| exp - current_time_ms)
    }

    /// 인증이 곧 만료될 예정인지 (경고 임계값 내)
    pub fn is_expiring_soon(&self, current_time_ms: i64, warning_threshold_ms: i64) -> bool {
        if let Some(remaining) = self.time_until_expiry(current_time_ms) {
            remaining > 0 && remaining <= warning_threshold_ms
        } else {
            false
        }
    }
}

/// 인증 만료 경고
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificationWarning {
    pub certification_id: String,
    pub worker_id: String,
    pub target_id: String,
    pub expires_at_ms: i64,
    pub remaining_ms: i64,
    pub warning_type: CertificationWarningType,
}

/// 인증 경고 유형
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum CertificationWarningType {
    /// 곧 만료 (30일 이내)
    ExpiringSoon,
    /// 임박 (7일 이내)
    Imminent,
    /// 만료됨
    Expired,
}

/// 자격 인증 매트릭스
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CertificationMatrix {
    /// 인증 목록
    certifications: Vec<Certification>,
    /// (작업자 ID, 대상 ID) → 인증 인덱스 (빠른 조회용)
    index: HashMap<(String, String), usize>,
}

impl CertificationMatrix {
    /// 새 인증 매트릭스 생성
    pub fn new() -> Self {
        Self {
            certifications: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// 인증 추가
    pub fn add_certification(&mut self, cert: Certification) {
        let key = (cert.worker_id.clone(), cert.target_id.clone());
        let idx = self.certifications.len();
        self.certifications.push(cert);
        self.index.insert(key, idx);
    }

    /// Builder 패턴: 인증 추가
    pub fn with_certification(mut self, cert: Certification) -> Self {
        self.add_certification(cert);
        self
    }

    /// 설비 인증 간편 추가
    pub fn with_machine_cert(
        self,
        worker_id: impl Into<String>,
        machine_id: impl Into<String>,
        level: CertificationLevel,
        issued_at_ms: i64,
    ) -> Self {
        let worker = worker_id.into();
        let machine = machine_id.into();
        let cert = Certification::new(
            format!("CERT-{}-{}", worker, machine),
            worker,
            machine,
            CertificationTarget::Machine,
            level,
            issued_at_ms,
        );
        self.with_certification(cert)
    }

    /// 공정 유형 인증 간편 추가
    pub fn with_operation_cert(
        self,
        worker_id: impl Into<String>,
        operation_type: impl Into<String>,
        level: CertificationLevel,
        issued_at_ms: i64,
    ) -> Self {
        let worker = worker_id.into();
        let op_type = operation_type.into();
        let cert = Certification::new(
            format!("CERT-{}-{}", worker, op_type),
            worker,
            op_type,
            CertificationTarget::OperationType,
            level,
            issued_at_ms,
        );
        self.with_certification(cert)
    }

    /// 특정 작업자의 특정 대상에 대한 인증 조회
    pub fn get_certification(&self, worker_id: &str, target_id: &str) -> Option<&Certification> {
        let key = (worker_id.to_string(), target_id.to_string());
        self.index
            .get(&key)
            .and_then(|&idx| self.certifications.get(idx))
    }

    /// 작업자가 설비/공정에 대해 인증이 있는지 확인
    pub fn is_certified(&self, worker_id: &str, target_id: &str, current_time_ms: i64) -> bool {
        self.get_certification(worker_id, target_id)
            .is_some_and(|cert| cert.is_valid(current_time_ms))
    }

    /// 작업자의 설비/공정에 대한 인증 레벨 조회
    pub fn get_certification_level(
        &self,
        worker_id: &str,
        target_id: &str,
        current_time_ms: i64,
    ) -> CertificationLevel {
        self.get_certification(worker_id, target_id)
            .filter(|cert| cert.is_valid(current_time_ms))
            .map_or(CertificationLevel::None, |cert| cert.level)
    }

    /// 특정 대상에 대해 인증된 작업자 목록
    pub fn certified_workers(
        &self,
        target_id: &str,
        current_time_ms: i64,
    ) -> Vec<(&str, CertificationLevel)> {
        self.certifications
            .iter()
            .filter(|cert| cert.target_id == target_id && cert.is_valid(current_time_ms))
            .map(|cert| (cert.worker_id.as_str(), cert.level))
            .collect()
    }

    /// 특정 작업자의 모든 유효 인증
    pub fn worker_certifications(
        &self,
        worker_id: &str,
        current_time_ms: i64,
    ) -> Vec<&Certification> {
        self.certifications
            .iter()
            .filter(|cert| cert.worker_id == worker_id && cert.is_valid(current_time_ms))
            .collect()
    }

    /// 만료 예정 인증 경고 생성
    pub fn check_expiring_certifications(
        &self,
        current_time_ms: i64,
        warning_threshold_ms: i64,
    ) -> Vec<CertificationWarning> {
        self.certifications
            .iter()
            .filter_map(|cert| {
                let remaining = cert.time_until_expiry(current_time_ms)?;

                let warning_type = if remaining <= 0 {
                    CertificationWarningType::Expired
                } else if remaining <= 7 * 24 * 3_600_000 {
                    // 7일 이내
                    CertificationWarningType::Imminent
                } else if remaining <= warning_threshold_ms {
                    CertificationWarningType::ExpiringSoon
                } else {
                    return None;
                };

                Some(CertificationWarning {
                    certification_id: cert.id.clone(),
                    worker_id: cert.worker_id.clone(),
                    target_id: cert.target_id.clone(),
                    expires_at_ms: cert.expires_at_ms.unwrap_or(0),
                    remaining_ms: remaining,
                    warning_type,
                })
            })
            .collect()
    }

    /// 인증 효율 보정값 계산
    pub fn get_efficiency_modifier(
        &self,
        worker_id: &str,
        target_id: &str,
        current_time_ms: i64,
    ) -> f64 {
        self.get_certification_level(worker_id, target_id, current_time_ms)
            .efficiency_modifier()
    }

    /// 처리 시간 계산 (인증 레벨 적용)
    pub fn calculate_process_time(
        &self,
        worker_id: &str,
        target_id: &str,
        base_time_ms: i64,
        current_time_ms: i64,
    ) -> Option<i64> {
        let modifier = self.get_efficiency_modifier(worker_id, target_id, current_time_ms);
        if modifier <= 0.0 {
            None // 인증 없음
        } else {
            Some((base_time_ms as f64 / modifier) as i64)
        }
    }

    /// 최적 인증 작업자 선택 (가장 높은 인증 레벨)
    pub fn best_certified_worker(
        &self,
        target_id: &str,
        current_time_ms: i64,
    ) -> Option<(&str, CertificationLevel)> {
        self.certified_workers(target_id, current_time_ms)
            .into_iter()
            .max_by_key(|(_, level)| *level)
    }
}

/// 통합 자격 관리자 (Skill Matrix + Certification Matrix)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QualificationManager {
    /// 스킬 매트릭스 (숙련도)
    pub skill_matrix: SkillMatrix,
    /// 인증 매트릭스 (자격)
    pub certification_matrix: CertificationMatrix,
}

impl QualificationManager {
    /// 새 자격 관리자 생성
    pub fn new() -> Self {
        Self {
            skill_matrix: SkillMatrix::new(),
            certification_matrix: CertificationMatrix::new(),
        }
    }

    /// 스킬 매트릭스 설정
    pub fn with_skills(mut self, skills: SkillMatrix) -> Self {
        self.skill_matrix = skills;
        self
    }

    /// 인증 매트릭스 설정
    pub fn with_certifications(mut self, certs: CertificationMatrix) -> Self {
        self.certification_matrix = certs;
        self
    }

    /// 작업자가 특정 설비/공정을 수행할 수 있는지 종합 확인
    /// (스킬 있음 AND 인증 유효)
    pub fn can_perform(&self, worker_id: &str, target_id: &str, current_time_ms: i64) -> bool {
        // 스킬 체크
        let has_skill = self.skill_matrix.can_perform(worker_id, target_id);

        // 인증이 정의되어 있으면 인증도 체크
        let cert = self
            .certification_matrix
            .get_certification(worker_id, target_id);
        let is_certified = cert.is_none_or(|c| c.is_valid(current_time_ms));

        has_skill && is_certified
    }

    /// 종합 효율 계산 (스킬 숙련도 × 인증 보정)
    pub fn get_combined_efficiency(
        &self,
        worker_id: &str,
        target_id: &str,
        current_time_ms: i64,
    ) -> f64 {
        let skill_proficiency = self.skill_matrix.get_proficiency(worker_id, target_id);
        let cert_modifier = self.certification_matrix.get_efficiency_modifier(
            worker_id,
            target_id,
            current_time_ms,
        );

        // 인증이 없으면 cert_modifier가 0이 되어 수행 불가
        // 인증이 정의되지 않은 경우는 스킬만 적용
        let cert = self
            .certification_matrix
            .get_certification(worker_id, target_id);
        if cert.is_none() {
            skill_proficiency
        } else {
            skill_proficiency * cert_modifier
        }
    }

    /// 종합 처리 시간 계산
    pub fn calculate_process_time(
        &self,
        worker_id: &str,
        target_id: &str,
        base_time_ms: i64,
        current_time_ms: i64,
    ) -> Option<i64> {
        let efficiency = self.get_combined_efficiency(worker_id, target_id, current_time_ms);
        if efficiency <= 0.0 {
            None
        } else {
            Some((base_time_ms as f64 / efficiency) as i64)
        }
    }

    /// 특정 대상에 대해 자격 있는 작업자 목록 (숙련도 순)
    pub fn qualified_workers(&self, target_id: &str, current_time_ms: i64) -> Vec<(String, f64)> {
        self.skill_matrix
            .qualified_workers(target_id)
            .into_iter()
            .filter(|(worker_id, _)| {
                // 인증이 있으면 유효해야 함
                let cert = self
                    .certification_matrix
                    .get_certification(worker_id, target_id);
                cert.is_none_or(|c| c.is_valid(current_time_ms))
            })
            .map(|(worker_id, skill)| {
                let cert_modifier = self.certification_matrix.get_efficiency_modifier(
                    &worker_id,
                    target_id,
                    current_time_ms,
                );
                let cert = self
                    .certification_matrix
                    .get_certification(&worker_id, target_id);
                let combined = if cert.is_none() {
                    skill
                } else {
                    skill * cert_modifier
                };
                (worker_id, combined)
            })
            .collect()
    }

    /// 최적 자격 작업자 선택
    pub fn best_qualified_worker(
        &self,
        target_id: &str,
        current_time_ms: i64,
    ) -> Option<(String, f64)> {
        self.qualified_workers(target_id, current_time_ms)
            .into_iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    }
}

impl LearningCurve {
    /// 새 학습 곡선 생성
    pub fn new(initial: f64, max: f64, rate: f64) -> Self {
        Self {
            initial_proficiency: initial.clamp(0.0, 2.0),
            max_proficiency: max.clamp(initial, 2.0),
            learning_rate: rate.clamp(0.0, 1.0),
            experience: 0,
        }
    }

    /// 현재 숙련도 계산
    pub fn current_proficiency(&self) -> f64 {
        // 지수적 학습 곡선: P(n) = P_max - (P_max - P_init) * e^(-rate * n)
        let range = self.max_proficiency - self.initial_proficiency;
        let decay = (-self.learning_rate * self.experience as f64).exp();
        self.max_proficiency - range * decay
    }

    /// 경험 추가
    pub fn add_experience(&mut self, count: u32) {
        self.experience = self.experience.saturating_add(count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_level_efficiency() {
        assert_eq!(SkillLevel::None.efficiency(), 0.0);
        assert_eq!(SkillLevel::Beginner.efficiency(), 0.5);
        assert_eq!(SkillLevel::Intermediate.efficiency(), 0.75);
        assert_eq!(SkillLevel::Advanced.efficiency(), 1.0);
        assert_eq!(SkillLevel::Expert.efficiency(), 1.2);
    }

    #[test]
    fn test_skill_matrix_basic() {
        let matrix = SkillMatrix::new()
            .with_skill("W1", "CNC", 1.0)
            .with_skill("W1", "Assembly", 0.7)
            .with_skill("W2", "CNC", 0.5)
            .with_skill("W2", "Welding", 1.2);

        assert_eq!(matrix.get_proficiency("W1", "CNC"), 1.0);
        assert_eq!(matrix.get_proficiency("W1", "Assembly"), 0.7);
        assert_eq!(matrix.get_proficiency("W2", "Welding"), 1.2);
    }

    #[test]
    fn test_skill_matrix_default() {
        let matrix = SkillMatrix::new().with_default(0.8);

        // 정의되지 않은 스킬은 기본값
        assert_eq!(matrix.get_proficiency("W1", "Unknown"), 0.8);
    }

    #[test]
    fn test_calculate_process_time() {
        let matrix = SkillMatrix::new()
            .with_skill("W1", "CNC", 1.0)
            .with_skill("W2", "CNC", 0.5)
            .with_skill("W3", "CNC", 0.0); // 수행 불가

        let base_time = 60_000; // 60초

        // W1: 숙련도 1.0 → 60초
        assert_eq!(
            matrix.calculate_process_time("W1", "CNC", base_time),
            Some(60_000)
        );

        // W2: 숙련도 0.5 → 120초
        assert_eq!(
            matrix.calculate_process_time("W2", "CNC", base_time),
            Some(120_000)
        );

        // W3: 숙련도 0.0 → 수행 불가
        assert_eq!(matrix.calculate_process_time("W3", "CNC", base_time), None);
    }

    #[test]
    fn test_qualified_workers() {
        let matrix = SkillMatrix::new()
            .with_skill("W1", "CNC", 1.0)
            .with_skill("W2", "CNC", 0.5)
            .with_skill("W3", "Assembly", 0.8);

        let cnc_workers = matrix.qualified_workers("CNC");
        assert_eq!(cnc_workers.len(), 2);
    }

    #[test]
    fn test_best_worker() {
        let matrix = SkillMatrix::new()
            .with_skill("W1", "CNC", 0.8)
            .with_skill("W2", "CNC", 1.2)
            .with_skill("W3", "CNC", 0.5);

        let best = matrix.best_worker_for("CNC");
        assert!(best.is_some());
        assert_eq!(best.unwrap().0, "W2");
    }

    #[test]
    fn test_learning_curve() {
        let mut curve = LearningCurve::new(0.5, 1.0, 0.1);

        let initial = curve.current_proficiency();
        assert!((initial - 0.5).abs() < 0.01);

        // 경험 추가
        curve.add_experience(10);
        let after_10 = curve.current_proficiency();
        assert!(after_10 > initial);

        // 더 많은 경험
        curve.add_experience(100);
        let after_110 = curve.current_proficiency();
        assert!(after_110 > after_10);
        assert!(after_110 <= 1.0);
    }

    #[test]
    fn test_skill_level_from_proficiency() {
        assert_eq!(SkillLevel::from_proficiency(0.0), SkillLevel::None);
        assert_eq!(SkillLevel::from_proficiency(0.4), SkillLevel::Beginner);
        assert_eq!(SkillLevel::from_proficiency(0.8), SkillLevel::Intermediate);
        assert_eq!(SkillLevel::from_proficiency(1.0), SkillLevel::Advanced);
        assert_eq!(SkillLevel::from_proficiency(1.5), SkillLevel::Expert);
    }

    #[test]
    fn test_certification_level_efficiency() {
        assert_eq!(CertificationLevel::None.efficiency_modifier(), 0.0);
        assert_eq!(CertificationLevel::Basic.efficiency_modifier(), 0.7);
        assert_eq!(CertificationLevel::Standard.efficiency_modifier(), 1.0);
        assert_eq!(CertificationLevel::Advanced.efficiency_modifier(), 1.15);
        assert_eq!(CertificationLevel::Master.efficiency_modifier(), 1.3);
    }

    #[test]
    fn test_certification_validity() {
        let now = 1_000_000_000i64; // 현재 시간
        let day_ms = 24 * 3_600_000;

        // 만료일 없는 인증
        let cert_no_expiry = Certification::new(
            "CERT-001",
            "W1",
            "CNC",
            CertificationTarget::Machine,
            CertificationLevel::Standard,
            now - day_ms * 30,
        );
        assert!(cert_no_expiry.is_valid(now));
        assert_eq!(cert_no_expiry.time_until_expiry(now), None);

        // 유효한 인증
        let cert_valid = Certification::new(
            "CERT-002",
            "W1",
            "Welding",
            CertificationTarget::OperationType,
            CertificationLevel::Advanced,
            now - day_ms * 30,
        )
        .with_expiry(now + day_ms * 60);
        assert!(cert_valid.is_valid(now));
        assert_eq!(cert_valid.time_until_expiry(now), Some(day_ms * 60));

        // 만료된 인증
        let cert_expired = Certification::new(
            "CERT-003",
            "W1",
            "Assembly",
            CertificationTarget::OperationType,
            CertificationLevel::Standard,
            now - day_ms * 90,
        )
        .with_expiry(now - day_ms * 10);
        assert!(!cert_expired.is_valid(now));
    }

    #[test]
    fn test_certification_expiring_soon() {
        let now = 1_000_000_000i64;
        let day_ms = 24 * 3_600_000;
        let warning_30_days = day_ms * 30;

        // 20일 후 만료 (30일 경고 내)
        let cert = Certification::new(
            "CERT-001",
            "W1",
            "CNC",
            CertificationTarget::Machine,
            CertificationLevel::Standard,
            now - day_ms * 100,
        )
        .with_expiry(now + day_ms * 20);

        assert!(cert.is_expiring_soon(now, warning_30_days));
        assert!(!cert.is_expiring_soon(now, day_ms * 10)); // 10일 경고엔 해당 안됨
    }

    #[test]
    fn test_certification_matrix_basic() {
        let now = 1_000_000_000i64;
        let day_ms = 24 * 3_600_000;

        let matrix = CertificationMatrix::new()
            .with_machine_cert("W1", "CNC-001", CertificationLevel::Standard, now - day_ms)
            .with_machine_cert("W1", "CNC-002", CertificationLevel::Advanced, now - day_ms)
            .with_operation_cert("W2", "welding", CertificationLevel::Master, now - day_ms);

        // 인증 확인
        assert!(matrix.is_certified("W1", "CNC-001", now));
        assert!(matrix.is_certified("W1", "CNC-002", now));
        assert!(matrix.is_certified("W2", "welding", now));
        assert!(!matrix.is_certified("W3", "CNC-001", now)); // 인증 없음

        // 인증 레벨 확인
        assert_eq!(
            matrix.get_certification_level("W1", "CNC-001", now),
            CertificationLevel::Standard
        );
        assert_eq!(
            matrix.get_certification_level("W1", "CNC-002", now),
            CertificationLevel::Advanced
        );
        assert_eq!(
            matrix.get_certification_level("W2", "welding", now),
            CertificationLevel::Master
        );
    }

    #[test]
    fn test_certification_matrix_certified_workers() {
        let now = 1_000_000_000i64;
        let day_ms = 24 * 3_600_000;

        let matrix = CertificationMatrix::new()
            .with_machine_cert("W1", "CNC-001", CertificationLevel::Standard, now - day_ms)
            .with_machine_cert("W2", "CNC-001", CertificationLevel::Advanced, now - day_ms)
            .with_machine_cert("W3", "CNC-001", CertificationLevel::Basic, now - day_ms);

        let certified = matrix.certified_workers("CNC-001", now);
        assert_eq!(certified.len(), 3);

        // 최적 작업자는 Advanced 인증을 가진 W2
        let best = matrix.best_certified_worker("CNC-001", now);
        assert!(best.is_some());
        let (worker, level) = best.unwrap();
        assert_eq!(worker, "W2");
        assert_eq!(level, CertificationLevel::Advanced);
    }

    #[test]
    fn test_certification_expiry_warnings() {
        let now = 1_000_000_000i64;
        let day_ms = 24 * 3_600_000;
        let warning_30_days = day_ms * 30;

        let mut matrix = CertificationMatrix::new();

        // 만료된 인증
        let expired_cert = Certification::new(
            "CERT-001",
            "W1",
            "CNC-001",
            CertificationTarget::Machine,
            CertificationLevel::Standard,
            now - day_ms * 100,
        )
        .with_expiry(now - day_ms * 5);
        matrix.add_certification(expired_cert);

        // 5일 후 만료 (임박)
        let imminent_cert = Certification::new(
            "CERT-002",
            "W2",
            "CNC-002",
            CertificationTarget::Machine,
            CertificationLevel::Standard,
            now - day_ms * 100,
        )
        .with_expiry(now + day_ms * 5);
        matrix.add_certification(imminent_cert);

        // 20일 후 만료 (곧 만료)
        let expiring_cert = Certification::new(
            "CERT-003",
            "W3",
            "Welding",
            CertificationTarget::OperationType,
            CertificationLevel::Advanced,
            now - day_ms * 100,
        )
        .with_expiry(now + day_ms * 20);
        matrix.add_certification(expiring_cert);

        // 무기한 (경고 없음)
        let valid_cert = Certification::new(
            "CERT-004",
            "W4",
            "Assembly",
            CertificationTarget::OperationType,
            CertificationLevel::Standard,
            now - day_ms * 100,
        );
        matrix.add_certification(valid_cert);

        let warnings = matrix.check_expiring_certifications(now, warning_30_days);
        assert_eq!(warnings.len(), 3);

        let expired_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| w.warning_type == CertificationWarningType::Expired)
            .collect();
        assert_eq!(expired_warnings.len(), 1);
        assert_eq!(expired_warnings[0].worker_id, "W1");

        let imminent_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| w.warning_type == CertificationWarningType::Imminent)
            .collect();
        assert_eq!(imminent_warnings.len(), 1);
        assert_eq!(imminent_warnings[0].worker_id, "W2");
    }

    #[test]
    fn test_certification_process_time() {
        let now = 1_000_000_000i64;
        let day_ms = 24 * 3_600_000;

        let matrix = CertificationMatrix::new()
            .with_machine_cert("W1", "CNC", CertificationLevel::Standard, now - day_ms)
            .with_machine_cert("W2", "CNC", CertificationLevel::Master, now - day_ms);

        let base_time = 60_000; // 60초

        // Standard (1.0) → 60초
        assert_eq!(
            matrix.calculate_process_time("W1", "CNC", base_time, now),
            Some(60_000)
        );

        // Master (1.3) → 약 46초
        let master_time = matrix.calculate_process_time("W2", "CNC", base_time, now);
        assert!(master_time.is_some());
        assert!(master_time.unwrap() < 50_000);

        // 인증 없음 → None
        assert_eq!(
            matrix.calculate_process_time("W3", "CNC", base_time, now),
            None
        );
    }

    #[test]
    fn test_qualification_manager_combined() {
        let now = 1_000_000_000i64;
        let day_ms = 24 * 3_600_000;

        // 스킬 매트릭스: 숙련도
        let skills = SkillMatrix::new()
            .with_skill("W1", "CNC", 1.0)
            .with_skill("W2", "CNC", 1.5);

        // 인증 매트릭스: 자격
        let certs = CertificationMatrix::new()
            .with_machine_cert("W1", "CNC", CertificationLevel::Standard, now - day_ms)
            .with_machine_cert("W2", "CNC", CertificationLevel::Advanced, now - day_ms);

        let manager = QualificationManager::new()
            .with_skills(skills)
            .with_certifications(certs);

        // W1: 스킬 1.0 × 인증 1.0 = 1.0
        assert!((manager.get_combined_efficiency("W1", "CNC", now) - 1.0).abs() < 0.01);

        // W2: 스킬 1.5 × 인증 1.15 = 1.725
        assert!((manager.get_combined_efficiency("W2", "CNC", now) - 1.725).abs() < 0.01);

        // 수행 가능 여부
        assert!(manager.can_perform("W1", "CNC", now));
        assert!(manager.can_perform("W2", "CNC", now));
    }

    #[test]
    fn test_qualification_manager_expired_cert() {
        let now = 1_000_000_000i64;
        let day_ms = 24 * 3_600_000;

        let skills = SkillMatrix::new().with_skill("W1", "CNC", 1.5);

        let mut certs = CertificationMatrix::new();
        // 만료된 인증
        let expired_cert = Certification::new(
            "CERT-001",
            "W1",
            "CNC",
            CertificationTarget::Machine,
            CertificationLevel::Advanced,
            now - day_ms * 100,
        )
        .with_expiry(now - day_ms * 10);
        certs.add_certification(expired_cert);

        let manager = QualificationManager::new()
            .with_skills(skills)
            .with_certifications(certs);

        // 스킬은 있지만 인증이 만료되어 수행 불가
        assert!(!manager.can_perform("W1", "CNC", now));
        assert_eq!(manager.get_combined_efficiency("W1", "CNC", now), 0.0);
    }

    #[test]
    fn test_qualification_manager_no_cert_requirement() {
        let now = 1_000_000_000i64;

        // 스킬만 있고 인증 요구 없음
        let skills = SkillMatrix::new().with_skill("W1", "Assembly", 1.2);
        let certs = CertificationMatrix::new(); // 빈 인증 매트릭스

        let manager = QualificationManager::new()
            .with_skills(skills)
            .with_certifications(certs);

        // 인증 요구가 없으면 스킬만으로 수행 가능
        assert!(manager.can_perform("W1", "Assembly", now));
        assert!((manager.get_combined_efficiency("W1", "Assembly", now) - 1.2).abs() < 0.01);
    }

    // ========== WorkerTimeOverride Tests ==========

    #[test]
    fn test_time_override_entry_basic() {
        let entry = TimeOverrideEntry::new()
            .with_setup(5000)
            .with_process(30000)
            .with_wait(2000);

        assert_eq!(entry.setup_ms, Some(5000));
        assert_eq!(entry.process_ms, Some(30000));
        assert_eq!(entry.wait_ms, Some(2000));
    }

    #[test]
    fn test_time_override_entry_full() {
        let entry = TimeOverrideEntry::full(1000, 2000, 3000);

        assert_eq!(entry.setup_ms, Some(1000));
        assert_eq!(entry.process_ms, Some(2000));
        assert_eq!(entry.wait_ms, Some(3000));
    }

    #[test]
    fn test_time_override_entry_process_only() {
        let entry = TimeOverrideEntry::process_only(15000);

        assert_eq!(entry.setup_ms, None);
        assert_eq!(entry.process_ms, Some(15000));
        assert_eq!(entry.wait_ms, None);
    }

    #[test]
    fn test_worker_time_override_basic() {
        let overrides = WorkerTimeOverride::new()
            .with_full_time("W1", "CNC", 5000, 30000, 2000)
            .with_process_time("W2", "Assembly", 20000);

        // W1 CNC override exists
        assert!(overrides.has_override("W1", "CNC"));
        let entry = overrides.get_override("W1", "CNC").unwrap();
        assert_eq!(entry.setup_ms, Some(5000));
        assert_eq!(entry.process_ms, Some(30000));
        assert_eq!(entry.wait_ms, Some(2000));

        // W2 Assembly - only process time
        assert!(overrides.has_override("W2", "Assembly"));
        let entry2 = overrides.get_override("W2", "Assembly").unwrap();
        assert_eq!(entry2.setup_ms, None);
        assert_eq!(entry2.process_ms, Some(20000));
        assert_eq!(entry2.wait_ms, None);

        // Non-existent
        assert!(!overrides.has_override("W3", "Welding"));
    }

    #[test]
    fn test_worker_time_override_calculate_times() {
        let overrides = WorkerTimeOverride::new()
            .with_full_time("W1", "CNC", 5000, 30000, 2000)
            .with_process_time("W2", "CNC", 25000);

        let default_setup = 10000;
        let default_process = 60000;
        let default_wait = 5000;

        // W1 CNC: All values overridden
        let (setup, process, wait) =
            overrides.calculate_times("W1", "CNC", default_setup, default_process, default_wait);
        assert_eq!(setup, 5000);
        assert_eq!(process, 30000);
        assert_eq!(wait, 2000);

        // W2 CNC: Only process overridden, others use default
        let (setup2, process2, wait2) =
            overrides.calculate_times("W2", "CNC", default_setup, default_process, default_wait);
        assert_eq!(setup2, default_setup);
        assert_eq!(process2, 25000);
        assert_eq!(wait2, default_wait);

        // W3 (no override): All default
        let (setup3, process3, wait3) =
            overrides.calculate_times("W3", "CNC", default_setup, default_process, default_wait);
        assert_eq!(setup3, default_setup);
        assert_eq!(process3, default_process);
        assert_eq!(wait3, default_wait);
    }

    #[test]
    fn test_worker_time_override_individual_getters() {
        let overrides = WorkerTimeOverride::new()
            .with_override("W1", "CNC", TimeOverrideEntry::new().with_setup(5000))
            .with_override(
                "W2",
                "Assembly",
                TimeOverrideEntry::new().with_process(20000),
            );

        // W1 CNC: Only setup
        assert_eq!(overrides.get_setup_time("W1", "CNC"), Some(5000));
        assert_eq!(overrides.get_process_time("W1", "CNC"), None);
        assert_eq!(overrides.get_wait_time("W1", "CNC"), None);

        // W2 Assembly: Only process
        assert_eq!(overrides.get_setup_time("W2", "Assembly"), None);
        assert_eq!(overrides.get_process_time("W2", "Assembly"), Some(20000));
        assert_eq!(overrides.get_wait_time("W2", "Assembly"), None);

        // Non-existent
        assert_eq!(overrides.get_setup_time("W3", "Welding"), None);
    }

    #[test]
    fn test_worker_time_override_queries() {
        let overrides = WorkerTimeOverride::new()
            .with_full_time("W1", "CNC", 5000, 30000, 2000)
            .with_full_time("W1", "Assembly", 3000, 20000, 1000)
            .with_process_time("W2", "CNC", 25000);

        // All overrides for W1
        let w1_overrides = overrides.worker_overrides("W1");
        assert_eq!(w1_overrides.len(), 2);

        // All overrides for CNC operation
        let cnc_overrides = overrides.operation_overrides("CNC");
        assert_eq!(cnc_overrides.len(), 2);

        // Count
        assert_eq!(overrides.len(), 3);
        assert!(!overrides.is_empty());
    }

    #[test]
    fn test_worker_time_override_priority_over_skill_matrix() {
        // Demonstrate priority: Override > SkillMatrix > default
        let skill_matrix = SkillMatrix::new().with_skill("W1", "CNC", 0.5); // 50% efficiency -> would double time

        let overrides = WorkerTimeOverride::new().with_process_time("W1", "CNC", 30000); // Explicit 30 seconds

        let default_process = 60000; // 60 seconds base

        // With SkillMatrix only (no override)
        let skill_adjusted = skill_matrix.calculate_process_time("W1", "CNC", default_process);
        assert_eq!(skill_adjusted, Some(120_000)); // 60000 / 0.5 = 120000

        // With Override (should use override value directly)
        if let Some(override_time) = overrides.get_process_time("W1", "CNC") {
            assert_eq!(override_time, 30000); // Direct override wins
        }

        // Priority check: use override if exists, else use skill-adjusted
        let final_time = overrides
            .get_process_time("W1", "CNC")
            .or_else(|| skill_matrix.calculate_process_time("W1", "CNC", default_process));
        assert_eq!(final_time, Some(30000)); // Override takes priority
    }
}
