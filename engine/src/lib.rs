//! # U-APS Scheduling Engine
//!
//! Manufacturing production scheduling engine built on the `u-schedule`
//! scheduling framework (`u-metaheur` metaheuristics, `u-numflow` math primitives).
//!
//! ## Algorithms
//!
//! Seven scheduling algorithms accessible via [`SchedulingEngine`]:
//!
//! | Algorithm | Type | Best For |
//! |-----------|------|----------|
//! | Simple | Dispatching rule | Fast baseline, small problems |
//! | GeneticAlgorithm | Metaheuristic | Medium problems (30-200 ops) |
//! | Production | GA + extensions | Real manufacturing with setup/split/overlap |
//! | Dynamic | Time-fence | Rescheduling with frozen horizons |
//! | CpSat | Constraint Programming | Exact solutions for small problems |
//! | Hybrid | GA + SA | Large problems needing escape from local optima |
//! | Auto | Adaptive | Selects algorithm based on problem characteristics |
//!
//! ## Modules
//!
//! - [`models`] — Manufacturing domain types (Job, Operation, Resource, SetupMatrix, Calendar, etc.)
//! - [`scheduler`] — Scheduling algorithms, KPI, rescheduling, analytics, what-if, CTP
//! - [`ga`] — Genetic Algorithm with dual-vector encoding (OSV/MAV)
//! - [`cp`] — Constraint Programming solver (CP-SAT)
//! - [`validation`] — Input validation
//! - [`benchmark`] — Standard JSP/FJSSP benchmark instances (Taillard, Lawrence)
//! - [`error`] — Error types
//! - [`ffi`] — C-ABI FFI interface for C# SDK interop
//!
//! ## FFI Interface
//!
//! Five C-ABI functions for external consumption:
//!
//! | Function | Purpose |
//! |----------|---------|
//! | `uaps_schedule()` | Schedule with algorithm selection + KPI + metrics |
//! | `uaps_validate()` | Pre-flight input validation |
//! | `uaps_reschedule()` | Event-driven rescheduling |
//! | `uaps_version()` | Engine version string |
//! | `uaps_free_string()` | Free allocated string memory |

pub mod benchmark;
pub mod cp;
pub mod error;
pub mod ffi;
pub mod ga;
pub mod models;
pub mod scheduler;
pub mod validation;

pub use benchmark::*;
pub use cp::*;
pub use error::*;
pub use ga::*;
pub use models::*;
pub use scheduler::*;
pub use validation::*;
