//! NSGA-II - Non-dominated Sorting Genetic Algorithm II
//!
//! 다목적 최적화를 위한 NSGA-II 구현
//! - Pareto 프론트 기반 비지배 정렬
//! - Crowding Distance로 다양성 유지
//! - 여러 목적 함수 동시 최적화 (Makespan, Tardiness, Utilization 등)

use crate::ga::{
    to_activity_infos, GaParams, GaScheduler, GeneticOperators, OperationInfo, ScheduleChromosome,
};
use crate::models::{Job, Resource, SetupMatrixCollection, SiteTransitions};
use crate::scheduler::Schedule;
use rand::prelude::*;
use rayon::prelude::*;
use std::collections::HashMap;

/// 다목적 최적화 목표
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NsgaObjective {
    /// Makespan 최소화
    Makespan,
    /// 총 지연 시간 최소화
    TotalTardiness,
    /// 평균 흐름 시간 최소화
    MeanFlowTime,
    /// 자원 가동률 최대화 (반전하여 최소화)
    Utilization,
    /// 납기 준수율 최대화 (반전하여 최소화)
    OnTimeDelivery,
}

/// 개체의 목적 함수 값
#[derive(Debug, Clone, Default)]
pub struct NsgaObjectiveValues {
    /// 목적별 값 (최소화 방향으로 정규화)
    pub values: HashMap<NsgaObjective, f64>,
}

impl NsgaObjectiveValues {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, obj: &NsgaObjective) -> f64 {
        *self.values.get(obj).unwrap_or(&f64::INFINITY)
    }

    /// 다른 개체를 지배하는지 확인
    pub fn dominates(&self, other: &NsgaObjectiveValues) -> bool {
        let mut dominated_in_any = false;
        let mut equal_in_all = true;

        for (obj, &val) in &self.values {
            let other_val = other.get(obj);
            if val > other_val {
                return false; // 하나라도 열등하면 지배하지 않음
            }
            if val < other_val {
                dominated_in_any = true;
                equal_in_all = false;
            }
        }

        dominated_in_any && !equal_in_all
    }
}

/// NSGA-II 개체
#[derive(Debug, Clone)]
pub struct NsgaIndividual {
    /// 염색체
    pub chromosome: ScheduleChromosome,
    /// 목적 함수 값
    pub objectives: NsgaObjectiveValues,
    /// 비지배 순위 (낮을수록 좋음)
    pub rank: usize,
    /// Crowding Distance (높을수록 좋음)
    pub crowding_distance: f64,
}

/// NSGA-II 결과
#[derive(Debug, Clone)]
pub struct Nsga2Result {
    /// Pareto 프론트 (비지배 해 집합)
    pub pareto_front: Vec<NsgaIndividual>,
    /// 최종 인구
    pub final_population: Vec<NsgaIndividual>,
    /// 세대 수
    pub generations: usize,
    /// 수렴 여부
    pub converged: bool,
}

