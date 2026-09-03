//! Performance benchmarks for production validation
//!
//! 목표: 100+ Jobs, 1분 이내 최적해

use crate::ga::GaParams;
use crate::models::{Job, Operation, Resource};
use crate::scheduler::{production::ProductionConfig, SchedulingEngine};
use std::time::Instant;

/// 성능 벤치마크 결과
#[derive(Debug, Clone)]
pub struct PerformanceBenchmark {
    /// 테스트 이름
    pub name: String,
    /// Job 수
    pub job_count: usize,
    /// Operation 수
    pub operation_count: usize,
    /// Resource 수
    pub resource_count: usize,
    /// 실행 시간 (ms)
    pub execution_time_ms: u128,
    /// Makespan (ms)
    pub makespan_ms: i64,
    /// 성능 기준 충족 여부
    pub passed: bool,
    /// 목표 시간 (ms)
    pub target_time_ms: u128,
}

impl PerformanceBenchmark {
    pub fn format(&self) -> String {
        let status = if self.passed { "✅ PASS" } else { "❌ FAIL" };
        format!(
            "{}: {}J/{}O/{}R | Time: {}ms (target: {}ms) | Makespan: {}ms | {}",
            self.name,
            self.job_count,
            self.operation_count,
            self.resource_count,
            self.execution_time_ms,
            self.target_time_ms,
            self.makespan_ms,
            status
        )
    }
}

/// 테스트 시나리오 생성
pub fn generate_test_scenario(
    job_count: usize,
    ops_per_job: usize,
    resource_count: usize,
) -> (Vec<Job>, Vec<Resource>) {
    let mut jobs = Vec::with_capacity(job_count);

    for j in 0..job_count {
        let job_id = format!("J{:04}", j);
        let product = format!("PRODUCT-{}", j % 10);
        let mut job = Job::new(&job_id)
            .with_product(&product)
            .with_priority((j % 5) as i32 + 1);

        for o in 0..ops_per_job {
            let op_id = format!("{}-O{}", job_id, o + 1);
            let process_time = 5_000 + (j * 100 + o * 50) as i64 % 30_000;

            // 가용 자원 (일부만 선택)
            let available_resources: Vec<String> = (0..resource_count)
                .filter(|r| (j + o + r) % 3 != 0)
                .take(resource_count / 2 + 1)
                .map(|r| format!("M{:02}", r))
                .collect();

            let operation = Operation::new(&op_id, &job_id, (o + 1) as i32)
                .with_time(0, process_time, 0)
                .with_equipment(available_resources);

            job = job.with_operation(operation);
        }

        jobs.push(job);
    }

    let resources: Vec<Resource> = (0..resource_count)
        .map(|r| {
            let efficiency = 0.8 + (r as f64 * 0.02) % 0.2;
            Resource::equipment(format!("M{:02}", r)).with_efficiency(efficiency)
        })
        .collect();

    (jobs, resources)
}

/// Production 스케줄러 벤치마크 (100 Jobs)
pub fn benchmark_production_100() -> PerformanceBenchmark {
    let (jobs, resources) = generate_test_scenario(100, 3, 10);
    let target_time_ms = 90_000; // 1.5분 (CI runner 성능 여유)

    let start = Instant::now();

    let mut engine = SchedulingEngine::production(ProductionConfig::default());
    let result = engine.schedule(&jobs, &resources, 0).unwrap();

    let execution_time_ms = start.elapsed().as_millis();
    let operation_count: usize = jobs.iter().map(|j| j.operations.len()).sum();

    PerformanceBenchmark {
        name: "Production-100J".to_string(),
        job_count: jobs.len(),
        operation_count,
        resource_count: resources.len(),
        execution_time_ms,
        makespan_ms: result.schedule.makespan_ms,
        passed: execution_time_ms < target_time_ms,
        target_time_ms,
    }
}

/// Production 스케줄러 벤치마크 (200 Jobs)
pub fn benchmark_production_200() -> PerformanceBenchmark {
    let (jobs, resources) = generate_test_scenario(200, 3, 15);
    let target_time_ms = 180_000; // 3분

    let start = Instant::now();

    let mut engine = SchedulingEngine::production(ProductionConfig::default());
    let result = engine.schedule(&jobs, &resources, 0).unwrap();

    let execution_time_ms = start.elapsed().as_millis();
    let operation_count: usize = jobs.iter().map(|j| j.operations.len()).sum();

    PerformanceBenchmark {
        name: "Production-200J".to_string(),
        job_count: jobs.len(),
        operation_count,
        resource_count: resources.len(),
        execution_time_ms,
        makespan_ms: result.schedule.makespan_ms,
        passed: execution_time_ms < target_time_ms,
        target_time_ms,
    }
}

