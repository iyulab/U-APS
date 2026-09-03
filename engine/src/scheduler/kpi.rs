//! KPI - Key Performance Indicators
//!
//! 스케줄 품질 측정 지표

use crate::models::Job;
use crate::scheduler::Schedule;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 스케줄 KPI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleKpi {
    /// Makespan (전체 완료 시간)
    pub makespan_ms: i64,
    /// 총 지연 시간
    pub total_tardiness_ms: i64,
    /// 최대 지연 시간
    pub max_tardiness_ms: i64,
    /// 납기 준수율 (0.0 ~ 1.0)
    pub on_time_delivery_rate: f64,
    /// 평균 가동률 (0.0 ~ 1.0)
    pub average_utilization: f64,
    /// 자원별 가동률
    pub resource_utilization: HashMap<String, f64>,
    /// 평균 대기 시간
    pub average_flow_time_ms: f64,
    /// MCE (Manufacturing Cycle Efficiency)
    pub mce: f64,
    /// 병목 자원 ID
    pub bottleneck_resource: Option<String>,
}

/// KPI 계산기
pub struct KpiCalculator {
    /// 계획 수평선 (총 가용 시간)
    horizon_ms: i64,
}

impl KpiCalculator {
    pub fn new(horizon_ms: i64) -> Self {
        Self { horizon_ms }
    }

    /// 스케줄 KPI 계산
    pub fn calculate(&self, schedule: &Schedule, jobs: &[Job]) -> ScheduleKpi {
        let makespan = self.calculate_makespan(schedule);
        let (total_tard, max_tard, on_time) = self.calculate_tardiness(schedule, jobs);
        let (avg_util, resource_util, bottleneck) = self.calculate_utilization(schedule);
        let avg_flow = self.calculate_average_flow_time(schedule, jobs);
        let mce = self.calculate_mce(schedule, jobs);

        ScheduleKpi {
            makespan_ms: makespan,
            total_tardiness_ms: total_tard,
            max_tardiness_ms: max_tard,
            on_time_delivery_rate: on_time,
            average_utilization: avg_util,
            resource_utilization: resource_util,
            average_flow_time_ms: avg_flow,
            mce,
            bottleneck_resource: bottleneck,
        }
    }

    /// Makespan 계산
    fn calculate_makespan(&self, schedule: &Schedule) -> i64 {
        schedule
            .assignments
            .iter()
            .map(|a| a.end_ms)
            .max()
            .unwrap_or(0)
    }

    /// 지연 관련 지표 계산
    fn calculate_tardiness(&self, schedule: &Schedule, jobs: &[Job]) -> (i64, i64, f64) {
        let mut total_tardiness = 0i64;
        let mut max_tardiness = 0i64;
        let mut on_time_count = 0usize;
        let mut job_count = 0usize;

        // Job별 완료 시간 계산
        let mut job_completion: HashMap<String, i64> = HashMap::new();
        for assign in &schedule.assignments {
            let entry = job_completion.entry(assign.job_id.clone()).or_insert(0);
            *entry = (*entry).max(assign.end_ms);
        }

        // 각 Job의 지연 계산
        for job in jobs {
            if let Some(&completion) = job_completion.get(&job.id) {
                if let Some(due_date) = &job.due_date {
                    let due_ms = due_date.timestamp_millis();
                    let tardiness = (completion - due_ms).max(0);
                    total_tardiness += tardiness;
                    max_tardiness = max_tardiness.max(tardiness);

                    if tardiness == 0 {
                        on_time_count += 1;
                    }
                    job_count += 1;
                }
            }
        }

        let on_time_rate = if job_count > 0 {
            on_time_count as f64 / job_count as f64
        } else {
            1.0
        };

        (total_tardiness, max_tardiness, on_time_rate)
    }

    /// 가동률 계산
    fn calculate_utilization(
        &self,
        schedule: &Schedule,
    ) -> (f64, HashMap<String, f64>, Option<String>) {
        let mut resource_busy_time: HashMap<String, i64> = HashMap::new();

        // 각 자원의 총 작업 시간 계산
        for assign in &schedule.assignments {
            let busy_time = assign.end_ms - assign.start_ms;
            *resource_busy_time
                .entry(assign.resource_id.clone())
                .or_insert(0) += busy_time;
        }

        // 가동률 계산
        let mut resource_utilization = HashMap::new();
        let mut max_util = 0.0;
        let mut bottleneck = None;

        for (resource_id, busy_time) in &resource_busy_time {
            let utilization = if self.horizon_ms > 0 {
                *busy_time as f64 / self.horizon_ms as f64
            } else {
                0.0
            };
            resource_utilization.insert(resource_id.clone(), utilization);

            if utilization > max_util {
                max_util = utilization;
                bottleneck = Some(resource_id.clone());
            }
        }

        let avg_utilization = if !resource_utilization.is_empty() {
            resource_utilization.values().sum::<f64>() / resource_utilization.len() as f64
        } else {
            0.0
        };

        (avg_utilization, resource_utilization, bottleneck)
    }

    /// 평균 Flow Time 계산
    fn calculate_average_flow_time(&self, schedule: &Schedule, jobs: &[Job]) -> f64 {
        // Job별 시작/완료 시간
        let mut job_start: HashMap<String, i64> = HashMap::new();
        let mut job_end: HashMap<String, i64> = HashMap::new();

        for assign in &schedule.assignments {
            let start_entry = job_start.entry(assign.job_id.clone()).or_insert(i64::MAX);
            *start_entry = (*start_entry).min(assign.start_ms);

            let end_entry = job_end.entry(assign.job_id.clone()).or_insert(0);
            *end_entry = (*end_entry).max(assign.end_ms);
        }

        // Flow Time = 완료시간 - 시작시간
        let mut total_flow = 0i64;
        let mut count = 0;

        for job in jobs {
            if let (Some(&start), Some(&end)) = (job_start.get(&job.id), job_end.get(&job.id)) {
                total_flow += end - start;
                count += 1;
            }
        }

        if count > 0 {
            total_flow as f64 / count as f64
        } else {
            0.0
        }
    }

