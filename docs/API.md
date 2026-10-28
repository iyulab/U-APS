# U-APS Engine API Reference

## Overview

U-APS Engine은 Rust로 작성된 고성능 생산 스케줄링 엔진입니다.

## Core Models

### Job
작업 단위 (생산 주문)

```rust
let job = Job::new("job-001", 1)
    .with_due_date(86_400_000)  // 1일 후
    .with_earliest_start(0);
```

**필드**:
- `id: String` - 작업 ID
- `priority: i32` - 우선순위 (높을수록 우선)
- `due_date_ms: Option<i64>` - 납기일 (밀리초)
- `earliest_start_ms: Option<i64>` - 최조 시작 가능 시점
- `is_splittable: bool` - 작업 단위 로트 분할 허용 — 이 작업의 모든 공정이 공정의 `is_splittable` 과 같이 분할될 수 있다

### Operation
공정 단위

```rust
let op = Operation::new("op-001", "job-001", 1)
    .with_time(5_000, 60_000, 1_000)  // setup, process, wait
    .with_equipment(vec!["m1".into(), "m2".into()])
    .with_workers(1);
```

**필드**:
- `id: String` - 공정 ID
- `job_id: String` - 소속 Job ID
- `sequence: i32` - 공정 순서
- `time: OperationTime` - 시간 정보
- `required_resources: Vec<ResourceRequirement>` - 필요 자원
- `is_splittable: bool` - 로트 분할 허용(`split_config` 와 함께). 근무 시간 밖에서 멈췄다 잇는 것은 이 값과 무관하다 — 작업 조각(task)은 공정이 실제로 일한 구간마다 하나
- `split_config: Option<SplitConfig>` - 분할 설정(없으면 기본값 — 최대 4 조각, 조각 ≥ 1 분). 조각은 공정의 후보 설비에서 동시에 또는 따로 돌고, 둘째 조각부터 `split_setup_overhead_ms` 가 붙는다. 조각 수(1 … `max_splits`)는 스케줄러가 공정의 끝이 가장 이른 쪽으로 고른다 — 통짜가 빠르면 나누지 않는다. 배정마다 `part`(`index` · `work_ms`)가 몇 번째 조각이고 처리 시간 중 얼마를 맡았는지 말한다
- `overlap_config: Option<OverlapConfig>` - 중첩 설정 — **이 공정 다음** 공정이 이 공정의 `min_predecessor_completion` 비율만큼 진행한 시각부터 시작할 수 있고, 이 공정보다 먼저 끝나지는 않는다. `transfer_batch_size` 는 읽지 않는다(수량 모델 없음)
- `dependencies: Vec<String>` - 선행 공정 ID(다른 작업의 공정도) — 모두 끝난 뒤 시작한다

### Resource
자원 (설비, 작업자)

```rust
let resource = Resource::new("cnc-001", "CNC Machine 1")
    .with_efficiency(1.0);
```

---

## Scheduling

### SimpleScheduler
공정 단위 비지연 디스패칭 스케줄러(규칙 · 우선순위). 배치는 u-schedule 이 한다.

```rust
let scheduler = SimpleScheduler::new();
// Err: 요청이 잘못됨(ScheduleError::Refused) 또는 엔진 결함(ScheduleError::Broken)
let schedule = scheduler.schedule(&request)?;
```

채울 수 없거나 배치할 수 없는 공정은 `UnfilledRequirement` 위반으로 보고된다.

### 디스패칭 탐색 (`algorithm: GeneticAlgorithm`)
난수 키 유전 탐색이 공정 디스패칭 순서를 찾는다. 후보 하나하나가 `SimpleScheduler` 가 만드는 스케줄이라 요청의 제약(분할 · 겹침 · 선행 · 자원 · 달력)을 모두 지키고, 디스패칭보다 나쁘지 않다. 비용은 완료 시각 + 지각 합의 1/10.

요청이 정확 모형(`CpSat`)이 통째로 적는 부류 — 공정마다 설비 요구 하나(후보 중 선택), 작업 순서 · 선행 · 최조 시작만 있고 작업자 · 달력 · 분할 · 자재 · 외주 · 조 등이 없는 유연 job shop — 이면 엔진의 `GeneticAlgorithm`(과 `Production` · `Dynamic`)은 대신 공정 순서 · 설비 선택을 직접 탐색하는 유전 알고리즘(`SimpleScheduler::sequence_search`, 능동 스케줄 디코딩 + 임계 경로 지역 탐색)을 쓴다. 같은 예산에서 Brandimarte mk01–10 최선값 대비 1.8 %(디스패칭 순서 탐색 13 %), JSPLIB 8 개 1.1 %(5.1 %).

