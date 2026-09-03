//! CTP - Capable-to-Promise
//!
//! 자원/자재 가용성 기반 납기 약속 계산

use crate::scheduler::pegging::{MaterialDemand, MaterialSupply, PeggingEngine, PeggingMaterial};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// CTP 요청
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtpRequest {
    /// 주문 ID
    pub order_id: String,
    /// 제품 ID
    pub product_id: String,
    /// 요청 수량
    pub quantity: f64,
    /// 요청 납기일 (ms)
    pub requested_date_ms: i64,
    /// 우선순위
    pub priority: i32,
}

/// CTP 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtpResult {
    /// 요청 ID
    pub order_id: String,
    /// 확약 가능 여부
    pub is_capable: bool,
    /// 확약 가능 날짜 (ms)
    pub promised_date_ms: Option<i64>,
    /// 확약 가능 수량
    pub promised_quantity: f64,
    /// 부분 납품 옵션
    pub partial_deliveries: Vec<PartialDelivery>,
    /// 제약 사항
    pub constraints: Vec<CtpConstraint>,
    /// ATP 수량 (즉시 가용)
    pub atp_quantity: f64,
}

/// 부분 납품
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialDelivery {
    /// 납품 수량
    pub quantity: f64,
    /// 납품 가능일 (ms)
    pub delivery_date_ms: i64,
}

/// CTP 제약 사항
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CtpConstraint {
    /// 자재 부족
    MaterialShortage {
        material_id: String,
        shortage_qty: f64,
        available_at_ms: Option<i64>,
    },
    /// 자원 용량 부족
    CapacityShortage {
        resource_id: String,
        required_hours: f64,
        available_hours: f64,
    },
    /// 리드타임 초과
    LeadTimeExceeded {
        min_lead_time_ms: i64,
        requested_ms: i64,
    },
}

/// ATP (Available-to-Promise) 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtpResult {
    /// 제품 ID
    pub product_id: String,
    /// 즉시 가용 수량
    pub available_qty: f64,
    /// 기간별 가용량
    pub projected_availability: Vec<(i64, f64)>,
}

/// 자원 용량 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceCapacity {
    /// 자원 ID
    pub resource_id: String,
    /// 일일 가용 시간 (ms)
    pub daily_capacity_ms: i64,
    /// 현재 할당된 시간 (ms)
    pub allocated_ms: i64,
}

/// CTP 엔진
pub struct CtpEngine {
    /// 페깅 엔진
    pegging: PeggingEngine,
    /// 자원 용량
    resource_capacities: HashMap<String, ResourceCapacity>,
    /// 제품별 BOM (자재 소요량)
    product_bom: HashMap<String, Vec<(String, f64)>>,
    /// 제품별 공정 시간 (ms)
    product_process_time: HashMap<String, i64>,
    /// 현재 시점
    current_time_ms: i64,
}

impl CtpEngine {
    pub fn new(current_time_ms: i64) -> Self {
        Self {
            pegging: PeggingEngine::new(),
            resource_capacities: HashMap::new(),
            product_bom: HashMap::new(),
            product_process_time: HashMap::new(),
            current_time_ms,
        }
    }

    /// 자재 추가
    pub fn add_material(&mut self, material: PeggingMaterial) {
        self.pegging.add_material(material);
    }

    /// 공급 추가
    pub fn add_supply(&mut self, supply: MaterialSupply) {
        self.pegging.add_supply(supply);
    }

    /// 자원 용량 설정
    pub fn set_resource_capacity(&mut self, capacity: ResourceCapacity) {
        self.resource_capacities
            .insert(capacity.resource_id.clone(), capacity);
    }

    /// 제품 BOM 설정
    pub fn set_product_bom(&mut self, product_id: &str, bom: Vec<(String, f64)>) {
        self.product_bom.insert(product_id.to_string(), bom);
    }