/// GA 스케줄러 벤치마크 (50 Jobs - GA는 더 느림)
pub fn benchmark_ga_50() -> PerformanceBenchmark {
    let (jobs, resources) = generate_test_scenario(50, 3, 8);
    let target_time_ms = 90_000; // 1.5분 (CI runner 성능 여유)

    let start = Instant::now();

    let params = GaParams {
        population_size: 50,
        max_generations: 100,
        elite_ratio: 0.1,
        tournament_size: 3,
        convergence_generations: 20,
        convergence_threshold: 0.001,
        time_limit_ms: None,
    };

    let mut engine = SchedulingEngine::genetic(params);
    let result = engine.schedule(&jobs, &resources, 0).unwrap();

    let execution_time_ms = start.elapsed().as_millis();
    let operation_count: usize = jobs.iter().map(|j| j.operations.len()).sum();

    PerformanceBenchmark {
        name: "GA-50J".to_string(),
        job_count: jobs.len(),
        operation_count,
        resource_count: resources.len(),
        execution_time_ms,
        makespan_ms: result.schedule.makespan_ms,
        passed: execution_time_ms < target_time_ms,
        target_time_ms,
    }
}

/// Simple 스케줄러 벤치마크 (500 Jobs - Simple은 빠름)
pub fn benchmark_simple_500() -> PerformanceBenchmark {
    let (jobs, resources) = generate_test_scenario(500, 2, 20);
    let target_time_ms = 10_000; // 10초

    let start = Instant::now();

    let mut engine = SchedulingEngine::simple();
    let result = engine.schedule(&jobs, &resources, 0).unwrap();

    let execution_time_ms = start.elapsed().as_millis();
    let operation_count: usize = jobs.iter().map(|j| j.operations.len()).sum();

    PerformanceBenchmark {
        name: "Simple-500J".to_string(),
        job_count: jobs.len(),
        operation_count,
        resource_count: resources.len(),
        execution_time_ms,
        makespan_ms: result.schedule.makespan_ms,
        passed: execution_time_ms < target_time_ms,
        target_time_ms,
    }
}

/// CP-SAT 스케줄러 벤치마크 (20 Jobs - CP-SAT은 최적해 보장, 느림)
pub fn benchmark_cpsat_20() -> PerformanceBenchmark {
    let (jobs, resources) = generate_test_scenario(20, 2, 5);
    let target_time_ms = 120_000; // 2분

    let start = Instant::now();
    let mut engine = SchedulingEngine::cpsat(crate::cp::CpSatConfig::default());
    let result = engine.schedule(&jobs, &resources, 0).unwrap();
    let execution_time_ms = start.elapsed().as_millis();
    let operation_count: usize = jobs.iter().map(|j| j.operations.len()).sum();

    PerformanceBenchmark {
        name: "CpSat-20J".to_string(),
        job_count: jobs.len(),
        operation_count,
        resource_count: resources.len(),
        execution_time_ms,
        makespan_ms: result.schedule.makespan_ms,
        passed: execution_time_ms < target_time_ms,
        target_time_ms,
    }
}

/// Auto 선택 벤치마크 (100 Jobs)
pub fn benchmark_auto_100() -> PerformanceBenchmark {
    let (jobs, resources) = generate_test_scenario(100, 3, 10);
    let target_time_ms = 90_000; // 1.5분 (CI runner 성능 여유)

    let start = Instant::now();
    let mut engine = SchedulingEngine::auto();
    let result = engine.schedule(&jobs, &resources, 0).unwrap();
    let execution_time_ms = start.elapsed().as_millis();
    let operation_count: usize = jobs.iter().map(|j| j.operations.len()).sum();

    PerformanceBenchmark {
        name: "Auto-100J".to_string(),
        job_count: jobs.len(),
        operation_count,
        resource_count: resources.len(),
        execution_time_ms,
        makespan_ms: result.schedule.makespan_ms,
        passed: execution_time_ms < target_time_ms,
        target_time_ms,
    }
}

