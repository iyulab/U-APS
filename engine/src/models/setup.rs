//! SetupMatrix - 순서 의존 준비 시간
//!
//! 이전 작업(제품)에 따라 다음 작업의 준비 시간이 달라지는 경우를 처리

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 준비 시간 항목 (직렬화용)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SetupEntry {
    pub from_product: String,
    pub to_product: String,
    pub setup_ms: i64,
}

/// 설비별 준비 시간 행렬
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SetupMatrix {
    /// 설비 ID
    pub resource_id: String,
    /// 기본 준비 시간 (행렬에 없는 경우)
    pub default_setup_ms: i64,
    /// 준비 시간 행렬 (직렬화용)
    #[serde(with = "entries_serde")]
    entries: HashMap<(String, String), i64>,
}

mod entries_serde {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(
        entries: &HashMap<(String, String), i64>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let vec: Vec<SetupEntry> = entries
            .iter()
            .map(|((from, to), ms)| SetupEntry {
                from_product: from.clone(),
                to_product: to.clone(),
                setup_ms: *ms,
            })
            .collect();
        vec.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<HashMap<(String, String), i64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let vec = Vec::<SetupEntry>::deserialize(deserializer)?;
        Ok(vec
            .into_iter()
            .map(|e| ((e.from_product, e.to_product), e.setup_ms))
            .collect())
    }
}

impl SetupMatrix {
    pub fn new(resource_id: impl Into<String>) -> Self {
        Self {
            resource_id: resource_id.into(),
            default_setup_ms: 0,
            entries: HashMap::new(),
        }
    }

    /// Builder: 기본 준비 시간 설정
    pub fn with_default_setup(mut self, setup_ms: i64) -> Self {
        self.default_setup_ms = setup_ms;
        self
    }

    /// Builder: 준비 시간 항목 추가
    pub fn with_setup(
        mut self,
        from_product: impl Into<String>,
        to_product: impl Into<String>,
        setup_ms: i64,
    ) -> Self {
        self.entries
            .insert((from_product.into(), to_product.into()), setup_ms);
        self
    }

    /// 준비 시간 조회
    ///
    /// from_product: 이전에 생산한 제품 (None이면 첫 작업)
    /// to_product: 다음에 생산할 제품
    pub fn get_setup_time(&self, from_product: Option<&str>, to_product: &str) -> i64 {
        match from_product {
            None => self.default_setup_ms,
            Some(from) => {
                // 동일 제품이면 준비 시간 없음 (또는 최소)
                if from == to_product {
                    return 0;
                }

                self.entries
                    .get(&(from.to_string(), to_product.to_string()))
                    .copied()
                    .unwrap_or(self.default_setup_ms)
            }
        }
    }

    /// 여러 항목을 한번에 추가 (from, to, setup_ms 튜플 배열)
    pub fn with_entries(mut self, entries: Vec<(&str, &str, i64)>) -> Self {
        for (from, to, setup_ms) in entries {
            self.entries
                .insert((from.to_string(), to.to_string()), setup_ms);
        }
        self
    }

    /// 대칭 행렬 생성 (A→B = B→A)
    pub fn with_symmetric_setup(
        mut self,
        product_a: impl Into<String>,
        product_b: impl Into<String>,
        setup_ms: i64,
    ) -> Self {
        let a = product_a.into();
        let b = product_b.into();
        self.entries.insert((a.clone(), b.clone()), setup_ms);
        self.entries.insert((b, a), setup_ms);
        self
    }
}

/// 여러 설비의 Setup Matrix를 관리하는 컬렉션
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SetupMatrixCollection {
    matrices: HashMap<String, SetupMatrix>,
}

impl SetupMatrixCollection {
    pub fn new() -> Self {
        Self {
            matrices: HashMap::new(),
        }
    }

    /// Setup Matrix 추가
    pub fn add_matrix(&mut self, matrix: SetupMatrix) {
        self.matrices.insert(matrix.resource_id.clone(), matrix);
    }

    /// Builder: Matrix 추가
    pub fn with_matrix(mut self, matrix: SetupMatrix) -> Self {
        self.add_matrix(matrix);
        self
    }

