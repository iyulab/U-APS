//! Pegging - 수요-공급 연결
//!
//! 동적 페깅과 자재 제약 스케줄링

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 자재/부품 정보 (Pegging용)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeggingMaterial {
    /// 자재 ID
    pub id: String,
    /// 자재명
    pub name: String,
    /// 현재 재고량
    pub on_hand_qty: f64,
    /// 단위
    pub unit: String,
}

/// 자재 공급 (입고 예정)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialSupply {
    /// 공급 ID
    pub id: String,
    /// 자재 ID
    pub material_id: String,
    /// 수량
    pub quantity: f64,
    /// 가용 시점 (ms)
    pub available_at_ms: i64,
    /// 공급 유형
    pub supply_type: SupplyType,
}

/// 공급 유형
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SupplyType {
    /// 현재 재고
    OnHand,
    /// 구매 발주
    PurchaseOrder,
    /// 생산 주문 (반제품)
    ProductionOrder,
    /// 이동 중
    InTransit,
}

/// 자재 수요
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialDemand {
    /// 수요 ID
    pub id: String,
    /// 자재 ID
    pub material_id: String,
    /// 필요 수량
    pub quantity: f64,
    /// 필요 시점 (ms)
    pub required_at_ms: i64,
    /// 연결된 공정 ID
    pub operation_id: String,
    /// 연결된 Job ID
    pub job_id: String,
}

/// 페깅 연결
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeggingLink {
    /// 수요 ID
    pub demand_id: String,
    /// 공급 ID
    pub supply_id: String,
    /// 할당 수량
    pub allocated_qty: f64,
}

/// 페깅 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeggingResult {
    /// 페깅 연결 목록
    pub links: Vec<PeggingLink>,
    /// 미충족 수요
    pub unmet_demands: Vec<UnmetDemand>,
    /// 자재별 가용 시점
    pub material_availability: HashMap<String, i64>,
}

/// 미충족 수요
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnmetDemand {
    /// 수요 ID
    pub demand_id: String,
    /// 자재 ID
    pub material_id: String,
    /// 부족 수량
    pub shortage_qty: f64,
    /// 예상 가용 시점 (없으면 None)
    pub expected_available_at: Option<i64>,
}

/// 페깅 엔진
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PeggingEngine {
    /// 자재 목록
    materials: HashMap<String, PeggingMaterial>,
    /// 공급 목록
    supplies: Vec<MaterialSupply>,
    /// 수요 목록
    demands: Vec<MaterialDemand>,
}

impl PeggingEngine {
    pub fn new() -> Self {
        Self {
            materials: HashMap::new(),
            supplies: Vec::new(),
            demands: Vec::new(),
        }
    }

    /// 자재 추가
    pub fn add_material(&mut self, material: PeggingMaterial) {
        self.materials.insert(material.id.clone(), material);
    }

    /// 공급 추가
    pub fn add_supply(&mut self, supply: MaterialSupply) {
        self.supplies.push(supply);
    }

    /// 수요 추가
    pub fn add_demand(&mut self, demand: MaterialDemand) {
        self.demands.push(demand);
    }

    /// 동적 페깅 수행
    pub fn execute_pegging(&self) -> PeggingResult {
        let mut links = Vec::new();
        let mut unmet_demands = Vec::new();
        let mut material_availability = HashMap::new();

        // 자재별로 처리
        let material_ids: Vec<String> = self.materials.keys().cloned().collect();

        for material_id in material_ids {
            // 해당 자재의 수요 (시간순 정렬)
            let mut mat_demands: Vec<&MaterialDemand> = self
                .demands
                .iter()
                .filter(|d| d.material_id == material_id)
                .collect();
            mat_demands.sort_by_key(|d| d.required_at_ms);

            // 해당 자재의 공급 (시간순 정렬)
            let mut mat_supplies: Vec<MaterialSupply> = self
                .supplies
                .iter()
                .filter(|s| s.material_id == material_id)
                .cloned()
                .collect();
            mat_supplies.sort_by_key(|s| s.available_at_ms);

            // 현재 재고 추가
            if let Some(material) = self.materials.get(&material_id) {
                if material.on_hand_qty > 0.0 {
                    mat_supplies.insert(
                        0,
                        MaterialSupply {
                            id: format!("{}_onhand", material_id),
                            material_id: material_id.clone(),
                            quantity: material.on_hand_qty,
                            available_at_ms: 0,
                            supply_type: SupplyType::OnHand,
                        },
                    );
                }
            }

            // 수요-공급 매칭 (FIFO)
            let mut supply_remaining: Vec<f64> = mat_supplies.iter().map(|s| s.quantity).collect();

            let mut latest_availability = 0i64;

            for demand in mat_demands {
                let mut remaining_demand = demand.quantity;
                let mut supply_idx = 0;

                while remaining_demand > 0.0 && supply_idx < mat_supplies.len() {
                    if supply_remaining[supply_idx] > 0.0 {
                        let allocate = remaining_demand.min(supply_remaining[supply_idx]);

                        links.push(PeggingLink {
                            demand_id: demand.id.clone(),
                            supply_id: mat_supplies[supply_idx].id.clone(),
                            allocated_qty: allocate,
                        });

                        supply_remaining[supply_idx] -= allocate;
                        remaining_demand -= allocate;

                        // 가용 시점 업데이트
                        latest_availability =
                            latest_availability.max(mat_supplies[supply_idx].available_at_ms);
                    }
                    supply_idx += 1;
                }

                if remaining_demand > 0.0 {
                    // 미충족 수요
                    unmet_demands.push(UnmetDemand {
                        demand_id: demand.id.clone(),
                        material_id: material_id.clone(),
                        shortage_qty: remaining_demand,
                        expected_available_at: None,
                    });
                }
            }

            material_availability.insert(material_id, latest_availability);
        }

        PeggingResult {
            links,
            unmet_demands,
            material_availability,
        }
    }