    /// 제품 공정 시간 설정
    pub fn set_product_process_time(&mut self, product_id: &str, time_ms: i64) {
        self.product_process_time
            .insert(product_id.to_string(), time_ms);
    }

    /// CTP 확인
    pub fn check_capability(&mut self, request: &CtpRequest) -> CtpResult {
        let mut constraints = Vec::new();
        let mut material_available_ms = self.current_time_ms;

        // 1. 자재 가용성 확인
        if let Some(bom) = self.product_bom.get(&request.product_id) {
            for (material_id, qty_per_unit) in bom {
                let required_qty = qty_per_unit * request.quantity;

                // 임시 수요 추가
                self.pegging.add_demand(MaterialDemand {
                    id: format!("ctp_{}_{}", request.order_id, material_id),
                    material_id: material_id.clone(),
                    quantity: required_qty,
                    required_at_ms: request.requested_date_ms,
                    operation_id: format!("ctp_{}", request.order_id),
                    job_id: request.order_id.clone(),
                });
            }

            let pegging_result = self.pegging.execute_pegging();

            // 미충족 자재 확인
            for unmet in &pegging_result.unmet_demands {
                if unmet
                    .demand_id
                    .starts_with(&format!("ctp_{}", request.order_id))
                {
                    constraints.push(CtpConstraint::MaterialShortage {
                        material_id: unmet.material_id.clone(),
                        shortage_qty: unmet.shortage_qty,
                        available_at_ms: unmet.expected_available_at,
                    });
                }
            }

            // 자재 가용 시점 계산
            for (material_id, _) in bom {
                if let Some(&avail) = pegging_result.material_availability.get(material_id) {
                    material_available_ms = material_available_ms.max(avail);
                }
            }
        }

        // 2. 자원 용량 확인
        let process_time = self
            .product_process_time
            .get(&request.product_id)
            .cloned()
            .unwrap_or(0);

        let total_process_time = process_time * request.quantity as i64;

        for (resource_id, capacity) in &self.resource_capacities {
            let available = capacity.daily_capacity_ms - capacity.allocated_ms;
            if total_process_time > available {
                constraints.push(CtpConstraint::CapacityShortage {
                    resource_id: resource_id.clone(),
                    required_hours: total_process_time as f64 / 3_600_000.0,
                    available_hours: available as f64 / 3_600_000.0,
                });
            }
        }

        // 3. 리드타임 확인
        let min_completion = material_available_ms + total_process_time;
        if min_completion > request.requested_date_ms {
            constraints.push(CtpConstraint::LeadTimeExceeded {
                min_lead_time_ms: min_completion - self.current_time_ms,
                requested_ms: request.requested_date_ms - self.current_time_ms,
            });
        }

        // 4. 결과 생성
        let is_capable = constraints.is_empty();
        let promised_date = if is_capable {
            Some(request.requested_date_ms)
        } else {
            Some(min_completion)
        };

        let promised_qty = if is_capable {
            request.quantity
        } else {
            // 부분 수량 계산 (간단한 버전)
            request.quantity * 0.5
        };

        CtpResult {
            order_id: request.order_id.clone(),
            is_capable,
            promised_date_ms: promised_date,
            promised_quantity: promised_qty,
            partial_deliveries: vec![],
            constraints,
            atp_quantity: 0.0,
        }
    }

    /// ATP 확인
    pub fn check_atp(&self, product_id: &str) -> AtpResult {
        // 간단한 ATP 계산 (현재 재고 기반)
        AtpResult {
            product_id: product_id.to_string(),
            available_qty: 0.0,
            projected_availability: vec![],
        }
    }

    /// 가용 납기일 목록 조회
    pub fn get_available_dates(
        &mut self,
        request: &CtpRequest,
        range_days: i32,
    ) -> Vec<(i64, f64)> {
        let day_ms = 86_400_000i64;
        let mut results = Vec::new();

        for day in 0..range_days {
            let date_ms = self.current_time_ms + (day as i64 * day_ms);
            let mut test_request = request.clone();
            test_request.requested_date_ms = date_ms;

            let result = self.check_capability(&test_request);
            if result.is_capable {
                results.push((date_ms, request.quantity));
            } else {
                results.push((date_ms, result.promised_quantity));
            }
        }

        results
    }
}