impl Nsga2Result {
    /// 특정 목적 기준 최적 해 반환
    pub fn best_for_objective(&self, obj: NsgaObjective) -> Option<&NsgaIndividual> {
        self.pareto_front.iter().min_by(|a, b| {
            a.objectives
                .get(&obj)
                .partial_cmp(&b.objectives.get(&obj))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// 균형잡힌 해 반환 (crowding distance 기준)
    pub fn balanced_solution(&self) -> Option<&NsgaIndividual> {
        self.pareto_front.iter().max_by(|a, b| {
            a.crowding_distance
                .partial_cmp(&b.crowding_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }
}

/// NSGA-II 설정
#[derive(Debug, Clone)]
pub struct Nsga2Config {
    /// GA 파라미터
    pub params: GaParams,
    /// 최적화 목표들
    pub objectives: Vec<NsgaObjective>,
}

impl Default for Nsga2Config {
    fn default() -> Self {
        Self {
            params: GaParams {
                population_size: 100,
                max_generations: 200,
                elite_ratio: 0.0, // NSGA-II는 엘리트 선택 대신 비지배 정렬 사용
                tournament_size: 2,
                convergence_generations: 30,
                convergence_threshold: 0.001,
                time_limit_ms: None,
            },
            objectives: vec![NsgaObjective::Makespan, NsgaObjective::TotalTardiness],
        }
    }
}

/// NSGA-II 스케줄러
pub struct Nsga2Scheduler {
    config: Nsga2Config,
    operators: GeneticOperators,
    setup_matrices: SetupMatrixCollection,
    site_transitions: SiteTransitions,
}

impl Nsga2Scheduler {
    pub fn new(config: Nsga2Config, operators: GeneticOperators) -> Self {
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

    /// NSGA-II 실행
    pub fn optimize(
        &self,
        jobs: &[Job],
        resources: &[Resource],
        start_time_ms: i64,
    ) -> Nsga2Result {
        let mut rng = rand::rng();

        // 연산 정보 추출
        let operations = self.extract_operations(jobs);

        // 초기 인구 생성
        let mut population = self.initialize_population(&operations, &mut rng);

        // 목적 함수 평가
        self.evaluate_population(&mut population, jobs, resources, start_time_ms);

        // 비지배 정렬 및 crowding distance 계산
        self.fast_non_dominated_sort(&mut population);
        self.calculate_crowding_distance(&mut population);

        let mut generations = 0;
        let mut converged = false;

        // 진화 루프
        for gen in 0..self.config.params.max_generations {
            generations = gen + 1;

            // 자식 생성
            let offspring = self.create_offspring(&population, &operations, &mut rng);

            // 부모 + 자식 결합
            let mut combined: Vec<NsgaIndividual> =
                population.into_iter().chain(offspring).collect();

            // 목적 함수 평가
            self.evaluate_population(&mut combined, jobs, resources, start_time_ms);

            // 비지배 정렬
            self.fast_non_dominated_sort(&mut combined);

            // 다음 세대 선택
            population = self.select_next_generation(combined);

            // Crowding distance 재계산
            self.calculate_crowding_distance(&mut population);

            // 수렴 체크
            if self.check_convergence(&population) {
                converged = true;
                break;
            }
        }

        // Pareto 프론트 추출
        let pareto_front: Vec<NsgaIndividual> = population
            .iter()
            .filter(|ind| ind.rank == 0)
            .cloned()
            .collect();

        Nsga2Result {
            pareto_front,
            final_population: population,
            generations,
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
    ) -> Vec<NsgaIndividual> {
        let activities = to_activity_infos(operations);
        (0..self.config.params.population_size)
            .map(|_| {
                let chromosome = ScheduleChromosome::random(&activities, rng);
                NsgaIndividual {
                    chromosome,
                    objectives: NsgaObjectiveValues::new(),
                    rank: 0,
                    crowding_distance: 0.0,
                }
            })
            .collect()
    }

    fn evaluate_population(
        &self,
        population: &mut [NsgaIndividual],
        jobs: &[Job],
        resources: &[Resource],
        start_time_ms: i64,
    ) {
        // GaScheduler를 사용하여 디코딩
        let ga_scheduler = GaScheduler {
            params: self.config.params.clone(),
            operators: self.operators.clone(),
            setup_matrices: self.setup_matrices.clone(),
            site_transitions: self.site_transitions.clone(),
        };

        // 병렬 평가
        let objectives: Vec<NsgaObjectiveValues> = population
            .par_iter()
            .map(|ind| {
                let schedule = ga_scheduler.decode(&ind.chromosome, jobs, resources, start_time_ms);
                self.calculate_objectives(&schedule, jobs)
            })
            .collect();

        for (ind, obj) in population.iter_mut().zip(objectives) {
            ind.objectives = obj;
        }
    }

    fn calculate_objectives(&self, schedule: &Schedule, jobs: &[Job]) -> NsgaObjectiveValues {
        let mut values = NsgaObjectiveValues::new();

        // KPI 계산
        let horizon = schedule.makespan_ms.max(1);
        let kpi_calc = crate::scheduler::kpi::KpiCalculator::new(horizon);
        let kpi = kpi_calc.calculate(schedule, jobs);

        for obj in &self.config.objectives {
            let value = match obj {
                NsgaObjective::Makespan => schedule.makespan_ms as f64,
                NsgaObjective::TotalTardiness => kpi.total_tardiness_ms as f64,
                NsgaObjective::MeanFlowTime => kpi.average_flow_time_ms,
                // 최대화 목표는 반전
                NsgaObjective::Utilization => 1.0 - kpi.average_utilization,
                NsgaObjective::OnTimeDelivery => 1.0 - kpi.on_time_delivery_rate,
            };
            values.values.insert(*obj, value);
        }

        values
    }

    fn fast_non_dominated_sort(&self, population: &mut [NsgaIndividual]) {
        let n = population.len();
        let mut domination_count = vec![0usize; n];
        let mut dominated_solutions: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut fronts: Vec<Vec<usize>> = vec![Vec::new()];

        // 지배 관계 계산
        for i in 0..n {
            for j in (i + 1)..n {
                if population[i]
                    .objectives
                    .dominates(&population[j].objectives)
                {
                    dominated_solutions[i].push(j);
                    domination_count[j] += 1;
                } else if population[j]
                    .objectives
                    .dominates(&population[i].objectives)
                {
                    dominated_solutions[j].push(i);
                    domination_count[i] += 1;
                }
            }

            if domination_count[i] == 0 {
                population[i].rank = 0;
                fronts[0].push(i);
            }
        }

        // 프론트 구성
        let mut current_front = 0;
        while !fronts[current_front].is_empty() {
            let mut next_front = Vec::new();
            for &i in &fronts[current_front] {
                for &j in &dominated_solutions[i] {
                    domination_count[j] -= 1;
                    if domination_count[j] == 0 {
                        population[j].rank = current_front + 1;
                        next_front.push(j);
                    }
                }
            }
            current_front += 1;
            fronts.push(next_front);
        }
    }

    fn calculate_crowding_distance(&self, population: &mut [NsgaIndividual]) {
        // 초기화
        for ind in population.iter_mut() {
            ind.crowding_distance = 0.0;
        }

        // 각 목적별로 crowding distance 계산
        for obj in &self.config.objectives {
            // 목적 값으로 정렬
            let mut indices: Vec<usize> = (0..population.len()).collect();
            indices.sort_by(|&a, &b| {
                population[a]
                    .objectives
                    .get(obj)
                    .partial_cmp(&population[b].objectives.get(obj))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let n = indices.len();
            if n < 3 {
                continue;
            }

            // 경계 개체는 무한대
            population[indices[0]].crowding_distance = f64::INFINITY;
            population[indices[n - 1]].crowding_distance = f64::INFINITY;

            // 목적 값 범위
            let min_val = population[indices[0]].objectives.get(obj);
            let max_val = population[indices[n - 1]].objectives.get(obj);
            let range = max_val - min_val;

            if range > 0.0 {
                for i in 1..(n - 1) {
                    let prev_val = population[indices[i - 1]].objectives.get(obj);
                    let next_val = population[indices[i + 1]].objectives.get(obj);
                    population[indices[i]].crowding_distance += (next_val - prev_val) / range;
                }
            }
        }
    }

    fn create_offspring(
        &self,
        population: &[NsgaIndividual],
        operations: &[OperationInfo],
        rng: &mut impl Rng,
    ) -> Vec<NsgaIndividual> {
        let activities = to_activity_infos(operations);
        let mut offspring = Vec::with_capacity(self.config.params.population_size);

        while offspring.len() < self.config.params.population_size {
            // 토너먼트 선택 (rank와 crowding distance 기준)
            let parent1 = self.tournament_select(population, rng);
            let parent2 = self.tournament_select(population, rng);

            // 교차
            let (mut child1, mut child2) = self.operators.crossover(
                &parent1.chromosome,
                &parent2.chromosome,
                &activities,
                rng,
            );

            // 돌연변이
            self.operators.mutate(&mut child1, &activities, rng);
            self.operators.mutate(&mut child2, &activities, rng);

            offspring.push(NsgaIndividual {
                chromosome: child1,
                objectives: NsgaObjectiveValues::new(),
                rank: 0,
                crowding_distance: 0.0,
            });

            if offspring.len() < self.config.params.population_size {
                offspring.push(NsgaIndividual {
                    chromosome: child2,
                    objectives: NsgaObjectiveValues::new(),
                    rank: 0,
                    crowding_distance: 0.0,
                });
            }
        }

        offspring
    }

    fn tournament_select<'a>(
        &self,
        population: &'a [NsgaIndividual],
        rng: &mut impl Rng,
    ) -> &'a NsgaIndividual {
        let mut best_idx = rng.random_range(0..population.len());

        for _ in 1..self.config.params.tournament_size {
            let idx = rng.random_range(0..population.len());
            if self.is_better(&population[idx], &population[best_idx]) {
                best_idx = idx;
            }
        }

        &population[best_idx]
    }

    fn is_better(&self, a: &NsgaIndividual, b: &NsgaIndividual) -> bool {
        // 낮은 rank가 우선
        if a.rank < b.rank {
            return true;
        }
        if a.rank > b.rank {
            return false;
        }
        // 같은 rank면 crowding distance가 큰 것이 우선
        a.crowding_distance > b.crowding_distance
    }

    fn select_next_generation(&self, mut combined: Vec<NsgaIndividual>) -> Vec<NsgaIndividual> {
        // Rank로 정렬
        combined.sort_by(|a, b| {
            a.rank.cmp(&b.rank).then_with(|| {
                b.crowding_distance
                    .partial_cmp(&a.crowding_distance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        });

        // population_size만큼 선택
        combined.truncate(self.config.params.population_size);
        combined
    }

    fn check_convergence(&self, population: &[NsgaIndividual]) -> bool {
        // Pareto 프론트 크기가 안정화되었는지 확인
        let front_size = population.iter().filter(|ind| ind.rank == 0).count();
        front_size >= self.config.params.population_size / 2
    }
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
    fn test_objective_dominance() {
        let mut obj1 = NsgaObjectiveValues::new();
        obj1.values.insert(NsgaObjective::Makespan, 100.0);
        obj1.values.insert(NsgaObjective::TotalTardiness, 50.0);

        let mut obj2 = NsgaObjectiveValues::new();
        obj2.values.insert(NsgaObjective::Makespan, 150.0);
        obj2.values.insert(NsgaObjective::TotalTardiness, 60.0);

        assert!(obj1.dominates(&obj2));
        assert!(!obj2.dominates(&obj1));
    }

    #[test]
    fn test_non_dominance() {
        let mut obj1 = NsgaObjectiveValues::new();
        obj1.values.insert(NsgaObjective::Makespan, 100.0);
        obj1.values.insert(NsgaObjective::TotalTardiness, 60.0);

        let mut obj2 = NsgaObjectiveValues::new();
        obj2.values.insert(NsgaObjective::Makespan, 150.0);
        obj2.values.insert(NsgaObjective::TotalTardiness, 40.0);

        // 서로 지배하지 않음
        assert!(!obj1.dominates(&obj2));
        assert!(!obj2.dominates(&obj1));
    }

    #[test]
    fn test_nsga2_basic() {
        let (jobs, resources) = create_test_scenario();

        let config = Nsga2Config {
            params: GaParams {
                population_size: 20,
                max_generations: 10,
                ..Default::default()
            },
            objectives: vec![NsgaObjective::Makespan, NsgaObjective::TotalTardiness],
        };

        let scheduler = Nsga2Scheduler::new(config, GeneticOperators::default());
        let result = scheduler.optimize(&jobs, &resources, 0);

        assert!(!result.pareto_front.is_empty());
        assert!(result.generations > 0);
    }

    #[test]
    fn test_pareto_front_quality() {
        let (jobs, resources) = create_test_scenario();

        let config = Nsga2Config {
            params: GaParams {
                population_size: 30,
                max_generations: 20,
                ..Default::default()
            },
            objectives: vec![NsgaObjective::Makespan, NsgaObjective::TotalTardiness],
        };

        let scheduler = Nsga2Scheduler::new(config, GeneticOperators::default());
        let result = scheduler.optimize(&jobs, &resources, 0);

        // Pareto 프론트의 모든 해는 rank 0
        for ind in &result.pareto_front {
            assert_eq!(ind.rank, 0);
        }

        // 최소 1개 이상의 해
        assert!(!result.pareto_front.is_empty());
    }

    #[test]
    fn test_best_for_objective() {
        let (jobs, resources) = create_test_scenario();

        let config = Nsga2Config {
            params: GaParams {
                population_size: 30,
                max_generations: 15,
                ..Default::default()
            },
            objectives: vec![NsgaObjective::Makespan, NsgaObjective::TotalTardiness],
        };

        let scheduler = Nsga2Scheduler::new(config, GeneticOperators::default());
        let result = scheduler.optimize(&jobs, &resources, 0);

        let best_makespan = result.best_for_objective(NsgaObjective::Makespan);
        assert!(best_makespan.is_some());

        let best_tardiness = result.best_for_objective(NsgaObjective::TotalTardiness);
        assert!(best_tardiness.is_some());
    }

    #[test]
    fn test_three_objectives() {
        let (jobs, resources) = create_test_scenario();

        let config = Nsga2Config {
            params: GaParams {
                population_size: 30,
                max_generations: 10,
                ..Default::default()
            },
            objectives: vec![
                NsgaObjective::Makespan,
                NsgaObjective::TotalTardiness,
                NsgaObjective::Utilization,
            ],
        };

        let scheduler = Nsga2Scheduler::new(config, GeneticOperators::default());
        let result = scheduler.optimize(&jobs, &resources, 0);

        assert!(!result.pareto_front.is_empty());

        // 모든 목적이 평가됨
        let front = &result.pareto_front[0];
        assert!(front
            .objectives
            .values
            .contains_key(&NsgaObjective::Makespan));
        assert!(front
            .objectives
            .values
            .contains_key(&NsgaObjective::TotalTardiness));
        assert!(front
            .objectives
            .values
            .contains_key(&NsgaObjective::Utilization));
    }
}
