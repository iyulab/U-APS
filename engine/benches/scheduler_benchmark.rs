//! Performance Benchmarks for U-APS Scheduling Engine

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use uaps_engine::*;

/// 테스트 데이터 생성 헬퍼
fn create_jobs(count: usize, ops_per_job: usize, machines: &[String]) -> Vec<Job> {
    (0..count)
        .map(|i| {
            let mut job = Job::new(format!("J{}", i)).with_priority((i % 5) as i32 + 1);
            for j in 0..ops_per_job {
                let op = Operation::new(format!("O{}_{}", i, j), format!("J{}", i), j as i32 + 1)
                    .with_time(1_000, 10_000 + (i as i64 * 1_000), 500)
                    .with_equipment(machines.to_vec());
                job = job.with_operation(op);
            }
            job
        })
        .collect()
}

fn create_resources(count: usize) -> Vec<Resource> {
    (0..count)
        .map(|i| Resource::equipment(format!("M{}", i)).with_efficiency(0.8 + (i as f64 * 0.05)))
        .collect()
}

/// SimpleScheduler 벤치마크
fn bench_simple_scheduler(c: &mut Criterion) {
    let mut group = c.benchmark_group("SimpleScheduler");

    // 다양한 규모 테스트
    for (jobs, machines, ops) in [(10, 3, 1), (20, 5, 2), (50, 10, 2), (100, 10, 3)] {
        let machine_ids: Vec<String> = (0..machines).map(|i| format!("M{}", i)).collect();
        let job_list = create_jobs(jobs, ops, &machine_ids);
        let resources = create_resources(machines);
        let request = ScheduleRequest::new(job_list, resources);

        group.bench_with_input(
            BenchmarkId::new("schedule", format!("{}j_{}m_{}o", jobs, machines, ops)),
            &request,
            |b, req| {
                b.iter(|| {
                    let scheduler = SimpleScheduler::new();
                    black_box(scheduler.schedule(req))
                })
            },
        );
    }

    group.finish();
}

/// GaScheduler 벤치마크
fn bench_ga_scheduler(c: &mut Criterion) {
    let mut group = c.benchmark_group("GaScheduler");
    group.sample_size(10); // GA는 시간이 오래 걸리므로 샘플 수 줄임

    // 소규모 테스트 (GA 특성상 시간이 오래 걸림)
    for (jobs, machines) in [(10, 3), (20, 5)] {
        let machine_ids: Vec<String> = (0..machines).map(|i| format!("M{}", i)).collect();
        let job_list = create_jobs(jobs, 1, &machine_ids);
        let resources = create_resources(machines);

        let params = GaParams {
            population_size: 20,
            max_generations: 30,
            ..Default::default()
        };

        group.bench_with_input(
            BenchmarkId::new("schedule", format!("{}j_{}m", jobs, machines)),
            &(&job_list, &resources),
            |b, (jobs, res)| {
                b.iter(|| {
                    let scheduler = GaScheduler::new(params.clone(), GeneticOperators::default());
                    black_box(scheduler.schedule(jobs, res, 0))
                })
            },
        );
    }

    group.finish();
}

/// KPI 계산 벤치마크
fn bench_kpi_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("KpiCalculator");

    for size in [10, 50, 100] {
        // 스케줄 생성
        let machine_ids: Vec<String> = (0..5).map(|i| format!("M{}", i)).collect();
        let jobs = create_jobs(size, 2, &machine_ids);
        let resources = create_resources(5);
        let request = ScheduleRequest::new(jobs.clone(), resources);

        let scheduler = SimpleScheduler::new();
        let schedule = scheduler.schedule(&request);

        group.bench_with_input(
            BenchmarkId::new("calculate", size),
            &(&schedule, &jobs),
            |b, (sched, job_list)| {
                b.iter(|| {
                    let calculator = KpiCalculator::new(1_000_000);
                    black_box(calculator.calculate(sched, job_list))
                })
            },
        );
    }

    group.finish();
}

/// Setup Matrix 조회 벤치마크
fn bench_setup_matrix(c: &mut Criterion) {
    let mut group = c.benchmark_group("SetupMatrix");

    // 대규모 Setup Matrix 생성
    let mut matrix = SetupMatrix::new("M1").with_default_setup(5_000);
    let products: Vec<String> = (0..100).map(|i| format!("Product{}", i)).collect();

    for i in 0..100 {
        for j in 0..100 {
            if i != j {
                matrix = matrix.with_setup(&products[i], &products[j], ((i + j) * 100) as i64);
            }
        }
    }

    let collection = SetupMatrixCollection::new().with_matrix(matrix);

    group.bench_function("lookup_100x100", |b| {
        b.iter(|| black_box(collection.get_setup_time("M1", Some("Product50"), "Product75")))
    });

    group.finish();
}

/// Validation 벤치마크
fn bench_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("Validation");

    for size in [20, 50, 100] {
        let machine_ids: Vec<String> = (0..5).map(|i| format!("M{}", i)).collect();
        let jobs = create_jobs(size, 2, &machine_ids);
        let resources = create_resources(5);

        // Operations 추출
        let operations: Vec<Operation> = jobs.iter().flat_map(|j| j.operations.clone()).collect();

        group.bench_with_input(
            BenchmarkId::new("validate_request", size),
            &(&jobs, &operations, &resources),
            |b, (jobs, ops, res)| {
                b.iter(|| black_box(Validator::validate_schedule_request(jobs, ops, res)))
            },
        );
    }

    group.finish();
}