    /// MCE (Manufacturing Cycle Efficiency) 계산
    /// MCE = 부가가치 시간 / 총 리드타임
    fn calculate_mce(&self, schedule: &Schedule, _jobs: &[Job]) -> f64 {
        let mut total_process_time = 0i64;
        let mut total_lead_time = 0i64;

        // Job별 시작/완료 시간
        let mut job_times: HashMap<String, (i64, i64)> = HashMap::new();

        for assign in &schedule.assignments {
            let process_time = assign.end_ms - assign.start_ms - assign.setup_ms;
            total_process_time += process_time;

            let entry = job_times
                .entry(assign.job_id.clone())
                .or_insert((i64::MAX, 0));
            entry.0 = entry.0.min(assign.start_ms);
            entry.1 = entry.1.max(assign.end_ms);
        }

        // 총 리드타임
        for (start, end) in job_times.values() {
            total_lead_time += end - start;
        }

        if total_lead_time > 0 {
            total_process_time as f64 / total_lead_time as f64
        } else {
            0.0
        }
    }
}

/// 병목 분석 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BottleneckAnalysis {
    /// 병목 자원 ID
    pub resource_id: String,
    /// 가동률
    pub utilization: f64,
    /// 대기 중인 공정 수
    pub queue_length: usize,
    /// 평균 대기 시간
    pub avg_wait_time_ms: f64,
    /// 개선 제안
    pub suggestions: Vec<String>,
}

/// 병목 분석기
pub fn analyze_bottlenecks(schedule: &Schedule, horizon_ms: i64) -> Vec<BottleneckAnalysis> {
    let calculator = KpiCalculator::new(horizon_ms);
    let (_, resource_util, _) = calculator.calculate_utilization(schedule);

    let mut analyses = Vec::new();

    for (resource_id, utilization) in resource_util {
        if utilization > 0.8 {
            // 80% 이상 가동률이면 병목 후보
            let mut suggestions = Vec::new();

            if utilization > 0.95 {
                suggestions.push("자원 추가 고려".into());
                suggestions.push("대체 자원 활용 검토".into());
            } else if utilization > 0.85 {
                suggestions.push("작업 순서 최적화".into());
                suggestions.push("Setup Time 단축".into());
            }

            analyses.push(BottleneckAnalysis {
                resource_id,
                utilization,
                queue_length: 0,       // 계산 필요
                avg_wait_time_ms: 0.0, // 계산 필요
                suggestions,
            });
        }
    }

    // 가동률 순으로 정렬
    analyses.sort_by(|a, b| {
        b.utilization
            .partial_cmp(&a.utilization)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    analyses
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
                    job_id: "job1".into(),
                    resource_id: "m2".into(),
                    start_ms: 30_000,
                    end_ms: 60_000,
                    setup_ms: 0,
                    site_id: None,
                },
                Assignment {
                    operation_id: "op3".into(),
                    job_id: "job2".into(),
                    resource_id: "m1".into(),
                    start_ms: 30_000,
                    end_ms: 50_000,
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
                .with_due_date(Utc.timestamp_millis_opt(40_000).unwrap()), // 지연됨
        ]
    }

    #[test]
    fn test_makespan_calculation() {
        let schedule = create_test_schedule();
        let calculator = KpiCalculator::new(100_000);

        let kpi = calculator.calculate(&schedule, &[]);
        assert_eq!(kpi.makespan_ms, 60_000);
    }

    #[test]
    fn test_tardiness_calculation() {
        let schedule = create_test_schedule();
        let jobs = create_test_jobs();
        let calculator = KpiCalculator::new(100_000);

        let kpi = calculator.calculate(&schedule, &jobs);

        // job2: 완료 50_000, 납기 40_000 → 지연 10_000
        assert_eq!(kpi.total_tardiness_ms, 10_000);
        assert_eq!(kpi.max_tardiness_ms, 10_000);
        assert_eq!(kpi.on_time_delivery_rate, 0.5); // 1/2
    }

    #[test]
    fn test_utilization_calculation() {
        let schedule = create_test_schedule();
        let calculator = KpiCalculator::new(100_000);

        let kpi = calculator.calculate(&schedule, &[]);

        // m1: 30_000 + 20_000 = 50_000 / 100_000 = 0.5
        // m2: 30_000 / 100_000 = 0.3
        assert!(kpi.resource_utilization.contains_key("m1"));
        assert!(kpi.resource_utilization.contains_key("m2"));
        assert_eq!(kpi.bottleneck_resource, Some("m1".into()));
    }

    #[test]
    fn test_bottleneck_analysis() {
        let schedule = create_test_schedule();
        let analyses = analyze_bottlenecks(&schedule, 60_000); // horizon = makespan

        // m1: 50_000/60_000 ≈ 0.83 → 병목
        assert!(!analyses.is_empty());
    }

    #[test]
    fn test_mce_calculation() {
        let schedule = create_test_schedule();
        let jobs = create_test_jobs();
        let calculator = KpiCalculator::new(100_000);

        let kpi = calculator.calculate(&schedule, &jobs);

        // MCE = 처리시간 / 리드타임
        assert!(kpi.mce > 0.0);
        assert!(kpi.mce <= 1.0);
    }
}
