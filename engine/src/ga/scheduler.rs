//! GA-based Scheduler for FJSSP
//!
//! 염색체를 스케줄로 디코딩하고 적합도를 계산

use crate::ga::{
    GaParams, GeneticOperators, OperationInfo, Population, PopulationStats, ScheduleChromosome,
};
use crate::models::{Job, Resource, SetupMatrixCollection, SiteTransitions};
use crate::scheduler::{Assignment, Schedule};
use rand::prelude::*;
use rayon::prelude::*;
use std::collections::HashMap;
use std::time::Instant;

/// GA 스케줄러 결과
#[derive(Debug, Clone)]
pub struct GaResult {
    /// 최적 스케줄
    pub schedule: Schedule,
    /// 최종 적합도
    pub fitness: f64,
    /// 총 세대 수
    pub generations: usize,
    /// 수렴 여부
    pub converged: bool,
    /// 세대별 통계
    pub history: Vec<PopulationStats>,
    /// Whether timeout occurred
    pub timed_out: bool,
}

/// GA 스케줄러
pub struct GaScheduler {
    /// GA 파라미터
    pub params: GaParams,
    /// 유전 연산자
    pub operators: GeneticOperators,
    /// Setup Matrix 컬렉션
    pub setup_matrices: SetupMatrixCollection,
    /// 사이트 간 이동 시간
    pub site_transitions: SiteTransitions,
}

impl Default for GaScheduler {
    fn default() -> Self {
        Self {
            params: GaParams::default(),
            operators: GeneticOperators::default(),
            setup_matrices: SetupMatrixCollection::new(),
            site_transitions: SiteTransitions::new(),
        }
    }
}

impl GaScheduler {
    /// 새 GA 스케줄러 생성
    pub fn new(params: GaParams, operators: GeneticOperators) -> Self {
        Self {
            params,
            operators,
            setup_matrices: SetupMatrixCollection::new(),
            site_transitions: SiteTransitions::new(),
        }
    }

    /// Setup Matrix 설정
    pub fn with_setup_matrices(mut self, matrices: SetupMatrixCollection) -> Self {
        self.setup_matrices = matrices;
        self
    }

    /// 사이트 간 이동 시간 설정
    pub fn with_site_transitions(mut self, transitions: SiteTransitions) -> Self {
        self.site_transitions = transitions;
        self
    }

    /// 스케줄링 실행
    pub fn schedule(&self, jobs: &[Job], resources: &[Resource], start_time_ms: i64) -> GaResult {
        let mut rng = rand::rng();
        self.schedule_with_rng(jobs, resources, start_time_ms, &mut rng)
    }

    /// RNG를 지정하여 스케줄링 실행
    pub fn schedule_with_rng(
        &self,
        jobs: &[Job],
        resources: &[Resource],
        start_time_ms: i64,
        rng: &mut impl Rng,
    ) -> GaResult {
        // 공정 정보 추출
        let operations = extract_operations(jobs);

        if operations.is_empty() {
            return GaResult {
                schedule: Schedule::new(),
                fitness: 0.0,
                generations: 0,
                converged: true,
                timed_out: false,
                history: Vec::new(),
            };
        }

        // 초기 모집단 생성
        let mut population = Population::new(
            &operations,
            resources,
            self.params.clone(),
            self.operators.clone(),
            rng,
        );

        // 초기 적합도 평가
        self.evaluate_population(&mut population, jobs, resources, start_time_ms);

        let mut history = Vec::new();
        history.push(population.statistics());

        // Timeout tracking
        let ga_start = Instant::now();
        let time_limit = self
            .params
            .time_limit_ms
            .map(|ms| std::time::Duration::from_millis(ms as u64));
        let mut timed_out = false;

        // 세대 진화
        for _ in 0..self.params.max_generations {
            // Check timeout (graceful degradation: return best so far)
            if let Some(limit) = time_limit {
                if ga_start.elapsed() >= limit {
                    timed_out = true;
                    break;
                }
            }

            population.evolve(&operations, rng);
            self.evaluate_population(&mut population, jobs, resources, start_time_ms);

            history.push(population.statistics());

            if population.is_converged() {
                break;
            }
        }

        // 최적 개체로 스케줄 생성
        let activities = crate::ga::to_activity_infos(&operations);
        let best = population
            .get_best()
            .cloned()
            .unwrap_or_else(|| ScheduleChromosome::random(&activities, rng));

        let schedule = self.decode(&best, jobs, resources, start_time_ms);

        GaResult {
            schedule,
            fitness: best.fitness,
            generations: population.generation,
            converged: population.is_converged(),
            timed_out,
            history,
        }
    }

