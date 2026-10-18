//! Constraint Programming Module
//!
//! Manufacturing domain-specific CP scheduling.
//! Core CP infrastructure is provided by u-metaheur.

// Re-export core CP types from u-metaheur
pub use u_metaheur::cp::{
    BoolVar, Constraint as CpConstraint, CpModel, CpSolution, CpSolver, DurationVar, IntVar,
    IntervalSolution, IntervalVar, Objective as CpObjective, SimpleCpSolver, SolverConfig,
    SolverStatus, TimeVar,
};

// Manufacturing domain-specific modules
pub mod cpsat;

pub use cpsat::*;
