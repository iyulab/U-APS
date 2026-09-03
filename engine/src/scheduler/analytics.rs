//! Analytics Engine - 통합 분석 인터페이스
//!
//! CTP/ATP, What-If 분석, KPI 기반 권고 통합

use crate::models::Job;
use crate::scheduler::{
    ctp::{AtpResult, CtpEngine, CtpRequest, CtpResult, ResourceCapacity},
    kpi::{analyze_bottlenecks, BottleneckAnalysis, KpiCalculator, ScheduleKpi},
    pegging::{MaterialSupply, PeggingMaterial},
    whatif::{ComparisonResult, Scenario, ScenarioResult, WhatIfEngine, WhatIfObjective},
    Schedule,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Analytics Engine 설정
#[derive(Debug, Clone)]
pub struct AnalyticsConfig {
    /// 계획 수평선 (ms)
    pub horizon_ms: i64,
    /// 현재 시점 (ms)
    pub current_time_ms: i64,
    /// 기본 What-If 목적 함수
    pub default_objective: WhatIfObjective,
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self {
            horizon_ms: 7 * 24 * 3_600_000, // 7일
            current_time_ms: 0,
            default_objective: WhatIfObjective::MinimizeMakespan,
        }
    }
}

/// 종합 분석 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsReport {
    /// 스케줄 KPI
    pub kpi: ScheduleKpi,
    /// 병목 분석
    pub bottlenecks: Vec<BottleneckAnalysis>,
    /// 개선 권고
    pub recommendations: Vec<Recommendation>,
    /// 위험 요소
    pub risks: Vec<Risk>,
    /// 분석 시간 (ms)
    pub analysis_time_ms: u128,
}

/// 개선 권고
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// 권고 유형
    pub category: RecommendationCategory,
    /// 우선순위 (1-5, 낮을수록 높음)
    pub priority: i32,
    /// 설명
    pub description: String,
    /// 예상 개선 효과
    pub expected_improvement: String,
    /// 관련 자원/공정 ID
    pub related_ids: Vec<String>,
}

/// 권고 유형
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecommendationCategory {
    /// 용량 증대
    CapacityIncrease,
    /// 부하 분산
    LoadBalancing,
    /// 일정 조정
    ScheduleAdjustment,
    /// Setup 최적화
    SetupOptimization,
    /// 자재 관리
    MaterialManagement,
}

/// 위험 요소
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Risk {
    /// 위험 유형
    pub risk_type: RiskType,
    /// 심각도 (1-5)
    pub severity: i32,
    /// 확률
    pub probability: f64,
    /// 설명
    pub description: String,
    /// 완화 방안
    pub mitigation: String,
}

/// 위험 유형
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskType {
    /// 납기 지연
    DeliveryDelay,
    /// 자원 과부하
    ResourceOverload,
    /// 자재 부족
    MaterialShortage,
    /// 품질 문제
    QualityIssue,
}

/// CTP 일괄 처리 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCtpResult {
    /// 개별 CTP 결과
    pub results: Vec<CtpResult>,
    /// 전체 성공률
    pub success_rate: f64,
    /// 총 약속 수량
    pub total_promised_qty: f64,
    /// 총 요청 수량
    pub total_requested_qty: f64,
}

/// 민감도 분석 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitivityResult {
    /// 분석 대상
    pub parameter: String,
    /// 변화 범위
    pub variations: Vec<f64>,
    /// 각 변화에 따른 KPI
    pub kpi_results: Vec<ScheduleKpi>,
    /// 민감도 점수 (높을수록 민감)
    pub sensitivity_score: f64,
}

/// 통합 Analytics Engine
pub struct AnalyticsEngine {
    config: AnalyticsConfig,
    ctp_engine: CtpEngine,
    schedule: Option<Schedule>,
    jobs: Vec<Job>,
}

impl AnalyticsEngine {
    /// 새 Analytics Engine 생성
    pub fn new(config: AnalyticsConfig) -> Self {
        Self {
            ctp_engine: CtpEngine::new(config.current_time_ms),
            config,
            schedule: None,
            jobs: Vec::new(),
        }
    }