    /// 모집단 적합도 평가 (병렬 처리)
    fn evaluate_population(
        &self,
        population: &mut Population,
        jobs: &[Job],
        resources: &[Resource],
        start_time_ms: i64,
    ) {
        use u_metaheur::ga::Individual;
        population
            .individuals
            .par_iter_mut()
            .for_each(|chromosome| {
                let schedule = self.decode(chromosome, jobs, resources, start_time_ms);
                let fitness = self.calculate_fitness(&schedule, jobs);
                chromosome.set_fitness(fitness);
            });
    }

    /// 염색체를 스케줄로 디코딩
    pub fn decode(
        &self,
        chromosome: &ScheduleChromosome,
        jobs: &[Job],
        resources: &[Resource],
        start_time_ms: i64,
    ) -> Schedule {
        let mut schedule = Schedule::new();

        // O(1) 조회용 인덱스 사전 구축
        let job_index: HashMap<&str, &Job> = jobs.iter().map(|j| (j.id.as_str(), j)).collect();
        let resource_index: HashMap<&str, &Resource> =
            resources.iter().map(|r| (r.id.as_str(), r)).collect();

        // 자원별 가용 시간 추적
        let mut resource_available: HashMap<String, i64> = HashMap::new();
        for res in resources {
            resource_available.insert(res.id.clone(), start_time_ms);
        }

        // Job별 다음 공정 시작 가능 시간 (선행 제약)
        let mut job_available: HashMap<String, i64> = HashMap::new();
        for job in jobs {
            job_available.insert(job.id.clone(), start_time_ms);
        }

        // 공정별 시작/종료 시간 추적 (Overlapping용)
        let mut operation_times: HashMap<String, (i64, i64)> = HashMap::new();

        // 자원별 마지막 제품 추적 (Setup Time용)
        let mut resource_last_product: HashMap<String, String> = HashMap::new();

        // Multi-site: 자원별 사이트 맵 + Job별 마지막 사이트 추적
        let resource_site_map: HashMap<&str, &str> = resources
            .iter()
            .filter_map(|r| r.site_id.as_ref().map(|s| (r.id.as_str(), s.as_str())))
            .collect();
        let mut job_last_site: HashMap<String, Option<String>> = HashMap::new();

        // OSV 순서대로 스케줄링
        let decoded_osv = chromosome.decode_osv();

        for (job_id, seq) in decoded_osv {
            // 해당 공정 찾기 (O(1) HashMap 조회)
            let job = match job_index.get(job_id.as_str()) {
                Some(j) => *j,
                None => continue,
            };

            let operation = match job.operations.iter().find(|op| op.sequence == seq) {
                Some(op) => op,
                None => continue,
            };

            // 할당된 기계 조회
            let machine_id = match chromosome.resource_for(&job_id, seq) {
                Some(m) => m.to_string(),
                None => continue,
            };

            // Job 준비 시간 (Overlapping 고려)
            let job_ready = if operation.sequence > 1 {
                // 선행 공정 찾기
                let prev_op = job
                    .operations
                    .iter()
                    .find(|op| op.sequence == operation.sequence - 1);

                if let Some(prev) = prev_op {
                    if let Some(overlap_config) = &prev.overlap_config {
                        // 선행 공정의 시간 정보 조회
                        if let Some(&(prev_start, prev_end)) = operation_times.get(&prev.id) {
                            let prev_duration = prev_end - prev_start;
                            // min_predecessor_completion 비율만큼 완료 후 시작 가능
                            prev_start
                                + (prev_duration as f64 * overlap_config.min_predecessor_completion)
                                    as i64
                        } else {
                            *job_available.get(&job_id).unwrap_or(&start_time_ms)
                        }
                    } else {
                        *job_available.get(&job_id).unwrap_or(&start_time_ms)
                    }
                } else {
                    *job_available.get(&job_id).unwrap_or(&start_time_ms)
                }
            } else {
                *job_available.get(&job_id).unwrap_or(&start_time_ms)
            };
            let product_name = job.product_name.as_deref().unwrap_or(&job.id);

            // Operation Splitting 처리
            if operation.is_splittable {
                // 가용 기계 후보 찾기
                let candidates: Vec<&String> = operation
                    .required_resources
                    .iter()
                    .flat_map(|req| &req.candidates)
                    .filter(|id| resource_index.contains_key(id.as_str()))
                    .collect();

                let num_machines = candidates.len().max(1);
                let max_splits = operation.max_possible_splits();
                let actual_splits = max_splits.min(num_machines);

                if actual_splits > 1 {
                    // 분할 생성
                    let splits = operation.create_splits(actual_splits);
                    let mut split_ends: Vec<i64> = Vec::new();

                    for (i, split) in splits.iter().enumerate() {
                        // 각 분할에 기계 할당 (라운드 로빈)
                        let split_machine = if i < candidates.len() {
                            candidates[i].clone()
                        } else {
                            machine_id.clone()
                        };

                        // 시작 시간 계산
                        let machine_ready = *resource_available
                            .get(&split_machine)
                            .unwrap_or(&start_time_ms);

                        // Inter-site transition time (첫 분할에만 적용)
                        let site_transition = if i == 0 {
                            let from_site = job_last_site.get(&job_id).and_then(|s| s.as_deref());
                            let to_site = resource_site_map.get(split_machine.as_str()).copied();
                            self.site_transitions
                                .get_transition_time(from_site, to_site)
                        } else {
                            0
                        };

                        let start = if site_transition > 0 {
                            job_ready.max(machine_ready).max(
                                job_available.get(&job_id).copied().unwrap_or(start_time_ms)
                                    + site_transition,
                            )
                        } else {
                            job_ready.max(machine_ready)
                        };

                        // Setup Time 계산 (분할별)
                        let last_product = resource_last_product
                            .get(&split_machine)
                            .map(|s| s.as_str());
                        let split_setup = if i == 0 {
                            self.setup_matrices.get_setup_time(
                                &split_machine,
                                last_product,
                                product_name,
                            )
                        } else {
                            // 추가 분할은 오버헤드만
                            operation
                                .split_config
                                .as_ref()
                                .map(|c| c.split_setup_overhead_ms)
                                .unwrap_or(0)
                        };

                        // 효율성 적용 (O(1) HashMap 조회)
                        let efficiency = resource_index
                            .get(split_machine.as_str())
                            .map(|r| r.efficiency)
                            .unwrap_or(1.0);

                        let process_time = (split.process_ms as f64 / efficiency) as i64;
                        let total_time = split_setup + process_time;
                        let end = start + total_time;

                        // 분할 할당 생성
                        let split_op_id = format!("{}_split_{}", operation.id, split.split_index);
                        let mut assignment = Assignment::with_setup(
                            split_op_id,
                            job_id.clone(),
                            split_machine.clone(),
                            start,
                            end,
                            split_setup,
                        );
                        assignment.site_id = resource_site_map
                            .get(split_machine.as_str())
                            .map(|s| s.to_string());

                        schedule.add_assignment(assignment);
                        split_ends.push(end);

                        // 자원 상태 업데이트
                        resource_available.insert(split_machine.clone(), end);
                        resource_last_product.insert(split_machine, product_name.to_string());
                    }

                    // 모든 분할 완료 후 다음 공정 시작 가능 + wait_ms
                    let all_splits_start = split_ends.iter().min().copied().unwrap_or(job_ready);
                    let all_splits_end = split_ends.into_iter().max().unwrap_or(job_ready);

                    // 공정 시간 기록 (Overlapping용)
                    operation_times
                        .insert(operation.id.clone(), (all_splits_start, all_splits_end));

                    // 마지막 사이트 추적 (마지막 분할 기계의 사이트)
                    let last_split_machine = if candidates.len() > 1 {
                        candidates
                            .last()
                            .map(|c| c.as_str())
                            .unwrap_or(machine_id.as_str())
                    } else {
                        machine_id.as_str()
                    };
                    job_last_site.insert(
                        job_id.clone(),
                        resource_site_map
                            .get(last_split_machine)
                            .map(|s| s.to_string()),
                    );

                    job_available.insert(job_id, all_splits_end + operation.time.wait_ms);
                    continue;
                }
            }

            // 일반 공정 처리 (분할 없음)
            let machine_ready = *resource_available
                .get(&machine_id)
                .unwrap_or(&start_time_ms);

            // Inter-site transition time 적용
            let site_transition = {
                let from_site = job_last_site.get(&job_id).and_then(|s| s.as_deref());
                let to_site = resource_site_map.get(machine_id.as_str()).copied();
                self.site_transitions
                    .get_transition_time(from_site, to_site)
            };

            let start = if site_transition > 0 {
                job_ready.max(machine_ready).max(
                    job_available.get(&job_id).copied().unwrap_or(start_time_ms) + site_transition,
                )
            } else {
                job_ready.max(machine_ready)
            };

            // Setup Time 계산
            let last_product = resource_last_product.get(&machine_id).map(|s| s.as_str());
            let setup_time =
                self.setup_matrices
                    .get_setup_time(&machine_id, last_product, product_name);

            // 효율성 적용 (O(1) HashMap 조회)
            let efficiency = resource_index
                .get(machine_id.as_str())
                .map(|r| r.efficiency)
                .unwrap_or(1.0);

            let process_time = (operation.time.process_ms as f64 / efficiency) as i64;
            let total_time = setup_time + process_time + operation.time.wait_ms;
            let end = start + total_time;

            // 할당 생성
            let mut assignment = Assignment::with_setup(
                operation.id.clone(),
                job_id.clone(),
                machine_id.clone(),
                start,
                end,
                setup_time,
            );
            assignment.site_id = resource_site_map
                .get(machine_id.as_str())
                .map(|s| s.to_string());

            schedule.add_assignment(assignment);

            // 공정 시간 기록 (Overlapping용)
            operation_times.insert(operation.id.clone(), (start, end));

            // 상태 업데이트
            resource_available.insert(machine_id.clone(), end);
            job_last_site.insert(
                job_id.clone(),
                resource_site_map
                    .get(machine_id.as_str())
                    .map(|s| s.to_string()),
            );
            job_available.insert(job_id, end);
            resource_last_product.insert(machine_id, product_name.to_string());
        }

        schedule
    }

