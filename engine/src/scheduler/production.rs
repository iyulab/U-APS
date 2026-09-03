//! Production Scheduler - 통합 생산 스케줄러
//!
//! GA, Campaign, Skill Matrix, Setup Matrix 등 모든 기능을 통합한 고수준 인터페이스

use crate::ga::{GaParams, GaScheduler, GeneticOperators};
use crate::models::{Job, Resource, SetupMatrixCollection, SiteTransitions, SkillMatrix};
use crate::scheduler::{
    campaign::{Campaign, CampaignConfig, CampaignPlanner, CampaignStats},
    kpi::{KpiCalculator, ScheduleKpi},
    Schedule,
};
use serde::{Deserialize, Serialize};

/// 생산 스케줄러 설정
#[derive(Debug, Clone)]
pub struct ProductionConfig {
    /// GA 파라미터
    pub ga_params: GaParams,
    /// 캠페인 설정 (None이면 캠페인 비활성화)
    pub campaign_config: Option<CampaignConfig>,
    /// 병렬 처리 활성화
    pub enable_parallel: bool,
    /// 자세한 결과 출력
    pub verbose: bool,
}

impl Default for ProductionConfig {
    fn default() -> Self {
        Self {
            ga_params: GaParams::default(),
            campaign_config: Some(CampaignConfig::default()),
            enable_parallel: true,
            verbose: false,
        }
    }
}

/// 생산 스케줄링 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionResult {
    /// 스케줄
    pub schedule: Schedule,
    /// KPI
    pub kpi: ScheduleKpi,
    /// GA 결과
    pub ga_generations: usize,
    pub ga_fitness: f64,
    pub ga_converged: bool,
    /// 캠페인 정보
    pub campaigns: Vec<Campaign>,
    pub campaign_stats: Option<CampaignStats>,
    /// 실행 시간 (ms)
    pub execution_time_ms: u128,
}

/// 통합 생산 스케줄러
#[derive(Debug, Clone)]
pub struct ProductionScheduler {
    config: ProductionConfig,
    setup_matrices: SetupMatrixCollection,
    skill_matrix: Option<SkillMatrix>,
    site_transitions: SiteTransitions,
}

impl ProductionScheduler {
    /// 새 생산 스케줄러 생성
    pub fn new(config: ProductionConfig) -> Self {
        Self {
            config,
            setup_matrices: SetupMatrixCollection::new(),
            skill_matrix: None,
            site_transitions: SiteTransitions::new(),
        }
    }

    /// 기본 설정으로 생성
    pub fn default_scheduler() -> Self {
        Self::new(ProductionConfig::default())
    }

    /// Setup Matrix 설정
    pub fn with_setup_matrices(mut self, matrices: SetupMatrixCollection) -> Self {
        self.setup_matrices = matrices;
        self
    }

    /// Skill Matrix 설정
    pub fn with_skill_matrix(mut self, matrix: SkillMatrix) -> Self {
        self.skill_matrix = Some(matrix);
        self
    }

    /// 사이트 간 이동 시간 설정
    pub fn with_site_transitions(mut self, transitions: SiteTransitions) -> Self {
        self.site_transitions = transitions;
        self
    }

    /// 생산 스케줄링 실행
    pub fn schedule(
        &self,
        jobs: &[Job],
        resources: &[Resource],
        start_time_ms: i64,
    ) -> ProductionResult {
        let start = std::time::Instant::now();

        // 1. 캠페인 생성 (활성화된 경우)
        let (campaigns, campaign_stats, job_sequence) =
            if let Some(ref campaign_config) = self.config.campaign_config {
                let planner = CampaignPlanner::new(campaign_config.clone())
                    .with_setup_matrices(self.setup_matrices.clone());

                let campaigns = planner.create_campaigns(jobs);
                let stats = planner.calculate_stats(&campaigns);
                let sequence = planner.get_job_sequence(&campaigns, jobs);

                (campaigns, Some(stats), Some(sequence))
            } else {
                (Vec::new(), None, None)
            };

        // 2. Job 순서 재배열 (캠페인 기반)
        let ordered_jobs: Vec<Job> = if let Some(ref sequence) = job_sequence {
            sequence
                .iter()
                .filter_map(|id| jobs.iter().find(|j| &j.id == id).cloned())
                .collect()
        } else {
            jobs.to_vec()
        };

        // 3. GA 스케줄링 실행
        let ga_scheduler =
            GaScheduler::new(self.config.ga_params.clone(), GeneticOperators::default())
                .with_setup_matrices(self.setup_matrices.clone())
                .with_site_transitions(self.site_transitions.clone());

        let ga_result = ga_scheduler.schedule(&ordered_jobs, resources, start_time_ms);

        // 4. KPI 계산
        let horizon_ms = ga_result.schedule.makespan_ms.max(1);
        let kpi_calculator = KpiCalculator::new(horizon_ms);
        let kpi = kpi_calculator.calculate(&ga_result.schedule, &ordered_jobs);

        let execution_time_ms = start.elapsed().as_millis();

        ProductionResult {
            schedule: ga_result.schedule,
            kpi,
            ga_generations: ga_result.generations,
            ga_fitness: ga_result.fitness,
            ga_converged: ga_result.converged,
            campaigns,
            campaign_stats,
            execution_time_ms,
        }
    }