    /// 특정 설비의 준비 시간 조회
    pub fn get_setup_time(
        &self,
        resource_id: &str,
        from_product: Option<&str>,
        to_product: &str,
    ) -> i64 {
        self.matrices
            .get(resource_id)
            .map(|m| m.get_setup_time(from_product, to_product))
            .unwrap_or(0)
    }

    /// 설비에 Matrix가 있는지 확인
    pub fn has_matrix(&self, resource_id: &str) -> bool {
        self.matrices.contains_key(resource_id)
    }

    /// 등록된 Setup Matrix가 하나도 없는지 확인
    pub fn is_empty(&self) -> bool {
        self.matrices.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_matrix_basic() {
        let matrix = SetupMatrix::new("EQP-001")
            .with_default_setup(30000) // 30초 기본
            .with_setup("ProductA", "ProductB", 60000) // A→B: 60초
            .with_setup("ProductB", "ProductA", 45000); // B→A: 45초

        // 기본 준비 시간
        assert_eq!(matrix.get_setup_time(None, "ProductA"), 30000);

        // 동일 제품: 0
        assert_eq!(matrix.get_setup_time(Some("ProductA"), "ProductA"), 0);

        // 명시된 전환
        assert_eq!(matrix.get_setup_time(Some("ProductA"), "ProductB"), 60000);
        assert_eq!(matrix.get_setup_time(Some("ProductB"), "ProductA"), 45000);

        // 명시되지 않은 전환: 기본값
        assert_eq!(matrix.get_setup_time(Some("ProductA"), "ProductC"), 30000);
    }

    #[test]
    fn test_setup_matrix_symmetric() {
        let matrix = SetupMatrix::new("EQP-001")
            .with_default_setup(10000)
            .with_symmetric_setup("Red", "Blue", 20000);

        assert_eq!(matrix.get_setup_time(Some("Red"), "Blue"), 20000);
        assert_eq!(matrix.get_setup_time(Some("Blue"), "Red"), 20000);
    }

    #[test]
    fn test_setup_matrix_entries() {
        let matrix = SetupMatrix::new("EQP-001")
            .with_default_setup(5000)
            .with_entries(vec![
                ("A", "B", 10000),
                ("B", "C", 15000),
                ("C", "A", 20000),
            ]);

        assert_eq!(matrix.get_setup_time(Some("A"), "B"), 10000);
        assert_eq!(matrix.get_setup_time(Some("B"), "C"), 15000);
        assert_eq!(matrix.get_setup_time(Some("C"), "A"), 20000);
    }

    #[test]
    fn test_setup_matrix_collection() {
        let matrix1 = SetupMatrix::new("EQP-001")
            .with_default_setup(10000)
            .with_setup("A", "B", 30000);

        let matrix2 = SetupMatrix::new("EQP-002")
            .with_default_setup(5000)
            .with_setup("A", "B", 15000);

        let collection = SetupMatrixCollection::new()
            .with_matrix(matrix1)
            .with_matrix(matrix2);

        // EQP-001: A→B = 30초
        assert_eq!(collection.get_setup_time("EQP-001", Some("A"), "B"), 30000);

        // EQP-002: A→B = 15초
        assert_eq!(collection.get_setup_time("EQP-002", Some("A"), "B"), 15000);

        // 없는 설비: 0
        assert_eq!(collection.get_setup_time("EQP-003", Some("A"), "B"), 0);
    }

    #[test]
    fn test_setup_matrix_serialization() {
        let matrix = SetupMatrix::new("EQP-001")
            .with_default_setup(10000)
            .with_setup("A", "B", 20000);

        let json = serde_json::to_string(&matrix).unwrap();
        let deserialized: SetupMatrix = serde_json::from_str(&json).unwrap();

        assert_eq!(matrix, deserialized);
    }

    #[test]
    fn test_collection_serialization() {
        let collection = SetupMatrixCollection::new()
            .with_matrix(SetupMatrix::new("EQP-001").with_default_setup(10000));

        let json = serde_json::to_string(&collection).unwrap();
        let deserialized: SetupMatrixCollection = serde_json::from_str(&json).unwrap();

        assert_eq!(collection, deserialized);
    }
}
