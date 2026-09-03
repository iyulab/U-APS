//! Hybrid GA-SA - Genetic Algorithm with Simulated Annealing
//!
//! GA의 전역 탐색과 SA의 지역 탐색을 결합한 하이브리드 알고리즘
//! - GA로 유망 영역 탐색
//! - SA로 엘리트 해 정제
//! - 적응형 파라미터 조정

use crate::ga::auto_tuner::{GaAutoTuner, ProblemCharacteristics};
use crate::ga::{
    to_activity_infos, GaParams, GaScheduler, GeneticOperators, OperationInfo, ScheduleChromosome,
};
use crate::models::{Job, Resource, SetupMatrixCollection, SiteTransitions};
use crate::scheduler::Schedule;
use rand::prelude::*;

/// Simulated Annealing 파라미터
#[derive(Debug, Clone)]
pub struct SaParams {
    /// 초기 온도
    pub initial_temperature: f64,
    /// 최종 온도
    pub final_temperature: f64,
    /// 냉각률 (0 < alpha < 1)
    pub cooling_rate: f64,
    /// 온도당 반복 횟수
    pub iterations_per_temperature: usize,
}

impl Default for SaParams {
    fn default() -> Self {
        Self {
            initial_temperature: 1000.0,
            final_temperature: 1.0,
            cooling_rate: 0.95,
            iterations_per_temperature: 10,
        }
    }
}

/// 하이브리드 GA-SA 설정
#[derive(Debug, Clone)]
pub struct HybridGaSaConfig {
    /// GA 파라미터
    pub ga_params: GaParams,
    /// SA 파라미터
    pub sa_params: SaParams,
    /// SA 적용 비율 (상위 몇 %에 적용)
    pub sa_elite_ratio: f64,
    /// SA 적용 주기 (매 N 세대마다)
    pub sa_frequency: usize,
}

impl Default for HybridGaSaConfig {
    fn default() -> Self {
        Self {
            ga_params: GaParams {
                population_size: 100,
                max_generations: 200,
                elite_ratio: 0.1,
                tournament_size: 3,
                convergence_generations: 30,
                convergence_threshold: 0.001,
                time_limit_ms: None,
            },
            sa_params: SaParams::default(),
            sa_elite_ratio: 0.1, // 상위 10%에 SA 적용
            sa_frequency: 10,    // 10세대마다
        }
    }
}

impl HybridGaSaConfig {
    /// 문제 특성에 맞게 자동 튜닝된 설정 생성
    pub fn auto_tune(jobs: &[Job], resources: &[Resource]) -> Self {
        let characteristics = ProblemCharacteristics::analyze(jobs, resources);
        let ga_params = GaAutoTuner::tune(&characteristics);
        let sa_params = Self::tune_sa_params(&characteristics);
        let (sa_elite_ratio, sa_frequency) = Self::tune_sa_schedule(&characteristics);

        Self {
            ga_params,
            sa_params,
            sa_elite_ratio,
            sa_frequency,
        }
    }

    /// SA 파라미터 자동 튜닝
    fn tune_sa_params(chars: &ProblemCharacteristics) -> SaParams {
        let complexity = chars.complexity_score();

        // 복잡한 문제일수록:
        // - 높은 초기 온도 (더 넓은 탐색)
        // - 낮은 냉각률 (천천히 수렴)
        let initial_temperature = if complexity > 0.7 {
            2000.0
        } else if complexity > 0.4 {
            1000.0
        } else {
            500.0
        };

        let cooling_rate = if complexity > 0.7 {
            0.92
        } else if complexity > 0.4 {
            0.95
        } else {
            0.97
        };

        let iterations_per_temperature = if complexity > 0.7 {
            15
        } else if complexity > 0.4 {
            10
        } else {
            5
        };

        SaParams {
            initial_temperature,
            final_temperature: 1.0,
            cooling_rate,
            iterations_per_temperature,
        }
    }

    /// SA 적용 스케줄 자동 튜닝
    fn tune_sa_schedule(chars: &ProblemCharacteristics) -> (f64, usize) {
        let complexity = chars.complexity_score();

        // 복잡한 문제일수록:
        // - 더 많은 elite에 SA 적용
        // - 더 자주 SA 적용
        let sa_elite_ratio = if complexity > 0.7 {
            0.15
        } else if complexity > 0.4 {
            0.10
        } else {
            0.05
        };

        let sa_frequency = if complexity > 0.7 {
            5
        } else if complexity > 0.4 {
            10
        } else {
            15
        };

        (sa_elite_ratio, sa_frequency)
    }
}

