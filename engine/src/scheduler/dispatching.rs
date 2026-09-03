//! Dispatching Rule integration for APS Engine
//!
//! Provides easy-to-use dispatching rule selection for manufacturing scheduling.
//! Supports multi-layer dispatching with dynamic rule switching.

use crate::Job;
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use u_schedule::dispatching::{rules, RuleEngine, SchedulingContext};

/// Available dispatching rules for job prioritization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DispatchingRuleName {
    // Time-based rules
    /// Shortest Processing Time - prioritize shorter jobs
    Spt,
    /// Longest Processing Time - prioritize longer jobs
    Lpt,
    /// Least Work Remaining - prioritize jobs near completion
    Lwkr,
    /// Most Work Remaining - prioritize jobs with most work left
    Mwkr,
    /// Weighted Shortest Processing Time - prioritize by weight/time ratio
    Wspt,

    // Due date rules
    /// Earliest Due Date - prioritize nearest deadlines
    Edd,
    /// Minimum Slack Time - prioritize jobs with least buffer
    Mst,
    /// Critical Ratio - prioritize jobs falling behind schedule
    Cr,
    /// Slack per Remaining Operations
    Sro,
    /// Apparent Tardiness Cost - balance SPT and due date urgency
    Atc,

    // Queue/Load rules
    /// First In First Out - process in arrival order
    #[default]
    Fifo,
    /// Work In Next Queue - prefer shorter downstream queues
    Winq,
    /// Least Planned Utilization Level - use underutilized resources
    Lpul,

    // Priority-based (uses Job.priority field)
    /// Job priority field (lower value = higher priority)
    Priority,
}

impl DispatchingRuleName {
    /// Add this rule to a RuleEngine with the given weight.
    ///
    /// Uses concrete types to satisfy u-schedule's generic `with_weighted_rule<R>`.
    fn add_to_engine(self, engine: RuleEngine, weight: f64) -> RuleEngine {
        match self {
            Self::Spt => engine.with_weighted_rule(rules::Spt, weight),
            Self::Lpt => engine.with_weighted_rule(rules::Lpt, weight),
            Self::Lwkr => engine.with_weighted_rule(rules::Lwkr, weight),
            Self::Mwkr => engine.with_weighted_rule(rules::Mwkr, weight),
            Self::Wspt => engine.with_weighted_rule(rules::Wspt, weight),
            Self::Edd => engine.with_weighted_rule(rules::Edd, weight),
            Self::Mst => engine.with_weighted_rule(rules::Mst, weight),
            Self::Cr => engine.with_weighted_rule(rules::Cr, weight),
            Self::Sro => engine.with_weighted_rule(rules::Sro, weight),
            Self::Atc => engine.with_weighted_rule(rules::Atc::default(), weight),
            Self::Fifo => engine.with_weighted_rule(rules::Fifo, weight),
            Self::Winq => engine.with_weighted_rule(rules::Winq, weight),
            Self::Lpul => engine.with_weighted_rule(rules::Lpul, weight),
            Self::Priority => engine.with_weighted_rule(PriorityRule, weight),
        }
    }

    /// Get the name as string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Spt => "SPT",
            Self::Lpt => "LPT",
            Self::Lwkr => "LWKR",
            Self::Mwkr => "MWKR",
            Self::Wspt => "WSPT",
            Self::Edd => "EDD",
            Self::Mst => "MST",
            Self::Cr => "CR",
            Self::Sro => "S/RO",
            Self::Atc => "ATC",
            Self::Fifo => "FIFO",
            Self::Winq => "WINQ",
            Self::Lpul => "LPUL",
            Self::Priority => "PRIORITY",
        }
    }

    /// Parse from string (case-insensitive)
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "SPT" => Some(Self::Spt),
            "LPT" => Some(Self::Lpt),
            "LWKR" => Some(Self::Lwkr),
            "MWKR" => Some(Self::Mwkr),
            "WSPT" => Some(Self::Wspt),
            "EDD" => Some(Self::Edd),
            "MST" => Some(Self::Mst),
            "CR" => Some(Self::Cr),
            "S/RO" | "SRO" => Some(Self::Sro),
            "ATC" => Some(Self::Atc),
            "FIFO" => Some(Self::Fifo),
            "WINQ" => Some(Self::Winq),
            "LPUL" => Some(Self::Lpul),
            "PRIORITY" => Some(Self::Priority),
            _ => None,
        }
    }
}