    /// 공정별 자재 가용 시점 계산
    pub fn get_operation_material_availability(&self, operation_id: &str) -> i64 {
        let result = self.execute_pegging();

        // 해당 공정의 모든 수요에 대한 자재 가용 시점 중 최대값
        self.demands
            .iter()
            .filter(|d| d.operation_id == operation_id)
            .map(|d| {
                result
                    .material_availability
                    .get(&d.material_id)
                    .cloned()
                    .unwrap_or(0)
            })
            .max()
            .unwrap_or(0)
    }
}

impl Default for PeggingEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// 자재 제약 스케줄링 헬퍼
pub fn calculate_material_constrained_start(base_start_ms: i64, material_available_ms: i64) -> i64 {
    base_start_ms.max(material_available_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_pegging() {
        let mut engine = PeggingEngine::new();

        // 자재 등록
        engine.add_material(PeggingMaterial {
            id: "mat1".into(),
            name: "Component A".into(),
            on_hand_qty: 50.0,
            unit: "EA".into(),
        });

        // 수요 등록
        engine.add_demand(MaterialDemand {
            id: "dem1".into(),
            material_id: "mat1".into(),
            quantity: 30.0,
            required_at_ms: 10_000,
            operation_id: "op1".into(),
            job_id: "job1".into(),
        });

        let result = engine.execute_pegging();

        assert_eq!(result.links.len(), 1);
        assert_eq!(result.unmet_demands.len(), 0);
        assert_eq!(result.links[0].allocated_qty, 30.0);
    }

    #[test]
    fn test_multiple_supplies() {
        let mut engine = PeggingEngine::new();

        engine.add_material(PeggingMaterial {
            id: "mat1".into(),
            name: "Component A".into(),
            on_hand_qty: 20.0,
            unit: "EA".into(),
        });

        engine.add_supply(MaterialSupply {
            id: "sup1".into(),
            material_id: "mat1".into(),
            quantity: 30.0,
            available_at_ms: 5_000,
            supply_type: SupplyType::PurchaseOrder,
        });

        engine.add_demand(MaterialDemand {
            id: "dem1".into(),
            material_id: "mat1".into(),
            quantity: 40.0,
            required_at_ms: 10_000,
            operation_id: "op1".into(),
            job_id: "job1".into(),
        });

        let result = engine.execute_pegging();

        // 재고 20 + 발주 20 = 40
        assert_eq!(result.links.len(), 2);
        assert_eq!(result.unmet_demands.len(), 0);
    }

    #[test]
    fn test_shortage() {
        let mut engine = PeggingEngine::new();

        engine.add_material(PeggingMaterial {
            id: "mat1".into(),
            name: "Component A".into(),
            on_hand_qty: 10.0,
            unit: "EA".into(),
        });

        engine.add_demand(MaterialDemand {
            id: "dem1".into(),
            material_id: "mat1".into(),
            quantity: 50.0,
            required_at_ms: 10_000,
            operation_id: "op1".into(),
            job_id: "job1".into(),
        });

        let result = engine.execute_pegging();

        assert_eq!(result.links.len(), 1);
        assert_eq!(result.links[0].allocated_qty, 10.0);
        assert_eq!(result.unmet_demands.len(), 1);
        assert_eq!(result.unmet_demands[0].shortage_qty, 40.0);
    }

    #[test]
    fn test_material_availability_time() {
        let mut engine = PeggingEngine::new();

        engine.add_material(PeggingMaterial {
            id: "mat1".into(),
            name: "Component A".into(),
            on_hand_qty: 0.0,
            unit: "EA".into(),
        });

        engine.add_supply(MaterialSupply {
            id: "sup1".into(),
            material_id: "mat1".into(),
            quantity: 50.0,
            available_at_ms: 20_000,
            supply_type: SupplyType::PurchaseOrder,
        });

        engine.add_demand(MaterialDemand {
            id: "dem1".into(),
            material_id: "mat1".into(),
            quantity: 30.0,
            required_at_ms: 10_000,
            operation_id: "op1".into(),
            job_id: "job1".into(),
        });

        let availability = engine.get_operation_material_availability("op1");
        assert_eq!(availability, 20_000);
    }

    #[test]
    fn test_material_constrained_start() {
        // 기계 가용: 10_000, 자재 가용: 20_000 → 시작: 20_000
        assert_eq!(calculate_material_constrained_start(10_000, 20_000), 20_000);

        // 기계 가용: 30_000, 자재 가용: 20_000 → 시작: 30_000
        assert_eq!(calculate_material_constrained_start(30_000, 20_000), 30_000);
    }
}
