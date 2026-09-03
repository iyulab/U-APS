# APS 핵심 지식 정리

U-APS 프로젝트 개발을 위한 핵심 개념과 기술 사양 정리

---

## 1. 문제 정의

### FJSSP (Flexible Job Shop Scheduling Problem)
- **정의**: 각 공정이 여러 대체 기계에서 처리 가능한 Job Shop 문제
- **복잡도**: NP-Hard (다항 시간 내 최적해 보장 불가)
- **구성요소**:
  - Jobs: n개의 작업
  - Operations: 각 Job의 순차 공정들
  - Machines: m개의 기계 (대체 가능)
  - Processing Times: 기계별 상이한 처리 시간

### RCPSP (Resource-Constrained Project Scheduling Problem)
- **정의**: 자원 제약 하에서 프로젝트 활동 스케줄링
- **특징**:
  - Renewable Resources (매 시점 용량 제한)
  - Non-renewable Resources (총량 제한)
  - Precedence Relations (선행 관계)

### 스케줄링 유형
| 유형 | 설명 | 적용 |
|------|------|------|
| **유한 용량** | 자원 제약 엄격 적용 | 현실적 일정 |
| **무한 용량** | 자원 제약 무시 | 초기 계획 |
| **전진 스케줄링** | 시작일부터 전진 | 조기 시작 |
| **후진 스케줄링** | 납기부터 역산 | JIT 생산 |

---

## 2. 알고리즘

### 디스패칭 룰 (단순 휴리스틱)
| 룰 | 설명 | 장점 | 단점 |
|----|------|------|------|
| **SPT** | Shortest Processing Time | 평균 체류시간 최소화 | 납기 무시 |
| **EDD** | Earliest Due Date | 최대 지연 최소화 | 처리시간 무시 |
| **CR** | Critical Ratio (납기-현재시간)/잔여처리시간 | 긴급도 반영 | 동적 변화 |
| **ATC** | Apparent Tardiness Cost | SPT+EDD 결합 | 파라미터 튜닝 |

### 메타휴리스틱
| 알고리즘 | 원리 | U-APS 적용 |
|----------|------|------------|
| **GA** | 자연선택, 유전 | 주 알고리즘 (Hybrid GA) |
| **Tabu Search** | 금기 목록 기반 탐색 | 지역 탐색 강화 |
| **SA** | 온도 기반 확률적 수용 | 하이브리드 가능 |

### CP-SAT (Constraint Programming)
- **Interval Variables**: 시작/종료/기간 변수
- **NoOverlap Constraint**: 기계별 비중첩
- **Cumulative Constraint**: 누적 자원 용량
- **Transition Matrix**: Setup Time 모델링
- **용도**: GA 해의 정교화, 실행 가능성 검증

---

## 3. Hybrid GA 설계

### Dual-Vector 염색체 인코딩

```
예: 2 Jobs, 각 2 Operations, 2 Machines

OSV (Operation Sequence Vector):
[1, 2, 1, 2] → Job1-Op1, Job2-Op1, Job1-Op2, Job2-Op2

MAV (Machine Assignment Vector):
{(J1,1): M1, (J1,2): M2, (J2,1): M2, (J2,2): M1}
```

**장점**:
- OSV: Job ID 반복으로 선행 제약 자동 유지
- MAV: 독립적 기계 할당으로 탐색 공간 확대

### 초기 모집단 생성 전략
| 전략 | 비율 | 목적 |
|------|------|------|
| **Random** | 50% | 다양성 확보 |
| **Load Balancing** | 25% | 부하 균등 |
| **Shortest Processing** | 25% | 빠른 수렴 |

### 유전 연산자

**Crossover (교차)**:
| 연산자 | 대상 | 방식 |
|--------|------|------|
| **POX** | OSV | Precedence-preserving Order Crossover |
| **LOX** | OSV | Linear Order Crossover |
| **Uniform** | MAV | 균일 교차 |

**Mutation (돌연변이)**:
- **Swap**: 두 위치 교환
- **Insert**: 위치 이동
- **Machine Change**: 대체 기계 선택

### Active Schedule Generation (디코딩)
1. OSV 순서대로 공정 선택
2. MAV에서 할당된 기계 확인
3. 가능한 가장 빠른 시작 시간 계산:
   - 기계 가용 시간
   - 선행 공정 완료 시간
   - Setup Time
4. 유휴 시간에 삽입 가능하면 삽입 (Gap Insertion)

### 적합도 함수
```
Fitness = Makespan + Σ(Tardiness × Priority × Weight)
```
- **Makespan**: 전체 완료 시간
- **Tardiness**: Max(0, 완료시간 - 납기)
- **Priority**: 작업 우선순위
- **Weight**: 지연 패널티 가중치