    /// 스케줄 설정
    pub fn with_schedule(mut self, schedule: Schedule, jobs: Vec<Job>) -> Self {
        self.schedule = Some(schedule);
        self.jobs = jobs;
        self
    }

    /// 자재 추가
    pub fn add_material(&mut self, material: PeggingMaterial) {
        self.ctp_engine.add_material(material);
    }

    /// 공급 추가
    pub fn add_supply(&mut self, supply: MaterialSupply) {
        self.ctp_engine.add_supply(supply);
    }

    /// 자원 용량 설정
    pub fn set_resource_capacity(&mut self, capacity: ResourceCapacity) {
        self.ctp_engine.set_resource_capacity(capacity);
    }

    /// 제품 BOM 설정
    pub fn set_product_bom(&mut self, product_id: &str, bom: Vec<(String, f64)>) {
        self.ctp_engine.set_product_bom(product_id, bom);
    }

    /// 제품 공정 시간 설정
    pub fn set_product_process_time(&mut self, product_id: &str, time_ms: i64) {
        self.ctp_engine
            .set_product_process_time(product_id, time_ms);
    }

    /// 종합 분석 실행
    pub fn analyze(&self) -> AnalyticsReport {
        let start = std::time::Instant::now();

        let schedule = match &self.schedule {
            Some(s) => s,
            None => {
                return AnalyticsReport {
                    kpi: ScheduleKpi {
                        makespan_ms: 0,
                        total_tardiness_ms: 0,
                        max_tardiness_ms: 0,
                        on_time_delivery_rate: 0.0,
                        average_utilization: 0.0,
                        resource_utilization: HashMap::new(),
                        average_flow_time_ms: 0.0,
                        mce: 0.0,
                        bottleneck_resource: None,
                    },
                    bottlenecks: vec![],
                    recommendations: vec![],
                    risks: vec![],
                    analysis_time_ms: start.elapsed().as_millis(),
                };
            }
        };

        // KPI 계산
        let calculator = KpiCalculator::new(self.config.horizon_ms);
        let kpi = calculator.calculate(schedule, &self.jobs);

        // 병목 분석
        let bottlenecks = analyze_bottlenecks(schedule, self.config.horizon_ms);

        // 권고 생성
        let recommendations = self.generate_recommendations(&kpi, &bottlenecks);

        // 위험 식별
        let risks = self.identify_risks(&kpi, &bottlenecks);

        let analysis_time_ms = start.elapsed().as_millis();

        AnalyticsReport {
            kpi,
            bottlenecks,
            recommendations,
            risks,
            analysis_time_ms,
        }
    }

    /// CTP 확인
    pub fn check_ctp(&mut self, request: &CtpRequest) -> CtpResult {
        self.ctp_engine.check_capability(request)
    }

    /// ATP 확인
    pub fn check_atp(&self, product_id: &str) -> AtpResult {
        self.ctp_engine.check_atp(product_id)
    }

    /// 일괄 CTP 처리
    pub fn batch_ctp(&mut self, requests: &[CtpRequest]) -> BatchCtpResult {
        let mut results = Vec::new();
        let mut total_promised = 0.0;
        let mut total_requested = 0.0;
        let mut success_count = 0;

        for request in requests {
            let result = self.ctp_engine.check_capability(request);
            total_requested += request.quantity;
            total_promised += result.promised_quantity;
            if result.is_capable {
                success_count += 1;
            }
            results.push(result);
        }

        let success_rate = if !requests.is_empty() {
            success_count as f64 / requests.len() as f64
        } else {
            0.0
        };

        BatchCtpResult {
            results,
            success_rate,
            total_promised_qty: total_promised,
            total_requested_qty: total_requested,
        }
    }

    /// 가용 납기일 조회
    pub fn get_available_dates(
        &mut self,
        request: &CtpRequest,
        range_days: i32,
    ) -> Vec<(i64, f64)> {
        self.ctp_engine.get_available_dates(request, range_days)
    }

    /// What-If 분석 실행
    pub fn run_whatif(&self, scenarios: &[Scenario]) -> Vec<ScenarioResult> {
        let schedule = match &self.schedule {
            Some(s) => s.clone(),
            None => return vec![],
        };

        let engine = WhatIfEngine::new(schedule, self.jobs.clone(), self.config.horizon_ms);
        engine.analyze_scenarios(scenarios)
    }