```rust
let params = GaParams {
    population_size: 100,
    max_generations: 500,
    ..Default::default()
};
let schedule = SimpleScheduler::new().search(&request, &params)?;
```

**GaParams**:
- `population_size` - 모집단 크기 (기본: 100)
- `max_generations` - 최대 세대 (기본: 500)
- `elite_ratio` - 엘리트 비율 (기본: 0.1)
- `convergence_generations` - 개선 없는 세대가 이만큼 이어지면 멈춤 (기본: 30)
- `time_limit_ms` - 시간 제한 (없으면 30 초)
- `tournament_size` · `convergence_threshold` - 받지만 읽지 않는다(예전 요청 호환 — 보내도 되고 안 보내도 된다)

---

## Rescheduling

### RescheduleStrategy
재스케줄링 전략

```rust
pub enum RescheduleStrategy {
    RightShift,        // 단순 지연 전파
    AOR,               // 영향 공정만 재스케줄
    TotalRegeneration, // 전체 재스케줄
}
```

### Rescheduler
재스케줄러

```rust
let rescheduler = Rescheduler::new()
    .with_fence(TimeFenceConfig::new(current_time, 24, 48));

let event = ScheduleEvent::OperationDelay {
    operation_id: "op-001".into(),
    delay_ms: 30_000,
};

let result = rescheduler.auto_reschedule(&schedule, &event);
```

### TimeFenceConfig
Time Fence 설정

```rust
let config = TimeFenceConfig::new(
    current_time_ms,
    24,  // Frozen: 24시간
    48,  // Slushy: 48시간 추가
);

let zone = config.get_zone(time_ms);  // Frozen | Slushy | Liquid
```

---

## KPI & Analytics

### KpiCalculator
KPI 계산기

```rust
let calculator = KpiCalculator::new(horizon_ms);
let kpi = calculator.calculate(&schedule, &jobs);

// kpi.makespan_ms
// kpi.total_tardiness_ms
// kpi.on_time_delivery_rate
// kpi.average_utilization
// kpi.mce
// kpi.bottleneck_resource
```

### Bottleneck Analysis
병목 분석

```rust
let analyses = analyze_bottlenecks(&schedule, horizon_ms);
for analysis in analyses {
    println!("{}: {:.1}%", analysis.resource_id, analysis.utilization * 100.0);
}
```

---

## Pegging & Material

### PeggingEngine
수요-공급 연결

```rust
let mut engine = PeggingEngine::new();

engine.add_material(Material {
    id: "mat-001".into(),
    name: "Component A".into(),
    on_hand_qty: 100.0,
    unit: "EA".into(),
});

engine.add_supply(MaterialSupply {
    id: "sup-001".into(),
    material_id: "mat-001".into(),
    quantity: 50.0,
    available_at_ms: 86_400_000,
    supply_type: SupplyType::PurchaseOrder,
});

engine.add_demand(MaterialDemand {
    id: "dem-001".into(),
    material_id: "mat-001".into(),
    quantity: 30.0,
    required_at_ms: 43_200_000,
    operation_id: "op-001".into(),
    job_id: "job-001".into(),
});

let result = engine.execute_pegging();
```

---

## CTP/ATP

### CtpEngine
Capable-to-Promise 엔진

```rust
let mut engine = CtpEngine::new(current_time_ms);

// 자재, BOM, 공정시간 설정
engine.add_material(material);
engine.set_product_bom("product-001", vec![("mat-001".into(), 2.0)]);
engine.set_product_process_time("product-001", 3_600_000);

let request = CtpRequest {
    order_id: "order-001".into(),
    product_id: "product-001".into(),
    quantity: 10.0,
    requested_date_ms: 172_800_000,  // 2일 후
    priority: 1,
};

let result = engine.check_capability(&request);

if result.is_capable {
    println!("확약 가능: {}", result.promised_date_ms.unwrap());
} else {
    for constraint in result.constraints {
        println!("제약: {:?}", constraint);
    }
}
```

---

## What-If Analysis

### WhatIfEngine
시나리오 분석 엔진

```rust
let engine = WhatIfEngine::new(baseline_schedule, jobs, horizon_ms);

let scenario = Scenario {
    id: "s1".into(),
    name: "10% 처리시간 감소".into(),
    changes: vec![ScenarioChange::ProcessingTime {
        operation_id: None,
        change_percent: -0.1,
    }],
};

let result = engine.apply_scenario(&scenario);
let comparison = engine.compare_scenarios(&baseline, &result);

println!("Makespan 변화: {:.1}%", comparison.kpi_diff.makespan_diff_percent);
```