/// 하이브리드 GA-SA 결과
#[derive(Debug, Clone)]
pub struct HybridResult {
    /// 최적 스케줄
    pub schedule: Schedule,
    /// 최종 적합도
    pub fitness: f64,
    /// GA 세대 수
    pub ga_generations: usize,
    /// SA 적용 횟수
    pub sa_applications: usize,
    /// SA 개선 횟수
    pub sa_improvements: usize,
    /// 수렴 여부
    pub converged: bool,
}

/// 하이브리드 GA-SA 스케줄러
pub struct HybridGaSaScheduler {
    config: HybridGaSaConfig,
    operators: GeneticOperators,
    setup_matrices: SetupMatrixCollection,
    site_transitions: SiteTransitions,
}

impl HybridGaSaScheduler {
    pub fn new(config: HybridGaSaConfig, operators: GeneticOperators) -> Self {
        Self {
            config,
            operators,
            setup_matrices: SetupMatrixCollection::new(),
            site_transitions: SiteTransitions::new(),
        }
    }

    pub fn with_setup_matrices(mut self, matrices: SetupMatrixCollection) -> Self {
        self.setup_matrices = matrices;
        self
    }

    pub fn with_site_transitions(mut self, transitions: SiteTransitions) -> Self {
        self.site_transitions = transitions;
        self
    }

