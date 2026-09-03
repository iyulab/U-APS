//! GA 파라미터 자동 튜닝
//!
//! 문제 특성 분석 기반 파라미터 최적화

use crate::ga::{GaParams, GeneticOperators};
use crate::models::{Job, Resource, ResourceKind};

/// 문제 특성 분석 결과
#[derive(Debug, Clone)]
pub struct ProblemCharacteristics {
    /// 총 공정 수
    pub operation_count: usize,
    /// 총 작업 수
    pub job_count: usize,
    /// 총 자원 수
    pub resource_count: usize,
    /// 장비 수
    pub equipment_count: usize,
    /// 작업자 수
    pub worker_count: usize,
    /// 평균 공정 수/작업
    pub avg_operations_per_job: f64,
    /// 자원 유연성 (공정당 평균 후보 자원 수)
    pub resource_flexibility: f64,
    /// 납기 압박도 (0.0 ~ 1.0)
    pub due_date_pressure: f64,
    /// 우선순위 다양성
    pub priority_diversity: f64,
}

impl ProblemCharacteristics {
    /// 문제 특성 분석
    pub fn analyze(jobs: &[Job], resources: &[Resource]) -> Self {
        let operation_count: usize = jobs.iter().map(|j| j.operations.len()).sum();
        let job_count = jobs.len();
        let resource_count = resources.len();

        let equipment_count = resources
            .iter()
            .filter(|r| r.kind == ResourceKind::Equipment)
            .count();
        let worker_count = resources
            .iter()
            .filter(|r| r.kind == ResourceKind::Worker)
            .count();

        let avg_operations_per_job = if job_count > 0 {
            operation_count as f64 / job_count as f64
        } else {
            0.0
        };

        // 자원 유연성 계산
        let mut total_candidates = 0;
        let mut total_requirements = 0;
        for job in jobs {
            for op in &job.operations {
                for req in &op.required_resources {
                    total_candidates += req.candidates.len();
                    total_requirements += 1;
                }
            }
        }
        let resource_flexibility = if total_requirements > 0 {
            total_candidates as f64 / total_requirements as f64
        } else {
            1.0
        };

        // 납기 압박도 계산 (납기 있는 작업 비율 + 긴급도)
        let jobs_with_due_date = jobs.iter().filter(|j| j.due_date.is_some()).count();
        let due_date_pressure = if job_count > 0 {
            jobs_with_due_date as f64 / job_count as f64
        } else {
            0.0
        };

        // 우선순위 다양성 (표준편차 기반)
        let priorities: Vec<i32> = jobs.iter().map(|j| j.priority).collect();
        let priority_diversity = if priorities.len() > 1 {
            let mean = priorities.iter().sum::<i32>() as f64 / priorities.len() as f64;
            let variance = priorities
                .iter()
                .map(|&p| (p as f64 - mean).powi(2))
                .sum::<f64>()
                / priorities.len() as f64;
            variance.sqrt() / 100.0 // 정규화
        } else {
            0.0
        };

        Self {
            operation_count,
            job_count,
            resource_count,
            equipment_count,
            worker_count,
            avg_operations_per_job,
            resource_flexibility,
            due_date_pressure,
            priority_diversity,
        }
    }

    /// 복잡도 점수 (0.0 ~ 1.0)
    pub fn complexity_score(&self) -> f64 {
        let size_factor = (self.operation_count as f64 / 500.0).min(1.0);
        let flexibility_factor = (1.0 - self.resource_flexibility / 10.0).max(0.0);
        let pressure_factor = self.due_date_pressure;

        (size_factor * 0.5 + flexibility_factor * 0.3 + pressure_factor * 0.2).min(1.0)
    }
}

/// GA 파라미터 자동 튜너
pub struct GaAutoTuner;

impl GaAutoTuner {
    /// 문제 특성에 맞는 GA 파라미터 자동 선택
    pub fn tune(characteristics: &ProblemCharacteristics) -> GaParams {
        let _complexity = characteristics.complexity_score();

        // 기본 파라미터 결정
        let (population_size, max_generations) = Self::size_params(characteristics);
        let elite_ratio = Self::elite_ratio(characteristics);
        let tournament_size = Self::tournament_size(characteristics);
        let (convergence_gens, convergence_thresh) = Self::convergence_params(characteristics);
        let time_limit = Self::time_limit(characteristics);

        GaParams {
            population_size,
            max_generations,
            elite_ratio,
            tournament_size,
            convergence_generations: convergence_gens,
            convergence_threshold: convergence_thresh,
            time_limit_ms: Some(time_limit),
        }
    }