### ScenarioChange
시나리오 변경 유형

```rust
pub enum ScenarioChange {
    ResourceCapacity { resource_id, change_percent },
    ProcessingTime { operation_id, change_percent },
    AddOrder { job },
    RemoveOrder { job_id },
    PriorityChange { job_id, new_priority },
    DueDateChange { job_id, new_due_date_ms },
    AddResource { resource_id, capacity_ms },
    RemoveResource { resource_id },
}
```

---

## Constraint Programming

### CpModel
CP 모델 정의

```rust
let mut model = CpModel::new("schedule", 1_000_000);

model.add_interval(IntervalVar::new("op1", 0, 100_000, 50_000, 200_000));
model.add_no_overlap(vec!["op1".into(), "op2".into()]);
model.add_precedence("op1".into(), "op2".into(), 0);
model.minimize_makespan();
```

### CpSolver
CP Solver

```rust
let solver = SimpleCpSolver::new();
let solution = solver.solve(&model, &SolverConfig::default());

if solution.is_solution_found() {
    println!("Makespan: {}", solution.makespan());
}
```

---

## Error Handling

### UapsError
통합 에러 타입

```rust
pub struct UapsError {
    pub code: ErrorCode,
    pub message: String,
    pub details: Option<String>,
    pub entity_id: Option<String>,
}
```

### ErrorCode
에러 코드

| 범위 | 분류 | 예시 |
|------|------|------|
| 1xxx | 입력 검증 | InvalidInput, MissingRequiredField |
| 2xxx | 스케줄링 | SchedulingFailed, NoFeasibleSolution |
| 3xxx | 자원 | ResourceUnavailable, CapacityExceeded |
| 4xxx | 자재 | MaterialShortage |
| 5xxx | 시스템 | InternalError, Timeout |
| 6xxx | GA | GaConvergenceFailed |
| 7xxx | CP | CpModelInvalid, CpInfeasible |

---

## Validation

### Validator
입력 검증기

```rust
let result = Validator::validate_schedule_request(&jobs, &operations, &resources);

if result.is_valid() {
    // 스케줄링 진행
} else {
    for error in result.errors {
        eprintln!("Error: {}", error);
    }
}
```

### 편의 함수

```rust
validate_time_range(start, end)?;
validate_quantity(qty, "quantity")?;
```

---

## Skill Matrix

### SkillMatrix
작업자 스킬 매트릭스

```rust
let matrix = SkillMatrix::new()
    .with_skill("worker-001", "CNC", 1.0)
    .with_skill("worker-001", "Assembly", 0.7)
    .with_skill("worker-002", "CNC", 0.5);

let proficiency = matrix.get_proficiency("worker-001", "CNC");
let process_time = matrix.calculate_process_time("worker-001", "CNC", base_time);
let best_worker = matrix.best_worker_for("CNC");
```

---

## Certification Matrix

### CertificationMatrix
작업자 자격/인증 매트릭스

```rust
let matrix = CertificationMatrix::new()
    .add_certification("worker-001", "welding", CertificationLevel::Master, Some(expiry_date))
    .add_certification("worker-002", "inspection", CertificationLevel::Standard, None);

let is_certified = matrix.is_certified("worker-001", "welding");
let cert_level = matrix.get_certification_level("worker-001", "welding");
let expiring_soon = matrix.get_expiring_certifications(30); // 30일 내 만료
```

### CertificationLevel
인증 수준

```rust
pub enum CertificationLevel {
    None,      // 무자격
    Trainee,   // 수습
    Standard,  // 표준
    Advanced,  // 고급
    Master,    // 마스터
}
```

---

## Dispatching Rules

### DispatchingConfig
디스패칭 룰 설정

```rust
let config = DispatchingConfig {
    primary_rule: "EDD",           // 기본 룰
    secondary_rules: vec!["SPT"],  // 보조 룰
    tie_breaker: Some("FIFO"),     // 동률 처리
    weights: HashMap::new(),       // 가중치
};
```

### 지원 룰 (11개)

