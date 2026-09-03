//! Material - 자재 제약 모델
//!
//! 공정에 필요한 자재의 가용성 및 소요량 관리

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 자재 요구사항 - 공정별 필요 자재
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MaterialRequirement {
    /// 자재 ID
    pub material_id: String,
    /// 필요 수량
    pub quantity: f64,
    /// 자재 가용일 (이 날짜 이후 사용 가능)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub availability_date: Option<DateTime<Utc>>,
}

impl MaterialRequirement {
    pub fn new(material_id: impl Into<String>, quantity: f64) -> Self {
        Self {
            material_id: material_id.into(),
            quantity,
            availability_date: None,
        }
    }

    /// Builder: 가용일 설정
    pub fn with_availability(mut self, date: DateTime<Utc>) -> Self {
        self.availability_date = Some(date);
        self
    }

    /// 지정 시점에 자재가 가용한지 확인
    pub fn is_available_at(&self, time: DateTime<Utc>) -> bool {
        match self.availability_date {
            Some(avail) => time >= avail,
            None => true, // 가용일 미지정 = 항상 가용
        }
    }
}

/// 자재 마스터 - 자재 기본 정보
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Material {
    /// 자재 ID
    pub id: String,
    /// 자재명
    pub name: String,
    /// 단위
    #[serde(default)]
    pub unit: String,
    /// 현재 재고량
    #[serde(default)]
    pub stock_quantity: f64,
    /// 안전재고 수량
    #[serde(default)]
    pub safety_stock: f64,
    /// 리드타임 (ms)
    #[serde(default)]
    pub lead_time_ms: i64,
}

impl Material {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            unit: "EA".to_string(),
            stock_quantity: 0.0,
            safety_stock: 0.0,
            lead_time_ms: 0,
        }
    }

    /// Builder: 재고량 설정
    pub fn with_stock(mut self, quantity: f64) -> Self {
        self.stock_quantity = quantity;
        self
    }

    /// Builder: 안전재고 설정
    pub fn with_safety_stock(mut self, quantity: f64) -> Self {
        self.safety_stock = quantity;
        self
    }

    /// Builder: 리드타임 설정
    pub fn with_lead_time_ms(mut self, lead_time_ms: i64) -> Self {
        self.lead_time_ms = lead_time_ms;
        self
    }

    /// 가용 재고량 (현재 재고 - 안전재고)
    pub fn available_stock(&self) -> f64 {
        (self.stock_quantity - self.safety_stock).max(0.0)
    }

    /// 안전재고 미달 여부
    pub fn is_below_safety_stock(&self) -> bool {
        self.stock_quantity < self.safety_stock
    }
}

/// BOM 항목 - 제품별 자재 소요량
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BomEntry {
    /// 자재 ID
    pub material_id: String,
    /// 단위당 소요량
    pub quantity_per_unit: f64,
    /// 손실률 (0.0 ~ 1.0, 예: 0.05 = 5% 손실)
    #[serde(default)]
    pub scrap_rate: f64,
}

impl BomEntry {
    pub fn new(material_id: impl Into<String>, quantity_per_unit: f64) -> Self {
        Self {
            material_id: material_id.into(),
            quantity_per_unit,
            scrap_rate: 0.0,
        }
    }

    /// Builder: 손실률 설정
    pub fn with_scrap_rate(mut self, rate: f64) -> Self {
        self.scrap_rate = rate.clamp(0.0, 1.0);
        self
    }

    /// 총 소요량 계산 (손실률 포함)
    pub fn calculate_requirement(&self, production_quantity: f64) -> f64 {
        self.quantity_per_unit * production_quantity * (1.0 + self.scrap_rate)
    }
}

/// BOM (Bill of Materials) - 제품별 자재 명세서
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BillOfMaterials {
    /// 제품 ID → BOM 항목 목록
    entries: HashMap<String, Vec<BomEntry>>,
}