### GA 파라미터 (기본값)
| 파라미터 | 값 | 설명 |
|----------|-----|------|
| Population Size | 100 | 모집단 크기 |
| Max Generations | 500 | 최대 세대 |
| Elite Ratio | 0.1 | 엘리트 보존 비율 |
| Crossover Rate | 0.8 | 교차 확률 |
| Mutation Rate | 0.1 | 돌연변이 확률 |
| Tournament Size | 3 | 토너먼트 크기 |
| Convergence Generations | 50 | 수렴 판정 세대 |

---

## 4. 고급 제약 조건

### SDST (Sequence-Dependent Setup Times)
```
Setup Matrix S[m][i][j] = 기계 m에서 제품 i → j 전환 시간

예:
     A    B    C
A    0   10   15
B   12    0    8
C   20   14    0
```

### Multi-Resource Operations
- 동시에 여러 자원 필요 (예: 설비 + 작업자)
- 모든 자원이 가용할 때만 시작 가능
- 공유 자원 경합 관리

### Alternative Resources
- 동일 공정을 여러 기계에서 처리 가능
- 각 기계별 효율성(Efficiency) 차이
- 처리시간 = 기준시간 / 효율성

### Operation Splitting (공정 분할)
- 하나의 공정을 여러 기계에서 병렬 처리
- 최소 분할 단위(Minimum Lot Size) 제약
- Setup Time 중복 발생 고려

### Operation Overlapping (공정 중첩)
- 선행 공정 완료 전 후속 공정 시작
- Transfer Batch vs Process Batch
- 파이프라이닝 효과

### Skill Matrix (스킬 매트릭스)
```
        CNC  Assembly  Welding
Worker1  0.9    0.7      0.5
Worker2  0.6    0.9      0.8
Worker3  0.8    0.5      0.9
```
- 작업자별 공정 숙련도
- 처리시간 = 기준시간 / 숙련도

---

## 5. 재스케줄링

### 재스케줄링 전략
| 전략 | 설명 | 적용 |
|------|------|------|
| **Right-Shift** | 영향 공정 단순 지연 | 소규모 지연 |
| **AOR** | 영향 공정만 재스케줄 | 중간 규모 변경 |
| **Total Regeneration** | 전체 재스케줄 | 대규모 변경 |

### Time Fences (계획 경계)
```
현재 ──────┬──────┬──────┬──────→ 미래
      Frozen │Slushy│Liquid│
      (변경X) │(제한) │(자유) │
```
- **Frozen Zone**: 변경 불가 (실행 중)
- **Slushy Zone**: 제한적 변경 (확정 자재)
- **Liquid Zone**: 자유 변경 가능

### Predictive-Reactive Scheduling
- **Predictive**: 예측 스케줄 생성 (버퍼 포함)
- **Reactive**: 실시간 이벤트 대응
- **버퍼 산정**: 과거 변동성 기반

---

## 6. Pegging (수요-공급 연결)

### Dynamic Pegging
- 수요(작업지시)와 공급(재고/입고예정) 연결
- 자재 가용 시점에 따른 공정 시작 제약
- 다계층 BOM 전개

### Material-Constrained Scheduling
```
공정 시작 가능 시간 = Max(
    기계 가용 시간,
    선행 공정 완료 시간,
    필요 자재 가용 시간
)
```

---

## 7. CTP/ATP (납기 약속)

### ATP (Available-to-Promise)
- 현재 재고 + 예정 입고 - 확정 수요
- 즉시 가용 수량 계산

### CTP (Capable-to-Promise)
- 자원/자재 가용성 기반 생산 가능 납기 계산
- 단계:
  1. 자재 가용성 검사
  2. 생산 용량 검사
  3. 가능 납기일 제시
  4. 부분 납품 옵션 제공

---

## 8. KPI (핵심 성과 지표)

| KPI | 정의 | 목표 |
|-----|------|------|
| **Makespan** | 전체 완료 시간 | 최소화 |
| **Utilization** | 자원 가동률 | 최대화 |
| **On-time Delivery** | 납기 준수율 | 최대화 |
| **Tardiness** | 지연 시간 합계 | 최소화 |
| **MCE** | Manufacturing Cycle Efficiency | 최대화 |
| **Schedule Adherence** | 계획 준수율 | 최대화 |

---

## 9. 아키텍처

### Rust Engine + C# SDK
```
C# Application
    ↓ (IPC)
C# SDK (UAPS.SDK)
    ↓ (Zero-Copy)
Rust Engine (uaps-engine)
    ↓
Schedule Result
```