    /// 문제 특성에 맞는 유전 연산자 선택
    pub fn tune_operators(characteristics: &ProblemCharacteristics) -> GeneticOperators {
        use crate::ga::{CrossoverType, MutationType};

        let complexity = characteristics.complexity_score();

        // 복잡한 문제: POX + Insert (더 탐색적)
        // 단순한 문제: JOX + Swap (더 안정적)
        let crossover_type = if complexity > 0.7 {
            CrossoverType::POX
        } else if complexity > 0.4 {
            CrossoverType::LOX
        } else {
            CrossoverType::JOX
        };

        let mutation_type = if complexity > 0.5 {
            MutationType::Insert
        } else {
            MutationType::Swap
        };

        GeneticOperators {
            crossover_type,
            mutation_type,
        }
    }

    fn size_params(chars: &ProblemCharacteristics) -> (usize, usize) {
        match chars.operation_count {
            0..=30 => (40, 80),
            31..=80 => (60, 150),
            81..=150 => (100, 300),
            151..=300 => (120, 400),
            _ => (150, 500),
        }
    }

    fn elite_ratio(chars: &ProblemCharacteristics) -> f64 {
        // 납기 압박이 높으면 elite 비율 증가 (안정적 수렴)
        if chars.due_date_pressure > 0.7 {
            0.15
        } else if chars.due_date_pressure > 0.3 {
            0.12
        } else {
            0.10
        }
    }

    fn tournament_size(chars: &ProblemCharacteristics) -> usize {
        // 자원 유연성이 높으면 tournament 크기 증가 (선택 압력 증가)
        if chars.resource_flexibility > 5.0 {
            5
        } else if chars.resource_flexibility > 2.0 {
            4
        } else {
            3
        }
    }

    fn convergence_params(chars: &ProblemCharacteristics) -> (usize, f64) {
        // 복잡한 문제는 더 오래 기다림
        let complexity = chars.complexity_score();

        if complexity > 0.7 {
            (60, 0.0005)
        } else if complexity > 0.4 {
            (40, 0.001)
        } else {
            (25, 0.002)
        }
    }

    fn time_limit(chars: &ProblemCharacteristics) -> i64 {
        // 공정 수 기반 시간 제한 (ms)
        match chars.operation_count {
            0..=30 => 5_000,
            31..=80 => 15_000,
            81..=150 => 30_000,
            151..=300 => 45_000,
            _ => 60_000,
        }
    }
}

/// 튜닝 결과 보고서
#[derive(Debug, Clone)]
pub struct TuningReport {
    pub characteristics: ProblemCharacteristics,
    pub params: GaParams,
    pub operators: GeneticOperators,
    pub complexity_score: f64,
    pub estimated_time_seconds: f64,
}

impl TuningReport {
    pub fn generate(jobs: &[Job], resources: &[Resource]) -> Self {
        let characteristics = ProblemCharacteristics::analyze(jobs, resources);
        let params = GaAutoTuner::tune(&characteristics);
        let operators = GaAutoTuner::tune_operators(&characteristics);
        let complexity_score = characteristics.complexity_score();
        let estimated_time_seconds = params.time_limit_ms.unwrap_or(30_000) as f64 / 1000.0;

        Self {
            characteristics,
            params,
            operators,
            complexity_score,
            estimated_time_seconds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Job, Operation, Resource};

    fn create_test_job(id: &str, op_count: usize) -> Job {
        let mut job = Job::new(id).with_priority(100);

        for i in 0..op_count {
            let op = Operation::new(format!("{}-OP{}", id, i), id, i as i32 + 1)
                .with_time(300_000, 600_000, 0)
                .with_equipment(vec!["EQP-1".to_string(), "EQP-2".to_string()]);
            job = job.with_operation(op);
        }

        job
    }

    #[test]
    fn test_small_problem_tuning() {
        let jobs: Vec<Job> = (0..5)
            .map(|i| create_test_job(&format!("JOB-{}", i), 2))
            .collect();
        let resources = vec![Resource::equipment("EQP-1"), Resource::equipment("EQP-2")];

        let report = TuningReport::generate(&jobs, &resources);

        assert_eq!(report.characteristics.operation_count, 10);
        assert!(report.params.population_size <= 60);
        assert!(report.params.max_generations <= 150);
    }

    #[test]
    fn test_large_problem_tuning() {
        let jobs: Vec<Job> = (0..50)
            .map(|i| create_test_job(&format!("JOB-{}", i), 5))
            .collect();
        let resources = vec![Resource::equipment("EQP-1"), Resource::equipment("EQP-2")];

        let report = TuningReport::generate(&jobs, &resources);

        assert_eq!(report.characteristics.operation_count, 250);
        assert!(report.params.population_size >= 120);
        assert!(report.params.max_generations >= 400);
    }
}
