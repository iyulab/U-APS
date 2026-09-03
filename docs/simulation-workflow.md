# APS Simulation Workflow

## 목표

```
AS-IS (현재 스케줄) → 조작 (공정 조정) → TO-BE (최적화된 스케줄)
                         ↑
                      반복 가능
```

## 시뮬레이션 시나리오

### 1. What-If 분석
- "이 공정 시간을 30분 늘리면?"
- "우선순위를 변경하면?"
- "새 주문을 추가하면?"
- "특정 설비가 고장나면?"

### 2. 사용 흐름

```csharp
// 1. 세션 시작 (AS-IS 로드)
var session = SimulationSession.Load("input.xlsx");

// 2. AS-IS 스케줄 확인
var asIs = session.CurrentSchedule;
Console.WriteLine($"AS-IS Makespan: {asIs.MakespanMs}");

// 3. 조작 (What-If)
session.ModifyOperation("OP-001-05", op => {
    op.ProcessMs = 2400 * 60000;  // 절연층코팅 40시간으로 변경
});

// 또는
session.ModifyJob("JOB-003", job => {
    job.Priority = 1;  // 우선순위 상향
});

// 또는
session.DisableResource("연마-2호기");  // 설비 비활성화

// 4. 재스케줄링 (TO-BE 생성)
var toBe = session.Reschedule();

// 5. 비교
var comparison = session.Compare(asIs, toBe);
Console.WriteLine($"Makespan 변화: {comparison.MakespanDelta}");
Console.WriteLine($"영향받은 작업: {comparison.AffectedOperations.Count}");

// 6. 결과 저장
session.Export("output.xlsx", includeComparison: true);

// 7. 반복 (추가 조작)
session.ModifyOperation("OP-002-03", op => op.SetupMs += 600000);
var toBe2 = session.Reschedule();
```

## 아키텍처

```
┌─────────────────────────────────────────────────────────┐
│                  SimulationSession                       │
├─────────────────────────────────────────────────────────┤
│  - BaselineJobs: List<Job>      (원본 - 불변)           │
│  - BaselineResources: List<Resource>                    │
│  - BaselineSchedule: Schedule   (AS-IS)                 │
│  - WorkingJobs: List<Job>       (수정 가능 복사본)       │
│  - WorkingResources: List<Resource>                     │
│  - CurrentSchedule: Schedule    (TO-BE)                 │
│  - History: List<ScheduleSnapshot>                      │
├─────────────────────────────────────────────────────────┤
│  + Load(path) → SimulationSession                       │
│  + ModifyJob(id, Action<Job>)                           │
│  + ModifyOperation(id, Action<Operation>)               │
│  + AddJob(Job)                                          │
│  + RemoveJob(id)                                        │
│  + DisableResource(id)                                  │
│  + EnableResource(id)                                   │
│  + Reschedule() → Schedule                              │
│  + Compare(Schedule, Schedule) → ScheduleComparison     │
│  + Rollback() → 이전 상태로                              │
│  + Reset() → AS-IS로 복원                                │
│  + Export(path, options)                                │
└─────────────────────────────────────────────────────────┘
```

## 비교 결과

```csharp
public class ScheduleComparison
{
    public long MakespanDelta { get; set; }           // Makespan 변화량
    public int ViolationsDelta { get; set; }          // 위반 수 변화
    public List<OperationDiff> AffectedOperations;    // 영향받은 공정들
    public List<ResourceDiff> ResourceUtilization;    // 설비 가동률 변화
}

public class OperationDiff
{
    public string OperationId { get; set; }
    public long StartDelta { get; set; }              // 시작시간 변화
    public long EndDelta { get; set; }                // 종료시간 변화
    public string? ResourceChange { get; set; }       // 할당 설비 변경
}
```

## CLI 확장

```bash
# 기본 (기존과 동일)
uaps input.xlsx output.xlsx

# 시뮬레이션 모드 (인터랙티브)
uaps simulate input.xlsx

# 배치 시뮬레이션 (스크립트)
uaps simulate input.xlsx --script changes.json --output result.xlsx
```

### changes.json 예시
```json
{
  "modifications": [
    {
      "type": "operation",
      "id": "OP-001-05",
      "changes": {
        "processMs": 2400000
      }
    },
    {
      "type": "job",
      "id": "JOB-003",
      "changes": {
        "priority": 1
      }
    },
    {
      "type": "resource",
      "id": "연마-2호기",
      "action": "disable"
    }
  ]
}
```

## 출력 Excel 형식

### Sheet: 스케줄비교
| JobId | OperationId | AS-IS 시작 | AS-IS 종료 | TO-BE 시작 | TO-BE 종료 | 변화량(분) | 설비변경 |
|-------|-------------|-----------|-----------|-----------|-----------|-----------|---------|
| JOB-001 | OP-001-05 | 09:00 | 15:00 | 10:00 | 16:00 | +60 | - |

### Sheet: KPI비교
| 지표 | AS-IS | TO-BE | 변화 |
|------|-------|-------|------|
| Makespan | 24170분 | 25200분 | +1030분 |
| 납기준수율 | 100% | 80% | -20% |
| 설비가동률 | 85% | 82% | -3% |