### Zero-Copy IPC 옵션
| 방식 | 장점 | 단점 |
|------|------|------|
| **Memory Mapped Files** | OS 지원, 간단 | 직렬화 필요 |
| **Apache Arrow** | 표준 포맷, 효율적 | 의존성 추가 |

### 데이터 흐름
```
입력 (Excel/JSON/B2MML)
    ↓
Parser → ScheduleRequest
    ↓
Engine (GA/CP-SAT)
    ↓
Schedule (Assignments)
    ↓
Writer → 출력 (Excel/JSON/Gantt)
```

---

## 10. 데이터 표준

### ISA-95 / B2MML
- **ISA-95**: 제조 운영 관리 표준
- **B2MML**: Business to Manufacturing Markup Language

### 주요 엔티티
| ISA-95 엔티티 | U-APS 매핑 | 설명 |
|---------------|------------|------|
| **OperationsSchedule** | ScheduleRequest | 스케줄 요청 |
| **OperationsRequest** | Job | 작업 지시 |
| **SegmentRequirement** | Operation | 공정 |
| **EquipmentRequirement** | Resource | 자원 요구 |
| **OperationsPerformance** | Schedule | 결과 스케줄 |

---

## 11. 벤치마크

### Taillard Instances
- **범위**: 15x15 ~ 100x20 (Jobs x Machines)
- **용도**: JSP 알고리즘 검증
- **목표**: Best Known Solution 대비 3% 이내

### Lawrence Instances
- **범위**: la01-la40
- **특징**: 다양한 문제 크기
- **용도**: FJSSP 확장 검증

### PSPLIB (j30, j60, j120)
- **용도**: RCPSP 검증
- **특징**: 자원 제약 포함

### 성능 목표
| 인스턴스 | Jobs | 목표 오차율 | 시간 제한 |
|----------|------|-------------|-----------|
| ta15 | 15 | 1% | 10초 |
| ta40 | 50 | 2% | 30초 |
| ta80 | 100 | 3% | 1분 |

---

## 12. 구현 현황

### 완료된 기능 (Phase 1-4)
- ✅ Core 데이터 모델 (Job, Operation, Resource)
- ✅ SimpleScheduler (우선순위 기반)
- ✅ 제약 조건 (DueDate, Calendar)
- ✅ SDST (Setup Time Matrix)
- ✅ Multi-Resource Operations
- ✅ Alternative Resources
- ✅ Dual-Vector 염색체 (OSV + MAV)
- ✅ GA 연산자 (POX, LOX, Mutation)
- ✅ Population Management
- ✅ Active Schedule Generation

### 구현 예정 기능 (Phase 5-8)
- 📋 Rayon 병렬화
- 📋 Zero-Copy IPC
- 📋 Operation Splitting/Overlapping
- 📋 CP-SAT Integration
- 📋 ISA-95/B2MML
- 📋 Skill Matrix
- 📋 Rescheduling (Time Fences)
- 📋 Dynamic Pegging
- 📋 CTP/ATP
- 📋 What-If Analysis
- 📋 Interactive Gantt

---

## 13. 참고 자료

### 연구 문서
- `research-docs/APS 핵심 기능 조사.extracted.json`
- `research-docs/U-APS 프로젝트 리서치.extracted.json`

### 주요 참고 논문/서적
- Pinedo, M. "Scheduling: Theory, Algorithms, and Systems"
- Brucker, P. "Scheduling Algorithms"
- Gen & Cheng "Genetic Algorithms and Engineering Optimization"

### 벤치마크 데이터
- Taillard: http://mistic.heig-vd.ch/taillard/
- PSPLIB: http://www.om-db.wi.tum.de/psplib/

---

## 14. 용어 정리

| 용어 | 영문 | 설명 |
|------|------|------|
| 공정 | Operation | 작업의 단위 활동 |
| 작업 | Job | 공정들의 집합 |
| 메이크스팬 | Makespan | 전체 완료 시간 |
| 납기 | Due Date | 완료 기한 |
| 지연 | Tardiness | 납기 초과 시간 |
| 준비시간 | Setup Time | 작업 전환 시간 |
| 페깅 | Pegging | 수요-공급 연결 |
| 염색체 | Chromosome | GA의 해 표현 |
| 적합도 | Fitness | 해의 품질 척도 |
| 모집단 | Population | 해 집합 |
| 세대 | Generation | GA 반복 단위 |
| 엘리트 | Elite | 우수 개체 보존 |

---

## 15. APS 도메인 개념 참조