/// Priority rule - uses Job.priority field (lower value = higher priority)
#[derive(Debug, Clone, Copy)]
pub struct PriorityRule;

impl u_schedule::dispatching::DispatchingRule for PriorityRule {
    fn name(&self) -> &'static str {
        "PRIORITY"
    }

    fn description(&self) -> &'static str {
        "Job Priority - prioritize by job priority field"
    }

    fn evaluate(&self, task: &u_schedule::models::Task, _context: &SchedulingContext) -> f64 {
        task.priority as f64
    }
}

/// Configuration for dispatching strategy
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DispatchingConfig {
    /// Primary dispatching rule
    pub primary_rule: DispatchingRuleName,
    /// Optional tie-breaker rule
    pub tie_breaker: Option<DispatchingRuleName>,
}

impl DispatchingConfig {
    pub fn new(rule: DispatchingRuleName) -> Self {
        Self {
            primary_rule: rule,
            tie_breaker: None,
        }
    }

    pub fn with_tie_breaker(mut self, rule: DispatchingRuleName) -> Self {
        self.tie_breaker = Some(rule);
        self
    }

    /// Build a RuleEngine from this configuration
    pub fn build_engine(&self) -> RuleEngine {
        let mut engine = RuleEngine::new();

        // Add primary rule (weight = 1.0)
        engine = self.primary_rule.add_to_engine(engine, 1.0);

        // Add tie-breaker if specified (weight = 0.0, used only for tie-breaking)
        if let Some(tb) = &self.tie_breaker {
            engine = tb.add_to_engine(engine, 0.0);
        }

        engine
    }
}

// ============================================================================
// Phase 13: Multi-Layer Dispatching
// ============================================================================

/// 다층 디스패칭 레이어
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchingLayer {
    /// 레이어 이름
    pub name: String,
    /// 규칙
    pub rule: DispatchingRuleName,
    /// 가중치 (0.0 = tie-breaker only, 1.0 = primary)
    pub weight: f64,
}

impl DispatchingLayer {
    pub fn new(name: impl Into<String>, rule: DispatchingRuleName) -> Self {
        Self {
            name: name.into(),
            rule,
            weight: 1.0,
        }
    }

    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight.clamp(0.0, 1.0);
        self
    }

    pub fn as_tie_breaker(mut self) -> Self {
        self.weight = 0.0;
        self
    }
}

/// 동적 규칙 전환 조건
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleSwitchCondition {
    /// 시간 기반: 특정 시간 이후 규칙 변경
    TimeAfter { threshold_ms: i64 },
    /// 시간 기반: 특정 시간 이전 규칙 변경
    TimeBefore { threshold_ms: i64 },
    /// 지연 기반: 지연 작업이 N개 이상일 때
    DelayedJobsExceed { count: usize },
    /// 자원 활용률 기반
    UtilizationAbove { resource_id: String, threshold: f64 },
    /// 자원 활용률 기반
    UtilizationBelow { resource_id: String, threshold: f64 },
    /// 대기 작업 수 기반
    QueueLengthAbove { threshold: usize },
    /// 긴급 작업 존재 시
    HasUrgentJobs { priority_threshold: i32 },
    /// 항상 적용
    Always,
}

impl RuleSwitchCondition {
    /// 조건 평가
    pub fn evaluate(&self, context: &DispatchingContext) -> bool {
        match self {
            Self::TimeAfter { threshold_ms } => context.current_time_ms >= *threshold_ms,
            Self::TimeBefore { threshold_ms } => context.current_time_ms < *threshold_ms,
            Self::DelayedJobsExceed { count } => context.delayed_job_count >= *count,
            Self::UtilizationAbove {
                resource_id,
                threshold,
            } => context
                .resource_utilization
                .get(resource_id)
                .map(|u| *u >= *threshold)
                .unwrap_or(false),
            Self::UtilizationBelow {
                resource_id,
                threshold,
            } => context
                .resource_utilization
                .get(resource_id)
                .map(|u| *u < *threshold)
                .unwrap_or(false),
            Self::QueueLengthAbove { threshold } => context.queue_length >= *threshold,
            Self::HasUrgentJobs { priority_threshold } => context
                .min_priority
                .map(|p| p <= *priority_threshold)
                .unwrap_or(false),
            Self::Always => true,
        }
    }
}