impl BillOfMaterials {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// 제품의 BOM 항목 추가
    pub fn add_entry(&mut self, product_id: impl Into<String>, entry: BomEntry) {
        self.entries
            .entry(product_id.into())
            .or_default()
            .push(entry);
    }

    /// 제품의 BOM 항목 조회
    pub fn get_entries(&self, product_id: &str) -> Option<&Vec<BomEntry>> {
        self.entries.get(product_id)
    }

    /// 제품의 자재 소요량 계산
    pub fn calculate_requirements(
        &self,
        product_id: &str,
        quantity: f64,
    ) -> Vec<MaterialRequirement> {
        self.entries
            .get(product_id)
            .map(|entries| {
                entries
                    .iter()
                    .map(|e| {
                        MaterialRequirement::new(&e.material_id, e.calculate_requirement(quantity))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// BOM에 등록된 제품 수
    pub fn product_count(&self) -> usize {
        self.entries.len()
    }

    /// 특정 제품의 BOM 존재 여부
    pub fn has_product(&self, product_id: &str) -> bool {
        self.entries.contains_key(product_id)
    }
}

/// 자재 가용성 검사 결과
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MaterialAvailabilityResult {
    /// 검사 통과 여부
    pub is_available: bool,
    /// 부족 자재 목록
    pub shortages: Vec<MaterialShortage>,
    /// 가장 빠른 가용 시점 (부족 시)
    pub earliest_available: Option<DateTime<Utc>>,
}

/// 자재 부족 정보
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MaterialShortage {
    /// 자재 ID
    pub material_id: String,
    /// 필요 수량
    pub required: f64,
    /// 가용 수량
    pub available: f64,
    /// 부족 수량
    pub shortage: f64,
    /// 자재 가용 예정일
    pub available_date: Option<DateTime<Utc>>,
}

/// 자재 관리자 - 자재 가용성 검사 및 관리
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MaterialManager {
    /// 자재 마스터
    materials: HashMap<String, Material>,
    /// BOM
    bom: BillOfMaterials,
}

impl MaterialManager {
    pub fn new() -> Self {
        Self {
            materials: HashMap::new(),
            bom: BillOfMaterials::new(),
        }
    }

    /// 자재 등록
    pub fn add_material(&mut self, material: Material) {
        self.materials.insert(material.id.clone(), material);
    }

    /// 자재 조회
    pub fn get_material(&self, material_id: &str) -> Option<&Material> {
        self.materials.get(material_id)
    }

    /// BOM 설정
    pub fn set_bom(&mut self, bom: BillOfMaterials) {
        self.bom = bom;
    }

    /// BOM 참조
    pub fn bom(&self) -> &BillOfMaterials {
        &self.bom
    }

    /// 자재 요구사항 가용성 검사
    pub fn check_availability(
        &self,
        requirements: &[MaterialRequirement],
        at_time: DateTime<Utc>,
    ) -> MaterialAvailabilityResult {
        let mut shortages = Vec::new();
        let mut earliest_available: Option<DateTime<Utc>> = None;

        for req in requirements {
            // 가용일 검사
            if !req.is_available_at(at_time) {
                if let Some(avail_date) = req.availability_date {
                    shortages.push(MaterialShortage {
                        material_id: req.material_id.clone(),
                        required: req.quantity,
                        available: 0.0,
                        shortage: req.quantity,
                        available_date: Some(avail_date),
                    });
                    earliest_available = Some(match earliest_available {
                        Some(current) => current.max(avail_date),
                        None => avail_date,
                    });
                }
                continue;
            }

            // 재고량 검사
            if let Some(material) = self.materials.get(&req.material_id) {
                let available = material.available_stock();
                if available < req.quantity {
                    shortages.push(MaterialShortage {
                        material_id: req.material_id.clone(),
                        required: req.quantity,
                        available,
                        shortage: req.quantity - available,
                        available_date: None,
                    });
                }
            }
        }

        MaterialAvailabilityResult {
            is_available: shortages.is_empty(),
            shortages,
            earliest_available,
        }
    }

    /// 안전재고 미달 자재 목록
    pub fn get_low_stock_materials(&self) -> Vec<&Material> {
        self.materials
            .values()
            .filter(|m| m.is_below_safety_stock())
            .collect()
    }

    /// BOM 기반 자재 요구사항 생성
    ///
    /// 제품(product_id)과 생산 수량(quantity)을 기반으로
    /// BOM을 조회하여 필요한 자재 요구사항 목록을 생성합니다.
    pub fn generate_requirements_from_bom(
        &self,
        product_id: &str,
        quantity: f64,
    ) -> Vec<MaterialRequirement> {
        self.bom
            .calculate_requirements(product_id, quantity)
            .into_iter()
            .map(|mut req| {
                // 자재의 리드타임을 기반으로 가용일 계산
                if let Some(material) = self.materials.get(&req.material_id) {
                    if material.lead_time_ms > 0 {
                        req.availability_date = Some(
                            Utc::now() + chrono::Duration::milliseconds(material.lead_time_ms),
                        );
                    }
                }
                req
            })
            .collect()
    }

    /// BOM 기반 자재 가용성 검사 (제품/수량 기반)
    ///
    /// 제품 ID와 생산 수량만으로 BOM을 조회하고 자재 가용성을 검사합니다.
    pub fn check_availability_by_product(
        &self,
        product_id: &str,
        quantity: f64,
        at_time: DateTime<Utc>,
    ) -> MaterialAvailabilityResult {
        let requirements = self.generate_requirements_from_bom(product_id, quantity);
        self.check_availability(&requirements, at_time)
    }

    /// 다중 제품 자재 가용성 검사
    ///
    /// 여러 제품의 자재 요구사항을 집계하여 한번에 가용성을 검사합니다.
    pub fn check_availability_for_jobs(
        &self,
        products: &[(String, f64)], // (product_id, quantity) pairs
        at_time: DateTime<Utc>,
    ) -> MaterialAvailabilityResult {
        let mut all_requirements: Vec<MaterialRequirement> = Vec::new();

        for (product_id, quantity) in products {
            let reqs = self.generate_requirements_from_bom(product_id, *quantity);
            all_requirements.extend(reqs);
        }

        // 동일 자재 요구량 집계
        let aggregated = Self::aggregate_requirements(&all_requirements);
        self.check_availability(&aggregated, at_time)
    }

    /// 동일 자재 요구량 집계
    fn aggregate_requirements(requirements: &[MaterialRequirement]) -> Vec<MaterialRequirement> {
        let mut aggregated: HashMap<String, MaterialRequirement> = HashMap::new();

        for req in requirements {
            aggregated
                .entry(req.material_id.clone())
                .and_modify(|existing| {
                    existing.quantity += req.quantity;
                    // 더 늦은 가용일 사용
                    if let Some(new_date) = req.availability_date {
                        existing.availability_date = match existing.availability_date {
                            Some(old_date) => Some(old_date.max(new_date)),
                            None => Some(new_date),
                        };
                    }
                })
                .or_insert_with(|| req.clone());
        }

        aggregated.into_values().collect()
    }

    /// BOM 항목 추가 (편의 메서드)
    pub fn add_bom_entry(&mut self, product_id: impl Into<String>, entry: BomEntry) {
        self.bom.add_entry(product_id, entry);
    }

    /// 안전재고 침범 검사 결과
    pub fn check_safety_stock_violations(
        &self,
        requirements: &[MaterialRequirement],
    ) -> Vec<SafetyStockWarning> {
        let mut warnings = Vec::new();

        // 자재별 소요량 집계
        let mut consumption_by_material: HashMap<String, f64> = HashMap::new();
        for req in requirements {
            *consumption_by_material
                .entry(req.material_id.clone())
                .or_default() += req.quantity;
        }

        // 각 자재별 안전재고 검사
        for (material_id, consumption) in consumption_by_material {
            if let Some(material) = self.materials.get(&material_id) {
                let after_consumption = material.stock_quantity - consumption;
                if after_consumption < material.safety_stock {
                    warnings.push(SafetyStockWarning {
                        material_id: material_id.clone(),
                        current_stock: material.stock_quantity,
                        safety_stock: material.safety_stock,
                        consumption,
                        after_consumption,
                    });
                }
            }
        }

        warnings
    }

    /// BOM 기반 안전재고 침범 검사
    pub fn check_safety_stock_by_product(
        &self,
        product_id: &str,
        quantity: f64,
    ) -> Vec<SafetyStockWarning> {
        let requirements = self.generate_requirements_from_bom(product_id, quantity);
        self.check_safety_stock_violations(&requirements)
    }
}

/// 안전재고 침범 경고
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SafetyStockWarning {
    /// 자재 ID
    pub material_id: String,
    /// 현재 재고
    pub current_stock: f64,
    /// 안전재고 수준
    pub safety_stock: f64,
    /// 소요량
    pub consumption: f64,
    /// 소비 후 재고
    pub after_consumption: f64,
}

impl SafetyStockWarning {
    /// 침범 정도 (음수 = 안전재고 아래로 내려감)
    pub fn violation_amount(&self) -> f64 {
        self.after_consumption - self.safety_stock
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_material_requirement_availability() {
        let now = Utc::now();
        let future = now + chrono::Duration::hours(24);

        // 가용일 미지정 = 항상 가용
        let req1 = MaterialRequirement::new("MAT-001", 10.0);
        assert!(req1.is_available_at(now));

        // 가용일 이전 = 불가
        let req2 = MaterialRequirement::new("MAT-002", 5.0).with_availability(future);
        assert!(!req2.is_available_at(now));
        assert!(req2.is_available_at(future));
    }

    #[test]
    fn test_material_stock() {
        let material = Material::new("MAT-001", "Steel Plate")
            .with_stock(100.0)
            .with_safety_stock(20.0);

        assert_eq!(material.available_stock(), 80.0);
        assert!(!material.is_below_safety_stock());

        let low_stock = Material::new("MAT-002", "Copper Wire")
            .with_stock(10.0)
            .with_safety_stock(20.0);

        assert_eq!(low_stock.available_stock(), 0.0);
        assert!(low_stock.is_below_safety_stock());
    }

    #[test]
    fn test_bom_calculation() {
        let mut bom = BillOfMaterials::new();
        bom.add_entry("PROD-A", BomEntry::new("MAT-001", 2.0));
        bom.add_entry("PROD-A", BomEntry::new("MAT-002", 0.5).with_scrap_rate(0.1));

        let reqs = bom.calculate_requirements("PROD-A", 10.0);
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0].quantity, 20.0); // 2.0 * 10
        assert_eq!(reqs[1].quantity, 5.5); // 0.5 * 10 * 1.1
    }

    #[test]
    fn test_material_manager_availability() {
        let mut manager = MaterialManager::new();
        manager.add_material(Material::new("MAT-001", "Steel").with_stock(50.0));
        manager.add_material(Material::new("MAT-002", "Copper").with_stock(10.0));

        let now = Utc::now();
        let future = now + chrono::Duration::hours(24);

        let requirements = vec![
            MaterialRequirement::new("MAT-001", 30.0), // 가용 (50 >= 30)
            MaterialRequirement::new("MAT-002", 20.0), // 부족 (10 < 20)
            MaterialRequirement::new("MAT-003", 5.0).with_availability(future), // 가용일 미도래
        ];

        let result = manager.check_availability(&requirements, now);
        assert!(!result.is_available);
        assert_eq!(result.shortages.len(), 2);

        // MAT-002 부족
        let shortage1 = result
            .shortages
            .iter()
            .find(|s| s.material_id == "MAT-002")
            .unwrap();
        assert_eq!(shortage1.required, 20.0);
        assert_eq!(shortage1.available, 10.0);
        assert_eq!(shortage1.shortage, 10.0);

        // MAT-003 가용일 미도래
        let shortage2 = result
            .shortages
            .iter()
            .find(|s| s.material_id == "MAT-003")
            .unwrap();
        assert_eq!(shortage2.available_date, Some(future));
    }

    #[test]
    fn test_low_stock_detection() {
        let mut manager = MaterialManager::new();
        manager.add_material(
            Material::new("MAT-001", "Normal")
                .with_stock(100.0)
                .with_safety_stock(50.0),
        );
        manager.add_material(
            Material::new("MAT-002", "Low")
                .with_stock(30.0)
                .with_safety_stock(50.0),
        );
        manager.add_material(
            Material::new("MAT-003", "Critical")
                .with_stock(10.0)
                .with_safety_stock(50.0),
        );

        let low_stock = manager.get_low_stock_materials();
        assert_eq!(low_stock.len(), 2);
    }

    #[test]
    fn test_bom_based_requirements_generation() {
        let mut manager = MaterialManager::new();

        // 자재 마스터 등록
        manager.add_material(Material::new("STEEL", "Steel Plate").with_stock(200.0));
        manager.add_material(
            Material::new("BOLT", "M10 Bolt")
                .with_stock(1000.0)
                .with_lead_time_ms(86_400_000), // 1일 리드타임
        );

        // BOM 등록: PRODUCT-A = STEEL 2개 + BOLT 4개
        manager.add_bom_entry("PRODUCT-A", BomEntry::new("STEEL", 2.0));
        manager.add_bom_entry("PRODUCT-A", BomEntry::new("BOLT", 4.0));

        // 10개 생산 시 자재 요구사항 생성
        let requirements = manager.generate_requirements_from_bom("PRODUCT-A", 10.0);
        assert_eq!(requirements.len(), 2);

        let steel_req = requirements
            .iter()
            .find(|r| r.material_id == "STEEL")
            .unwrap();
        assert_eq!(steel_req.quantity, 20.0); // 2.0 * 10
        assert!(steel_req.availability_date.is_none()); // 리드타임 없음

        let bolt_req = requirements
            .iter()
            .find(|r| r.material_id == "BOLT")
            .unwrap();
        assert_eq!(bolt_req.quantity, 40.0); // 4.0 * 10
        assert!(bolt_req.availability_date.is_some()); // 리드타임 있음
    }

    #[test]
    fn test_bom_based_availability_check() {
        let mut manager = MaterialManager::new();

        // 자재 마스터 등록
        manager.add_material(Material::new("STEEL", "Steel Plate").with_stock(50.0));
        manager.add_material(Material::new("BOLT", "M10 Bolt").with_stock(100.0));

        // BOM 등록: PRODUCT-A = STEEL 2개 + BOLT 4개
        manager.add_bom_entry("PRODUCT-A", BomEntry::new("STEEL", 2.0));
        manager.add_bom_entry("PRODUCT-A", BomEntry::new("BOLT", 4.0));

        let now = Utc::now();

        // 10개 생산 → STEEL 20개, BOLT 40개 필요 → 재고 충분
        let result = manager.check_availability_by_product("PRODUCT-A", 10.0, now);
        assert!(result.is_available);

        // 30개 생산 → STEEL 60개, BOLT 120개 필요 → STEEL, BOLT 둘 다 부족
        let result = manager.check_availability_by_product("PRODUCT-A", 30.0, now);
        assert!(!result.is_available);
        assert_eq!(result.shortages.len(), 2);
    }

    #[test]
    fn test_multi_product_material_aggregation() {
        let mut manager = MaterialManager::new();

        // 자재 마스터 등록
        manager.add_material(Material::new("STEEL", "Steel Plate").with_stock(100.0));

        // BOM 등록: 두 제품 모두 STEEL 사용
        manager.add_bom_entry("PRODUCT-A", BomEntry::new("STEEL", 2.0));
        manager.add_bom_entry("PRODUCT-B", BomEntry::new("STEEL", 3.0));

        let now = Utc::now();

        // PRODUCT-A 10개 + PRODUCT-B 10개 → STEEL 50개 필요
        let products = vec![
            ("PRODUCT-A".to_string(), 10.0),
            ("PRODUCT-B".to_string(), 10.0),
        ];
        let result = manager.check_availability_for_jobs(&products, now);
        assert!(result.is_available);

        // PRODUCT-A 30개 + PRODUCT-B 20개 → STEEL 120개 필요 → 부족
        let products = vec![
            ("PRODUCT-A".to_string(), 30.0),
            ("PRODUCT-B".to_string(), 20.0),
        ];
        let result = manager.check_availability_for_jobs(&products, now);
        assert!(!result.is_available);
        let shortage = &result.shortages[0];
        assert_eq!(shortage.material_id, "STEEL");
        assert_eq!(shortage.required, 120.0); // 30*2 + 20*3 = 120
        assert_eq!(shortage.shortage, 20.0); // 120 - 100 = 20
    }

    #[test]
    fn test_safety_stock_warning() {
        let mut manager = MaterialManager::new();

        // 자재 마스터 등록 (재고 100, 안전재고 30)
        manager.add_material(
            Material::new("STEEL", "Steel Plate")
                .with_stock(100.0)
                .with_safety_stock(30.0),
        );

        // 60개 소비 시 → 100 - 60 = 40 > 30 → 안전재고 유지
        let requirements = vec![MaterialRequirement::new("STEEL", 60.0)];
        let warnings = manager.check_safety_stock_violations(&requirements);
        assert!(warnings.is_empty());

        // 80개 소비 시 → 100 - 80 = 20 < 30 → 안전재고 침범
        let requirements = vec![MaterialRequirement::new("STEEL", 80.0)];
        let warnings = manager.check_safety_stock_violations(&requirements);
        assert_eq!(warnings.len(), 1);

        let warning = &warnings[0];
        assert_eq!(warning.material_id, "STEEL");
        assert_eq!(warning.current_stock, 100.0);
        assert_eq!(warning.safety_stock, 30.0);
        assert_eq!(warning.consumption, 80.0);
        assert_eq!(warning.after_consumption, 20.0);
        assert_eq!(warning.violation_amount(), -10.0); // 20 - 30 = -10
    }

    #[test]
    fn test_safety_stock_with_bom() {
        let mut manager = MaterialManager::new();

        // 자재 마스터 등록
        manager.add_material(
            Material::new("STEEL", "Steel Plate")
                .with_stock(100.0)
                .with_safety_stock(40.0),
        );
        manager.add_material(
            Material::new("BOLT", "M10 Bolt")
                .with_stock(200.0)
                .with_safety_stock(50.0),
        );

        // BOM: PRODUCT-A = STEEL 2개 + BOLT 4개
        manager.add_bom_entry("PRODUCT-A", BomEntry::new("STEEL", 2.0));
        manager.add_bom_entry("PRODUCT-A", BomEntry::new("BOLT", 4.0));

        // 20개 생산 시: STEEL 40개 소비 (100→60, 60>40 OK), BOLT 80개 소비 (200→120, 120>50 OK)
        let warnings = manager.check_safety_stock_by_product("PRODUCT-A", 20.0);
        assert!(warnings.is_empty());

        // 35개 생산 시: STEEL 70개 소비 (100→30, 30<40 NG), BOLT 140개 소비 (200→60, 60>50 OK)
        let warnings = manager.check_safety_stock_by_product("PRODUCT-A", 35.0);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].material_id, "STEEL");
    }
}
