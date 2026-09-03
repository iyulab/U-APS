# U-APS Core Concepts

현업 요구사항을 스케줄링 엔진 개념으로 정리

---

## 1. Operation (작업)

단일 공정 단계의 시간 구조

```
┌─────────────┬─────────────┬─────────────┐
│  SetupTime  │ ProcessTime │  WaitTime   │
│  (공정전)    │   (공정)     │  (공정후)    │
└─────────────┴─────────────┴─────────────┘
        ↓             ↓             ↓
    준비/세팅      실제 가공      대기/이동
```

### Properties

| Property | Type | 설명 | 비고 |
|----------|------|------|------|
| `id` | string | 작업 식별자 | |
| `sequence` | int | 공정 순서 | |
| `setup_time` | Duration | 공정전 작업시간 | 모델변경 setup 포함 |
| `process_time` | Duration | 실제 공정시간 | |
| `wait_time` | Duration | 공정후 대기시간 | |
| `time_variance` | TimeVariance | 시간 편차 (min/max) | 공정전/중/후 각각 |
| `required_resources` | ResourceReq[] | 필요 자원 목록 | 장비, 작업자 |
| `is_splittable` | bool | 분할 가능 여부 | 휴지시간 발생시 |
| `allow_parallel` | bool | 병렬 처리 가능 | |
| `earliest_start` | DateTime? | 시작 가능 시간 | |
| `transit_time` | Duration | 다음 공정 이동시간 | |
| `affects_utilization` | bool | 가동률 포함 여부 | |

### TimeVariance Structure

```rust
struct TimeVariance {
    setup_min: Duration,
    setup_max: Duration,
    process_min: Duration,
    process_max: Duration,
    wait_min: Duration,
    wait_max: Duration,
}
```

---

## 2. Resource (자원)

### 2.1 Equipment (장비)

| Property | Type | 설명 |
|----------|------|------|
| `id` | string | 장비 코드 |
| `name` | string | 장비명 |
| `capabilities` | string[] | 수행 가능 작업 |
| `calendar_id` | string | 운영 캘린더 |
| `pm_schedule` | MaintenanceSlot[] | PM 일정 |
| `setup_matrix` | SetupMatrix | 모델별 setup 시간 |

### 2.2 Worker (작업자)

| Property | Type | 설명 |
|----------|------|------|
| `id` | string | 사원번호 |
| `name` | string | 성명 |
| `skill_level` | float | 숙련도 (0.0 ~ 1.0) |
| `certifications` | string[] | 자격/인증 |
| `team_id` | string | 조 편성 |
| `calendar_id` | string | 근무 캘린더 |

### 2.3 ResourceRequirement (자원 요구사항)

```rust
struct ResourceRequirement {
    resource_type: ResourceType,      // Equipment | Worker
    quantity: i32,                    // 필요 인원/수량
    candidates: Vec<String>,          // 가용 자원 ID 목록
    skill_required: Option<float>,    // 최소 숙련도
    certifications: Vec<String>,      // 필요 자격

    // 작업자별 시간 (장비와 다를 수 있음)
    worker_setup_time: Option<Duration>,
    worker_process_time: Option<Duration>,
    worker_wait_time: Option<Duration>,
}
```

---

## 3. Calendar (캘린더)

운영 시간 및 휴무 관리

| Property | Type | 설명 |
|----------|------|------|
| `id` | string | 캘린더 ID |
| `shifts` | Shift[] | 근무 교대 (주간/야간) |
| `holidays` | Date[] | 휴일 |
| `non_working_days` | Date[] | 휴무일 |
| `break_times` | TimeRange[] | 휴식 시간 |

### Shift Structure

```rust
struct Shift {
    name: String,           // "주간", "야간"
    start: Time,            // 08:00
    end: Time,              // 17:00
    days: Vec<DayOfWeek>,   // [Mon, Tue, Wed, Thu, Fri]
}
```

---

## 4. Constraint (제약조건)

### 4.1 Time Constraints

| Type | 설명 | 예시 |
|------|------|------|
| `DueDate` | 납기일 | Job 완료 기한 |
| `EarliestStart` | 최조 시작 가능 | 자재 입고일 이후 |
| `LatestEnd` | 최종 완료 시한 | 출하 마감 |

### 4.2 Resource Constraints