/// 동적 규칙 전환 규칙
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSwitchRule {
    /// 조건
    pub condition: RuleSwitchCondition,
    /// 조건 충족 시 적용할 규칙
    pub target_rule: DispatchingRuleName,
    /// 우선순위 (높을수록 먼저 평가)
    pub priority: i32,
}

impl RuleSwitchRule {
    pub fn new(condition: RuleSwitchCondition, rule: DispatchingRuleName) -> Self {
        Self {
            condition,
            target_rule: rule,
            priority: 0,
        }
    }

    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

/// 디스패칭 컨텍스트 - 동적 규칙 전환용
#[derive(Debug, Clone, Default)]
pub struct DispatchingContext {
    /// 현재 시간 (ms)
    pub current_time_ms: i64,
    /// 지연된 작업 수
    pub delayed_job_count: usize,
    /// 자원별 활용률
    pub resource_utilization: HashMap<String, f64>,
    /// 대기 작업 수
    pub queue_length: usize,
    /// 최소 우선순위 (가장 긴급한 작업)
    pub min_priority: Option<i32>,
}

impl DispatchingContext {
    pub fn new(current_time_ms: i64) -> Self {
        Self {
            current_time_ms,
            ..Default::default()
        }
    }

    /// Job 목록에서 컨텍스트 생성
    pub fn from_jobs(jobs: &[Job], current_time_ms: i64) -> Self {
        let mut ctx = Self::new(current_time_ms);
        ctx.queue_length = jobs.len();

        // 지연 작업 수 계산
        let current_time = DateTime::from_timestamp_millis(current_time_ms);
        ctx.delayed_job_count = jobs
            .iter()
            .filter(|j| {
                j.due_date
                    .map(|d| current_time.map(|ct| d < ct).unwrap_or(false))
                    .unwrap_or(false)
            })
            .count();

        // 최소 우선순위
        ctx.min_priority = jobs.iter().map(|j| j.priority).min();

        ctx
    }

    pub fn with_utilization(mut self, resource_id: impl Into<String>, utilization: f64) -> Self {
        self.resource_utilization
            .insert(resource_id.into(), utilization);
        self
    }
}

/// 다층 디스패칭 설정
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MultiLayerDispatchingConfig {
    /// 레이어 목록 (순서대로 적용)
    pub layers: Vec<DispatchingLayer>,
    /// 동적 전환 규칙
    #[serde(default)]
    pub switch_rules: Vec<RuleSwitchRule>,
    /// 기본 규칙 (전환 규칙 미적용 시)
    pub default_rule: DispatchingRuleName,
}

impl MultiLayerDispatchingConfig {
    pub fn new(default_rule: DispatchingRuleName) -> Self {
        Self {
            layers: Vec::new(),
            switch_rules: Vec::new(),
            default_rule,
        }
    }

    /// 레이어 추가
    pub fn with_layer(mut self, layer: DispatchingLayer) -> Self {
        self.layers.push(layer);
        self
    }

    /// 전환 규칙 추가
    pub fn with_switch_rule(mut self, rule: RuleSwitchRule) -> Self {
        self.switch_rules.push(rule);
        self
    }

    /// 동적으로 적용할 규칙 결정
    pub fn select_rule(&self, context: &DispatchingContext) -> DispatchingRuleName {
        // 우선순위 순으로 전환 규칙 평가
        let mut sorted_rules = self.switch_rules.clone();
        sorted_rules.sort_by_key(|r| std::cmp::Reverse(r.priority));

        for rule in &sorted_rules {
            if rule.condition.evaluate(context) {
                return rule.target_rule;
            }
        }

        self.default_rule
    }

