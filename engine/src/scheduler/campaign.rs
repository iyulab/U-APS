//! Campaign Scheduling - 동일 제품 연속 배치로 Setup Time 최소화
//!
//! 캠페인은 동일 제품의 Job들을 그룹화하여 Setup Time을 줄이는 전략

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
// chrono::Utc available if needed
use crate::models::{Job, SetupMatrixCollection};

/// 캠페인 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignConfig {
    /// 최소 캠페인 크기 (Job 수)
    pub min_campaign_size: usize,
    /// 최대 캠페인 크기 (Job 수)
    pub max_campaign_size: usize,
    /// Setup Time 절감 임계값 (ms) - 이 이상 절감 시 캠페인 구성
    pub setup_saving_threshold_ms: i64,
    /// 납기 준수 가중치 (0.0 ~ 1.0)
    pub due_date_weight: f64,
}

impl Default for CampaignConfig {
    fn default() -> Self {
        Self {
            min_campaign_size: 2,
            max_campaign_size: 10,
            setup_saving_threshold_ms: 5000, // 5초
            due_date_weight: 0.5,
        }
    }
}

/// 캠페인 (동일 제품 Job 그룹)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Campaign {
    /// 캠페인 ID
    pub id: String,
    /// 제품명
    pub product_name: String,
    /// 포함된 Job ID 목록
    pub job_ids: Vec<String>,
    /// 예상 Setup Time 절감 (ms)
    pub estimated_setup_saving_ms: i64,
    /// 캠페인 우선순위 (낮을수록 높은 우선순위)
    pub priority: i32,
    /// 가장 이른 납기
    pub earliest_due_date: Option<i64>,
}

/// 캠페인 플래너
#[derive(Debug, Clone)]
pub struct CampaignPlanner {
    config: CampaignConfig,
    setup_matrices: SetupMatrixCollection,
}

impl CampaignPlanner {
    /// 새 캠페인 플래너 생성
    pub fn new(config: CampaignConfig) -> Self {
        Self {
            config,
            setup_matrices: SetupMatrixCollection::new(),
        }
    }

    /// Setup Matrix 설정
    pub fn with_setup_matrices(mut self, matrices: SetupMatrixCollection) -> Self {
        self.setup_matrices = matrices;
        self
    }

    /// Job 목록에서 캠페인 생성
    pub fn create_campaigns(&self, jobs: &[Job]) -> Vec<Campaign> {
        // 제품별로 Job 그룹화
        let mut product_jobs: HashMap<String, Vec<&Job>> = HashMap::new();

        for job in jobs {
            let product = job.product_name.clone().unwrap_or_else(|| job.id.clone());
            product_jobs.entry(product).or_default().push(job);
        }

        let mut campaigns = Vec::new();
        let mut campaign_id = 0;

        for (product_name, mut jobs_for_product) in product_jobs {
            // 최소 캠페인 크기 미만이면 스킵
            if jobs_for_product.len() < self.config.min_campaign_size {
                continue;
            }

            // 납기 순으로 정렬
            jobs_for_product
                .sort_by_key(|j| j.due_date.map(|d| d.timestamp_millis()).unwrap_or(i64::MAX));

            // 캠페인으로 분할
            for chunk in jobs_for_product.chunks(self.config.max_campaign_size) {
                if chunk.len() < self.config.min_campaign_size {
                    continue;
                }

                let job_ids: Vec<String> = chunk.iter().map(|j| j.id.clone()).collect();
                let earliest_due = chunk
                    .iter()
                    .filter_map(|j| j.due_date.map(|d| d.timestamp_millis()))
                    .min();

                // 우선순위 계산 (가장 높은 우선순위 사용)
                let priority = chunk.iter().map(|j| j.priority).min().unwrap_or(100);

                // Setup Time 절감 예상치 계산
                let setup_saving = self.estimate_setup_saving(&product_name, chunk.len());

                campaigns.push(Campaign {
                    id: format!("CAMPAIGN-{}", campaign_id),
                    product_name: product_name.clone(),
                    job_ids,
                    estimated_setup_saving_ms: setup_saving,
                    priority,
                    earliest_due_date: earliest_due,
                });

                campaign_id += 1;
            }
        }

        // 우선순위 → 납기 순으로 정렬
        campaigns.sort_by(|a, b| match a.priority.cmp(&b.priority) {
            std::cmp::Ordering::Equal => a.earliest_due_date.cmp(&b.earliest_due_date),
            other => other,
        });

        campaigns
    }