    /// 빠른 스케줄링 (적은 세대 수)
    pub fn quick_schedule(
        &self,
        jobs: &[Job],
        resources: &[Resource],
        start_time_ms: i64,
    ) -> ProductionResult {
        let mut quick_config = self.config.clone();
        quick_config.ga_params.max_generations = 100;
        quick_config.ga_params.population_size = 50;

        let quick_scheduler =
            ProductionScheduler::new(quick_config).with_setup_matrices(self.setup_matrices.clone());

        if let Some(ref matrix) = self.skill_matrix {
            quick_scheduler.with_skill_matrix(matrix.clone()).schedule(
                jobs,
                resources,
                start_time_ms,
            )
        } else {
            quick_scheduler.schedule(jobs, resources, start_time_ms)
        }
    }
}

impl Default for ProductionScheduler {
    fn default() -> Self {
        Self::default_scheduler()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Job, Operation, Resource, SetupMatrix};

    fn create_test_scenario() -> (Vec<Job>, Vec<Resource>) {
        let jobs = vec![
            Job::new("J1")
                .with_product("PRODUCT-A")
                .with_priority(1)
                .with_operation(
                    Operation::new("J1-O1", "J1", 1)
                        .with_time(0, 30_000, 0)
                        .with_equipment(vec!["M1".to_string(), "M2".to_string()]),
                ),
            Job::new("J2")
                .with_product("PRODUCT-A")
                .with_priority(2)
                .with_operation(
                    Operation::new("J2-O1", "J2", 1)
                        .with_time(0, 25_000, 0)
                        .with_equipment(vec!["M1".to_string(), "M2".to_string()]),
                ),
            Job::new("J3")
                .with_product("PRODUCT-B")
                .with_priority(1)
                .with_operation(
                    Operation::new("J3-O1", "J3", 1)
                        .with_time(0, 35_000, 0)
                        .with_equipment(vec!["M1".to_string(), "M2".to_string()]),
                ),
            Job::new("J4")
                .with_product("PRODUCT-A")
                .with_priority(3)
                .with_operation(
                    Operation::new("J4-O1", "J4", 1)
                        .with_time(0, 20_000, 0)
                        .with_equipment(vec!["M1".to_string(), "M2".to_string()]),
                ),
        ];

        let resources = vec![
            Resource::equipment("M1").with_efficiency(1.0),
            Resource::equipment("M2").with_efficiency(0.9),
        ];

        (jobs, resources)
    }

    #[test]
    fn test_production_scheduler_basic() {
        let (jobs, resources) = create_test_scenario();

        let scheduler = ProductionScheduler::default();
        let result = scheduler.schedule(&jobs, &resources, 0);

        assert!(!result.schedule.assignments.is_empty());
        assert!(result.schedule.makespan_ms > 0);
        assert!(result.ga_generations > 0);
    }

    #[test]
    fn test_production_scheduler_with_campaigns() {
        let (jobs, resources) = create_test_scenario();

        let config = ProductionConfig {
            campaign_config: Some(CampaignConfig {
                min_campaign_size: 2,
                max_campaign_size: 5,
                ..Default::default()
            }),
            ..Default::default()
        };

        let scheduler = ProductionScheduler::new(config);
        let result = scheduler.schedule(&jobs, &resources, 0);

        // PRODUCT-A has 3 jobs -> should create 1 campaign
        assert!(!result.campaigns.is_empty());
        assert!(result.campaign_stats.is_some());

        let stats = result.campaign_stats.unwrap();
        assert!(stats.total_campaigns >= 1);
    }

    #[test]
    fn test_production_scheduler_with_setup_matrix() {
        let (jobs, resources) = create_test_scenario();

        let setup_matrix = SetupMatrix::new("M1")
            .with_default_setup(10_000)
            .with_setup("PRODUCT-A", "PRODUCT-A", 1_000)
            .with_setup("PRODUCT-A", "PRODUCT-B", 15_000);

        let matrices = SetupMatrixCollection::new().with_matrix(setup_matrix);

        let scheduler = ProductionScheduler::default().with_setup_matrices(matrices);

        let result = scheduler.schedule(&jobs, &resources, 0);

        assert!(!result.schedule.assignments.is_empty());
    }

    #[test]
    fn test_quick_schedule() {
        let (jobs, resources) = create_test_scenario();

        let scheduler = ProductionScheduler::default();
        let result = scheduler.quick_schedule(&jobs, &resources, 0);

        // Quick schedule should complete faster with fewer generations
        assert!(result.ga_generations <= 100);
        assert!(!result.schedule.assignments.is_empty());
    }

    #[test]
    fn test_kpi_calculation() {
        let (jobs, resources) = create_test_scenario();

        let scheduler = ProductionScheduler::default();
        let result = scheduler.schedule(&jobs, &resources, 0);

        // KPI should be calculated
        assert!(result.kpi.makespan_ms > 0);
        assert!(result.kpi.average_utilization >= 0.0);
    }

    #[test]
    fn test_no_campaigns_config() {
        let (jobs, resources) = create_test_scenario();

        let config = ProductionConfig {
            campaign_config: None, // Disable campaigns
            ..Default::default()
        };

        let scheduler = ProductionScheduler::new(config);
        let result = scheduler.schedule(&jobs, &resources, 0);

        assert!(result.campaigns.is_empty());
        assert!(result.campaign_stats.is_none());
    }

    #[test]
    fn test_execution_time_tracking() {
        let (jobs, resources) = create_test_scenario();

        let scheduler = ProductionScheduler::default();
        let result = scheduler.schedule(&jobs, &resources, 0);

        // Execution time should be tracked
        assert!(result.execution_time_ms > 0);
    }
}