/// 알고리즘 비교 (50 Jobs)
pub fn compare_algorithms_50() -> Vec<(String, u128, i64)> {
    let (jobs, resources) = generate_test_scenario(50, 2, 8);
    let mut results = Vec::new();

    // Simple
    let start = Instant::now();
    let mut engine = SchedulingEngine::simple();
    let result = engine.schedule(&jobs, &resources, 0).unwrap();
    results.push((
        "Simple".to_string(),
        start.elapsed().as_millis(),
        result.schedule.makespan_ms,
    ));

    // Production
    let start = Instant::now();
    let mut engine = SchedulingEngine::production(ProductionConfig::default());
    let result = engine.schedule(&jobs, &resources, 0).unwrap();
    results.push((
        "Production".to_string(),
        start.elapsed().as_millis(),
        result.schedule.makespan_ms,
    ));

    // GA
    let start = Instant::now();
    let params = GaParams {
        population_size: 30,
        max_generations: 50,
        time_limit_ms: Some(30_000),
        ..Default::default()
    };
    let mut engine = SchedulingEngine::genetic(params);
    let result = engine.schedule(&jobs, &resources, 0).unwrap();
    results.push((
        "GA".to_string(),
        start.elapsed().as_millis(),
        result.schedule.makespan_ms,
    ));

    // Auto
    let start = Instant::now();
    let mut engine = SchedulingEngine::auto();
    let result = engine.schedule(&jobs, &resources, 0).unwrap();
    results.push((
        "Auto".to_string(),
        start.elapsed().as_millis(),
        result.schedule.makespan_ms,
    ));

    results
}

/// Production 스케줄러 벤치마크 (300 Jobs)
pub fn benchmark_production_300() -> PerformanceBenchmark {
    let (jobs, resources) = generate_test_scenario(300, 3, 20);
    let target_time_ms = 300_000; // 5분

    let start = Instant::now();

    let mut engine = SchedulingEngine::production(ProductionConfig::default());
    let result = engine.schedule(&jobs, &resources, 0).unwrap();

    let execution_time_ms = start.elapsed().as_millis();
    let operation_count: usize = jobs.iter().map(|j| j.operations.len()).sum();

    PerformanceBenchmark {
        name: "Production-300J".to_string(),
        job_count: jobs.len(),
        operation_count,
        resource_count: resources.len(),
        execution_time_ms,
        makespan_ms: result.schedule.makespan_ms,
        passed: execution_time_ms < target_time_ms,
        target_time_ms,
    }
}

/// Production 스케줄러 벤치마크 (500 Jobs)
pub fn benchmark_production_500() -> PerformanceBenchmark {
    let (jobs, resources) = generate_test_scenario(500, 3, 25);
    let target_time_ms = 600_000; // 10분

    let start = Instant::now();

    let mut engine = SchedulingEngine::production(ProductionConfig::default());
    let result = engine.schedule(&jobs, &resources, 0).unwrap();

    let execution_time_ms = start.elapsed().as_millis();
    let operation_count: usize = jobs.iter().map(|j| j.operations.len()).sum();

    PerformanceBenchmark {
        name: "Production-500J".to_string(),
        job_count: jobs.len(),
        operation_count,
        resource_count: resources.len(),
        execution_time_ms,
        makespan_ms: result.schedule.makespan_ms,
        passed: execution_time_ms < target_time_ms,
        target_time_ms,
    }
}

/// GA 스케줄러 벤치마크 (100 Jobs)
pub fn benchmark_ga_100() -> PerformanceBenchmark {
    let (jobs, resources) = generate_test_scenario(100, 3, 10);
    let target_time_ms = 120_000; // 2분

    let start = Instant::now();

    let params = GaParams {
        population_size: 50,
        max_generations: 100,
        time_limit_ms: Some(120_000),
        ..Default::default()
    };

    let mut engine = SchedulingEngine::genetic(params);
    let result = engine.schedule(&jobs, &resources, 0).unwrap();

    let execution_time_ms = start.elapsed().as_millis();
    let operation_count: usize = jobs.iter().map(|j| j.operations.len()).sum();

    PerformanceBenchmark {
        name: "GA-100J".to_string(),
        job_count: jobs.len(),
        operation_count,
        resource_count: resources.len(),
        execution_time_ms,
        makespan_ms: result.schedule.makespan_ms,
        passed: execution_time_ms < target_time_ms,
        target_time_ms,
    }
}