| Type | 설명 | 예시 |
|------|------|------|
| `Capacity` | 자원 용량 | 장비 동시 처리 수 |
| `Availability` | 가용 시간 | PM, 휴무 제외 |
| `Qualification` | 자격 요건 | 특정 인증 필요 |

### 4.3 Sequence Constraints

| Type | 설명 | 예시 |
|------|------|------|
| `Precedence` | 선후행 관계 | Op1 → Op2 |
| `NoWait` | 대기 불가 | 열처리 후 즉시 |
| `MinGap` | 최소 간격 | 경화 시간 필요 |
| `MaxGap` | 최대 간격 | 품질 유지 |

---

## 5. External Factors (외부 요소)

### 5.1 Outsourcing (외주)

```rust
struct OutsourcingOp {
    operation_id: String,
    vendor_id: String,
    lead_time: Duration,      // 외주 리드타임
    transit_time: Duration,   // 운송 시간
}
```

### 5.2 Inspection & Rework (검사/재작업)

```rust
struct QualityEvent {
    event_type: QualityEventType,  // Inspection | Rework | Scrap
    operation_id: String,
    probability: float,            // 발생 확률 (통계 기반)
    additional_time: Duration,     // 추가 소요 시간
    triggers_reschedule: bool,     // 재스케줄링 트리거
}
```

---

## 6. Metrics & Load (지표)

### 6.1 Resource Load (부하량)

```rust
struct ResourceLoad {
    resource_id: String,
    period: DateRange,
    scheduled_hours: f64,
    available_hours: f64,
    utilization_rate: f64,    // scheduled / available
}
```

### 6.2 Utilization Calculation

```
가동률 = Σ(affects_utilization=true인 작업시간) / 가용시간
```

---

## 7. Concept Relationships

```
Job (생산오더)
 └─► Operation[] (공정)
      ├─► ResourceRequirement[] (필요자원)
      │    ├─► Equipment (장비)
      │    │    └─► Calendar (운영일정)
      │    │    └─► SetupMatrix (setup시간)
      │    │    └─► MaintenanceSlot[] (PM일정)
      │    └─► Worker (작업자)
      │         └─► Calendar (근무일정)
      │         └─► SkillLevel (숙련도)
      │         └─► Certification[] (자격)
      ├─► TimeVariance (시간편차)
      ├─► Constraint[] (제약조건)
      └─► QualityEvent[] (품질이벤트)
```

---

## 8. Engine Input/Output

### Input

```rust
struct ScheduleRequest {
    jobs: Vec<Job>,
    resources: Vec<Resource>,
    calendars: Vec<Calendar>,
    constraints: Vec<Constraint>,
    options: ScheduleOptions,
}
```

### Output

```rust
struct Schedule {
    assignments: Vec<Assignment>,
    metrics: ScheduleMetrics,
    violations: Vec<Violation>,
    warnings: Vec<Warning>,
}

struct Assignment {
    operation_id: String,
    resource_id: String,
    start_time: DateTime,
    end_time: DateTime,
    setup_start: DateTime,
    process_start: DateTime,
    wait_end: DateTime,
}
```

---

## 9. Rescheduling Triggers

스케줄 재조정이 필요한 상황

| Trigger | 설명 | 우선순위 |
|---------|------|----------|
| `QualityFail` | 검사 불합격/재작업 | 높음 |
| `EquipmentDown` | 설비 고장 | 높음 |
| `MaterialDelay` | 자재 지연 | 높음 |
| `OrderChange` | 오더 변경/취소 | 중간 |
| `CapacityChange` | 인력/설비 변동 | 중간 |
| `PriorityChange` | 우선순위 변경 | 낮음 |

---

## 10. Alias Helpers 매핑

| 현업 용어 | Core Concept | Helper 함수 |
|-----------|--------------|-------------|
| 생산오더 | Job | `CreateOrder()` |
| 공정 | Operation | `CreateProcess()` |
| 설비 | Resource(Equipment) | `CreateEquipment()` |
| 작업자 | Resource(Worker) | `CreateWorker()` |
| 납기 | Constraint(DueDate) | `SetDueDate()` |
| PM일정 | MaintenanceSlot | `AddPMSchedule()` |
| 조편성 | Team | `AssignTeam()` |
| 숙련도 | SkillLevel | `SetSkillLevel()` |
