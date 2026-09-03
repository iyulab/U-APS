//! What-If Analysis - 시나리오 비교 분석
//!
//! 스케줄 시나리오 시뮬레이션 및 비교

// HashMap available if needed
use crate::models::Job;
use crate::scheduler::{KpiCalculator, Schedule, ScheduleKpi};
use serde::{Deserialize, Serialize};

/// 시나리오 변경 유형
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScenarioChange {
    /// 자원 용량 변경
    ResourceCapacity {
        resource_id: String,
        change_percent: f64, // +10% = 0.1
    },
    /// 처리 시간 변경
    ProcessingTime {
        operation_id: Option<String>, // None = 전체
        change_percent: f64,
    },
    /// 주문 추가
    AddOrder { job: Job },
    /// 주문 삭제
    RemoveOrder { job_id: String },
    /// 우선순위 변경
    PriorityChange { job_id: String, new_priority: i32 },
    /// 납기 변경
    DueDateChange {
        job_id: String,
        new_due_date_ms: i64,
    },
    /// 자원 추가
    AddResource {
        resource_id: String,
        capacity_ms: i64,
    },
    /// 자원 제거
    RemoveResource { resource_id: String },
}

/// 시나리오
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    /// 시나리오 ID
    pub id: String,
    /// 시나리오 이름
    pub name: String,
    /// 적용할 변경 사항
    pub changes: Vec<ScenarioChange>,
}

/// 시나리오 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioResult {
    /// 시나리오 ID
    pub scenario_id: String,
    /// 적용된 스케줄
    pub schedule: Schedule,
    /// KPI
    pub kpi: ScheduleKpi,
}

/// 시나리오 비교 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonResult {
    /// 기준 시나리오 ID
    pub baseline_id: String,
    /// 비교 시나리오 ID
    pub compare_id: String,
    /// KPI 차이
    pub kpi_diff: KpiDiff,
    /// 개선/악화 요약
    pub summary: ComparisonSummary,
}

/// KPI 차이
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KpiDiff {
    /// Makespan 변화 (ms)
    pub makespan_diff_ms: i64,
    /// Makespan 변화율 (%)
    pub makespan_diff_percent: f64,
    /// 총 지연 변화
    pub tardiness_diff_ms: i64,
    /// 납기 준수율 변화
    pub on_time_rate_diff: f64,
    /// 가동률 변화
    pub utilization_diff: f64,
}

/// 비교 요약
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonSummary {
    /// 전체 평가 (개선/악화/동일)
    pub overall: ChangeAssessment,
    /// 주요 개선 항목
    pub improvements: Vec<String>,
    /// 주요 악화 항목
    pub degradations: Vec<String>,
    /// 권고 사항
    pub recommendations: Vec<String>,
}

/// 변화 평가
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ChangeAssessment {
    Improved,
    Degraded,
    Neutral,
}

/// What-If 분석 엔진
pub struct WhatIfEngine {
    /// 기준 스케줄
    baseline_schedule: Schedule,
    /// 기준 Jobs
    baseline_jobs: Vec<Job>,
    /// 계획 수평선
    horizon_ms: i64,
}

impl WhatIfEngine {
    pub fn new(baseline_schedule: Schedule, baseline_jobs: Vec<Job>, horizon_ms: i64) -> Self {
        Self {
            baseline_schedule,
            baseline_jobs,
            horizon_ms,
        }
    }

    /// 시나리오 적용
    pub fn apply_scenario(&self, scenario: &Scenario) -> ScenarioResult {
        let mut schedule = self.baseline_schedule.clone();
        let mut jobs = self.baseline_jobs.clone();

        // 변경 사항 적용
        for change in &scenario.changes {
            match change {
                ScenarioChange::ProcessingTime {
                    operation_id,
                    change_percent,
                } => {
                    self.apply_processing_time_change(&mut schedule, operation_id, *change_percent);
                }
                ScenarioChange::RemoveOrder { job_id } => {
                    schedule.assignments.retain(|a| a.job_id != *job_id);
                    jobs.retain(|j| j.id != *job_id);
                }
                ScenarioChange::PriorityChange {
                    job_id,
                    new_priority,
                } => {
                    if let Some(job) = jobs.iter_mut().find(|j| j.id == *job_id) {
                        job.priority = *new_priority;
                    }
                }
                ScenarioChange::DueDateChange {
                    job_id,
                    new_due_date_ms,
                } => {
                    if let Some(job) = jobs.iter_mut().find(|j| j.id == *job_id) {
                        use chrono::{TimeZone, Utc};
                        job.due_date = Some(Utc.timestamp_millis_opt(*new_due_date_ms).unwrap());
                    }
                }
                _ => {
                    // 다른 변경은 재스케줄링 필요
                }
            }
        }

        // Makespan 재계산
        schedule.makespan_ms = schedule
            .assignments
            .iter()
            .map(|a| a.end_ms)
            .max()
            .unwrap_or(0);

        // KPI 계산
        let calculator = KpiCalculator::new(self.horizon_ms);
        let kpi = calculator.calculate(&schedule, &jobs);

        ScenarioResult {
            scenario_id: scenario.id.clone(),
            schedule,
            kpi,
        }
    }

