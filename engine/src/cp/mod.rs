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

/// GA solution to CP initial solution converter
pub fn ga_solution_to_cp_initial(ga_schedule: &crate::scheduler::Schedule, model: &mut CpModel) {
    for assignment in &ga_schedule.assignments {
        if let Some(interval) = model.intervals.get_mut(&assignment.operation_id) {
            interval.start.fixed = Some(assignment.start_ms);
            interval.end.fixed = Some(assignment.end_ms);
        }
    }
}
