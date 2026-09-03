//! Outsourcing - External vendor operations management
//!
//! Handles outsourced operations with lead times and vendor capacity constraints.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Outsourcing provider (vendor) information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutsourcingProvider {
    /// Provider ID
    pub id: String,
    /// Provider name
    pub name: String,
    /// Operation types this provider can handle
    pub supported_operations: Vec<String>,
    /// Default lead time in milliseconds
    pub default_lead_time_ms: i64,
    /// Daily capacity (number of operations per day, None = unlimited)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_capacity: Option<u32>,
    /// Whether provider is active
    #[serde(default = "default_true")]
    pub is_active: bool,
    /// Cost per operation (for optimization)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_per_operation: Option<f64>,
    /// Quality rating (0.0 - 1.0)
    #[serde(default = "default_quality")]
    pub quality_rating: f64,
    /// Working days (0=Sun, 1=Mon, ..., 6=Sat)
    #[serde(default = "default_working_days")]
    pub working_days: Vec<u8>,
}

fn default_true() -> bool {
    true
}
fn default_quality() -> f64 {
    1.0
}
fn default_working_days() -> Vec<u8> {
    vec![1, 2, 3, 4, 5]
} // Mon-Fri

impl OutsourcingProvider {
    /// Create a new outsourcing provider
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            supported_operations: Vec::new(),
            default_lead_time_ms: 7 * 24 * 3_600_000, // 7 days default
            daily_capacity: None,
            is_active: true,
            cost_per_operation: None,
            quality_rating: 1.0,
            working_days: default_working_days(),
        }
    }

    /// Set supported operation types
    pub fn with_operations(mut self, operations: Vec<String>) -> Self {
        self.supported_operations = operations;
        self
    }

    /// Add a supported operation type
    pub fn with_operation(mut self, operation: impl Into<String>) -> Self {
        self.supported_operations.push(operation.into());
        self
    }

    /// Set lead time in days
    pub fn with_lead_time_days(mut self, days: i64) -> Self {
        self.default_lead_time_ms = days * 24 * 3_600_000;
        self
    }

    /// Set lead time in milliseconds
    pub fn with_lead_time_ms(mut self, ms: i64) -> Self {
        self.default_lead_time_ms = ms;
        self
    }

    /// Set daily capacity
    pub fn with_daily_capacity(mut self, capacity: u32) -> Self {
        self.daily_capacity = Some(capacity);
        self
    }

    /// Set cost per operation
    pub fn with_cost(mut self, cost: f64) -> Self {
        self.cost_per_operation = Some(cost);
        self
    }

    /// Set quality rating
    pub fn with_quality(mut self, rating: f64) -> Self {
        self.quality_rating = rating.clamp(0.0, 1.0);
        self
    }

    /// Set working days
    pub fn with_working_days(mut self, days: Vec<u8>) -> Self {
        self.working_days = days;
        self
    }

    /// Check if provider supports an operation type
    pub fn supports_operation(&self, op_type: &str) -> bool {
        self.supported_operations.is_empty()
            || self.supported_operations.iter().any(|t| t == op_type)
    }

    /// Check if provider is working on a given day (0=Sun, 1=Mon, ..., 6=Sat)
    pub fn is_working_day(&self, day_of_week: u8) -> bool {
        self.working_days.contains(&day_of_week)
    }

    /// Calculate actual lead time considering working days only
    pub fn calculate_lead_time_ms(&self, start_ms: i64) -> i64 {
        if self.working_days.len() == 7 {
            // All days are working days
            return self.default_lead_time_ms;
        }

        let ms_per_day = 24 * 3_600_000i64;
        let mut remaining_ms = self.default_lead_time_ms;
        let mut current_ms = start_ms;

        while remaining_ms > 0 {
            let days_since_epoch = current_ms / ms_per_day;
            let day_of_week = ((days_since_epoch + 4) % 7) as u8; // Thursday = 4

            if self.is_working_day(day_of_week) {
                let day_remaining = ms_per_day - (current_ms % ms_per_day);
                if remaining_ms <= day_remaining {
                    return current_ms + remaining_ms - start_ms;
                }
                remaining_ms -= day_remaining;
                current_ms += day_remaining;
            } else {
                // Skip to next day
                current_ms += ms_per_day - (current_ms % ms_per_day);
            }
        }

        current_ms - start_ms
    }
}