    /// 시나리오 비교
    pub fn compare_scenarios(
        &self,
        baseline: &ScenarioResult,
        compare: &ScenarioResult,
    ) -> ComparisonResult {
        let kpi_diff = self.calculate_kpi_diff(&baseline.kpi, &compare.kpi);
        let summary = self.generate_summary(&kpi_diff);

        ComparisonResult {
            baseline_id: baseline.scenario_id.clone(),
            compare_id: compare.scenario_id.clone(),
            kpi_diff,
            summary,
        }
    }

    /// 다중 시나리오 분석
    pub fn analyze_scenarios(&self, scenarios: &[Scenario]) -> Vec<ScenarioResult> {
        scenarios.iter().map(|s| self.apply_scenario(s)).collect()
    }

    /// 최적 시나리오 선택
    pub fn find_best_scenario<'a>(
        &self,
        results: &'a [ScenarioResult],
        objective: WhatIfObjective,
    ) -> Option<&'a ScenarioResult> {
        results.iter().min_by(|a, b| {
            let score_a = self.calculate_score(&a.kpi, &objective);
            let score_b = self.calculate_score(&b.kpi, &objective);
            score_a
                .partial_cmp(&score_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    // 내부 헬퍼 함수들

    fn apply_processing_time_change(
        &self,
        schedule: &mut Schedule,
        operation_id: &Option<String>,
        change_percent: f64,
    ) {
        for assign in &mut schedule.assignments {
            let should_apply = match operation_id {
                Some(id) => assign.operation_id == *id,
                None => true,
            };

            if should_apply {
                let duration = assign.end_ms - assign.start_ms;
                let new_duration = (duration as f64 * (1.0 + change_percent)) as i64;
                assign.end_ms = assign.start_ms + new_duration;
            }
        }
    }

    fn calculate_kpi_diff(&self, baseline: &ScheduleKpi, compare: &ScheduleKpi) -> KpiDiff {
        let makespan_diff = compare.makespan_ms - baseline.makespan_ms;
        let makespan_percent = if baseline.makespan_ms > 0 {
            (makespan_diff as f64 / baseline.makespan_ms as f64) * 100.0
        } else {
            0.0
        };

        KpiDiff {
            makespan_diff_ms: makespan_diff,
            makespan_diff_percent: makespan_percent,
            tardiness_diff_ms: compare.total_tardiness_ms - baseline.total_tardiness_ms,
            on_time_rate_diff: compare.on_time_delivery_rate - baseline.on_time_delivery_rate,
            utilization_diff: compare.average_utilization - baseline.average_utilization,
        }
    }

    fn generate_summary(&self, diff: &KpiDiff) -> ComparisonSummary {
        let mut improvements = Vec::new();
        let mut degradations = Vec::new();
        let mut recommendations = Vec::new();

        // Makespan 평가
        if diff.makespan_diff_ms < 0 {
            improvements.push(format!(
                "Makespan {:.1}% 감소",
                diff.makespan_diff_percent.abs()
            ));
        } else if diff.makespan_diff_ms > 0 {
            degradations.push(format!("Makespan {:.1}% 증가", diff.makespan_diff_percent));
        }

        // 납기 준수율 평가
        if diff.on_time_rate_diff > 0.0 {
            improvements.push(format!(
                "납기 준수율 {:.1}% 개선",
                diff.on_time_rate_diff * 100.0
            ));
        } else if diff.on_time_rate_diff < 0.0 {
            degradations.push(format!(
                "납기 준수율 {:.1}% 악화",
                diff.on_time_rate_diff.abs() * 100.0
            ));
        }

        // 가동률 평가
        if diff.utilization_diff > 0.05 {
            improvements.push("자원 가동률 개선".into());
        } else if diff.utilization_diff < -0.05 {
            degradations.push("자원 가동률 저하".into());
        }

        // 전체 평가
        let overall = if improvements.len() > degradations.len() {
            ChangeAssessment::Improved
        } else if degradations.len() > improvements.len() {
            ChangeAssessment::Degraded
        } else {
            ChangeAssessment::Neutral
        };

        // 권고 사항
        if diff.makespan_diff_percent > 10.0 {
            recommendations.push("Makespan 증가 원인 분석 필요".into());
        }
        if diff.on_time_rate_diff < -0.1 {
            recommendations.push("납기 지연 공정 검토 필요".into());
        }

        ComparisonSummary {
            overall,
            improvements,
            degradations,
            recommendations,
        }
    }

    fn calculate_score(&self, kpi: &ScheduleKpi, objective: &WhatIfObjective) -> f64 {
        match objective {
            WhatIfObjective::MinimizeMakespan => kpi.makespan_ms as f64,
            WhatIfObjective::MinimizeTardiness => kpi.total_tardiness_ms as f64,
            WhatIfObjective::MaximizeOnTime => -kpi.on_time_delivery_rate,
            WhatIfObjective::MaximizeUtilization => -kpi.average_utilization,
            WhatIfObjective::Weighted {
                makespan,
                tardiness,
                on_time,
                utilization,
            } => {
                makespan * kpi.makespan_ms as f64 + tardiness * kpi.total_tardiness_ms as f64
                    - on_time * kpi.on_time_delivery_rate
                    - utilization * kpi.average_utilization
            }
        }
    }
}

/// What-If 목적 함수
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WhatIfObjective {
    MinimizeMakespan,
    MinimizeTardiness,
    MaximizeOnTime,
    MaximizeUtilization,
    Weighted {
        makespan: f64,
        tardiness: f64,
        on_time: f64,
        utilization: f64,
    },
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
                    setup_ms: 0,
                    site_id: None,
                },
                Assignment {
                    operation_id: "op2".into(),
                    job_id: "job2".into(),
                    resource_id: "m1".into(),
                    start_ms: 30_000,
                    end_ms: 60_000,
                    setup_ms: 0,
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
                .with_due_date(Utc.timestamp_millis_opt(50_000).unwrap()),
            Job::new("job2")
                .with_priority(2)
                .with_due_date(Utc.timestamp_millis_opt(80_000).unwrap()),
        ]
    }

    #[test]
    fn test_scenario_apply() {
        let schedule = create_test_schedule();
        let jobs = create_test_jobs();
        let engine = WhatIfEngine::new(schedule, jobs, 100_000);

        let scenario = Scenario {
            id: "s1".into(),
            name: "10% 처리시간 증가".into(),
            changes: vec![ScenarioChange::ProcessingTime {
                operation_id: None,
                change_percent: 0.1,
            }],
        };

        let result = engine.apply_scenario(&scenario);

        // 처리시간 10% 증가
        assert!(result.schedule.makespan_ms > 60_000);
    }

    #[test]
    fn test_scenario_comparison() {
        let schedule = create_test_schedule();
        let jobs = create_test_jobs();
        let engine = WhatIfEngine::new(schedule.clone(), jobs.clone(), 100_000);

        let baseline = Scenario {
            id: "baseline".into(),
            name: "기준".into(),
            changes: vec![],
        };

        let improved = Scenario {
            id: "improved".into(),
            name: "개선".into(),
            changes: vec![ScenarioChange::ProcessingTime {
                operation_id: None,
                change_percent: -0.2, // 20% 감소
            }],
        };

        let baseline_result = engine.apply_scenario(&baseline);
        let improved_result = engine.apply_scenario(&improved);

        let comparison = engine.compare_scenarios(&baseline_result, &improved_result);

        assert!(comparison.kpi_diff.makespan_diff_ms < 0);
    }

    #[test]
    fn test_best_scenario() {
        let schedule = create_test_schedule();
        let jobs = create_test_jobs();
        let engine = WhatIfEngine::new(schedule, jobs, 100_000);

        let scenarios = vec![
            Scenario {
                id: "s1".into(),
                name: "기준".into(),
                changes: vec![],
            },
            Scenario {
                id: "s2".into(),
                name: "10% 감소".into(),
                changes: vec![ScenarioChange::ProcessingTime {
                    operation_id: None,
                    change_percent: -0.1,
                }],
            },
        ];

        let results = engine.analyze_scenarios(&scenarios);
        let best = engine.find_best_scenario(&results, WhatIfObjective::MinimizeMakespan);

        assert!(best.is_some());
        assert_eq!(best.unwrap().scenario_id, "s2");
    }
}