    /// RuleEngine 빌드
    pub fn build_engine(&self, context: &DispatchingContext) -> RuleEngine {
        let mut engine = RuleEngine::new();

        // 동적 규칙 선택
        let dynamic_rule = self.select_rule(context);

        // 레이어가 있으면 레이어 사용, 없으면 동적 규칙 사용
        if self.layers.is_empty() {
            engine = dynamic_rule.add_to_engine(engine, 1.0);
        } else {
            for layer in &self.layers {
                engine = layer.rule.add_to_engine(engine, layer.weight);
            }
        }

        engine
    }

    /// 단순 설정으로 변환 (호환성)
    pub fn to_simple_config(&self) -> DispatchingConfig {
        let primary = self
            .layers
            .first()
            .map(|l| l.rule)
            .unwrap_or(self.default_rule);

        let tie_breaker = self
            .layers
            .get(1)
            .filter(|l| l.weight == 0.0)
            .map(|l| l.rule);

        DispatchingConfig {
            primary_rule: primary,
            tie_breaker,
        }
    }
}

/// 프리셋 디스패칭 전략
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DispatchingPreset {
    /// 납기 준수 우선 (EDD + SPT tie-breaker)
    DueDateFocused,
    /// 처리량 최대화 (SPT)
    ThroughputMaximize,
    /// 균형 (Critical Ratio)
    Balanced,
    /// 긴급 대응 (Priority + EDD)
    UrgencyResponse,
    /// 자원 부하 균형 (LPUL + WINQ)
    LoadBalancing,
    /// 선입선출 (FIFO)
    FairQueue,
}

impl DispatchingPreset {
    /// 프리셋을 MultiLayerDispatchingConfig로 변환
    pub fn to_config(&self) -> MultiLayerDispatchingConfig {
        match self {
            Self::DueDateFocused => MultiLayerDispatchingConfig::new(DispatchingRuleName::Edd)
                .with_layer(DispatchingLayer::new("due_date", DispatchingRuleName::Edd))
                .with_layer(
                    DispatchingLayer::new("tie_breaker", DispatchingRuleName::Spt).as_tie_breaker(),
                ),
            Self::ThroughputMaximize => {
                MultiLayerDispatchingConfig::new(DispatchingRuleName::Spt).with_layer(
                    DispatchingLayer::new("shortest_first", DispatchingRuleName::Spt),
                )
            }
            Self::Balanced => MultiLayerDispatchingConfig::new(DispatchingRuleName::Cr)
                .with_layer(DispatchingLayer::new(
                    "critical_ratio",
                    DispatchingRuleName::Cr,
                ))
                .with_layer(
                    DispatchingLayer::new("tie_breaker", DispatchingRuleName::Fifo)
                        .as_tie_breaker(),
                ),
            Self::UrgencyResponse => {
                MultiLayerDispatchingConfig::new(DispatchingRuleName::Priority)
                    .with_layer(DispatchingLayer::new(
                        "priority",
                        DispatchingRuleName::Priority,
                    ))
                    .with_layer(
                        DispatchingLayer::new("due_date", DispatchingRuleName::Edd)
                            .with_weight(0.5),
                    )
            }
            Self::LoadBalancing => MultiLayerDispatchingConfig::new(DispatchingRuleName::Lpul)
                .with_layer(DispatchingLayer::new(
                    "utilization",
                    DispatchingRuleName::Lpul,
                ))
                .with_layer(
                    DispatchingLayer::new("queue", DispatchingRuleName::Winq).with_weight(0.5),
                ),
            Self::FairQueue => MultiLayerDispatchingConfig::new(DispatchingRuleName::Fifo)
                .with_layer(DispatchingLayer::new("fifo", DispatchingRuleName::Fifo)),
        }
    }