    /// 하이브리드 최적화 실행
    pub fn optimize(
        &self,
        jobs: &[Job],
        resources: &[Resource],
        start_time_ms: i64,
    ) -> HybridResult {
        let mut rng = rand::rng();

        // GA 스케줄러 생성
        let ga_scheduler = GaScheduler {
            params: self.config.ga_params.clone(),
            operators: self.operators.clone(),
            setup_matrices: self.setup_matrices.clone(),
            site_transitions: self.site_transitions.clone(),
        };

        // 연산 정보 추출
        let operations = self.extract_operations(jobs);

        // 초기 인구 생성
        let mut population = self.initialize_population(&operations, &mut rng);

        // 적합도 평가
        self.evaluate_population(
            &mut population,
            &ga_scheduler,
            jobs,
            resources,
            start_time_ms,
        );

        let mut sa_applications = 0;
        let mut sa_improvements = 0;
        let mut generations = 0;
        let mut converged = false;
        let mut best_fitness = f64::INFINITY;
        let mut stagnant_generations = 0;

        // 진화 루프
        for gen in 0..self.config.ga_params.max_generations {
            generations = gen + 1;

            // 선택, 교차, 돌연변이
            population = self.evolve_population(population, &operations, &mut rng);

            // 적합도 평가
            self.evaluate_population(
                &mut population,
                &ga_scheduler,
                jobs,
                resources,
                start_time_ms,
            );

            // 정렬 (적합도 오름차순)
            population.sort_by(|a, b| {
                a.fitness
                    .partial_cmp(&b.fitness)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            // SA 적용 (주기적으로)
            if gen > 0 && gen % self.config.sa_frequency == 0 {
                let (apps, imps) = self.apply_sa_to_elites(
                    &mut population,
                    &operations,
                    &ga_scheduler,
                    jobs,
                    resources,
                    start_time_ms,
                    &mut rng,
                );
                sa_applications += apps;
                sa_improvements += imps;
            }

            // 최적해 업데이트
            let current_best = population[0].fitness;
            if current_best < best_fitness {
                let improvement = (best_fitness - current_best) / best_fitness;
                best_fitness = current_best;

                if improvement < self.config.ga_params.convergence_threshold {
                    stagnant_generations += 1;
                } else {
                    stagnant_generations = 0;
                }
            } else {
                stagnant_generations += 1;
            }

            // 수렴 체크
            if stagnant_generations >= self.config.ga_params.convergence_generations {
                converged = true;
                break;
            }
        }

        // 최종 SA 정제
        let (apps, imps) = self.apply_sa_to_elites(
            &mut population,
            &operations,
            &ga_scheduler,
            jobs,
            resources,
            start_time_ms,
            &mut rng,
        );
        sa_applications += apps;
        sa_improvements += imps;

        // 최적 해 디코딩
        let best_chromosome = &population[0];
        let schedule = ga_scheduler.decode(best_chromosome, jobs, resources, start_time_ms);

        HybridResult {
            schedule,
            fitness: population[0].fitness,
            ga_generations: generations,
            sa_applications,
            sa_improvements,
            converged,
        }
    }

    fn extract_operations(&self, jobs: &[Job]) -> Vec<OperationInfo> {
        let mut operations = Vec::new();

        for job in jobs {
            for op in &job.operations {
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

        operations.sort_by(|a, b| (&a.task_id, a.sequence).cmp(&(&b.task_id, b.sequence)));

        operations
    }

    fn initialize_population(
        &self,
        operations: &[OperationInfo],
        rng: &mut impl Rng,
    ) -> Vec<ScheduleChromosome> {
        let activities = to_activity_infos(operations);
        (0..self.config.ga_params.population_size)
            .map(|_| ScheduleChromosome::random(&activities, rng))
            .collect()
    }

    fn evaluate_population(
        &self,
        population: &mut [ScheduleChromosome],
        ga_scheduler: &GaScheduler,
        jobs: &[Job],
        resources: &[Resource],
        start_time_ms: i64,
    ) {
        for chromosome in population.iter_mut() {
            if chromosome.fitness == f64::INFINITY {
                let schedule = ga_scheduler.decode(chromosome, jobs, resources, start_time_ms);
                chromosome.fitness = calculate_fitness(&schedule, jobs);
            }
        }
    }

    fn evolve_population(
        &self,
        population: Vec<ScheduleChromosome>,
        operations: &[OperationInfo],
        rng: &mut impl Rng,
    ) -> Vec<ScheduleChromosome> {
        let activities = to_activity_infos(operations);
        let mut new_population = Vec::with_capacity(self.config.ga_params.population_size);

        // 엘리트 보존
        let elite_count = (self.config.ga_params.population_size as f64
            * self.config.ga_params.elite_ratio) as usize;

        for ind in population.iter().take(elite_count) {
            new_population.push(ind.clone());
        }

        // 나머지는 교차 및 돌연변이
        while new_population.len() < self.config.ga_params.population_size {
            let parent1 = self.tournament_select(&population, rng);
            let parent2 = self.tournament_select(&population, rng);

            let (mut child1, mut child2) =
                self.operators.crossover(parent1, parent2, &activities, rng);

            self.operators.mutate(&mut child1, &activities, rng);
            self.operators.mutate(&mut child2, &activities, rng);

            new_population.push(child1);
            if new_population.len() < self.config.ga_params.population_size {
                new_population.push(child2);
            }
        }

        new_population
    }

    fn tournament_select<'a>(
        &self,
        population: &'a [ScheduleChromosome],
        rng: &mut impl Rng,
    ) -> &'a ScheduleChromosome {
        let mut best_idx = rng.random_range(0..population.len());

        for _ in 1..self.config.ga_params.tournament_size {
            let idx = rng.random_range(0..population.len());
            if population[idx].fitness < population[best_idx].fitness {
                best_idx = idx;
            }
        }

        &population[best_idx]
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_sa_to_elites(
        &self,
        population: &mut [ScheduleChromosome],
        operations: &[OperationInfo],
        ga_scheduler: &GaScheduler,
        jobs: &[Job],
        resources: &[Resource],
        start_time_ms: i64,
        rng: &mut impl Rng,
    ) -> (usize, usize) {
        let elite_count = (population.len() as f64 * self.config.sa_elite_ratio) as usize;
        let elite_count = elite_count.max(1);

        let mut applications = 0;
        let mut improvements = 0;

        for chromosome in population.iter_mut().take(elite_count) {
            let original_fitness = chromosome.fitness;

            let improved = self.simulated_annealing(
                chromosome,
                operations,
                ga_scheduler,
                jobs,
                resources,
                start_time_ms,
                rng,
            );

            applications += 1;
            if improved < original_fitness {
                improvements += 1;
            }
        }

        (applications, improvements)
    }

    #[allow(clippy::too_many_arguments)]
    fn simulated_annealing(
        &self,
        chromosome: &mut ScheduleChromosome,
        operations: &[OperationInfo],
        ga_scheduler: &GaScheduler,
        jobs: &[Job],
        resources: &[Resource],
        start_time_ms: i64,
        rng: &mut impl Rng,
    ) -> f64 {
        let mut current = chromosome.clone();
        let mut current_fitness = current.fitness;
        let mut best = current.clone();
        let mut best_fitness = current_fitness;

        let mut temperature = self.config.sa_params.initial_temperature;

        while temperature > self.config.sa_params.final_temperature {
            for _ in 0..self.config.sa_params.iterations_per_temperature {
                // 이웃 해 생성 (작은 변이)
                let mut neighbor = current.clone();
                neighbor.fitness = f64::INFINITY;
                self.small_mutation(&mut neighbor, operations, rng);

                // 적합도 계산
                let schedule = ga_scheduler.decode(&neighbor, jobs, resources, start_time_ms);
                let neighbor_fitness = calculate_fitness(&schedule, jobs);
                neighbor.fitness = neighbor_fitness;

                // 수락 여부 결정
                let delta = neighbor_fitness - current_fitness;

                if delta < 0.0 || rng.random::<f64>() < (-delta / temperature).exp() {
                    current = neighbor;
                    current_fitness = current.fitness;

                    if current_fitness < best_fitness {
                        best = current.clone();
                        best_fitness = current_fitness;
                    }
                }
            }

            temperature *= self.config.sa_params.cooling_rate;
        }

        *chromosome = best;
        best_fitness
    }

    fn small_mutation(
        &self,
        chromosome: &mut ScheduleChromosome,
        operations: &[OperationInfo],
        rng: &mut impl Rng,
    ) {
        // 50% 확률로 OSV 또는 MAV 변이
        if rng.random_bool(0.5) {
            // OSV: 인접 교환
            if chromosome.osv.len() >= 2 {
                let idx = rng.random_range(0..chromosome.osv.len() - 1);
                chromosome.osv.swap(idx, idx + 1);
            }
        } else {
            // MAV: 단일 기계 변경
            if !chromosome.mav.is_empty() && !operations.is_empty() {
                let idx = rng.random_range(0..chromosome.mav.len().min(operations.len()));
                if !operations[idx].candidates.is_empty() {
                    let new_machine = operations[idx].candidates.choose(rng).unwrap().clone();
                    chromosome.mav[idx] = new_machine;
                }
            }
        }
    }
}

/// 적합도 계산 (낮을수록 좋음)
fn calculate_fitness(schedule: &Schedule, jobs: &[Job]) -> f64 {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Operation;

    fn create_test_scenario() -> (Vec<Job>, Vec<Resource>) {
        let jobs: Vec<Job> = (0..10)
            .map(|i| {
                let job_id = format!("J{}", i);
                Job::new(&job_id)
                    .with_product(format!("P{}", i % 3))
                    .with_priority(i % 3 + 1)
                    .with_operation(
                        Operation::new(format!("{}-O1", job_id), &job_id, 1)
                            .with_time(0, 10_000 + (i * 1000) as i64, 0)
                            .with_equipment(vec!["M1".to_string(), "M2".to_string()]),
                    )
            })
            .collect();

        let resources = vec![
            Resource::equipment("M1").with_efficiency(1.0),
            Resource::equipment("M2").with_efficiency(0.9),
        ];

        (jobs, resources)
    }

    #[test]
    fn test_hybrid_basic() {
        let (jobs, resources) = create_test_scenario();

        let config = HybridGaSaConfig {
            ga_params: GaParams {
                population_size: 20,
                max_generations: 10,
                ..Default::default()
            },
            sa_params: SaParams {
                initial_temperature: 100.0,
                final_temperature: 1.0,
                cooling_rate: 0.9,
                iterations_per_temperature: 5,
            },
            sa_frequency: 3,
            ..Default::default()
        };

        let scheduler = HybridGaSaScheduler::new(config, GeneticOperators::default());
        let result = scheduler.optimize(&jobs, &resources, 0);

        assert!(result.fitness < f64::INFINITY);
        assert!(result.ga_generations > 0);
        assert!(result.schedule.makespan_ms > 0);
    }

    #[test]
    fn test_sa_improves_solution() {
        let (jobs, resources) = create_test_scenario();

        let config = HybridGaSaConfig {
            ga_params: GaParams {
                population_size: 30,
                max_generations: 20,
                ..Default::default()
            },
            sa_frequency: 5,
            ..Default::default()
        };

        let scheduler = HybridGaSaScheduler::new(config, GeneticOperators::default());
        let result = scheduler.optimize(&jobs, &resources, 0);

        // SA가 적용되었는지 확인
        assert!(result.sa_applications > 0);
    }

    #[test]
    fn test_convergence() {
        let (jobs, resources) = create_test_scenario();

        let config = HybridGaSaConfig {
            ga_params: GaParams {
                population_size: 30,
                max_generations: 100,
                convergence_generations: 10,
                ..Default::default()
            },
            ..Default::default()
        };

        let scheduler = HybridGaSaScheduler::new(config, GeneticOperators::default());
        let result = scheduler.optimize(&jobs, &resources, 0);

        // 수렴했거나 최대 세대에 도달
        assert!(result.converged || result.ga_generations == 100);
    }

    #[test]
    fn test_different_temperatures() {
        let (jobs, resources) = create_test_scenario();

        // 높은 온도 (더 많은 탐색)
        let config_high = HybridGaSaConfig {
            ga_params: GaParams {
                population_size: 20,
                max_generations: 10,
                ..Default::default()
            },
            sa_params: SaParams {
                initial_temperature: 10000.0,
                ..Default::default()
            },
            sa_frequency: 3,
            ..Default::default()
        };

        let scheduler = HybridGaSaScheduler::new(config_high, GeneticOperators::default());
        let result = scheduler.optimize(&jobs, &resources, 0);

        assert!(result.fitness < f64::INFINITY);
    }
}