| 카테고리 | 룰 | 설명 |
|---------|-----|------|
| **Time** | SPT | 짧은 공정시간 우선 |
| | LPT | 긴 공정시간 우선 |
| | LWKR | 잔여작업 적은 순 |
| | MWKR | 잔여작업 많은 순 |
| **Due Date** | EDD | 납기 빠른 순 |
| | SLACK | 여유시간 적은 순 |
| | CR | 임계비율 낮은 순 |
| **Queue** | FIFO | 도착 순서 |
| | PRIORITY | 우선순위 등급 |
| | MOPNR | 잔여공정 많은 순 |
| | RANDOM | 무작위 선택 |

### CLI 사용 예

```bash
# 단일 룰
uaps input.json output.json --strategy EDD

# 룰 + Tie-Breaker
uaps input.json output.json --strategy EDD --tie-breaker SPT
```

---

## Material Constraints

### MaterialRequirement
공정별 자재 요구사항

```rust
let requirement = MaterialRequirement {
    material_id: "mat-001".into(),
    quantity: 10.0,
    availability_date: Some(1_000_000), // 밀리초
};
```

### Material
자재 마스터

```rust
let material = Material {
    id: "mat-001".into(),
    name: "Component A".into(),
    unit: "EA".into(),
    stock_quantity: 100.0,
    safety_stock: 20.0,
    lead_time_ms: 86_400_000, // 1일
};

// 가용 재고량 (현재 - 안전재고)
let available = material.available_stock();
// 안전재고 미달 여부
let is_low = material.is_below_safety_stock();
```

### BomEntry
BOM 항목

```rust
let bom = BomEntry {
    material_id: "mat-001".into(),
    quantity_per_unit: 2.0,
    scrap_rate: 0.05, // 5% 손실률
};

// 총 소요량 계산 (손실률 포함)
let total_required = bom.calculate_requirement(100.0); // 210.0
```

### MaterialManager
자재 관리자

```rust
let manager = MaterialManager::new()
    .add_material(material)
    .add_bom_entry("product-001", bom);

// 제품 생산을 위한 자재 소요량 계산
let requirements = manager.calculate_requirements("product-001", 100.0);
```

---

## Rescheduling Events

### ScheduleEvent (18개 이벤트 타입)

#### Equipment Events
```rust
MachineBreakdown { resource_id, start_ms, duration_ms }  // 설비 고장
MachineRecovered { resource_id }                          // 설비 복구
MaintenanceScheduled { resource_id, start_ms, duration_ms } // 예방정비
EfficiencyChange { resource_id, new_efficiency }          // 효율 변경
```

#### Material Events
```rust
MaterialShortage { material_id, shortage_quantity }       // 자재 부족
MaterialDelay { material_id, new_availability_ms }        // 입고 지연
MaterialArrival { material_id, quantity }                 // 자재 도착
```

#### Worker Events
```rust
WorkerUnavailable { worker_id, start_ms, duration_ms }    // 작업자 불가
WorkerAvailable { worker_id }                             // 작업자 복귀
WorkerSkillChange { worker_id, skill_id, new_level }      // 스킬 변경
```

#### Quality Events
```rust
QualityDefect { operation_id, rework_time_ms }            // 품질 불량
InspectionDelay { operation_id, delay_ms }                // 검사 지연
```

#### Order Events
```rust
OperationDelay { operation_id, delay_ms }                 // 공정 지연
UrgentOrder { job }                                       // 긴급 투입
OrderCancellation { job_id }                              // 주문 취소
DueDateChange { job_id, new_due_date_ms }                 // 납기 변경
QuantityChange { job_id, new_quantity }                   // 수량 변경
PriorityChange { job_id, new_priority }                   // 우선순위 변경
ProcessTimeChange { operation_id, new_process_time_ms }   // 공정시간 변경
```

### RescheduleStrategy
재스케줄링 전략

```rust
pub enum RescheduleStrategy {
    RightShift,        // 단순 지연 전파
    AOR,               // 영향 공정만 재스케줄
    Partial,           // 부분 재생성
    Full,              // 전체 재생성
    TotalRegeneration, // 완전 재생성
}
```

### SimulationSession (SDK)
시뮬레이션 세션에서 이벤트 적용

```csharp
var session = SimulationSession.Create(client, request);

// 단일 이벤트 적용
var breakdown = MachineBreakdownEvent.Create("M1", startMs, durationMs);
var newSchedule = session.ApplyEvent(breakdown);

// 복수 이벤트 적용
var events = new[] { event1, event2, event3 };
var finalSchedule = session.ApplyEvents(events);
```

---

## Constants

### Time Units (milliseconds)
```rust
const SECOND: i64 = 1_000;
const MINUTE: i64 = 60_000;
const HOUR: i64 = 3_600_000;
const DAY: i64 = 86_400_000;
```