/// What-If 분석 벤치마크
fn bench_whatif(c: &mut Criterion) {
    let mut group = c.benchmark_group("WhatIfEngine");

    let machine_ids: Vec<String> = (0..3).map(|i| format!("M{}", i)).collect();
    let jobs = create_jobs(20, 1, &machine_ids);
    let resources = create_resources(3);
    let request = ScheduleRequest::new(jobs.clone(), resources);

    let scheduler = SimpleScheduler::new();
    let schedule = scheduler.schedule(&request);

    let engine = WhatIfEngine::new(schedule, jobs, 500_000);

    let scenarios: Vec<Scenario> = (0..5)
        .map(|i| Scenario {
            id: format!("S{}", i),
            name: format!("Scenario {}", i),
            changes: vec![ScenarioChange::ProcessingTime {
                operation_id: None,
                change_percent: -0.1 * (i as f64 + 1.0),
            }],
        })
        .collect();

    group.bench_function("analyze_5_scenarios", |b| {
        b.iter(|| black_box(engine.analyze_scenarios(&scenarios)))
    });

    group.finish();
}

/// Auto-Tuning 벤치마크
fn bench_auto_tuner(c: &mut Criterion) {
    let mut group = c.benchmark_group("AutoTuner");

    // 다양한 문제 규모에 대한 자동 튜닝 속도 테스트
    for (jobs_count, ops_per_job, machines) in [(10, 2, 3), (30, 3, 5), (80, 4, 10), (150, 5, 15)] {
        let machine_ids: Vec<String> = (0..machines).map(|i| format!("M{}", i)).collect();
        let jobs = create_jobs(jobs_count, ops_per_job, &machine_ids);
        let resources = create_resources(machines);

        group.bench_with_input(
            BenchmarkId::new(
                "analyze_characteristics",
                format!("{}j_{}o", jobs_count, ops_per_job),
            ),
            &(&jobs, &resources),
            |b, (jobs, res)| {
                b.iter(|| {
                    let chars = ProblemCharacteristics::analyze(jobs, res);
                    black_box(chars)
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("tune_params", format!("{}j_{}o", jobs_count, ops_per_job)),
            &(&jobs, &resources),
            |b, (jobs, res)| {
                b.iter(|| {
                    let chars = ProblemCharacteristics::analyze(jobs, res);
                    let params = GaAutoTuner::tune(&chars);
                    let operators = GaAutoTuner::tune_operators(&chars);
                    black_box((params, operators))
                })
            },
        );
    }

    group.finish();
}

/// Hybrid GA-SA 벤치마크
fn bench_hybrid_ga_sa(c: &mut Criterion) {
    let mut group = c.benchmark_group("HybridGaSa");
    group.sample_size(10); // Hybrid는 시간이 오래 걸리므로 샘플 수 줄임

    // 소규모 테스트로 자동 튜닝 Hybrid 알고리즘 성능 측정
    for (jobs_count, machines) in [(10, 3), (20, 5)] {
        let machine_ids: Vec<String> = (0..machines).map(|i| format!("M{}", i)).collect();
        let jobs = create_jobs(jobs_count, 2, &machine_ids);
        let resources = create_resources(machines);

        group.bench_with_input(
            BenchmarkId::new("auto_tune_config", format!("{}j_{}m", jobs_count, machines)),
            &(&jobs, &resources),
            |b, (jobs, res)| {
                b.iter(|| {
                    let config = HybridGaSaConfig::auto_tune(jobs, res);
                    black_box(config)
                })
            },
        );

        // 자동 튜닝된 설정으로 스케줄링 실행
        group.bench_with_input(
            BenchmarkId::new(
                "schedule_with_auto_tune",
                format!("{}j_{}m", jobs_count, machines),
            ),
            &(&jobs, &resources),
            |b, (jobs, res)| {
                b.iter(|| {
                    let mut config = HybridGaSaConfig::auto_tune(jobs, res);
                    // 벤치마크용으로 제한
                    config.ga_params.max_generations = 20;
                    config.sa_params.iterations_per_temperature = 5;
                    config.ga_params.time_limit_ms = Some(2000);

                    let scheduler = HybridGaSaScheduler::new(config, GeneticOperators::default());
                    black_box(scheduler.optimize(jobs, res, 0))
                })
            },
        );
    }

    group.finish();
}

/// TuningReport 생성 벤치마크
fn bench_tuning_report(c: &mut Criterion) {
    let mut group = c.benchmark_group("TuningReport");

    for (jobs_count, machines) in [(20, 5), (50, 10), (100, 15)] {
        let machine_ids: Vec<String> = (0..machines).map(|i| format!("M{}", i)).collect();
        let jobs = create_jobs(jobs_count, 3, &machine_ids);
        let resources = create_resources(machines);

        group.bench_with_input(
            BenchmarkId::new("generate", format!("{}j_{}m", jobs_count, machines)),
            &(&jobs, &resources),
            |b, (jobs, res)| {
                b.iter(|| {
                    let report = TuningReport::generate(jobs, res);
                    black_box(report)
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_simple_scheduler,
    bench_ga_scheduler,
    bench_kpi_calculation,
    bench_setup_matrix,
    bench_validation,
    bench_whatif,
    bench_auto_tuner,
    bench_hybrid_ga_sa,
    bench_tuning_report,
);
criterion_main!(benches);