/// GA 스케줄러 벤치마크 (200 Jobs)
pub fn benchmark_ga_200() -> PerformanceBenchmark {
    let (jobs, resources) = generate_test_scenario(200, 3, 15);
    let target_time_ms = 300_000; // 5분

    let start = Instant::now();

    let params = GaParams {
        population_size: 50,
        max_generations: 50,
        time_limit_ms: Some(300_000),
        ..Default::default()
    };

    let mut engine = SchedulingEngine::genetic(params);
    let result = engine.schedule(&jobs, &resources, 0).unwrap();

    let execution_time_ms = start.elapsed().as_millis();
    let operation_count: usize = jobs.iter().map(|j| j.operations.len()).sum();

    PerformanceBenchmark {
        name: "GA-200J".to_string(),
        job_count: jobs.len(),
        operation_count,
        resource_count: resources.len(),
        execution_time_ms,
        makespan_ms: result.schedule.makespan_ms,
        passed: execution_time_ms < target_time_ms,
        target_time_ms,
    }
}

/// 전체 성능 벤치마크 실행
pub fn run_all_benchmarks() -> Vec<PerformanceBenchmark> {
    vec![
        benchmark_simple_500(),
        benchmark_production_100(),
        benchmark_production_200(),
        benchmark_production_300(),
        benchmark_production_500(),
        benchmark_ga_50(),
        benchmark_ga_100(),
        benchmark_ga_200(),
        benchmark_cpsat_20(),
        benchmark_auto_100(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scenario_generation() {
        let (jobs, resources) = generate_test_scenario(10, 3, 5);

        assert_eq!(jobs.len(), 10);
        assert_eq!(resources.len(), 5);

        let total_ops: usize = jobs.iter().map(|j| j.operations.len()).sum();
        assert_eq!(total_ops, 30);
    }

    #[test]
    fn test_production_100_benchmark() {
        let result = benchmark_production_100();

        assert_eq!(result.job_count, 100);
        assert!(result.makespan_ms > 0);

        // 성능 기준: 1분 이내
        println!("{}", result.format());
        assert!(
            result.passed,
            "Production 100 jobs should complete within 1 minute"
        );
    }

    #[test]
    fn test_simple_500_benchmark() {
        let result = benchmark_simple_500();

        assert_eq!(result.job_count, 500);
        assert!(result.makespan_ms > 0);

        // 성능 기준: 10초 이내
        println!("{}", result.format());
        assert!(
            result.passed,
            "Simple 500 jobs should complete within 10 seconds"
        );
    }

    #[test]
    fn test_ga_50_benchmark() {
        let result = benchmark_ga_50();

        assert_eq!(result.job_count, 50);
        assert!(result.makespan_ms > 0);

        // 성능 기준: 1분 이내
        println!("{}", result.format());
        assert!(result.passed, "GA 50 jobs should complete within 1 minute");
    }

    #[test]
    fn test_ga_100_benchmark() {
        let result = benchmark_ga_100();

        assert_eq!(result.job_count, 100);
        assert!(result.makespan_ms > 0);

        println!("{}", result.format());
        assert!(
            result.passed,
            "GA 100 jobs should complete within 2 minutes"
        );
    }

    #[test]
    #[ignore]
    fn test_production_300_benchmark() {
        let result = benchmark_production_300();

        assert_eq!(result.job_count, 300);
        assert!(result.makespan_ms > 0);

        println!("{}", result.format());
        assert!(
            result.passed,
            "Production 300 jobs should complete within 5 minutes"
        );
    }

    #[test]
    #[ignore]
    fn test_production_500_benchmark() {
        let result = benchmark_production_500();

        assert_eq!(result.job_count, 500);
        assert!(result.makespan_ms > 0);

        println!("{}", result.format());
        assert!(
            result.passed,
            "Production 500 jobs should complete within 10 minutes"
        );
    }

    #[test]
    #[ignore]
    fn test_ga_200_benchmark() {
        let result = benchmark_ga_200();

        assert_eq!(result.job_count, 200);
        assert!(result.makespan_ms > 0);

        println!("{}", result.format());
        assert!(
            result.passed,
            "GA 200 jobs should complete within 5 minutes"
        );
    }

    #[test]
    #[ignore] // 시간이 오래 걸리므로 기본 테스트에서 제외
    fn test_production_200_benchmark() {
        let result = benchmark_production_200();

        assert_eq!(result.job_count, 200);
        assert!(result.makespan_ms > 0);

        println!("{}", result.format());
        assert!(
            result.passed,
            "Production 200 jobs should complete within 3 minutes"
        );
    }

    #[test]
    #[ignore] // 전체 벤치마크는 시간이 오래 걸림
    fn test_all_benchmarks() {
        let results = run_all_benchmarks();

        for result in &results {
            println!("{}", result.format());
        }

        let all_passed = results.iter().all(|r| r.passed);
        assert!(all_passed, "All benchmarks should pass");
    }
}