    /// 적합도 계산 (낮을수록 좋음)
    fn calculate_fitness(&self, schedule: &Schedule, jobs: &[Job]) -> f64 {
        let mut fitness = schedule.makespan_ms as f64;

        // 납기 지연 패널티
        for job in jobs {
            if let Some(due_date) = &job.due_date {
                let due_date_ms = due_date.timestamp_millis();

                // Job의 마지막 공정 완료 시간 찾기
                let job_end = job
                    .operations
                    .iter()
                    .filter_map(|op| schedule.assignment_for_operation(&op.id).map(|a| a.end_ms))
                    .max()
                    .unwrap_or(0);

                if job_end > due_date_ms {
                    let tardiness = (job_end - due_date_ms) as f64;
                    let priority_factor = job.priority as f64;
                    fitness += tardiness * priority_factor * 0.1;
                }
            }
        }

        fitness
    }
}

/// Job 목록에서 공정 정보 추출
fn extract_operations(jobs: &[Job]) -> Vec<OperationInfo> {
    let mut operations = Vec::new();

    for job in jobs {
        for op in &job.operations {
            // required_resources에서 첫 번째 장비 요구사항의 후보 추출
            let candidates: Vec<String> = op
                .required_resources
                .iter()
                .flat_map(|req| req.candidates.clone())
                .collect();

            operations.push(OperationInfo {
                task_id: job.id.clone(),
                activity_id: op.id.clone(),
                sequence: op.sequence,
                candidates,
                process_time_ms: op.time.process_ms,
            });
        }
    }

    // Task ID, sequence 순으로 정렬
    operations.sort_by(|a, b| (&a.task_id, a.sequence).cmp(&(&b.task_id, b.sequence)));

    operations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Operation;

    fn create_test_jobs() -> Vec<Job> {
        vec![
            Job::new("JOB-1")
                .with_product("PRODUCT-A")
                .with_priority(1)
                .with_operation(
                    Operation::new("OP-1-1", "JOB-1", 1)
                        .with_time(0, 30000, 0)
                        .with_equipment(vec!["M1".to_string(), "M2".to_string()]),
                )
                .with_operation(
                    Operation::new("OP-1-2", "JOB-1", 2)
                        .with_time(0, 45000, 0)
                        .with_equipment(vec!["M2".to_string(), "M3".to_string()]),
                ),
            Job::new("JOB-2")
                .with_product("PRODUCT-B")
                .with_priority(2)
                .with_operation(
                    Operation::new("OP-2-1", "JOB-2", 1)
                        .with_time(0, 20000, 0)
                        .with_equipment(vec!["M1".to_string(), "M3".to_string()]),
                ),
        ]
    }

    fn create_test_resources() -> Vec<Resource> {
        vec![
            Resource::equipment("M1"),
            Resource::equipment("M2"),
            Resource::equipment("M3"),
        ]
    }

    #[test]
    fn test_extract_operations() {
        let jobs = create_test_jobs();
        let operations = extract_operations(&jobs);

        assert_eq!(operations.len(), 3);
        assert_eq!(operations[0].task_id, "JOB-1");
        assert_eq!(operations[0].sequence, 1);
        assert_eq!(operations[1].task_id, "JOB-1");
        assert_eq!(operations[1].sequence, 2);
        assert_eq!(operations[2].task_id, "JOB-2");
    }

    #[test]
    fn test_ga_scheduler_basic() {
        let jobs = create_test_jobs();
        let resources = create_test_resources();

        let params = GaParams {
            population_size: 20,
            max_generations: 10,
            ..Default::default()
        };

        let scheduler = GaScheduler::new(params, GeneticOperators::default());
        let result = scheduler.schedule(&jobs, &resources, 0);

        assert!(!result.schedule.assignments.is_empty());
        assert_eq!(result.schedule.assignments.len(), 3);
        assert!(result.generations <= 10);
    }

    #[test]
    fn test_decode_chromosome() {
        let jobs = create_test_jobs();
        let resources = create_test_resources();
        let operations = extract_operations(&jobs);
        let activities = crate::ga::to_activity_infos(&operations);

        let mut rng = rand::rng();
        let chromosome = ScheduleChromosome::random(&activities, &mut rng);

        let scheduler = GaScheduler::default();
        let schedule = scheduler.decode(&chromosome, &jobs, &resources, 0);

        // 모든 공정이 할당되었는지 확인
        assert_eq!(schedule.assignments.len(), 3);

        // 선행 제약 확인: JOB-1의 OP-1-2는 OP-1-1 이후에 시작
        let op1_1 = schedule
            .assignments
            .iter()
            .find(|a| a.operation_id == "OP-1-1");
        let op1_2 = schedule
            .assignments
            .iter()
            .find(|a| a.operation_id == "OP-1-2");

        if let (Some(first), Some(second)) = (op1_1, op1_2) {
            assert!(second.start_ms >= first.end_ms);
        }
    }

    #[test]
    fn test_fitness_calculation() {
        let jobs = create_test_jobs();
        let resources = create_test_resources();
        let operations = extract_operations(&jobs);
        let activities = crate::ga::to_activity_infos(&operations);

        let mut rng = rand::rng();
        let chromosome = ScheduleChromosome::random(&activities, &mut rng);

        let scheduler = GaScheduler::default();
        let schedule = scheduler.decode(&chromosome, &jobs, &resources, 0);
        let fitness = scheduler.calculate_fitness(&schedule, &jobs);

        assert!(fitness >= schedule.makespan_ms as f64);
    }

    #[test]
    fn test_ga_convergence() {
        let jobs = create_test_jobs();
        let resources = create_test_resources();

        let params = GaParams {
            population_size: 30,
            max_generations: 100,
            convergence_generations: 10,
            convergence_threshold: 0.001,
            ..Default::default()
        };

        let scheduler = GaScheduler::new(params, GeneticOperators::default());
        let result = scheduler.schedule(&jobs, &resources, 0);

        // 히스토리가 기록되었는지 확인
        assert!(!result.history.is_empty());

        // 적합도가 개선되었는지 확인 (첫 세대보다 마지막 세대가 좋거나 같음)
        if result.history.len() > 1 {
            let first_fitness = result.history[0].best_fitness;
            let last_fitness = result.history.last().unwrap().best_fitness;
            assert!(last_fitness <= first_fitness);
        }
    }

    #[test]
    fn test_empty_jobs() {
        let jobs: Vec<Job> = vec![];
        let resources = create_test_resources();

        let scheduler = GaScheduler::default();
        let result = scheduler.schedule(&jobs, &resources, 0);

        assert!(result.schedule.assignments.is_empty());
        assert_eq!(result.fitness, 0.0);
        assert!(result.converged);
    }

    #[test]
    fn test_with_setup_matrices() {
        let jobs = create_test_jobs();
        let resources = create_test_resources();

        let setup_matrix = crate::SetupMatrix::new("M1")
            .with_default_setup(5000)
            .with_setup("PRODUCT-A", "PRODUCT-B", 10000);

        let matrices = SetupMatrixCollection::new().with_matrix(setup_matrix);

        let scheduler = GaScheduler::default().with_setup_matrices(matrices);
        let result = scheduler.schedule(&jobs, &resources, 0);

        assert!(!result.schedule.assignments.is_empty());
    }

    #[test]
    fn test_operation_splitting_in_scheduler() {
        use crate::models::SplitConfig;

        // 분할 가능한 공정이 있는 Job 생성
        let split_config = SplitConfig {
            min_split_size_ms: 10_000,
            max_splits: 3,
            split_setup_overhead_ms: 1000,
        };

        let jobs = vec![Job::new("JOB-1")
            .with_product("PRODUCT-A")
            .with_priority(1)
            .with_operation(
                Operation::new("OP-1-1", "JOB-1", 1)
                    .with_time(0, 90_000, 0) // 90초 - 3분할 가능
                    .with_equipment(vec!["M1".to_string(), "M2".to_string(), "M3".to_string()])
                    .with_splitting(split_config),
            )];

        let resources = vec![
            Resource::equipment("M1"),
            Resource::equipment("M2"),
            Resource::equipment("M3"),
        ];

        let params = GaParams {
            population_size: 10,
            max_generations: 5,
            ..Default::default()
        };

        let scheduler = GaScheduler::new(params, GeneticOperators::default());
        let result = scheduler.schedule(&jobs, &resources, 0);

        // 3개의 분할 할당이 생성되어야 함
        assert_eq!(result.schedule.assignments.len(), 3);

        // 각 분할이 다른 기계에 할당되었는지 확인
        let machines: Vec<&String> = result
            .schedule
            .assignments
            .iter()
            .map(|a| &a.resource_id)
            .collect();

        // 최소 2개 이상의 기계 사용 확인
        let unique_machines: std::collections::HashSet<&String> =
            machines.iter().cloned().collect();
        assert!(unique_machines.len() >= 2);

        // 분할 ID 형식 확인
        assert!(result
            .schedule
            .assignments
            .iter()
            .any(|a| a.operation_id.contains("_split_")));
    }

    #[test]
    fn test_split_makespan_improvement() {
        use crate::models::SplitConfig;

        let split_config = SplitConfig {
            min_split_size_ms: 30_000,
            max_splits: 2,
            split_setup_overhead_ms: 0,
        };

        let resources = vec![Resource::equipment("M1"), Resource::equipment("M2")];

        // 분할 없는 경우
        let jobs_no_split = vec![Job::new("JOB-1").with_operation(
            Operation::new("OP-1", "JOB-1", 1)
                .with_time(0, 60_000, 0)
                .with_equipment(vec!["M1".to_string()]),
        )];

        // 분할 있는 경우
        let jobs_with_split = vec![Job::new("JOB-1").with_operation(
            Operation::new("OP-1", "JOB-1", 1)
                .with_time(0, 60_000, 0)
                .with_equipment(vec!["M1".to_string(), "M2".to_string()])
                .with_splitting(split_config),
        )];

        let params = GaParams {
            population_size: 10,
            max_generations: 5,
            ..Default::default()
        };

        let scheduler = GaScheduler::new(params, GeneticOperators::default());

        let result_no_split = scheduler.schedule(&jobs_no_split, &resources, 0);
        let result_with_split = scheduler.schedule(&jobs_with_split, &resources, 0);

        // 분할 시 makespan이 줄어들어야 함 (이상적으로 절반)
        assert!(result_with_split.schedule.makespan_ms <= result_no_split.schedule.makespan_ms);
    }

    #[test]
    fn test_operation_overlapping() {
        use crate::models::OverlapConfig;

        // Overlapping 설정 (50% 완료 후 다음 공정 시작 가능)
        let overlap_config = OverlapConfig {
            min_predecessor_completion: 0.5,
            transfer_batch_size: 1,
        };

        // Overlapping 없는 Job
        let jobs_no_overlap = vec![Job::new("JOB-1")
            .with_operation(
                Operation::new("OP-1", "JOB-1", 1)
                    .with_time(0, 60_000, 0)
                    .with_equipment(vec!["M1".to_string()]),
            )
            .with_operation(
                Operation::new("OP-2", "JOB-1", 2)
                    .with_time(0, 40_000, 0)
                    .with_equipment(vec!["M2".to_string()]),
            )];

        // Overlapping 있는 Job
        let jobs_with_overlap = vec![Job::new("JOB-1")
            .with_operation(
                Operation::new("OP-1", "JOB-1", 1)
                    .with_time(0, 60_000, 0)
                    .with_equipment(vec!["M1".to_string()])
                    .with_overlapping(overlap_config),
            )
            .with_operation(
                Operation::new("OP-2", "JOB-1", 2)
                    .with_time(0, 40_000, 0)
                    .with_equipment(vec!["M2".to_string()]),
            )];

        let resources = vec![Resource::equipment("M1"), Resource::equipment("M2")];

        let params = GaParams {
            population_size: 10,
            max_generations: 5,
            ..Default::default()
        };

        let scheduler = GaScheduler::new(params, GeneticOperators::default());

        let result_no_overlap = scheduler.schedule(&jobs_no_overlap, &resources, 0);
        let result_with_overlap = scheduler.schedule(&jobs_with_overlap, &resources, 0);

        // Overlapping 없는 경우: 60000 + 40000 = 100000ms
        // Overlapping 있는 경우: 60000 * 0.5 = 30000 (OP-2 시작) + 40000 = 70000ms
        // makespan이 줄어들어야 함
        assert!(result_with_overlap.schedule.makespan_ms < result_no_overlap.schedule.makespan_ms);

        // OP-2가 OP-1 완료 전에 시작되었는지 확인
        let op1_end = result_with_overlap
            .schedule
            .assignments
            .iter()
            .find(|a| a.operation_id == "OP-1")
            .map(|a| a.end_ms)
            .unwrap_or(0);

        let op2_start = result_with_overlap
            .schedule
            .assignments
            .iter()
            .find(|a| a.operation_id == "OP-2")
            .map(|a| a.start_ms)
            .unwrap_or(0);

        // OP-2는 OP-1 종료 전에 시작해야 함
        assert!(op2_start < op1_end);
    }

    #[test]
    fn test_overlapping_respects_completion_ratio() {
        use crate::models::OverlapConfig;

        // 30% 완료 후 시작 가능
        let overlap_config = OverlapConfig {
            min_predecessor_completion: 0.3,
            transfer_batch_size: 1,
        };

        let jobs = vec![Job::new("JOB-1")
            .with_operation(
                Operation::new("OP-1", "JOB-1", 1)
                    .with_time(0, 100_000, 0) // 100초
                    .with_equipment(vec!["M1".to_string()])
                    .with_overlapping(overlap_config),
            )
            .with_operation(
                Operation::new("OP-2", "JOB-1", 2)
                    .with_time(0, 50_000, 0)
                    .with_equipment(vec!["M2".to_string()]),
            )];

        let resources = vec![Resource::equipment("M1"), Resource::equipment("M2")];

        let params = GaParams {
            population_size: 10,
            max_generations: 5,
            ..Default::default()
        };

        let scheduler = GaScheduler::new(params, GeneticOperators::default());
        let result = scheduler.schedule(&jobs, &resources, 0);

        let op2_start = result
            .schedule
            .assignments
            .iter()
            .find(|a| a.operation_id == "OP-2")
            .map(|a| a.start_ms)
            .unwrap_or(0);

        // OP-2는 30000ms (100000 * 0.3) 이후에 시작해야 함
        assert!(op2_start >= 30_000);
    }

    #[test]
    fn test_ga_timeout() {
        let jobs: Vec<Job> = (0..20)
            .map(|i| {
                let job_id = format!("J{}", i);
                Job::new(&job_id).with_operation(
                    Operation::new(format!("{}-O1", job_id), &job_id, 1)
                        .with_time(0, 5000, 0)
                        .with_equipment(vec!["M1".to_string()]),
                )
            })
            .collect();
        let resources = vec![Resource::equipment("M1").with_efficiency(1.0)];

        let params = GaParams {
            population_size: 50,
            max_generations: 1000,
            time_limit_ms: Some(10),
            ..Default::default()
        };
        let scheduler = GaScheduler::new(params, GeneticOperators::default());
        let start = std::time::Instant::now();
        let result = scheduler.schedule(&jobs, &resources, 0);
        let elapsed = start.elapsed();

        assert!(elapsed.as_millis() < 500, "Should timeout quickly");
        assert!(result.timed_out || result.generations < 1000);
        assert!(!result.schedule.assignments.is_empty());
    }

    #[test]
    fn test_ga_multi_site_assignment() {
        let jobs = vec![
            Job::new("JOB-1").with_site("SITE-A").with_operation(
                Operation::new("OP-1-1", "JOB-1", 1)
                    .with_time(0, 30_000, 0)
                    .with_equipment(vec!["M1".to_string(), "M2".to_string()]),
            ),
            Job::new("JOB-2").with_operation(
                Operation::new("OP-2-1", "JOB-2", 1)
                    .with_time(0, 20_000, 0)
                    .with_equipment(vec!["M1".to_string(), "M2".to_string()]),
            ),
        ];

        let resources = vec![
            Resource::equipment("M1").with_site("SITE-A"),
            Resource::equipment("M2").with_site("SITE-B"),
        ];

        let params = GaParams {
            population_size: 10,
            max_generations: 5,
            ..Default::default()
        };

        let scheduler = GaScheduler::new(params, GeneticOperators::default());
        let result = scheduler.schedule(&jobs, &resources, 0);

        // 모든 할당에 site_id가 설정되어야 함
        for assignment in &result.schedule.assignments {
            assert!(
                assignment.site_id.is_some(),
                "Assignment {} should have site_id",
                assignment.operation_id
            );
        }
    }

    #[test]
    fn test_ga_multi_site_transition_time() {
        use crate::models::SiteTransitions;

        // 2개 공정이 다른 사이트에서 실행되면 이동 시간 추가
        let jobs = vec![Job::new("JOB-1")
            .with_operation(
                Operation::new("OP-1", "JOB-1", 1)
                    .with_time(0, 30_000, 0)
                    .with_equipment(vec!["M1".to_string()]),
            )
            .with_operation(
                Operation::new("OP-2", "JOB-1", 2)
                    .with_time(0, 20_000, 0)
                    .with_equipment(vec!["M2".to_string()]),
            )];

        let resources = vec![
            Resource::equipment("M1").with_site("SITE-A"),
            Resource::equipment("M2").with_site("SITE-B"),
        ];

        let mut transitions = SiteTransitions::new();
        transitions.add_bidirectional("SITE-A", "SITE-B", 10_000);

        let params = GaParams {
            population_size: 10,
            max_generations: 5,
            ..Default::default()
        };

        // 이동 시간 없는 경우
        let scheduler_no_transit = GaScheduler::new(params.clone(), GeneticOperators::default());
        let result_no_transit = scheduler_no_transit.schedule(&jobs, &resources, 0);

        // 이동 시간 있는 경우
        let scheduler_with_transit = GaScheduler::new(params, GeneticOperators::default())
            .with_site_transitions(transitions);
        let result_with_transit = scheduler_with_transit.schedule(&jobs, &resources, 0);

        // 이동 시간이 있으면 makespan이 더 길어야 함
        assert!(
            result_with_transit.schedule.makespan_ms >= result_no_transit.schedule.makespan_ms,
            "With transition: {}ms, without: {}ms",
            result_with_transit.schedule.makespan_ms,
            result_no_transit.schedule.makespan_ms
        );
    }
}