    /// 동적 전환 규칙이 포함된 설정 생성
    pub fn to_adaptive_config(&self) -> MultiLayerDispatchingConfig {
        let mut config = self.to_config();

        // 긴급 상황 시 Priority로 전환
        config = config.with_switch_rule(
            RuleSwitchRule::new(
                RuleSwitchCondition::HasUrgentJobs {
                    priority_threshold: 1,
                },
                DispatchingRuleName::Priority,
            )
            .with_priority(100),
        );

        // 지연 작업 많을 시 EDD로 전환
        config = config.with_switch_rule(
            RuleSwitchRule::new(
                RuleSwitchCondition::DelayedJobsExceed { count: 5 },
                DispatchingRuleName::Edd,
            )
            .with_priority(50),
        );

        config
    }
}

/// Convert Jobs to scheduling Tasks for rule evaluation
pub fn jobs_to_tasks(jobs: &[Job], _current_time_ms: i64) -> Vec<u_schedule::models::Task> {
    jobs.iter()
        .map(|job| {
            let mut task = u_schedule::models::Task::new(&job.id)
                .with_name(&job.id)
                .with_category(job.product_name.clone().unwrap_or_default())
                .with_priority(job.priority);

            // Convert DateTime<Utc> to milliseconds
            if let Some(due) = job.due_date {
                task.deadline = Some(due.timestamp_millis());
            }
            if let Some(start) = job.earliest_start {
                task.release_time = Some(start.timestamp_millis());
            }

            // Convert operations to activities
            for op in &job.operations {
                task = task.with_activity(
                    u_schedule::models::Activity::new(&op.id, &job.id, op.sequence).with_duration(
                        u_schedule::models::ActivityDuration::new(
                            op.time.setup_ms,
                            op.time.process_ms,
                            0, // teardown not used in APS
                        ),
                    ),
                );
            }

            task
        })
        .collect()
}

/// Build scheduling context from current state
pub fn build_context(current_time_ms: i64) -> SchedulingContext {
    SchedulingContext::at_time(current_time_ms)
}

/// Sort jobs by dispatching rule
///
/// Returns indices in priority order (first = highest priority)
pub fn sort_jobs_by_rule(
    jobs: &[Job],
    config: &DispatchingConfig,
    current_time_ms: i64,
) -> Vec<usize> {
    let tasks = jobs_to_tasks(jobs, current_time_ms);
    let context = build_context(current_time_ms);
    let engine = config.build_engine();

    // sort_indices returns task indices; since tasks are 1:1 with jobs in order,
    // the returned indices directly map to job indices.
    engine.sort_indices(&tasks, &context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Job, Operation, OperationTime};
    use chrono::Utc;

    fn make_job(id: &str, priority: i32, duration_ms: i64, due_date: Option<DateTime<Utc>>) -> Job {
        Job {
            id: id.to_string(),
            product_name: None,
            priority,
            due_date,
            earliest_start: None,
            quantity: 1,
            operations: vec![Operation {
                id: format!("{}-OP1", id),
                job_id: id.to_string(),
                sequence: 1,
                operation_type: None,
                time: OperationTime {
                    setup_ms: 0,
                    process_ms: duration_ms,
                    wait_ms: 0,
                },
                required_resources: vec![],
                material_requirements: vec![],
                is_splittable: false,
                allow_parallel: false,
                split_config: None,
                overlap_config: None,
                requires_operator: true,
                crew_id: None,
                outsourcing_config: None,
                time_window: None,
                pert_duration: None,
                daily_work_window: None,
            }],
            target_site_id: None,
        }
    }

    #[test]
    fn test_spt_sorts_by_duration() {
        let jobs = vec![
            make_job("long", 1, 10000, None),
            make_job("short", 1, 2000, None),
            make_job("medium", 1, 5000, None),
        ];

        let config = DispatchingConfig::new(DispatchingRuleName::Spt);
        let sorted_indices = sort_jobs_by_rule(&jobs, &config, 0);

        // SPT: shortest first
        assert_eq!(jobs[sorted_indices[0]].id, "short");
        assert_eq!(jobs[sorted_indices[1]].id, "medium");
        assert_eq!(jobs[sorted_indices[2]].id, "long");
    }

    #[test]
    fn test_edd_sorts_by_due_date() {
        let base = Utc::now();
        let jobs = vec![
            make_job("late", 1, 1000, Some(base + chrono::Duration::seconds(10))),
            make_job("urgent", 1, 1000, Some(base + chrono::Duration::seconds(3))),
            make_job("no_deadline", 1, 1000, None),
        ];

        let config = DispatchingConfig::new(DispatchingRuleName::Edd);
        let sorted_indices = sort_jobs_by_rule(&jobs, &config, 0);

        // EDD: earliest due date first, no deadline last
        assert_eq!(jobs[sorted_indices[0]].id, "urgent");
        assert_eq!(jobs[sorted_indices[1]].id, "late");
        assert_eq!(jobs[sorted_indices[2]].id, "no_deadline");
    }

    #[test]
    fn test_priority_rule() {
        let jobs = vec![
            make_job("low", 10, 1000, None),
            make_job("high", 1, 1000, None),
            make_job("medium", 5, 1000, None),
        ];

        let config = DispatchingConfig::new(DispatchingRuleName::Priority);
        let sorted_indices = sort_jobs_by_rule(&jobs, &config, 0);

        // Priority: lower value = higher priority
        assert_eq!(jobs[sorted_indices[0]].id, "high");
        assert_eq!(jobs[sorted_indices[1]].id, "medium");
        assert_eq!(jobs[sorted_indices[2]].id, "low");
    }

    #[test]
    fn test_with_tie_breaker() {
        let base = Utc::now();
        let due = base + chrono::Duration::seconds(10);
        let jobs = vec![
            make_job("A", 1, 5000, Some(due)),
            make_job("B", 1, 2000, Some(due)), // same due date, shorter
            make_job("C", 1, 5000, Some(due)),
        ];

        let config = DispatchingConfig::new(DispatchingRuleName::Edd)
            .with_tie_breaker(DispatchingRuleName::Spt);

        let sorted_indices = sort_jobs_by_rule(&jobs, &config, 0);

        // Same EDD, so SPT breaks tie: B is shortest
        assert_eq!(jobs[sorted_indices[0]].id, "B");
    }

    #[test]
    fn test_rule_name_parsing() {
        assert_eq!(
            DispatchingRuleName::parse("spt"),
            Some(DispatchingRuleName::Spt)
        );
        assert_eq!(
            DispatchingRuleName::parse("EDD"),
            Some(DispatchingRuleName::Edd)
        );
        assert_eq!(
            DispatchingRuleName::parse("s/ro"),
            Some(DispatchingRuleName::Sro)
        );
        assert_eq!(DispatchingRuleName::parse("unknown"), None);
    }

    // ============================================================================
    // Phase 13: Multi-Layer Dispatching Tests
    // ============================================================================

    #[test]
    fn test_multi_layer_config() {
        let config = MultiLayerDispatchingConfig::new(DispatchingRuleName::Fifo)
            .with_layer(DispatchingLayer::new("primary", DispatchingRuleName::Edd))
            .with_layer(
                DispatchingLayer::new("secondary", DispatchingRuleName::Spt).with_weight(0.5),
            )
            .with_layer(
                DispatchingLayer::new("tie_breaker", DispatchingRuleName::Fifo).as_tie_breaker(),
            );

        assert_eq!(config.layers.len(), 3);
        assert_eq!(config.layers[0].weight, 1.0);
        assert_eq!(config.layers[1].weight, 0.5);
        assert_eq!(config.layers[2].weight, 0.0);
    }

    #[test]
    fn test_dispatching_context_from_jobs() {
        let base = Utc::now() - chrono::Duration::hours(1); // 1시간 전 = 지연
        let jobs = vec![
            make_job("urgent", 1, 1000, Some(base)), // 지연됨
            make_job("normal", 5, 1000, None),
            make_job("low", 10, 1000, None),
        ];

        let current_ms = Utc::now().timestamp_millis();
        let ctx = DispatchingContext::from_jobs(&jobs, current_ms);

        assert_eq!(ctx.queue_length, 3);
        assert_eq!(ctx.min_priority, Some(1)); // 가장 긴급
        assert!(ctx.delayed_job_count >= 1); // 최소 1개 지연
    }

    #[test]
    fn test_rule_switch_condition_time() {
        let ctx = DispatchingContext::new(10_000);

        let before = RuleSwitchCondition::TimeBefore {
            threshold_ms: 20_000,
        };
        let after = RuleSwitchCondition::TimeAfter {
            threshold_ms: 5_000,
        };

        assert!(before.evaluate(&ctx)); // 10000 < 20000
        assert!(after.evaluate(&ctx)); // 10000 >= 5000
    }

    #[test]
    fn test_rule_switch_condition_urgent() {
        let ctx_urgent = DispatchingContext {
            min_priority: Some(1),
            ..Default::default()
        };
        let ctx_normal = DispatchingContext {
            min_priority: Some(5),
            ..Default::default()
        };

        let condition = RuleSwitchCondition::HasUrgentJobs {
            priority_threshold: 2,
        };

        assert!(condition.evaluate(&ctx_urgent)); // 1 <= 2
        assert!(!condition.evaluate(&ctx_normal)); // 5 > 2
    }

    #[test]
    fn test_dynamic_rule_selection() {
        let config = MultiLayerDispatchingConfig::new(DispatchingRuleName::Fifo)
            .with_switch_rule(
                RuleSwitchRule::new(
                    RuleSwitchCondition::HasUrgentJobs {
                        priority_threshold: 2,
                    },
                    DispatchingRuleName::Priority,
                )
                .with_priority(100),
            )
            .with_switch_rule(
                RuleSwitchRule::new(
                    RuleSwitchCondition::DelayedJobsExceed { count: 3 },
                    DispatchingRuleName::Edd,
                )
                .with_priority(50),
            );

        // 긴급 작업 있을 때 → Priority
        let ctx_urgent = DispatchingContext {
            min_priority: Some(1),
            ..Default::default()
        };
        assert_eq!(
            config.select_rule(&ctx_urgent),
            DispatchingRuleName::Priority
        );

        // 지연 작업 많을 때 → EDD
        let ctx_delayed = DispatchingContext {
            delayed_job_count: 5,
            min_priority: Some(5),
            ..Default::default()
        };
        assert_eq!(config.select_rule(&ctx_delayed), DispatchingRuleName::Edd);

        // 일반 상황 → 기본 규칙 (FIFO)
        let ctx_normal = DispatchingContext::default();
        assert_eq!(config.select_rule(&ctx_normal), DispatchingRuleName::Fifo);
    }

    #[test]
    fn test_dispatching_preset() {
        let config = DispatchingPreset::DueDateFocused.to_config();
        assert_eq!(config.default_rule, DispatchingRuleName::Edd);
        assert_eq!(config.layers.len(), 2);
        assert_eq!(config.layers[0].rule, DispatchingRuleName::Edd);
        assert_eq!(config.layers[1].rule, DispatchingRuleName::Spt);
    }

    #[test]
    fn test_adaptive_preset() {
        let config = DispatchingPreset::ThroughputMaximize.to_adaptive_config();

        // 기본 규칙 확인
        assert_eq!(config.default_rule, DispatchingRuleName::Spt);

        // 동적 전환 규칙 확인
        assert_eq!(config.switch_rules.len(), 2);

        // 긴급 상황 시 Priority로 전환되어야 함
        let ctx_urgent = DispatchingContext {
            min_priority: Some(1),
            ..Default::default()
        };
        assert_eq!(
            config.select_rule(&ctx_urgent),
            DispatchingRuleName::Priority
        );
    }

    #[test]
    fn test_multi_layer_to_simple_config() {
        let multi = MultiLayerDispatchingConfig::new(DispatchingRuleName::Fifo)
            .with_layer(DispatchingLayer::new("primary", DispatchingRuleName::Edd))
            .with_layer(DispatchingLayer::new("tie", DispatchingRuleName::Spt).as_tie_breaker());

        let simple = multi.to_simple_config();
        assert_eq!(simple.primary_rule, DispatchingRuleName::Edd);
        assert_eq!(simple.tie_breaker, Some(DispatchingRuleName::Spt));
    }

    #[test]
    fn test_utilization_condition() {
        let ctx = DispatchingContext::new(0)
            .with_utilization("M1", 0.85)
            .with_utilization("M2", 0.50);

        let above = RuleSwitchCondition::UtilizationAbove {
            resource_id: "M1".to_string(),
            threshold: 0.80,
        };
        let below = RuleSwitchCondition::UtilizationBelow {
            resource_id: "M2".to_string(),
            threshold: 0.60,
        };

        assert!(above.evaluate(&ctx)); // 0.85 >= 0.80
        assert!(below.evaluate(&ctx)); // 0.50 < 0.60
    }
}
