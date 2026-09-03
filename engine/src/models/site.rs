//! Site - 사이트/공장 간 이동 시간 관리

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 사이트 간 이동 시간 매트릭스
///
/// Multi-site 스케줄링 시 공장 간 자재/반제품 이동에 소요되는 시간을 정의.
/// Job의 연속된 Operation이 다른 site에 할당될 경우 이동 시간이 추가된다.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SiteTransitions {
    /// (from_site, to_site) → 이동 시간 (ms)
    transitions: HashMap<String, i64>,
}

impl SiteTransitions {
    pub fn new() -> Self {
        Self {
            transitions: HashMap::new(),
        }
    }

    /// 사이트 간 이동 시간 등록 (양방향)
    pub fn add_bidirectional(
        &mut self,
        site_a: impl Into<String>,
        site_b: impl Into<String>,
        time_ms: i64,
    ) {
        let a = site_a.into();
        let b = site_b.into();
        let key_ab = format!("{}→{}", a, b);
        let key_ba = format!("{}→{}", b, a);
        self.transitions.insert(key_ab, time_ms);
        self.transitions.insert(key_ba, time_ms);
    }

    /// 사이트 간 이동 시간 등록 (단방향)
    pub fn add(&mut self, from_site: impl Into<String>, to_site: impl Into<String>, time_ms: i64) {
        let key = format!("{}→{}", from_site.into(), to_site.into());
        self.transitions.insert(key, time_ms);
    }

    /// 사이트 간 이동 시간 조회
    /// 같은 사이트이거나 미정의 경로면 0 반환
    pub fn get_transition_time(&self, from_site: Option<&str>, to_site: Option<&str>) -> i64 {
        match (from_site, to_site) {
            (Some(from), Some(to)) if from != to => {
                let key = format!("{from}→{to}");
                self.transitions.get(&key).copied().unwrap_or(0)
            }
            _ => 0,
        }
    }

    /// 이동 시간이 정의되어 있는지 확인
    pub fn is_empty(&self) -> bool {
        self.transitions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_site_transitions_bidirectional() {
        let mut transitions = SiteTransitions::new();
        transitions.add_bidirectional("SITE-A", "SITE-B", 3_600_000); // 1시간

        assert_eq!(
            transitions.get_transition_time(Some("SITE-A"), Some("SITE-B")),
            3_600_000
        );
        assert_eq!(
            transitions.get_transition_time(Some("SITE-B"), Some("SITE-A")),
            3_600_000
        );
    }

    #[test]
    fn test_site_transitions_same_site() {
        let transitions = SiteTransitions::new();
        assert_eq!(
            transitions.get_transition_time(Some("SITE-A"), Some("SITE-A")),
            0
        );
    }

    #[test]
    fn test_site_transitions_none() {
        let transitions = SiteTransitions::new();
        assert_eq!(transitions.get_transition_time(None, Some("SITE-A")), 0);
        assert_eq!(transitions.get_transition_time(Some("SITE-A"), None), 0);
        assert_eq!(transitions.get_transition_time(None, None), 0);
    }

    #[test]
    fn test_site_transitions_undefined_route() {
        let mut transitions = SiteTransitions::new();
        transitions.add_bidirectional("SITE-A", "SITE-B", 3_600_000);

        // SITE-A → SITE-C 미정의
        assert_eq!(
            transitions.get_transition_time(Some("SITE-A"), Some("SITE-C")),
            0
        );
    }

    #[test]
    fn test_site_transitions_unidirectional() {
        let mut transitions = SiteTransitions::new();
        transitions.add("SITE-A", "SITE-B", 3_600_000);

        assert_eq!(
            transitions.get_transition_time(Some("SITE-A"), Some("SITE-B")),
            3_600_000
        );
        // 반대 방향은 미정의
        assert_eq!(
            transitions.get_transition_time(Some("SITE-B"), Some("SITE-A")),
            0
        );
    }
}
