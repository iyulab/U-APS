# U-APS Architecture

## 설계 철학

### 핵심 원칙: 역할 분리

```
┌─────────────────────────────────────────────────────┐
│                    SDK (C#)                         │
│  ┌───────────────┐  ┌───────────────┐              │
│  │   Handlers    │  │   Providers   │              │
│  │  (Transform)  │  │  (Resolve)    │              │
│  └───────┬───────┘  └───────┬───────┘              │
│          │                  │                       │
│          ▼                  ▼                       │
│  ┌─────────────────────────────────────┐           │
│  │     User-Friendly → Explicit Data   │           │
│  └─────────────────┬───────────────────┘           │
└────────────────────┼────────────────────────────────┘
                     │ FFI (JSON)
┌────────────────────┼────────────────────────────────┐
│                    ▼                                │
│              Engine (Rust)                          │
│  ┌─────────────────────────────────────┐           │
│  │   Explicit Data Only                │           │
│  │   No Interpretation                 │           │
│  │   Deterministic Results             │           │
│  └─────────────────────────────────────┘           │
└─────────────────────────────────────────────────────┘
```

---

## Engine (Rust) - 정확성과 단순성

### 원칙
- **명시적 데이터만 처리**: 해석의 여지 없음
- **No Ambiguity**: 모든 입력이 완전히 정의됨
- **Deterministic**: 동일 입력 → 동일 출력
- **Pure Computation**: 외부 상태 의존 없음

### 데이터 형식

| 개념 | Engine 입력 형식 | 설명 |
|------|-----------------|------|
| 시간 | `timestamp_ms: i64` | Unix epoch milliseconds |
| 기간 | `duration_ms: i64` | 밀리초 단위 정확한 값 |
| 근무시간 | `TimeWindow { start_ms, end_ms }[]` | 각 근무 구간의 정확한 시작/종료 |
| 납기 | `due_date_ms: i64` | 정확한 납기 timestamp |

### 예시

```rust
// Engine이 받는 데이터 - 완전히 명시적
WorkCalendar {
    time_windows: [
        TimeWindow { start_ms: 1735689600000, end_ms: 1735722000000 }, // 2025-01-01 09:00-18:00
        TimeWindow { start_ms: 1735776000000, end_ms: 1735808400000 }, // 2025-01-02 09:00-18:00
    ]
}
```

### Engine의 책임
- ✅ 최적화 알고리즘 수행
- ✅ 제약 조건 검증
- ✅ 스케줄 생성
- ❌ 날짜 패턴 해석 (SDK 책임)
- ❌ 휴일 계산 (SDK 책임)
- ❌ 시간대 변환 (SDK 책임)

---

## SDK (C#) - 사용성과 유연성

### 원칙
- **User-Friendly Interface**: 직관적인 API
- **Flexible Input**: 다양한 입력 형식 지원
- **Transformation**: 사용자 입력 → Engine 명시 데이터
- **Extensibility**: Handlers, Providers로 기능 확장

### 컴포넌트 구조

#### Handlers (변환기)
사용자 친화적 입력을 Engine 명시 데이터로 변환

```csharp
// 사용자 입력: 편리한 형식
var calendar = Calendar.Create("Production")
    .WithWorkingHours(TimeOnly.Parse("09:00"), TimeOnly.Parse("18:00"))
    .WithWeekends(DayOfWeek.Saturday, DayOfWeek.Sunday)
    .WithHolidays(koreanHolidays2025);

// SDK Handler가 변환 → Engine 데이터: 명시적 TimeWindow 배열
```

#### Providers (공급자)
외부 데이터 소스와 연동하여 정보 제공

```csharp
public interface IHolidayProvider
{
    IEnumerable<DateOnly> GetHolidays(int year, string region);
}

public interface IEquipmentStatusProvider
{
    EquipmentStatus GetStatus(string equipmentId, DateTime at);
}
```

---

## Core Concepts (공통 개념 모델)

SDK와 Engine이 동일한 스케줄링 개념 사용

| Concept | 설명 | 생산 관점 Alias |
|---------|------|-----------------|
| **Job** | 스케줄링 대상 작업 단위 | Project, Order |
| **Operation** | Job 내 개별 작업 단계 | Task, Step |
| **Resource** | 작업 수행 자원 | Equipment, Worker, Tool |
| **Constraint** | 제약 조건 | DueDate, Capacity, Dependency |
| **Schedule** | 스케줄링 결과 | Plan, Timeline |
| **Assignment** | 시간 할당 결과 | TimeBlock, Slot |

---

## SDK Structure

### Core Classes

