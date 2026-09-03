# UAPS Workbench Redesign Plan

## APS 이론 기반 워크플로우 재설계

### 참조 자료
- [Advanced Planning and Scheduling - Wikipedia](https://en.wikipedia.org/wiki/Advanced_planning_and_scheduling)
- [What-If Scenarios in Production Planning - PlanetTogether](https://www.planettogether.com/blog/the-importance-of-what-if-scenarios-in-production-planning)
- [Gantt Chart for Job Scheduling - Microsoft Dynamics 365](https://learn.microsoft.com/en-us/dynamics365/supply-chain/production-control/visual-scheduling-production)
- [Visual Production Scheduling - PlanetTogether](https://www.planettogether.com/blog/reasons-why-visual-production-scheduling-is-a-must)

---

## 1. 현재 상태 (AS-IS)

### 현재 워크플로우
```
[현재상황] → [조건변경] → [시뮬레이션]
```

### 문제점
| 문제 | 설명 |
|------|------|
| 용어 불명확 | "현재상황", "조건변경"은 APS 도메인 용어가 아님 |
| 개념 혼재 | 데이터 편집과 시나리오 변경이 구분되지 않음 |
| 비교 기능 부족 | Baseline vs Proposed 개념 없음 |
| Disruption 유형 미지원 | 장비 고장, 긴급 주문 등 표준 이벤트 없음 |

---

## 2. APS 이론 기반 새로운 워크플로우 (TO-BE)

### 핵심 개념
```
┌─────────────────────────────────────────────────────────────────┐
│                    APS Rescheduling Workflow                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐      │
│  │   BASELINE   │ →  │  DISRUPTION  │ →  │  RESCHEDULE  │      │
│  │   Schedule   │    │   Scenario   │    │   & Compare  │      │
│  └──────────────┘    └──────────────┘    └──────────────┘      │
│                                                                  │
│  "What we have"      "What changed"      "What we propose"      │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 새로운 3단계 정의

#### Stage 1: BASELINE (기준 스케줄)
> **"현재 확정된 생산 스케줄"**

- 현재 운영 중인 스케줄 표시
- KPI 대시보드 (Makespan, Utilization, On-Time Delivery)
- Gantt Chart (Resource-based view)
- 데이터 테이블 (읽기 전용)

#### Stage 2: DISRUPTION (변경 시나리오)  
> **"What-If 시나리오 정의"**

**Disruption Types (변경 유형):**
```
┌─────────────────────────────────────────────────────────────────┐
│ DEMAND CHANGES          │ RESOURCE CHANGES                      │
│ ─────────────────────   │ ──────────────────────────────────── │
│ + New Order (긴급주문)   │ - Machine Down (장비고장)             │
│ + Order Change (수량변경)│ - Worker Absence (인력부족)           │
│ + Priority Change       │ - Capacity Change (가동률변경)        │
│ + Due Date Change       │ + New Resource (신규설비)             │
│ - Order Cancel          │                                       │
├─────────────────────────────────────────────────────────────────┤
│ PROCESS CHANGES         │ MATERIAL CHANGES                      │
│ ─────────────────────   │ ──────────────────────────────────── │
│ ~ Operation Delay       │ - Material Shortage                   │
│ ~ Setup Time Change     │ ~ Delivery Delay                      │
│ ~ Route Change          │ + Material Arrival                    │
└─────────────────────────────────────────────────────────────────┘
```

**UI 구성:**
- Disruption Type 선택 패널
- 영향 받는 항목 하이라이트
- 변경 사항 요약 카드

#### Stage 3: RESCHEDULE (재스케줄링 & 비교)
> **"최적화 결과 및 Baseline 비교"**

**비교 시각화:**
```
┌─────────────────────────────────────────────────────────────────┐
│                    SCHEDULE COMPARISON                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─ BASELINE ──────────────────────────────────────────────┐   │
│  │ ████████░░░░░░░░░░░░░░░░░░░░░░░ Makespan: 4h 30m       │   │
│  └──────────────────────────────────────────────────────────┘   │
│                           ↕ Δ -15%                              │
│  ┌─ PROPOSED ──────────────────────────────────────────────┐   │
│  │ ██████░░░░░░░░░░░░░░░░░░░░░░░░░ Makespan: 3h 50m       │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
├─────────────────────────────────────────────────────────────────┤
│  KPI Comparison:                                                │
│  ┌──────────────┬──────────────┬──────────────┬────────────┐   │
│  │ Metric       │ Baseline     │ Proposed     │ Δ Change   │   │
│  ├──────────────┼──────────────┼──────────────┼────────────┤   │
│  │ Makespan     │ 4h 30m       │ 3h 50m       │ ▼ -14.8%   │   │
│  │ Utilization  │ 72.3%        │ 81.5%        │ ▲ +9.2%    │   │
│  │ Late Jobs    │ 2            │ 0            │ ▼ -100%    │   │
│  └──────────────┴──────────────┴──────────────┴────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

---

## 3. 갭 분석 (Gap Analysis)

### 기능 갭

| 기능 | AS-IS | TO-BE | 갭 |
|------|-------|-------|-----|
| **Baseline 개념** | X | O | 신규 개발 필요 |
| **Disruption Types** | X | O | 신규 개발 필요 |
| **비교 Gantt** | △ (기본) | O (상세) | 개선 필요 |
| **KPI 비교** | △ (기본) | O (상세) | 개선 필요 |
| **변경 이력** | X | O | 신규 개발 필요 |
| **What-If 저장** | X | O | 신규 개발 필요 |

### UI/UX 갭

| 항목 | AS-IS | TO-BE |
|------|-------|-------|
| Stage 명칭 | 현재상황/조건변경/시뮬레이션 | Baseline/Disruption/Reschedule |
| 진행 표시 | 숫자 (1,2,3) | 의미있는 아이콘 + 레이블 |
| Disruption 입력 | 테이블 직접 편집 | 유형별 마법사/카드 |
| 결과 비교 | 단순 병렬 | 오버레이 + 차이 하이라이트 |

---

## 4. 새로운 UI 설계

### 4.1 Top Navigation Bar
```
┌─────────────────────────────────────────────────────────────────┐
│  ◉ BASELINE        ◯ DISRUPTION        ◯ RESCHEDULE           │
│  Current Schedule  │ Define Changes    │ Compare & Apply       │
│  ───────────────────────────────────────────────────────────── │
│  📊 4h 30m         │ 3 changes         │                       │
│  ✓ On Schedule     │ pending           │ [▶ RUN]               │
└─────────────────────────────────────────────────────────────────┘
```

### 4.2 Stage 1: BASELINE
```
┌─────────────────────────────────────────────────────────────────┐
│ BASELINE SCHEDULE                              [📁 Load] [💾]  │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─ KPI Summary ──────────────────────────────────────────────┐ │
│  │  📊 Makespan     🔧 Utilization    ⏰ On-Time     ⚠️ Late   │ │
│  │     4h 30m          72.3%            85.7%          2       │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌─ Gantt Chart (Resource View) ──────────────────────────────┐ │
│  │ EQP-001 ████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │ │
│  │ EQP-002 ░░░░████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │ │
│  │ EQP-003 ░░░░░░░░░░░░████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │ │
│  │ WRK-001 ████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │ │
│  │ WRK-002 ░░░░░░░░████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │ │
│  │         ├────┼────┼────┼────┼────┼────┼────┼────┼────┼──── │ │
│  │         0    1h   2h   3h   4h   5h   6h   7h   8h         │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌─ Data Tables ──────────────────────────────────────────────┐ │
│  │ [Jobs] [Operations] [Resources] [Assignments]              │ │
│  │ ┌──────────┬──────────┬──────────┬──────────┬───────────┐ │ │
│  │ │ Job ID   │ Product  │ Priority │ Qty      │ Due Date  │ │ │
│  │ ├──────────┼──────────┼──────────┼──────────┼───────────┤ │ │
│  │ │ JOB-001  │Product-A │ 1        │ 10       │ 12/31     │ │ │
│  │ │ JOB-002  │Product-B │ 2        │ 5        │ 12/25 ⚠️  │ │ │
│  │ │ JOB-003  │Product-A │ 1        │ 8        │ 12/28     │ │ │
│  │ └──────────┴──────────┴──────────┴──────────┴───────────┘ │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│                                    [→ Define Disruption]        │
└─────────────────────────────────────────────────────────────────┘
```

### 4.3 Stage 2: DISRUPTION
```
┌─────────────────────────────────────────────────────────────────┐
│ DISRUPTION SCENARIO                            [Clear All]      │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─ Disruption Type ──────────────────────────────────────────┐ │
│  │                                                              │ │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │ │
│  │  │ 📦 DEMAND    │  │ 🔧 RESOURCE  │  │ ⚙️ PROCESS   │      │ │
│  │  │              │  │              │  │              │      │ │
│  │  │ + New Order  │  │ - Machine    │  │ ~ Delay      │      │ │
│  │  │ ± Change     │  │   Down       │  │ ~ Route      │      │ │
│  │  │ - Cancel     │  │ - Worker Out │  │   Change     │      │ │
│  │  │              │  │ ± Capacity   │  │              │      │ │
│  │  └──────────────┘  └──────────────┘  └──────────────┘      │ │
│  │                                                              │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌─ Applied Disruptions ──────────────────────────────────────┐ │
│  │                                                              │ │
│  │  ┌─────────────────────────────────────────────────────┐   │ │
│  │  │ 🔧 Machine Down                              [✕]    │   │ │
│  │  │    EQP-001 unavailable 10:00 - 14:00               │   │ │
│  │  │    Impact: 3 operations affected                    │   │ │
│  │  └─────────────────────────────────────────────────────┘   │ │
│  │                                                              │ │
│  │  ┌─────────────────────────────────────────────────────┐   │ │
│  │  │ 📦 New Rush Order                            [✕]    │   │ │
│  │  │    JOB-004: Product-C, Qty: 15, Due: 12/24         │   │ │
│  │  │    Priority: URGENT (1)                             │   │ │
│  │  └─────────────────────────────────────────────────────┘   │ │
│  │                                                              │ │
│  │  [+ Add Disruption]                                         │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌─ Impact Preview ───────────────────────────────────────────┐ │
│  │ ⚠️ 3 operations need rescheduling                          │ │
│  │ ⚠️ JOB-002 may be late                                     │ │
│  │ ℹ️ 1 new job added to queue                                │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│                                    [▶ Run Rescheduling]         │
└─────────────────────────────────────────────────────────────────┘
```

### 4.4 Stage 3: RESCHEDULE
```
┌─────────────────────────────────────────────────────────────────┐
│ RESCHEDULE COMPARISON              [Apply as Baseline] [Export] │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─ KPI Comparison ───────────────────────────────────────────┐ │
│  │  Metric        │ Baseline    │ Proposed    │ Change        │ │
│  │  ─────────────────────────────────────────────────────────  │ │
│  │  Makespan      │ 4h 30m      │ 5h 15m      │ ▲ +16.7% ⚠️   │ │
│  │  Utilization   │ 72.3%       │ 68.1%       │ ▼ -4.2%       │ │
│  │  On-Time       │ 85.7%       │ 100%        │ ▲ +14.3% ✓    │ │
│  │  Late Jobs     │ 2           │ 0           │ ▼ -100% ✓     │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌─ Gantt Comparison ─────────────────────────────────────────┐ │
│  │                                                              │ │
│  │  BASELINE (Before)                                          │ │
│  │  ─────────────────────────────────────────────────────────  │ │
│  │  EQP-001 ████████░░░░░░░░░░░░░░░░░░░░│                     │ │
│  │  EQP-002 ░░░░████████░░░░░░░░░░░░░░░░│                     │ │
│  │  EQP-003 ░░░░░░░░░░░░████████░░░░░░░░│                     │ │
│  │          ├────┼────┼────┼────┼────┼──│                     │ │
│  │                                                              │ │
│  │  PROPOSED (After Rescheduling)                              │ │
│  │  ─────────────────────────────────────────────────────────  │ │
│  │  EQP-001 ░░░░░░░░░░████████████░░░░░░│  ← Machine down     │ │
│  │  EQP-002 ████████████████░░░░░░░░░░░░│  ← Absorbed load    │ │
│  │  EQP-003 ░░░░░░░░░░░░░░░░████████████│                     │ │
│  │          ├────┼────┼────┼────┼────┼──│                     │ │
│  │          0    1h   2h   3h   4h   5h                        │ │
│  │                                                              │ │
│  │  Legend: ████ JOB-001  ████ JOB-002  ████ JOB-003          │ │
│  │          ░░░░ Idle     ▓▓▓▓ New JOB-004                     │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌─ Changes Detail ───────────────────────────────────────────┐ │
│  │ Operation    │ Original Start │ New Start │ Change         │ │
│  │ ─────────────────────────────────────────────────────────  │ │
│  │ OP-001-2     │ 10:00          │ 14:00     │ +4h (delayed)  │ │
│  │ OP-002-1     │ 09:00          │ 08:00     │ -1h (earlier)  │ │
│  │ OP-003-1     │ 11:00          │ 10:00     │ -1h (earlier)  │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 5. 구현 계획

### Phase 1: 용어 및 기본 구조 변경
- [ ] Stage 명칭 변경 (현재상황→Baseline, 조건변경→Disruption, 시뮬레이션→Reschedule)
- [ ] 아이콘 및 레이블 업데이트
- [ ] 상태 표시줄 개선

### Phase 2: Disruption 시스템
- [ ] Disruption Type 정의 (enum/model)
- [ ] Disruption 카드 컴포넌트
- [ ] Disruption 입력 다이얼로그
- [ ] Impact Preview 계산

### Phase 3: 비교 시각화
- [ ] Baseline/Proposed Gantt 오버레이
- [ ] KPI 비교 테이블 개선
- [ ] 변경 항목 하이라이트
- [ ] 차이 상세 테이블

### Phase 4: 워크플로우 완성
- [ ] "Apply as Baseline" 기능
- [ ] 시나리오 저장/불러오기
- [ ] 변경 이력 관리

---

## 6. 데이터 모델 변경

### 새로운 모델

```csharp
// Disruption Types
public enum DisruptionType
{
    // Demand
    NewOrder,
    OrderChange,
    PriorityChange,
    DueDateChange,
    OrderCancel,
    
    // Resource
    MachineDown,
    WorkerAbsence,
    CapacityChange,
    NewResource,
    
    // Process
    OperationDelay,
    SetupTimeChange,
    RouteChange,
    
    // Material
    MaterialShortage,
    DeliveryDelay,
    MaterialArrival
}

// Disruption Entry
public class Disruption
{
    public string Id { get; set; }
    public DisruptionType Type { get; set; }
    public string TargetId { get; set; }  // Job, Resource, Operation ID
    public Dictionary<string, object> Parameters { get; set; }
    public DateTime CreatedAt { get; set; }
    public string Description { get; set; }
}

// Scenario (What-If)
public class Scenario
{
    public string Id { get; set; }
    public string Name { get; set; }
    public ScheduleResult BaselineSchedule { get; set; }
    public List<Disruption> Disruptions { get; set; }
    public ScheduleResult ProposedSchedule { get; set; }
    public ScenarioComparison Comparison { get; set; }
}
```

---

## 7. 참고 화면 (Commercial APS)

### Simio APS
- 3D 시각화 + Gantt
- What-If 시나리오 복제
- Risk 분석 통합

### PlanetTogether
- Color-coded Late Jobs (Red=Capacity, Pink=Material)
- Drag & Drop 수동 조정
- Multi-scenario 병렬 비교

### Dynamics 365 Supply Chain
- Baseline vs Tracking Gantt
- Variance 자동 계산
- Integration with ERP

---

*Document Version: 1.0*
*Created: 2026-01-16*