/// Outsourcing configuration for an operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutsourcingConfig {
    /// Provider ID
    pub provider_id: String,
    /// Custom lead time override (ms), None = use provider default
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead_time_ms: Option<i64>,
    /// Expected delivery date (timestamp ms)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_delivery_ms: Option<i64>,
    /// Order date calculation mode
    #[serde(default)]
    pub order_date_mode: OrderDateMode,
    /// Priority for this outsourcing (affects provider allocation)
    #[serde(default)]
    pub priority: i32,
}

/// How to calculate the order date for outsourcing
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum OrderDateMode {
    /// Calculate from operation's earliest start (default)
    #[default]
    FromEarliestStart,
    /// Calculate backward from due date
    BackwardFromDueDate,
    /// Use fixed order date
    FixedDate,
}

impl OutsourcingConfig {
    /// Create a new outsourcing configuration
    pub fn new(provider_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            lead_time_ms: None,
            expected_delivery_ms: None,
            order_date_mode: OrderDateMode::default(),
            priority: 0,
        }
    }

    /// Set custom lead time in days
    pub fn with_lead_time_days(mut self, days: i64) -> Self {
        self.lead_time_ms = Some(days * 24 * 3_600_000);
        self
    }

    /// Set custom lead time in milliseconds
    pub fn with_lead_time_ms(mut self, ms: i64) -> Self {
        self.lead_time_ms = Some(ms);
        self
    }

    /// Set expected delivery date
    pub fn with_expected_delivery_ms(mut self, ms: i64) -> Self {
        self.expected_delivery_ms = Some(ms);
        self
    }

    /// Set order date calculation mode
    pub fn with_order_date_mode(mut self, mode: OrderDateMode) -> Self {
        self.order_date_mode = mode;
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

/// Provider capacity tracking for a specific day
#[derive(Debug, Clone, Default)]
pub struct DailyCapacityUsage {
    /// Operations scheduled for this day
    pub scheduled_count: u32,
    /// Operation IDs scheduled
    pub operation_ids: Vec<String>,
}

/// Outsourcing manager - coordinates providers and tracks capacity
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutsourcingManager {
    /// Registered providers
    providers: HashMap<String, OutsourcingProvider>,
    /// Capacity usage per provider per day (provider_id -> day_index -> usage)
    #[serde(skip)]
    capacity_usage: HashMap<String, HashMap<i64, DailyCapacityUsage>>,
}

impl OutsourcingManager {
    /// Create a new outsourcing manager
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            capacity_usage: HashMap::new(),
        }
    }

    /// Add a provider
    pub fn add_provider(&mut self, provider: OutsourcingProvider) {
        self.providers.insert(provider.id.clone(), provider);
    }

    /// Builder pattern: add provider
    pub fn with_provider(mut self, provider: OutsourcingProvider) -> Self {
        self.add_provider(provider);
        self
    }

    /// Get a provider by ID
    pub fn get_provider(&self, provider_id: &str) -> Option<&OutsourcingProvider> {
        self.providers.get(provider_id)
    }

    /// Get all providers
    pub fn all_providers(&self) -> Vec<&OutsourcingProvider> {
        self.providers.values().collect()
    }

    /// Get active providers
    pub fn active_providers(&self) -> Vec<&OutsourcingProvider> {
        self.providers.values().filter(|p| p.is_active).collect()
    }

    /// Find providers that support an operation type
    pub fn providers_for_operation(&self, op_type: &str) -> Vec<&OutsourcingProvider> {
        self.providers
            .values()
            .filter(|p| p.is_active && p.supports_operation(op_type))
            .collect()
    }

    /// Find the best provider for an operation type (lowest cost, highest quality)
    pub fn best_provider_for_operation(&self, op_type: &str) -> Option<&OutsourcingProvider> {
        self.providers_for_operation(op_type)
            .into_iter()
            .max_by(|a, b| {
                // Prioritize by: quality (higher is better), then cost (lower is better)
                let quality_cmp = a
                    .quality_rating
                    .partial_cmp(&b.quality_rating)
                    .unwrap_or(std::cmp::Ordering::Equal);
                if quality_cmp != std::cmp::Ordering::Equal {
                    return quality_cmp;
                }
                // Lower cost is better, so reverse the comparison
                let cost_a = a.cost_per_operation.unwrap_or(f64::MAX);
                let cost_b = b.cost_per_operation.unwrap_or(f64::MAX);
                cost_b
                    .partial_cmp(&cost_a)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// Calculate lead time for an outsourced operation
    pub fn calculate_lead_time(&self, config: &OutsourcingConfig, _start_ms: i64) -> Option<i64> {
        let provider = self.get_provider(&config.provider_id)?;

        // Use custom lead time if specified, otherwise use provider default
        let base_lead_time = config.lead_time_ms.unwrap_or(provider.default_lead_time_ms);

        // For simplicity, return the base lead time
        // A more sophisticated implementation could consider working days
        Some(base_lead_time)
    }

    /// Calculate order date (when to place the outsourcing order)
    pub fn calculate_order_date(
        &self,
        config: &OutsourcingConfig,
        operation_earliest_start: i64,
        job_due_date: Option<i64>,
    ) -> Option<i64> {
        let lead_time = self.calculate_lead_time(config, operation_earliest_start)?;

        match config.order_date_mode {
            OrderDateMode::FromEarliestStart => {
                // Order immediately, delivery = start + lead_time
                Some(operation_earliest_start)
            }
            OrderDateMode::BackwardFromDueDate => {
                // Calculate backward from due date
                if let Some(due_date) = job_due_date {
                    Some((due_date - lead_time).max(0))
                } else {
                    Some(operation_earliest_start)
                }
            }
            OrderDateMode::FixedDate => {
                // Use expected delivery date if set
                config.expected_delivery_ms
            }
        }
    }

    /// Check if a provider has capacity on a specific day
    pub fn has_capacity(&self, provider_id: &str, day_index: i64) -> bool {
        if let Some(provider) = self.get_provider(provider_id) {
            if let Some(daily_cap) = provider.daily_capacity {
                let usage = self
                    .capacity_usage
                    .get(provider_id)
                    .and_then(|m| m.get(&day_index))
                    .map(|u| u.scheduled_count)
                    .unwrap_or(0);
                return usage < daily_cap;
            }
        }
        true // No capacity limit or provider not found
    }

    /// Reserve capacity for an operation
    pub fn reserve_capacity(
        &mut self,
        provider_id: &str,
        day_index: i64,
        operation_id: &str,
    ) -> bool {
        if !self.has_capacity(provider_id, day_index) {
            return false;
        }

        let provider_usage = self
            .capacity_usage
            .entry(provider_id.to_string())
            .or_default();

        let day_usage = provider_usage.entry(day_index).or_default();

        day_usage.scheduled_count += 1;
        day_usage.operation_ids.push(operation_id.to_string());

        true
    }

    /// Release capacity for an operation
    pub fn release_capacity(&mut self, provider_id: &str, day_index: i64, operation_id: &str) {
        if let Some(provider_usage) = self.capacity_usage.get_mut(provider_id) {
            if let Some(day_usage) = provider_usage.get_mut(&day_index) {
                if let Some(pos) = day_usage
                    .operation_ids
                    .iter()
                    .position(|id| id == operation_id)
                {
                    day_usage.operation_ids.remove(pos);
                    day_usage.scheduled_count = day_usage.scheduled_count.saturating_sub(1);
                }
            }
        }
    }

    /// Clear all capacity reservations
    pub fn clear_capacity(&mut self) {
        self.capacity_usage.clear();
    }

    /// Get capacity usage summary for a provider
    pub fn get_capacity_summary(&self, provider_id: &str) -> HashMap<i64, u32> {
        self.capacity_usage
            .get(provider_id)
            .map(|m| {
                m.iter()
                    .map(|(day, usage)| (*day, usage.scheduled_count))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Outsourcing scheduling result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutsourcingScheduleResult {
    /// Operation ID
    pub operation_id: String,
    /// Provider ID
    pub provider_id: String,
    /// Order date (when to place the order)
    pub order_date_ms: i64,
    /// Expected delivery date
    pub delivery_date_ms: i64,
    /// Lead time used
    pub lead_time_ms: i64,
    /// Cost (if available)
    pub cost: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = OutsourcingProvider::new("VENDOR-001", "External Machining Co.")
            .with_operation("machining")
            .with_operation("grinding")
            .with_lead_time_days(5)
            .with_daily_capacity(10)
            .with_cost(100.0)
            .with_quality(0.95);

        assert_eq!(provider.id, "VENDOR-001");
        assert_eq!(provider.supported_operations.len(), 2);
        assert_eq!(provider.default_lead_time_ms, 5 * 24 * 3_600_000);
        assert_eq!(provider.daily_capacity, Some(10));
        assert_eq!(provider.cost_per_operation, Some(100.0));
        assert_eq!(provider.quality_rating, 0.95);
    }

    #[test]
    fn test_provider_operation_support() {
        let provider = OutsourcingProvider::new("V1", "Vendor 1")
            .with_operation("machining")
            .with_operation("welding");

        assert!(provider.supports_operation("machining"));
        assert!(provider.supports_operation("welding"));
        assert!(!provider.supports_operation("assembly"));

        // Empty operations = supports all
        let universal = OutsourcingProvider::new("V2", "Universal Vendor");
        assert!(universal.supports_operation("anything"));
    }

    #[test]
    fn test_outsourcing_config() {
        let config = OutsourcingConfig::new("VENDOR-001")
            .with_lead_time_days(10)
            .with_priority(5);

        assert_eq!(config.provider_id, "VENDOR-001");
        assert_eq!(config.lead_time_ms, Some(10 * 24 * 3_600_000));
        assert_eq!(config.priority, 5);
    }

    #[test]
    fn test_outsourcing_manager() {
        let manager = OutsourcingManager::new()
            .with_provider(
                OutsourcingProvider::new("V1", "Machining Vendor")
                    .with_operation("machining")
                    .with_lead_time_days(5)
                    .with_quality(0.9),
            )
            .with_provider(
                OutsourcingProvider::new("V2", "Premium Machining")
                    .with_operation("machining")
                    .with_lead_time_days(3)
                    .with_quality(0.95)
                    .with_cost(150.0),
            );

        assert_eq!(manager.all_providers().len(), 2);

        // Find providers for machining
        let machining_providers = manager.providers_for_operation("machining");
        assert_eq!(machining_providers.len(), 2);

        // Best provider should be the one with higher quality
        let best = manager.best_provider_for_operation("machining");
        assert!(best.is_some());
        assert_eq!(best.unwrap().id, "V2"); // Higher quality
    }

    #[test]
    fn test_capacity_management() {
        let mut manager = OutsourcingManager::new()
            .with_provider(OutsourcingProvider::new("V1", "Limited Vendor").with_daily_capacity(2));

        let day = 100; // Arbitrary day index

        // Should have capacity initially
        assert!(manager.has_capacity("V1", day));

        // Reserve first slot
        assert!(manager.reserve_capacity("V1", day, "OP-001"));
        assert!(manager.has_capacity("V1", day));

        // Reserve second slot
        assert!(manager.reserve_capacity("V1", day, "OP-002"));
        assert!(!manager.has_capacity("V1", day)); // At capacity

        // Cannot reserve more
        assert!(!manager.reserve_capacity("V1", day, "OP-003"));

        // Release one slot
        manager.release_capacity("V1", day, "OP-001");
        assert!(manager.has_capacity("V1", day));

        // Can reserve again
        assert!(manager.reserve_capacity("V1", day, "OP-003"));
    }

    #[test]
    fn test_lead_time_calculation() {
        let manager = OutsourcingManager::new()
            .with_provider(OutsourcingProvider::new("V1", "Vendor").with_lead_time_days(7));

        let config = OutsourcingConfig::new("V1");
        let lead_time = manager.calculate_lead_time(&config, 0);
        assert_eq!(lead_time, Some(7 * 24 * 3_600_000));

        // Custom lead time should override
        let custom_config = OutsourcingConfig::new("V1").with_lead_time_days(3);
        let custom_lead_time = manager.calculate_lead_time(&custom_config, 0);
        assert_eq!(custom_lead_time, Some(3 * 24 * 3_600_000));
    }

    #[test]
    fn test_order_date_calculation() {
        let manager = OutsourcingManager::new()
            .with_provider(OutsourcingProvider::new("V1", "Vendor").with_lead_time_days(5));

        let earliest_start = 10 * 24 * 3_600_000i64; // Day 10
        let due_date = 20 * 24 * 3_600_000i64; // Day 20

        // FromEarliestStart mode
        let config1 = OutsourcingConfig::new("V1");
        let order_date1 = manager.calculate_order_date(&config1, earliest_start, Some(due_date));
        assert_eq!(order_date1, Some(earliest_start));

        // BackwardFromDueDate mode
        let config2 =
            OutsourcingConfig::new("V1").with_order_date_mode(OrderDateMode::BackwardFromDueDate);
        let order_date2 = manager.calculate_order_date(&config2, earliest_start, Some(due_date));
        assert_eq!(order_date2, Some(due_date - 5 * 24 * 3_600_000)); // Day 15
    }

    #[test]
    fn test_working_days() {
        // Only Mon-Fri
        let provider =
            OutsourcingProvider::new("V1", "Weekday Vendor").with_working_days(vec![1, 2, 3, 4, 5]);

        assert!(provider.is_working_day(1)); // Monday
        assert!(provider.is_working_day(5)); // Friday
        assert!(!provider.is_working_day(0)); // Sunday
        assert!(!provider.is_working_day(6)); // Saturday
    }
}