### 15.1 생산 자원 (Production Resources)

#### Equipment (설비)
- Machine Groups/Work Centers - 설비 그룹
- Capabilities/Specifications - 설비 능력
- Alternative Machines - 대체 설비
- PM Schedule - 예방 정비 일정

#### Worker (작업자)
- Skills/Qualifications - 스킬/자격
- Certifications - 인증/자격증
- Shift Patterns - 교대 패턴
- Labor Pools/Teams - 작업팀

#### WorkingPlace (작업장)
- Production Lines - 생산 라인
- Storage Areas/Buffers - 저장 공간

#### Materials (자재)
- Raw Materials - 원자재
- WIP (Work In Process) - 재공품
- Finished Goods - 완제품
- Material Availability - 자재 가용성

#### Tools & Fixtures (공구)
- Tool Life/Wear - 공구 수명
- Tool Allocation - 공구 할당

### 15.2 시간 요소 (Time Elements)

| 요소 | 영문 | 설명 |
|------|------|------|
| 준비시간 | Setup Time | 공정 전 준비 |
| 처리시간 | Process Time | 실제 가공 |
| 대기시간 | Wait/Queue Time | 공정 후 대기 |
| 이동시간 | Move/Transit Time | 자원간 이동 |
| 리드타임 | Lead Time | 총 소요시간 |

### 15.3 제약 조건 (Constraints)

#### Capacity Constraints (능력 제약)
- Machine Capacity - 설비 용량
- Labor Capacity - 인력 용량
- Space/Storage - 공간 제약

#### Dependencies (의존 관계)
- Precedence (FS/SS/FF/SF) - 선후행
- No-Wait - 대기 불가
- Min/Max Gap - 최소/최대 간격

#### Material Constraints (자재 제약)
- BOM - 자재명세서
- Shelf Life - 유통기한
- Lot/Batch Tracking - 추적

### 15.4 성과 지표 (Performance Metrics)

| 지표 | 영문 | 목표 |
|------|------|------|
| 납기준수율 | On-Time Delivery | 최대화 |
| 가동률 | Utilization | 최대화 |
| 처리량 | Throughput | 최대화 |
| 완료시간 | Makespan | 최소화 |
| 재고비용 | Inventory Cost | 최소화 |

### 15.5 계획/스케줄링 기능

#### Planning Capabilities
- MPS (Master Production Scheduling) - 기준생산계획
- MRP (Material Requirements Planning) - 자재소요계획
- CRP (Capacity Requirements Planning) - 능력소요계획

#### Scheduling Methods
- Finite/Infinite Capacity - 유한/무한 능력
- Forward/Backward - 순방향/역방향
- Constraint-Based - 제약기반

#### Sequencing Rules
- FIFO - 선입선출
- SPT - 최단처리시간
- EDD - 최단납기
- CR - 긴급비율

### 15.6 동적 스케줄링 (Dynamic Scheduling)

#### Rescheduling Triggers
- Machine Breakdown - 설비 고장
- Rush Orders - 긴급 주문
- Material Shortage - 자재 부족
- Quality Issues - 품질 문제

#### Rescheduling Strategies
- Right-Shift - 단순 지연
- AOR - 영향 공정만
- Total Regeneration - 전체 재스케줄

### 15.7 고급 기능

#### ATP/CTP
- ATP (Available to Promise) - 가용재고 약속
- CTP (Capable to Promise) - 생산능력 약속

#### Pegging
- Order Pegging - 주문 페깅
- Dynamic Pegging - 동적 페깅
- Where-Used - 사용처 추적

#### What-If Analysis
- Scenario Planning - 시나리오 계획
- Monte Carlo Simulation - 몬테카를로
- Sensitivity Analysis - 민감도 분석

### 15.8 산업별 특화

| 산업 | 특화 기능 |
|------|-----------|
| 조립제조 | Assembly Scheduling, Kitting |
| 공정산업 | Recipe, Batch Scheduling |
| 자동차 | Mixed-Model Sequencing, JIT |
| 제약 | Campaign, Compliance |
| 반도체 | Re-entrant Flows, Batch |

### 15.9 통합 (Integration)

| 시스템 | 연동 내용 |
|--------|-----------|
| ERP | Order, Inventory, Master Data |
| MES | Production Feedback, Status |
| PLM | BOM, Routing |
| SCM | Supplier, Demand-Supply |
| IoT | Real-time Machine Data |

### 15.10 시각화 (Visualization)

- Gantt Charts - 간트 차트
- Capacity Load Charts - 부하도
- Resource Histograms - 자원 히스토그램
- KPI Dashboards - KPI 대시보드