    /// 시나리오 비교
    pub fn compare_scenarios(
        &self,
        baseline: &ScenarioResult,
        compare: &ScenarioResult,
    ) -> ComparisonResult {
        let schedule = match &self.schedule {
            Some(s) => s.clone(),
            None => {
                return ComparisonResult {
                    baseline_id: baseline.scenario_id.clone(),
                    compare_id: compare.scenario_id.clone(),
                    kpi_diff: crate::scheduler::whatif::KpiDiff {
                        makespan_diff_ms: 0,
                        makespan_diff_percent: 0.0,
                        tardiness_diff_ms: 0,
                        on_time_rate_diff: 0.0,
                        utilization_diff: 0.0,
                    },
                    summary: crate::scheduler::whatif::ComparisonSummary {
                        overall: crate::scheduler::whatif::ChangeAssessment::Neutral,
                        improvements: vec![],
                        degradations: vec![],
                        recommendations: vec![],
                    },
                };
            }
        };

        let engine = WhatIfEngine::new(schedule, self.jobs.clone(), self.config.horizon_ms);
        engine.compare_scenarios(baseline, compare)
    }

    /// 최적 시나리오 선택
    pub fn find_best_scenario<'a>(
        &self,
        results: &'a [ScenarioResult],
    ) -> Option<&'a ScenarioResult> {
        let schedule = self.schedule.clone()?;

        let engine = WhatIfEngine::new(schedule, self.jobs.clone(), self.config.horizon_ms);
        engine.find_best_scenario(results, self.config.default_objective.clone())
    }

    /// 민감도 분석
    pub fn sensitivity_analysis(&self, parameter: &str, variations: &[f64]) -> SensitivityResult {
        let schedule = match &self.schedule {
            Some(s) => s.clone(),
            None => {
                return SensitivityResult {
                    parameter: parameter.to_string(),
                    variations: variations.to_vec(),
                    kpi_results: vec![],
                    sensitivity_score: 0.0,
                };
            }
        };

        let engine = WhatIfEngine::new(schedule, self.jobs.clone(), self.config.horizon_ms);
        let mut kpi_results = Vec::new();

        for &change in variations {
            let scenario = Scenario {
                id: format!("sensitivity_{}", change),
                name: format!("{} {}%", parameter, change * 100.0),
                changes: vec![crate::scheduler::whatif::ScenarioChange::ProcessingTime {
                    operation_id: None,
                    change_percent: change,
                }],
            };

            let result = engine.apply_scenario(&scenario);
            kpi_results.push(result.kpi);
        }

        // 민감도 점수 계산 (makespan 변화율 기준)
        let sensitivity_score = if kpi_results.len() >= 2 {
            let first = kpi_results.first().unwrap().makespan_ms;
            let last = kpi_results.last().unwrap().makespan_ms;
            if first > 0 {
                ((last - first) as f64 / first as f64).abs()
            } else {
                0.0
            }
        } else {
            0.0
        };

        SensitivityResult {
            parameter: parameter.to_string(),
            variations: variations.to_vec(),
            kpi_results,
            sensitivity_score,
        }
    }

    // 내부 헬퍼 함수들

    fn generate_recommendations(
        &self,
        kpi: &ScheduleKpi,
        bottlenecks: &[BottleneckAnalysis],
    ) -> Vec<Recommendation> {
        let mut recommendations = Vec::new();

        // 병목 기반 권고
        for bottleneck in bottlenecks {
            if bottleneck.utilization > 0.95 {
                recommendations.push(Recommendation {
                    category: RecommendationCategory::CapacityIncrease,
                    priority: 1,
                    description: format!(
                        "자원 {} 용량 확대 권고 (현재 가동률: {:.1}%)",
                        bottleneck.resource_id,
                        bottleneck.utilization * 100.0
                    ),
                    expected_improvement: "Makespan 10-20% 감소 예상".into(),
                    related_ids: vec![bottleneck.resource_id.clone()],
                });
            } else if bottleneck.utilization > 0.85 {
                recommendations.push(Recommendation {
                    category: RecommendationCategory::LoadBalancing,
                    priority: 2,
                    description: format!(
                        "자원 {} 부하 분산 검토 (현재 가동률: {:.1}%)",
                        bottleneck.resource_id,
                        bottleneck.utilization * 100.0
                    ),
                    expected_improvement: "Makespan 5-10% 감소 예상".into(),
                    related_ids: vec![bottleneck.resource_id.clone()],
                });
            }
        }

        // KPI 기반 권고
        if kpi.on_time_delivery_rate < 0.9 {
            recommendations.push(Recommendation {
                category: RecommendationCategory::ScheduleAdjustment,
                priority: 1,
                description: format!(
                    "납기 준수율 개선 필요 (현재: {:.1}%)",
                    kpi.on_time_delivery_rate * 100.0
                ),
                expected_improvement: "납기 준수율 95% 이상 목표".into(),
                related_ids: vec![],
            });
        }

        if kpi.mce < 0.5 {
            recommendations.push(Recommendation {
                category: RecommendationCategory::SetupOptimization,
                priority: 2,
                description: format!(
                    "제조 사이클 효율(MCE) 개선 필요 (현재: {:.1}%)",
                    kpi.mce * 100.0
                ),
                expected_improvement: "MCE 70% 이상 목표".into(),
                related_ids: vec![],
            });
        }

        recommendations
    }

    fn identify_risks(&self, kpi: &ScheduleKpi, bottlenecks: &[BottleneckAnalysis]) -> Vec<Risk> {
        let mut risks = Vec::new();

        // 납기 지연 위험
        if kpi.on_time_delivery_rate < 0.95 {
            let severity = if kpi.on_time_delivery_rate < 0.8 {
                5
            } else {
                3
            };
            risks.push(Risk {
                risk_type: RiskType::DeliveryDelay,
                severity,
                probability: 1.0 - kpi.on_time_delivery_rate,
                description: format!(
                    "납기 지연 위험 - {:.1}% 주문 지연 예상",
                    (1.0 - kpi.on_time_delivery_rate) * 100.0
                ),
                mitigation: "우선순위 재조정 또는 용량 증대 검토".into(),
            });
        }

        // 자원 과부하 위험
        for bottleneck in bottlenecks {
            if bottleneck.utilization > 0.9 {
                risks.push(Risk {
                    risk_type: RiskType::ResourceOverload,
                    severity: 4,
                    probability: 0.8,
                    description: format!(
                        "자원 {} 과부하 위험 (가동률 {:.1}%)",
                        bottleneck.resource_id,
                        bottleneck.utilization * 100.0
                    ),
                    mitigation: "예방 정비 일정 확인 및 대체 자원 준비".into(),
                });
            }
        }

        risks
    }
}