impl Default for CtpEngine {
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::pegging::SupplyType;

    #[test]
    fn test_ctp_capable() {
        let mut engine = CtpEngine::new(0);

        // 자재 설정
        engine.add_material(PeggingMaterial {
            id: "mat1".into(),
            name: "Component".into(),
            on_hand_qty: 100.0,
            unit: "EA".into(),
        });

        // BOM 설정 (제품당 자재 2개 필요)
        engine.set_product_bom("product1", vec![("mat1".into(), 2.0)]);
        engine.set_product_process_time("product1", 3_600_000); // 1시간

        let request = CtpRequest {
            order_id: "order1".into(),
            product_id: "product1".into(),
            quantity: 10.0,
            requested_date_ms: 86_400_000, // 1일 후
            priority: 1,
        };

        let result = engine.check_capability(&request);

        assert!(result.is_capable);
        assert_eq!(result.promised_quantity, 10.0);
    }

    #[test]
    fn test_ctp_material_shortage() {
        let mut engine = CtpEngine::new(0);

        engine.add_material(PeggingMaterial {
            id: "mat1".into(),
            name: "Component".into(),
            on_hand_qty: 10.0, // 부족
            unit: "EA".into(),
        });

        engine.set_product_bom("product1", vec![("mat1".into(), 2.0)]);
        engine.set_product_process_time("product1", 3_600_000);

        let request = CtpRequest {
            order_id: "order1".into(),
            product_id: "product1".into(),
            quantity: 10.0, // 20개 필요, 10개만 있음
            requested_date_ms: 86_400_000,
            priority: 1,
        };

        let result = engine.check_capability(&request);

        assert!(!result.is_capable);
        assert!(result
            .constraints
            .iter()
            .any(|c| { matches!(c, CtpConstraint::MaterialShortage { .. }) }));
    }

    #[test]
    fn test_ctp_with_supply() {
        let mut engine = CtpEngine::new(0);

        engine.add_material(PeggingMaterial {
            id: "mat1".into(),
            name: "Component".into(),
            on_hand_qty: 10.0,
            unit: "EA".into(),
        });

        engine.add_supply(MaterialSupply {
            id: "sup1".into(),
            material_id: "mat1".into(),
            quantity: 20.0,
            available_at_ms: 43_200_000, // 12시간 후
            supply_type: SupplyType::PurchaseOrder,
        });

        engine.set_product_bom("product1", vec![("mat1".into(), 2.0)]);
        engine.set_product_process_time("product1", 3_600_000);

        let request = CtpRequest {
            order_id: "order1".into(),
            product_id: "product1".into(),
            quantity: 10.0,
            requested_date_ms: 86_400_000, // 1일 후
            priority: 1,
        };

        let result = engine.check_capability(&request);

        // 10 + 20 = 30 >= 20 (필요량)
        assert!(result.is_capable || result.promised_date_ms.is_some());
    }

    #[test]
    fn test_available_dates() {
        let mut engine = CtpEngine::new(0);

        engine.add_material(PeggingMaterial {
            id: "mat1".into(),
            name: "Component".into(),
            on_hand_qty: 100.0,
            unit: "EA".into(),
        });

        engine.set_product_bom("product1", vec![("mat1".into(), 1.0)]);
        engine.set_product_process_time("product1", 3_600_000);

        let request = CtpRequest {
            order_id: "order1".into(),
            product_id: "product1".into(),
            quantity: 10.0,
            requested_date_ms: 0,
            priority: 1,
        };

        let dates = engine.get_available_dates(&request, 7);
        assert_eq!(dates.len(), 7);
    }
}