    /// Setup Time 절감 예상치 계산
    fn estimate_setup_saving(&self, product_name: &str, job_count: usize) -> i64 {
        if job_count <= 1 {
            return 0;
        }

        // 캠페인 내 Job 수 - 1 만큼 Setup 절감
        // (동일 제품 간 전환은 Setup이 없거나 최소)
        // 다른 제품에서 전환 시 기본 setup time 사용
        let default_setup = 10_000i64; // 10초 기본값
        let same_product_setup = self.setup_matrices.get_setup_time(
            "", // 기본값 사용
            Some(product_name),
            product_name,
        );

        let saving_per_transition = default_setup - same_product_setup;
        saving_per_transition.max(0) * (job_count as i64 - 1)
    }

    /// 캠페인을 Job 순서로 변환 (스케줄링용)
    pub fn get_job_sequence(&self, campaigns: &[Campaign], remaining_jobs: &[Job]) -> Vec<String> {
        let mut sequence = Vec::new();
        let mut scheduled_jobs: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        // 캠페인 Job 먼저 추가
        for campaign in campaigns {
            for job_id in &campaign.job_ids {
                if !scheduled_jobs.contains(job_id) {
                    sequence.push(job_id.clone());
                    scheduled_jobs.insert(job_id.clone());
                }
            }
        }

        // 캠페인에 포함되지 않은 Job 추가
        for job in remaining_jobs {
            if !scheduled_jobs.contains(&job.id) {
                sequence.push(job.id.clone());
            }
        }

        sequence
    }

    /// 캠페인 통계 계산
    pub fn calculate_stats(&self, campaigns: &[Campaign]) -> CampaignStats {
        let total_campaigns = campaigns.len();
        let total_jobs: usize = campaigns.iter().map(|c| c.job_ids.len()).sum();
        let total_setup_saving: i64 = campaigns.iter().map(|c| c.estimated_setup_saving_ms).sum();

        let avg_campaign_size = if total_campaigns > 0 {
            total_jobs as f64 / total_campaigns as f64
        } else {
            0.0
        };

        CampaignStats {
            total_campaigns,
            total_jobs_in_campaigns: total_jobs,
            avg_campaign_size,
            total_estimated_setup_saving_ms: total_setup_saving,
        }
    }
}

