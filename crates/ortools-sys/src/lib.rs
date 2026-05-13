#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case)]

#[cfg(feature = "link")]
use std::os::raw::{c_char, c_double, c_int};

// ── Status ────────────────────────────────────────────────────────────────────

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrtoolsStatus {
    Unknown = 0,
    Optimal = 1,
    Feasible = 2,
    Infeasible = 3,
    Unbounded = 4,
    ModelInvalid = 5,
    Error = 6,
}

impl OrtoolsStatus {
    pub fn is_success(self) -> bool {
        matches!(self, Self::Optimal | Self::Feasible)
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinCostFlowStatus {
    NotSolved = 0,
    Optimal = 1,
    Feasible = 2,
    Infeasible = 3,
    Unbalanced = 4,
    BadResult = 5,
    BadCostRange = 6,
    BadCapacityRange = 7,
    Error = 8,
}

impl MinCostFlowStatus {
    pub fn is_success(self) -> bool {
        matches!(self, Self::Optimal | Self::Feasible)
    }
}

// ── Opaque C types ────────────────────────────────────────────────────────────

#[repr(C)]
pub struct CpModelBuilder {
    _p: [u8; 0],
}
#[repr(C)]
pub struct CpSolverResponse {
    _p: [u8; 0],
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LpSolverType {
    Glop = 0,
}

#[repr(C)]
pub struct MpSolver {
    _p: [u8; 0],
}

#[repr(C)]
pub struct MinCostFlow {
    _p: [u8; 0],
}

// ── FFI declarations ──────────────────────────────────────────────────────────

#[cfg(feature = "link")]
unsafe extern "C" {
    pub fn cpmodel_new() -> *mut CpModelBuilder;
    pub fn cpmodel_free(m: *mut CpModelBuilder);
    pub fn cpmodel_new_int_var(
        m: *mut CpModelBuilder,
        lb: i64,
        ub: i64,
        name: *const c_char,
    ) -> i32;
    pub fn cpmodel_new_bool_var(m: *mut CpModelBuilder, name: *const c_char) -> i32;
    pub fn cpmodel_add_linear_le(
        m: *mut CpModelBuilder,
        idx: *const i32,
        c: *const i64,
        n: usize,
        rhs: i64,
    );
    pub fn cpmodel_add_linear_ge(
        m: *mut CpModelBuilder,
        idx: *const i32,
        c: *const i64,
        n: usize,
        rhs: i64,
    );
    pub fn cpmodel_add_linear_eq(
        m: *mut CpModelBuilder,
        idx: *const i32,
        c: *const i64,
        n: usize,
        rhs: i64,
    );
    pub fn cpmodel_add_all_different(m: *mut CpModelBuilder, idx: *const i32, n: usize);
    pub fn cpmodel_add_bool_or(m: *mut CpModelBuilder, lits: *const i32, n: usize);
    pub fn cpmodel_add_bool_and(m: *mut CpModelBuilder, lits: *const i32, n: usize);
    pub fn cpmodel_add_bool_xor(m: *mut CpModelBuilder, lits: *const i32, n: usize);
    pub fn cpmodel_add_implication(m: *mut CpModelBuilder, lhs: i32, rhs: i32);
    pub fn cpmodel_add_at_most_one(m: *mut CpModelBuilder, lits: *const i32, n: usize);
    pub fn cpmodel_add_exactly_one(m: *mut CpModelBuilder, lits: *const i32, n: usize);
    pub fn cpmodel_add_allowed_assignments(
        m: *mut CpModelBuilder,
        idx: *const i32,
        var_count: usize,
        tuples: *const i64,
        tuple_count: usize,
    );
    pub fn cpmodel_minimize(m: *mut CpModelBuilder, idx: *const i32, c: *const i64, n: usize);
    pub fn cpmodel_maximize(m: *mut CpModelBuilder, idx: *const i32, c: *const i64, n: usize);
    pub fn cpmodel_solve(m: *mut CpModelBuilder, time_limit: c_double) -> *mut CpSolverResponse;
    pub fn cpresponse_status(r: *const CpSolverResponse) -> OrtoolsStatus;
    pub fn cpresponse_objective_value(r: *const CpSolverResponse) -> i64;
    pub fn cpresponse_value(r: *const CpSolverResponse, var_index: i32) -> i64;
    pub fn cpresponse_wall_time(r: *const CpSolverResponse) -> c_double;
    pub fn cpresponse_free(r: *mut CpSolverResponse);

    pub fn cpmodel_new_interval_var(
        m: *mut CpModelBuilder,
        start: c_int,
        size: i64,
        end: c_int,
        name: *const c_char,
    ) -> c_int;
    pub fn cpmodel_new_optional_interval_var(
        m: *mut CpModelBuilder,
        start: c_int,
        size: i64,
        end: c_int,
        lit: c_int,
        name: *const c_char,
    ) -> c_int;
    pub fn cpmodel_add_circuit(
        m: *mut CpModelBuilder,
        tails: *const c_int,
        heads: *const c_int,
        lits: *const c_int,
        n: usize,
    );
    pub fn cpmodel_add_no_overlap(m: *mut CpModelBuilder, idx: *const c_int, n: usize);
    pub fn cpmodel_add_cumulative(
        m: *mut CpModelBuilder,
        intervals: *const c_int,
        demands: *const i64,
        n: usize,
        capacity: i64,
    );
    pub fn cpmodel_add_no_overlap_2d(
        m: *mut CpModelBuilder,
        x_intervals: *const c_int,
        y_intervals: *const c_int,
        n: usize,
    );

    pub fn mpsolver_new(name: *const c_char, t: LpSolverType) -> *mut MpSolver;
    pub fn mpsolver_free(s: *mut MpSolver);
    pub fn mpsolver_num_var(
        s: *mut MpSolver,
        lb: c_double,
        ub: c_double,
        name: *const c_char,
    ) -> i32;
    pub fn mpsolver_int_var(
        s: *mut MpSolver,
        lb: c_double,
        ub: c_double,
        name: *const c_char,
    ) -> i32;
    pub fn mpsolver_bool_var(s: *mut MpSolver, name: *const c_char) -> i32;
    pub fn mpsolver_add_constraint(
        s: *mut MpSolver,
        lb: c_double,
        ub: c_double,
        name: *const c_char,
    ) -> i32;
    pub fn mpsolver_set_constraint_coeff(s: *mut MpSolver, ci: c_int, vi: c_int, coeff: c_double);
    pub fn mpsolver_set_objective_coeff(s: *mut MpSolver, vi: c_int, coeff: c_double);
    pub fn mpsolver_minimize(s: *mut MpSolver);
    pub fn mpsolver_maximize(s: *mut MpSolver);
    pub fn mpsolver_solve(s: *mut MpSolver) -> OrtoolsStatus;
    pub fn mpsolver_objective_value(s: *const MpSolver) -> c_double;
    pub fn mpsolver_var_value(s: *const MpSolver, vi: c_int) -> c_double;

    pub fn mincostflow_new(reserve_num_nodes: c_int, reserve_num_arcs: c_int) -> *mut MinCostFlow;
    pub fn mincostflow_free(f: *mut MinCostFlow);
    pub fn mincostflow_add_arc(
        f: *mut MinCostFlow,
        tail: c_int,
        head: c_int,
        capacity: i64,
        unit_cost: i64,
    ) -> c_int;
    pub fn mincostflow_set_node_supply(f: *mut MinCostFlow, node: c_int, supply: i64);
    pub fn mincostflow_solve(f: *mut MinCostFlow) -> MinCostFlowStatus;
    pub fn mincostflow_solve_max_flow_with_min_cost(f: *mut MinCostFlow) -> MinCostFlowStatus;
    pub fn mincostflow_optimal_cost(f: *const MinCostFlow) -> i64;
    pub fn mincostflow_maximum_flow(f: *const MinCostFlow) -> i64;
    pub fn mincostflow_flow(f: *const MinCostFlow, arc: c_int) -> i64;
    pub fn mincostflow_num_arcs(f: *const MinCostFlow) -> c_int;
    pub fn mincostflow_tail(f: *const MinCostFlow, arc: c_int) -> c_int;
    pub fn mincostflow_head(f: *const MinCostFlow, arc: c_int) -> c_int;
    pub fn mincostflow_capacity(f: *const MinCostFlow, arc: c_int) -> i64;
    pub fn mincostflow_unit_cost(f: *const MinCostFlow, arc: c_int) -> i64;
}

// ── Safe wrappers ─────────────────────────────────────────────────────────────

#[cfg(feature = "link")]
pub mod safe {
    #[allow(clippy::wildcard_imports)]
    use super::*;
    use std::{ffi::CString, ptr::NonNull};

    pub struct CpModel {
        ptr: NonNull<CpModelBuilder>,
    }

    impl CpModel {
        pub fn new() -> Self {
            unsafe {
                Self {
                    ptr: NonNull::new(cpmodel_new()).expect("cpmodel_new returned null"),
                }
            }
        }

        pub fn new_int_var(&mut self, lb: i64, ub: i64, name: &str) -> i32 {
            let c = CString::new(name).unwrap();
            unsafe { cpmodel_new_int_var(self.ptr.as_ptr(), lb, ub, c.as_ptr()) }
        }

        pub fn new_bool_var(&mut self, name: &str) -> i32 {
            let c = CString::new(name).unwrap();
            unsafe { cpmodel_new_bool_var(self.ptr.as_ptr(), c.as_ptr()) }
        }

        pub fn add_linear_le(&mut self, vars: &[i32], coeffs: &[i64], rhs: i64) {
            assert_eq!(vars.len(), coeffs.len());
            unsafe {
                cpmodel_add_linear_le(
                    self.ptr.as_ptr(),
                    vars.as_ptr(),
                    coeffs.as_ptr(),
                    vars.len(),
                    rhs,
                );
            }
        }

        pub fn add_linear_ge(&mut self, vars: &[i32], coeffs: &[i64], rhs: i64) {
            assert_eq!(vars.len(), coeffs.len());
            unsafe {
                cpmodel_add_linear_ge(
                    self.ptr.as_ptr(),
                    vars.as_ptr(),
                    coeffs.as_ptr(),
                    vars.len(),
                    rhs,
                );
            }
        }

        pub fn add_linear_eq(&mut self, vars: &[i32], coeffs: &[i64], rhs: i64) {
            assert_eq!(vars.len(), coeffs.len());
            unsafe {
                cpmodel_add_linear_eq(
                    self.ptr.as_ptr(),
                    vars.as_ptr(),
                    coeffs.as_ptr(),
                    vars.len(),
                    rhs,
                );
            }
        }

        pub fn add_all_different(&mut self, vars: &[i32]) {
            unsafe {
                cpmodel_add_all_different(self.ptr.as_ptr(), vars.as_ptr(), vars.len());
            }
        }

        pub fn add_bool_or(&mut self, lits: &[i32]) {
            unsafe {
                cpmodel_add_bool_or(self.ptr.as_ptr(), lits.as_ptr(), lits.len());
            }
        }

        pub fn add_bool_and(&mut self, lits: &[i32]) {
            unsafe {
                cpmodel_add_bool_and(self.ptr.as_ptr(), lits.as_ptr(), lits.len());
            }
        }

        pub fn add_bool_xor(&mut self, lits: &[i32]) {
            unsafe {
                cpmodel_add_bool_xor(self.ptr.as_ptr(), lits.as_ptr(), lits.len());
            }
        }

        pub fn add_implication(&mut self, lhs: i32, rhs: i32) {
            unsafe {
                cpmodel_add_implication(self.ptr.as_ptr(), lhs, rhs);
            }
        }

        pub fn add_at_most_one(&mut self, lits: &[i32]) {
            unsafe {
                cpmodel_add_at_most_one(self.ptr.as_ptr(), lits.as_ptr(), lits.len());
            }
        }

        pub fn add_exactly_one(&mut self, lits: &[i32]) {
            unsafe {
                cpmodel_add_exactly_one(self.ptr.as_ptr(), lits.as_ptr(), lits.len());
            }
        }

        pub fn add_allowed_assignments(&mut self, vars: &[i32], tuples: &[Vec<i64>]) {
            assert!(tuples.iter().all(|tuple| tuple.len() == vars.len()));
            let flat: Vec<i64> = tuples.iter().flatten().copied().collect();
            unsafe {
                cpmodel_add_allowed_assignments(
                    self.ptr.as_ptr(),
                    vars.as_ptr(),
                    vars.len(),
                    flat.as_ptr(),
                    tuples.len(),
                );
            }
        }

        pub fn minimize(&mut self, vars: &[i32], coeffs: &[i64]) {
            assert_eq!(vars.len(), coeffs.len());
            unsafe {
                cpmodel_minimize(
                    self.ptr.as_ptr(),
                    vars.as_ptr(),
                    coeffs.as_ptr(),
                    vars.len(),
                );
            }
        }

        pub fn maximize(&mut self, vars: &[i32], coeffs: &[i64]) {
            assert_eq!(vars.len(), coeffs.len());
            unsafe {
                cpmodel_maximize(
                    self.ptr.as_ptr(),
                    vars.as_ptr(),
                    coeffs.as_ptr(),
                    vars.len(),
                );
            }
        }

        pub fn new_fixed_interval_var(
            &mut self,
            start: i32,
            size: i64,
            end: i32,
            name: &str,
        ) -> i32 {
            let c = CString::new(name).unwrap();
            unsafe { cpmodel_new_interval_var(self.ptr.as_ptr(), start, size, end, c.as_ptr()) }
        }

        pub fn new_optional_interval_var(
            &mut self,
            start: i32,
            size: i64,
            end: i32,
            lit: i32,
            name: &str,
        ) -> i32 {
            let c = CString::new(name).unwrap();
            unsafe {
                cpmodel_new_optional_interval_var(
                    self.ptr.as_ptr(),
                    start,
                    size,
                    end,
                    lit,
                    c.as_ptr(),
                )
            }
        }

        /// Add a Hamiltonian circuit constraint.
        ///
        /// Each arc is `(tail, head, literal)`.  The arcs whose literal equals 1
        /// must form a single circuit covering all nodes that have no self-loop
        /// literal set to 1.  Use a self-loop `(i, i, lit)` to make node `i`
        /// optional — if `lit = 1` the node is skipped.
        pub fn add_circuit(&mut self, tails: &[i32], heads: &[i32], lits: &[i32]) {
            assert_eq!(tails.len(), heads.len());
            assert_eq!(tails.len(), lits.len());
            unsafe {
                cpmodel_add_circuit(
                    self.ptr.as_ptr(),
                    tails.as_ptr(),
                    heads.as_ptr(),
                    lits.as_ptr(),
                    tails.len(),
                );
            }
        }

        pub fn add_no_overlap(&mut self, intervals: &[i32]) {
            unsafe {
                cpmodel_add_no_overlap(self.ptr.as_ptr(), intervals.as_ptr(), intervals.len());
            }
        }

        pub fn add_cumulative(&mut self, intervals: &[i32], demands: &[i64], capacity: i64) {
            assert_eq!(intervals.len(), demands.len());
            unsafe {
                cpmodel_add_cumulative(
                    self.ptr.as_ptr(),
                    intervals.as_ptr(),
                    demands.as_ptr(),
                    intervals.len(),
                    capacity,
                );
            }
        }

        pub fn add_no_overlap_2d(&mut self, x_intervals: &[i32], y_intervals: &[i32]) {
            assert_eq!(x_intervals.len(), y_intervals.len());
            unsafe {
                cpmodel_add_no_overlap_2d(
                    self.ptr.as_ptr(),
                    x_intervals.as_ptr(),
                    y_intervals.as_ptr(),
                    x_intervals.len(),
                );
            }
        }

        pub fn solve(&self, time_limit_seconds: f64) -> CpSolution {
            unsafe {
                CpSolution {
                    ptr: NonNull::new(cpmodel_solve(self.ptr.as_ptr(), time_limit_seconds))
                        .expect("cpmodel_solve returned null"),
                }
            }
        }
    }

    impl Default for CpModel {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Drop for CpModel {
        fn drop(&mut self) {
            unsafe { cpmodel_free(self.ptr.as_ptr()) }
        }
    }

    pub struct CpSolution {
        ptr: NonNull<CpSolverResponse>,
    }

    impl CpSolution {
        pub fn status(&self) -> OrtoolsStatus {
            unsafe { cpresponse_status(self.ptr.as_ptr()) }
        }
        pub fn objective_value(&self) -> i64 {
            unsafe { cpresponse_objective_value(self.ptr.as_ptr()) }
        }
        pub fn value(&self, var_index: i32) -> i64 {
            unsafe { cpresponse_value(self.ptr.as_ptr(), var_index) }
        }
        pub fn wall_time(&self) -> f64 {
            unsafe { cpresponse_wall_time(self.ptr.as_ptr()) }
        }
    }

    impl Drop for CpSolution {
        fn drop(&mut self) {
            unsafe { cpresponse_free(self.ptr.as_ptr()) }
        }
    }

    pub struct LinearSolver {
        ptr: NonNull<MpSolver>,
    }

    impl LinearSolver {
        pub fn new_glop(name: &str) -> Self {
            let c = CString::new(name).unwrap();
            unsafe {
                Self {
                    ptr: NonNull::new(mpsolver_new(c.as_ptr(), LpSolverType::Glop))
                        .expect("mpsolver_new returned null"),
                }
            }
        }

        pub fn num_var(&mut self, lb: f64, ub: f64, name: &str) -> i32 {
            let c = CString::new(name).unwrap();
            unsafe { mpsolver_num_var(self.ptr.as_ptr(), lb, ub, c.as_ptr()) }
        }

        pub fn add_constraint(&mut self, lb: f64, ub: f64, name: &str) -> i32 {
            let c = CString::new(name).unwrap();
            unsafe { mpsolver_add_constraint(self.ptr.as_ptr(), lb, ub, c.as_ptr()) }
        }

        pub fn set_constraint_coeff(&mut self, ci: i32, vi: i32, coeff: f64) {
            unsafe { mpsolver_set_constraint_coeff(self.ptr.as_ptr(), ci, vi, coeff) }
        }

        pub fn set_objective_coeff(&mut self, vi: i32, coeff: f64) {
            unsafe { mpsolver_set_objective_coeff(self.ptr.as_ptr(), vi, coeff) }
        }

        pub fn maximize(&mut self) {
            unsafe { mpsolver_maximize(self.ptr.as_ptr()) }
        }
        pub fn minimize(&mut self) {
            unsafe { mpsolver_minimize(self.ptr.as_ptr()) }
        }

        pub fn solve(&mut self) -> OrtoolsStatus {
            unsafe { mpsolver_solve(self.ptr.as_ptr()) }
        }

        pub fn objective_value(&self) -> f64 {
            unsafe { mpsolver_objective_value(self.ptr.as_ptr()) }
        }

        pub fn var_value(&self, vi: i32) -> f64 {
            unsafe { mpsolver_var_value(self.ptr.as_ptr(), vi) }
        }
    }

    impl Drop for LinearSolver {
        fn drop(&mut self) {
            unsafe { mpsolver_free(self.ptr.as_ptr()) }
        }
    }

    pub struct SimpleMinCostFlow {
        ptr: NonNull<MinCostFlow>,
    }

    impl SimpleMinCostFlow {
        pub fn new(reserve_num_nodes: i32, reserve_num_arcs: i32) -> Self {
            unsafe {
                Self {
                    ptr: NonNull::new(mincostflow_new(reserve_num_nodes, reserve_num_arcs))
                        .expect("mincostflow_new returned null"),
                }
            }
        }

        pub fn add_arc_with_capacity_and_unit_cost(
            &mut self,
            tail: i32,
            head: i32,
            capacity: i64,
            unit_cost: i64,
        ) -> i32 {
            unsafe { mincostflow_add_arc(self.ptr.as_ptr(), tail, head, capacity, unit_cost) }
        }

        pub fn set_node_supply(&mut self, node: i32, supply: i64) {
            unsafe { mincostflow_set_node_supply(self.ptr.as_ptr(), node, supply) }
        }

        pub fn solve(&mut self) -> MinCostFlowStatus {
            unsafe { mincostflow_solve(self.ptr.as_ptr()) }
        }

        pub fn solve_max_flow_with_min_cost(&mut self) -> MinCostFlowStatus {
            unsafe { mincostflow_solve_max_flow_with_min_cost(self.ptr.as_ptr()) }
        }

        pub fn optimal_cost(&self) -> i64 {
            unsafe { mincostflow_optimal_cost(self.ptr.as_ptr()) }
        }

        pub fn maximum_flow(&self) -> i64 {
            unsafe { mincostflow_maximum_flow(self.ptr.as_ptr()) }
        }

        pub fn flow(&self, arc: i32) -> i64 {
            unsafe { mincostflow_flow(self.ptr.as_ptr(), arc) }
        }

        pub fn num_arcs(&self) -> i32 {
            unsafe { mincostflow_num_arcs(self.ptr.as_ptr()) }
        }

        pub fn tail(&self, arc: i32) -> i32 {
            unsafe { mincostflow_tail(self.ptr.as_ptr(), arc) }
        }

        pub fn head(&self, arc: i32) -> i32 {
            unsafe { mincostflow_head(self.ptr.as_ptr(), arc) }
        }

        pub fn capacity(&self, arc: i32) -> i64 {
            unsafe { mincostflow_capacity(self.ptr.as_ptr(), arc) }
        }

        pub fn unit_cost(&self, arc: i32) -> i64 {
            unsafe { mincostflow_unit_cost(self.ptr.as_ptr(), arc) }
        }
    }

    impl Drop for SimpleMinCostFlow {
        fn drop(&mut self) {
            unsafe { mincostflow_free(self.ptr.as_ptr()) }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_values() {
        assert_eq!(OrtoolsStatus::Optimal as i32, 1);
        assert!(OrtoolsStatus::Optimal.is_success());
        assert!(!OrtoolsStatus::Infeasible.is_success());
        assert_eq!(MinCostFlowStatus::Optimal as i32, 1);
        assert!(MinCostFlowStatus::Optimal.is_success());
        assert!(!MinCostFlowStatus::Unbalanced.is_success());
    }

    #[cfg(feature = "link")]
    mod integration {
        use crate::MinCostFlowStatus;

        use super::super::safe::*;

        #[test]
        fn cpsat_minimize() {
            let mut m = CpModel::new();
            let x = m.new_int_var(0, 10, "x");
            let y = m.new_int_var(0, 10, "y");
            m.add_linear_eq(&[x, y], &[1, 1], 10);
            m.minimize(&[x], &[1]);
            let s = m.solve(60.0);
            assert!(s.status().is_success());
            assert_eq!(s.value(x), 0);
            assert_eq!(s.value(y), 10);
        }

        #[test]
        fn cpsat_all_different() {
            let mut model = CpModel::new();
            let va = model.new_int_var(1, 3, "a");
            let vb = model.new_int_var(1, 3, "b");
            let vc = model.new_int_var(1, 3, "c");
            model.add_all_different(&[va, vb, vc]);
            let sol = model.solve(60.0);
            assert!(sol.status().is_success());
            let vals = [sol.value(va), sol.value(vb), sol.value(vc)];
            assert!(vals.contains(&1) && vals.contains(&2) && vals.contains(&3));
        }

        #[test]
        fn glop_maximize() {
            let mut s = LinearSolver::new_glop("test");
            let x = s.num_var(0.0, f64::INFINITY, "x");
            let y = s.num_var(0.0, f64::INFINITY, "y");
            let c = s.add_constraint(f64::NEG_INFINITY, 10.0, "c1");
            s.set_constraint_coeff(c, x, 1.0);
            s.set_constraint_coeff(c, y, 1.0);
            s.set_objective_coeff(x, 1.0);
            s.set_objective_coeff(y, 1.0);
            s.maximize();
            assert!(s.solve().is_success());
            assert!((s.objective_value() - 10.0).abs() < 1e-6);
        }

        #[test]
        fn min_cost_flow_balanced() {
            let mut flow = SimpleMinCostFlow::new(4, 4);
            let a0 = flow.add_arc_with_capacity_and_unit_cost(0, 1, 3, 1);
            let a1 = flow.add_arc_with_capacity_and_unit_cost(0, 2, 5, 2);
            let a2 = flow.add_arc_with_capacity_and_unit_cost(1, 3, 3, 1);
            let a3 = flow.add_arc_with_capacity_and_unit_cost(2, 3, 5, 1);
            flow.set_node_supply(0, 5);
            flow.set_node_supply(3, -5);

            assert_eq!(flow.solve(), MinCostFlowStatus::Optimal);
            assert_eq!(flow.optimal_cost(), 12);
            assert_eq!(flow.maximum_flow(), 5);
            assert_eq!(flow.flow(a0), 3);
            assert_eq!(flow.flow(a1), 2);
            assert_eq!(flow.flow(a2), 3);
            assert_eq!(flow.flow(a3), 2);
        }
    }
}