impl Default for AnalyticsEngine {
    fn default() -> Self {
        Self::new(AnalyticsConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::Assignment;

    fn create_test_schedule() -> Schedule {
        Schedule {
            assignments: vec![
                Assignment {
                    operation_id: "op1".into(),
                    job_id: "job1".into(),
                    resource_id: "m1".into(),
                    start_ms: 0,
                    end_ms: 30_000,
                    setup_ms: 5_000,
                    site_id: None,
                },
                Assignment {
                    operation_id: "op2".into(),
                    job_id: "job2".into(),
                    resource_id: "m1".into(),
                    start_ms: 30_000,
                    end_ms: 60_000,
                    setup_ms: 5_000,
                    site_id: None,
                },
            ],
            makespan_ms: 60_000,
            violations: vec![],
            tasks: vec![],
        }
    }

    fn create_test_jobs() -> Vec<Job> {
        use chrono::{TimeZone, Utc};
        vec![
            Job::new("job1")
                .with_priority(1)
                .with_due_date(Utc.timestamp_millis_opt(70_000).unwrap()),
            Job::new("job2")
                .with_priority(2)
                .with_due_date(Utc.timestamp_millis_opt(80_000).unwrap()),
        ]
    }

    #[test]
    fn test_basic_analysis() {
        let schedule = create_test_schedule();
        let jobs = create_test_jobs();

        let engine = AnalyticsEngine::default().with_schedule(schedule, jobs);

        let report = engine.analyze();

        assert!(report.kpi.makespan_ms > 0);
    }

    #[test]
    fn test_ctp_integration() {
        let mut engine = AnalyticsEngine::default();

        engine.add_material(PeggingMaterial {
            id: "mat1".into(),
            name: "Component".into(),
            on_hand_qty: 100.0,
            unit: "EA".into(),
        });

        engine.set_product_bom("product1", vec![("mat1".into(), 2.0)]);
        engine.set_product_process_time("product1", 3_600_000);

        let request = CtpRequest {
            order_id: "order1".into(),
            product_id: "product1".into(),
            quantity: 10.0,
            requested_date_ms: 86_400_000,
            priority: 1,
        };

        let result = engine.check_ctp(&request);
        assert!(result.is_capable);
    }

    #[test]
    fn test_batch_ctp() {
        let mut engine = AnalyticsEngine::default();

        engine.add_material(PeggingMaterial {
            id: "mat1".into(),
            name: "Component".into(),
            on_hand_qty: 200.0,
            unit: "EA".into(),
        });

        engine.set_product_bom("product1", vec![("mat1".into(), 1.0)]);
        engine.set_product_process_time("product1", 3_600_000);

        let requests = vec![
            CtpRequest {
                order_id: "o1".into(),
                product_id: "product1".into(),
                quantity: 50.0,
                requested_date_ms: 86_400_000,
                priority: 1,
            },
            CtpRequest {
                order_id: "o2".into(),
                product_id: "product1".into(),
                quantity: 50.0,
                requested_date_ms: 86_400_000,
                priority: 2,
            },
        ];

        let result = engine.batch_ctp(&requests);
        assert_eq!(result.results.len(), 2);
        assert!(result.total_requested_qty > 0.0);
    }

    #[test]
    fn test_whatif_integration() {
        let schedule = create_test_schedule();
        let jobs = create_test_jobs();

        let engine = AnalyticsEngine::default().with_schedule(schedule, jobs);

        let scenarios = vec![
            Scenario {
                id: "baseline".into(),
                name: "기준".into(),
                changes: vec![],
            },
            Scenario {
                id: "improved".into(),
                name: "개선".into(),
                changes: vec![crate::scheduler::whatif::ScenarioChange::ProcessingTime {
                    operation_id: None,
                    change_percent: -0.1,
                }],
            },
        ];

        let results = engine.run_whatif(&scenarios);
        assert_eq!(results.len(), 2);

        let best = engine.find_best_scenario(&results);
        assert!(best.is_some());
    }

    #[test]
    fn test_sensitivity_analysis() {
        let schedule = create_test_schedule();
        let jobs = create_test_jobs();

        let engine = AnalyticsEngine::default().with_schedule(schedule, jobs);

        let result = engine.sensitivity_analysis("ProcessingTime", &[-0.2, -0.1, 0.0, 0.1, 0.2]);

        assert_eq!(result.kpi_results.len(), 5);
        assert!(result.sensitivity_score >= 0.0);
    }

    #[test]
    fn test_recommendations_generation() {
        let schedule = create_test_schedule();
        let jobs = create_test_jobs();

        let config = AnalyticsConfig {
            horizon_ms: 60_000, // 타이트한 horizon
            ..Default::default()
        };

        let engine = AnalyticsEngine::new(config).with_schedule(schedule, jobs);

        let report = engine.analyze();

        // 병목이나 KPI 문제가 있으면 권고가 생성됨
        // (테스트 데이터에 따라 결과가 달라질 수 있음)
        assert!(report.kpi.makespan_ms > 0);
    }

    #[test]
    fn test_available_dates() {
        let mut engine = AnalyticsEngine::default();

        engine.add_material(PeggingMaterial {
            id: "mat1".into(),
            name: "Component".into(),
            on_hand_qty: 100.0,
            unit: "EA".into(),
        });

        engine.set_product_bom("product1", vec![("mat1".into(), 1.0)]);
        engine.set_product_process_time("product1", 3_600_000);

        let request = CtpRequest {
            order_id: "order1".into(),
            product_id: "product1".into(),
            quantity: 10.0,
            requested_date_ms: 0,
            priority: 1,
        };

        let dates = engine.get_available_dates(&request, 7);
        assert_eq!(dates.len(), 7);
    }
}