/// 캠페인 통계
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignStats {
    /// 총 캠페인 수
    pub total_campaigns: usize,
    /// 캠페인에 포함된 총 Job 수
    pub total_jobs_in_campaigns: usize,
    /// 평균 캠페인 크기
    pub avg_campaign_size: f64,
    /// 총 예상 Setup Time 절감 (ms)
    pub total_estimated_setup_saving_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Job, SetupMatrix};
    use chrono::{TimeZone, Utc};

    fn create_test_jobs() -> Vec<Job> {
        vec![
            Job::new("J1")
                .with_product("PRODUCT-A")
                .with_priority(1)
                .with_due_date(Utc.timestamp_millis_opt(100_000).unwrap()),
            Job::new("J2")
                .with_product("PRODUCT-A")
                .with_priority(2)
                .with_due_date(Utc.timestamp_millis_opt(200_000).unwrap()),
            Job::new("J3")
                .with_product("PRODUCT-A")
                .with_priority(1)
                .with_due_date(Utc.timestamp_millis_opt(150_000).unwrap()),
            Job::new("J4")
                .with_product("PRODUCT-B")
                .with_priority(3)
                .with_due_date(Utc.timestamp_millis_opt(300_000).unwrap()),
            Job::new("J5")
                .with_product("PRODUCT-B")
                .with_priority(2)
                .with_due_date(Utc.timestamp_millis_opt(250_000).unwrap()),
            Job::new("J6")
                .with_product("PRODUCT-C")
                .with_priority(1)
                .with_due_date(Utc.timestamp_millis_opt(180_000).unwrap()),
        ]
    }

    #[test]
    fn test_create_campaigns() {
        let jobs = create_test_jobs();
        let config = CampaignConfig {
            min_campaign_size: 2,
            max_campaign_size: 5,
            ..Default::default()
        };

        let planner = CampaignPlanner::new(config);
        let campaigns = planner.create_campaigns(&jobs);

        // PRODUCT-A: 3 jobs (1 campaign)
        // PRODUCT-B: 2 jobs (1 campaign)
        // PRODUCT-C: 1 job (no campaign, below min size)
        assert_eq!(campaigns.len(), 2);

        let product_a_campaign = campaigns
            .iter()
            .find(|c| c.product_name == "PRODUCT-A")
            .unwrap();
        assert_eq!(product_a_campaign.job_ids.len(), 3);

        let product_b_campaign = campaigns
            .iter()
            .find(|c| c.product_name == "PRODUCT-B")
            .unwrap();
        assert_eq!(product_b_campaign.job_ids.len(), 2);
    }

    #[test]
    fn test_campaign_sorting_by_priority() {
        let jobs = create_test_jobs();
        let config = CampaignConfig::default();

        let planner = CampaignPlanner::new(config);
        let campaigns = planner.create_campaigns(&jobs);

        // PRODUCT-A has priority 1, PRODUCT-B has priority 2
        // PRODUCT-A should come first
        assert_eq!(campaigns[0].product_name, "PRODUCT-A");
    }

    #[test]
    fn test_campaign_job_sequence() {
        let jobs = create_test_jobs();
        let config = CampaignConfig::default();

        let planner = CampaignPlanner::new(config);
        let campaigns = planner.create_campaigns(&jobs);
        let sequence = planner.get_job_sequence(&campaigns, &jobs);

        // All jobs should be in sequence
        assert_eq!(sequence.len(), 6);

        // Campaign jobs should come first
        // PRODUCT-A jobs should be consecutive
        let j1_pos = sequence.iter().position(|id| id == "J1").unwrap();
        let j2_pos = sequence.iter().position(|id| id == "J2").unwrap();
        let j3_pos = sequence.iter().position(|id| id == "J3").unwrap();

        // All PRODUCT-A jobs should be grouped
        let min_pos = j1_pos.min(j2_pos).min(j3_pos);
        let max_pos = j1_pos.max(j2_pos).max(j3_pos);
        assert_eq!(max_pos - min_pos, 2); // Consecutive
    }

    #[test]
    fn test_setup_saving_estimation() {
        let setup_matrix = SetupMatrix::new("M1")
            .with_default_setup(10_000)
            .with_setup("PRODUCT-A", "PRODUCT-A", 1_000);

        let matrices = SetupMatrixCollection::new().with_matrix(setup_matrix);

        let config = CampaignConfig::default();
        let planner = CampaignPlanner::new(config).with_setup_matrices(matrices);

        // 3 jobs → 2 transitions → (10000 - 1000) * 2 = 18000ms saving
        let saving = planner.estimate_setup_saving("PRODUCT-A", 3);
        assert!(saving > 0);
    }

    #[test]
    fn test_campaign_stats() {
        let jobs = create_test_jobs();
        let config = CampaignConfig::default();

        let planner = CampaignPlanner::new(config);
        let campaigns = planner.create_campaigns(&jobs);
        let stats = planner.calculate_stats(&campaigns);

        assert_eq!(stats.total_campaigns, 2);
        assert_eq!(stats.total_jobs_in_campaigns, 5); // 3 + 2
        assert!(stats.avg_campaign_size > 2.0);
    }

    #[test]
    fn test_max_campaign_size() {
        // Create 15 jobs of same product
        let jobs: Vec<Job> = (0..15)
            .map(|i| Job::new(format!("J{}", i)).with_product("PRODUCT-X"))
            .collect();

        let config = CampaignConfig {
            min_campaign_size: 2,
            max_campaign_size: 5,
            ..Default::default()
        };

        let planner = CampaignPlanner::new(config);
        let campaigns = planner.create_campaigns(&jobs);

        // Should create 3 campaigns (5, 5, 5)
        assert_eq!(campaigns.len(), 3);
        for campaign in &campaigns {
            assert!(campaign.job_ids.len() <= 5);
        }
    }

    #[test]
    fn test_no_campaigns_below_min_size() {
        let jobs = vec![
            Job::new("J1").with_product("PRODUCT-A"),
            Job::new("J2").with_product("PRODUCT-B"),
            Job::new("J3").with_product("PRODUCT-C"),
        ];

        let config = CampaignConfig {
            min_campaign_size: 2,
            ..Default::default()
        };

        let planner = CampaignPlanner::new(config);
        let campaigns = planner.create_campaigns(&jobs);

        // No campaigns (each product has only 1 job)
        assert_eq!(campaigns.len(), 0);
    }
}