```csharp
public class Job
{
    public string Id { get; set; }
    public string? ProductName { get; set; }
    public int Priority { get; set; }
    public int Quantity { get; set; }
    public DateTime? DueDate { get; set; }
    public List<Operation> Operations { get; set; }
}

public class Operation
{
    public string Id { get; set; }
    public string JobId { get; set; }
    public int Sequence { get; set; }
    public OperationTime Time { get; set; }
    public List<ResourceRequirement> RequiredResources { get; set; }
}

public class Resource
{
    public string Id { get; set; }
    public ResourceType Type { get; set; }
    public string? Capability { get; set; }
    public double Efficiency { get; set; }
}
```

### Builder Pattern (Fluent API)

```csharp
// Core concepts with fluent builders
var job = Job.Create("PO-001")
    .WithPriority(1)
    .WithQuantity(100)
    .WithDueDate(DateTime.Parse("2025-01-25"))
    .WithOperation(
        Operation.Create("OP-001", "PO-001", 10)
            .WithTime(5 * 60000, 20 * 60000, 0)
            .WithEquipment("EQP-001", "EQP-002")
    );

var resource = Resource.Equipment("EQP-001")
    .WithCapability("가공")
    .WithEfficiency(1.2);
```

---

## Engine Interface (Rust)

```rust
pub struct Job {
    pub id: String,
    pub operations: Vec<Operation>,
    pub priority: i32,
    pub due_date_ms: Option<i64>,
}

pub struct Operation {
    pub id: String,
    pub job_id: String,
    pub sequence: u32,
    pub setup_ms: i64,
    pub process_ms: i64,
    pub wait_ms: i64,
    pub required_resources: Vec<ResourceRequirement>,
}

pub struct Resource {
    pub id: String,
    pub resource_type: ResourceType,
    pub efficiency: f64,
    pub calendar: Option<WorkCalendar>,
}

// 스케줄링 알고리즘
pub trait Scheduler {
    fn schedule(&self, request: ScheduleRequest) -> ScheduleResponse;
}
```

---

## Communication (SDK ↔ Engine)

```
┌─────────────┐      FFI (JSON)       ┌─────────────┐
│   C# SDK    │ ◄──────────────────► │ Rust Engine │
│             │                       │             │
│ User Input  │   Transformation      │ Explicit    │
│ (Flexible)  │ ───────────────────► │ Data Only   │
│             │                       │             │
│ Formatted   │   Direct Mapping      │ Raw         │
│ Output      │ ◄─────────────────── │ Results     │
└─────────────┘                       └─────────────┘
```

### 변환 예시

| 사용자 입력 (SDK) | Engine 데이터 |
|-------------------|---------------|
| `"09:00 - 18:00"` | `TimeWindow[] { start_ms, end_ms }` |
| `"2주 후"` | `due_date_ms: 1736899200000` |
| `"고효율 설비"` | `equipment_ids: ["EQP-001", "EQP-003"]` |

---

## 확장 패턴

### Custom Handler

```csharp
public class RecurrenceHandler : ITimeWindowHandler
{
    public IEnumerable<TimeWindow> Expand(RecurrencePattern pattern)
    {
        // "매주 월-금 09:00-18:00" → 명시적 TimeWindow 배열
        foreach (var date in GetDatesInRange(pattern.StartDate, pattern.EndDate))
        {
            if (pattern.DaysOfWeek.Contains(date.DayOfWeek))
            {
                yield return new TimeWindow
                {
                    StartMs = ToUnixMs(date, pattern.StartTime),
                    EndMs = ToUnixMs(date, pattern.EndTime)
                };
            }
        }
    }
}
```

### Custom Provider

```csharp
public class MesEquipmentProvider : IEquipmentStatusProvider
{
    private readonly IMesClient _mes;

    public EquipmentStatus GetStatus(string equipmentId, DateTime at)
    {
        var mesStatus = _mes.GetEquipmentStatus(equipmentId, at);
        return new EquipmentStatus
        {
            IsAvailable = mesStatus.State == "RUNNING",
            Efficiency = mesStatus.OEE / 100.0,
            NextMaintenanceMs = ToUnixMs(mesStatus.NextPM)
        };
    }
}
```

---

## Benefits

### Engine
- **성능**: 불필요한 해석 로직 없음
- **정확성**: 명시적 데이터로 오류 최소화
- **테스트 용이**: 입력-출력이 완전히 결정적
- **이식성**: 다양한 SDK에서 재사용 가능

### SDK
- **사용성**: 직관적인 API로 생산성 향상
- **유연성**: 다양한 입력 형식 지원
- **확장성**: Handler/Provider로 무한 확장
- **통합성**: 기존 시스템과 쉬운 연동
