#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case)]

#[cfg(feature = "link")]
use std::os::raw::{c_char, c_double, c_int};

pub const ORTOOLS_NAME: &str = "OR-Tools";
pub const ORTOOLS_VERSION: &str = "9.15";
pub const ORTOOLS_TAG: &str = "v9.15";
pub const ORTOOLS_SOURCE_URL: &str = "https://github.com/google/or-tools";
pub const ORTOOLS_EXPECTED_COMMIT: &str = "551ad10d94835c99e5e1e684500d3db398c0e345";
pub const ORTOOLS_BUILD_CONFIG: &str = concat!(
    "cmake:",
    "CMAKE_BUILD_TYPE=Release;",
    "BUILD_DEPS=ON;",
    "BUILD_SHARED_LIBS=ON;",
    "BUILD_EXAMPLES=OFF;",
    "BUILD_TESTS=OFF;",
    "USE_GLOP=ON;",
    "USE_CP_SAT=ON;",
    "USE_SCIP=OFF;",
    "USE_COINOR=OFF"
);

pub fn ortools_source_mode() -> &'static str {
    option_env!("FERROX_ORTOOLS_SOURCE_MODE").unwrap_or("native-link-disabled")
}

pub fn ortools_source_commit() -> &'static str {
    option_env!("FERROX_ORTOOLS_SOURCE_COMMIT").unwrap_or("native-link-disabled")
}

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
    use std::{collections::HashSet, error::Error, ffi::CString, fmt, ptr::NonNull};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum OrtoolsError {
        InvalidInput(String),
        NativeCallFailed(&'static str),
        MissingSolution(&'static str),
    }

    impl fmt::Display for OrtoolsError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::InvalidInput(message) => f.write_str(message),
                Self::NativeCallFailed(operation) => {
                    write!(f, "native OR-Tools call failed: {operation}")
                }
                Self::MissingSolution(operation) => {
                    write!(f, "OR-Tools solution is not available for {operation}")
                }
            }
        }
    }

    impl Error for OrtoolsError {}

    pub type Result<T> = std::result::Result<T, OrtoolsError>;

    fn cstring(value: &str, field: &'static str) -> Result<CString> {
        CString::new(value).map_err(|_| {
            OrtoolsError::InvalidInput(format!("{field} contains an interior NUL byte"))
        })
    }

    fn validate_int_domain(lb: i64, ub: i64, label: &'static str) -> Result<()> {
        if lb <= ub {
            Ok(())
        } else {
            Err(OrtoolsError::InvalidInput(format!(
                "{label} lower bound exceeds upper bound"
            )))
        }
    }

    fn validate_float_domain(lb: f64, ub: f64, label: &'static str) -> Result<()> {
        if lb.is_nan() || ub.is_nan() {
            return Err(OrtoolsError::InvalidInput(format!(
                "{label} bounds must not be NaN"
            )));
        }
        if lb > ub {
            return Err(OrtoolsError::InvalidInput(format!(
                "{label} lower bound exceeds upper bound"
            )));
        }
        Ok(())
    }

    fn validate_finite(value: f64, label: &'static str) -> Result<()> {
        if value.is_finite() {
            Ok(())
        } else {
            Err(OrtoolsError::InvalidInput(format!(
                "{label} must be finite"
            )))
        }
    }

    fn ensure_known_ref(known: &HashSet<i32>, index: i32, label: &'static str) -> Result<()> {
        if known.contains(&index) {
            Ok(())
        } else {
            Err(OrtoolsError::InvalidInput(format!(
                "{label} references unknown index {index}"
            )))
        }
    }

    fn ensure_known_refs(known: &HashSet<i32>, indices: &[i32], label: &'static str) -> Result<()> {
        for &index in indices {
            ensure_known_ref(known, index, label)?;
        }
        Ok(())
    }

    // CP-SAT literal encoding: non-negative index = variable as-is;
    // negative index = -(var_index + 1) represents the negation of that bool variable.
    // This matches the OR-Tools CpModelBuilder::BoolVar::Not() convention.
    fn bool_index_from_literal(lit: i32) -> Option<i32> {
        if lit >= 0 {
            Some(lit)
        } else {
            lit.checked_neg()?.checked_sub(1)
        }
    }

    fn ensure_bool_literal_refs(
        bools: &HashSet<i32>,
        literals: &[i32],
        label: &'static str,
    ) -> Result<()> {
        for &literal in literals {
            let Some(index) = bool_index_from_literal(literal) else {
                return Err(OrtoolsError::InvalidInput(format!(
                    "{label} contains invalid bool literal {literal}"
                )));
            };
            ensure_known_ref(bools, index, label)?;
        }
        Ok(())
    }

    fn ensure_non_negative(value: i32, label: &'static str) -> Result<()> {
        if value >= 0 {
            Ok(())
        } else {
            Err(OrtoolsError::InvalidInput(format!(
                "{label} must be non-negative"
            )))
        }
    }

    fn ensure_non_negative_i64(value: i64, label: &'static str) -> Result<()> {
        if value >= 0 {
            Ok(())
        } else {
            Err(OrtoolsError::InvalidInput(format!(
                "{label} must be non-negative"
            )))
        }
    }

    fn validate_arc_index(arc: i32, arc_count: i32) -> Result<()> {
        if (0..arc_count).contains(&arc) {
            Ok(())
        } else {
            Err(OrtoolsError::InvalidInput(format!(
                "arc index {arc} is outside 0..{arc_count}"
            )))
        }
    }

    pub struct CpModel {
        ptr: NonNull<CpModelBuilder>,
        vars: HashSet<i32>,
        bools: HashSet<i32>,
        intervals: HashSet<i32>,
    }

    impl CpModel {
        pub fn new() -> Self {
            Self::try_new().expect("cpmodel_new returned null")
        }

        pub fn try_new() -> Result<Self> {
            // SAFETY: `cpmodel_new` allocates a fresh CpModelBuilder and returns
            // a non-null pointer on success; we wrap it in NonNull immediately and
            // the resulting CpModel becomes its unique owner.
            unsafe {
                let ptr = NonNull::new(cpmodel_new())
                    .ok_or(OrtoolsError::NativeCallFailed("cpmodel_new"))?;
                Ok(Self {
                    ptr,
                    vars: HashSet::new(),
                    bools: HashSet::new(),
                    intervals: HashSet::new(),
                })
            }
        }

        pub fn new_int_var(&mut self, lb: i64, ub: i64, name: &str) -> i32 {
            self.try_new_int_var(lb, ub, name)
                .expect("invalid CP-SAT int variable")
        }

        pub fn try_new_int_var(&mut self, lb: i64, ub: i64, name: &str) -> Result<i32> {
            validate_int_domain(lb, ub, "CP-SAT int variable")?;
            let c = cstring(name, "CP-SAT int variable name")?;
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; lb/ub have been validated (lb <= ub); c is a valid
            // NUL-terminated CString that lives for the duration of the call.
            let idx = unsafe { cpmodel_new_int_var(self.ptr.as_ptr(), lb, ub, c.as_ptr()) };
            if idx < 0 {
                return Err(OrtoolsError::NativeCallFailed("cpmodel_new_int_var"));
            }
            self.vars.insert(idx);
            Ok(idx)
        }

        pub fn new_bool_var(&mut self, name: &str) -> i32 {
            self.try_new_bool_var(name)
                .expect("invalid CP-SAT bool variable")
        }

        pub fn try_new_bool_var(&mut self, name: &str) -> Result<i32> {
            let c = cstring(name, "CP-SAT bool variable name")?;
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; c is a valid NUL-terminated CString that lives for
            // the duration of the call.
            let idx = unsafe { cpmodel_new_bool_var(self.ptr.as_ptr(), c.as_ptr()) };
            if idx < 0 {
                return Err(OrtoolsError::NativeCallFailed("cpmodel_new_bool_var"));
            }
            self.vars.insert(idx);
            self.bools.insert(idx);
            Ok(idx)
        }

        pub fn add_linear_le(&mut self, vars: &[i32], coeffs: &[i64], rhs: i64) {
            self.try_add_linear_le(vars, coeffs, rhs)
                .expect("invalid CP-SAT linear <= constraint");
        }

        pub fn try_add_linear_le(&mut self, vars: &[i32], coeffs: &[i64], rhs: i64) -> Result<()> {
            self.validate_linear_terms(vars, coeffs, "CP-SAT linear <= constraint")?;
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; vars and coeffs have been validated (equal length,
            // all indices known) by validate_linear_terms; slice pointers are valid
            // for the duration of the call.
            unsafe {
                cpmodel_add_linear_le(
                    self.ptr.as_ptr(),
                    vars.as_ptr(),
                    coeffs.as_ptr(),
                    vars.len(),
                    rhs,
                );
            }
            Ok(())
        }

        pub fn add_linear_ge(&mut self, vars: &[i32], coeffs: &[i64], rhs: i64) {
            self.try_add_linear_ge(vars, coeffs, rhs)
                .expect("invalid CP-SAT linear >= constraint");
        }

        pub fn try_add_linear_ge(&mut self, vars: &[i32], coeffs: &[i64], rhs: i64) -> Result<()> {
            self.validate_linear_terms(vars, coeffs, "CP-SAT linear >= constraint")?;
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; vars and coeffs have been validated (equal length,
            // all indices known) by validate_linear_terms; slice pointers are valid
            // for the duration of the call.
            unsafe {
                cpmodel_add_linear_ge(
                    self.ptr.as_ptr(),
                    vars.as_ptr(),
                    coeffs.as_ptr(),
                    vars.len(),
                    rhs,
                );
            }
            Ok(())
        }

        pub fn add_linear_eq(&mut self, vars: &[i32], coeffs: &[i64], rhs: i64) {
            self.try_add_linear_eq(vars, coeffs, rhs)
                .expect("invalid CP-SAT linear == constraint");
        }

        pub fn try_add_linear_eq(&mut self, vars: &[i32], coeffs: &[i64], rhs: i64) -> Result<()> {
            self.validate_linear_terms(vars, coeffs, "CP-SAT linear == constraint")?;
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; vars and coeffs have been validated (equal length,
            // all indices known) by validate_linear_terms; slice pointers are valid
            // for the duration of the call.
            unsafe {
                cpmodel_add_linear_eq(
                    self.ptr.as_ptr(),
                    vars.as_ptr(),
                    coeffs.as_ptr(),
                    vars.len(),
                    rhs,
                );
            }
            Ok(())
        }

        pub fn add_all_different(&mut self, vars: &[i32]) {
            self.try_add_all_different(vars)
                .expect("invalid CP-SAT AllDifferent constraint");
        }

        pub fn try_add_all_different(&mut self, vars: &[i32]) -> Result<()> {
            ensure_known_refs(&self.vars, vars, "CP-SAT AllDifferent")?;
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; all variable indices in `vars` have been verified to
            // exist in self.vars; the slice pointer is valid for the duration of the call.
            unsafe {
                cpmodel_add_all_different(self.ptr.as_ptr(), vars.as_ptr(), vars.len());
            }
            Ok(())
        }

        pub fn add_bool_or(&mut self, lits: &[i32]) {
            self.try_add_bool_or(lits)
                .expect("invalid CP-SAT BoolOr constraint");
        }

        pub fn try_add_bool_or(&mut self, lits: &[i32]) -> Result<()> {
            ensure_bool_literal_refs(&self.bools, lits, "CP-SAT BoolOr")?;
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; all literals in `lits` resolve to known bool variable
            // indices (verified by ensure_bool_literal_refs); the slice pointer is
            // valid for the duration of the call.
            unsafe {
                cpmodel_add_bool_or(self.ptr.as_ptr(), lits.as_ptr(), lits.len());
            }
            Ok(())
        }

        pub fn add_bool_and(&mut self, lits: &[i32]) {
            self.try_add_bool_and(lits)
                .expect("invalid CP-SAT BoolAnd constraint");
        }

        pub fn try_add_bool_and(&mut self, lits: &[i32]) -> Result<()> {
            ensure_bool_literal_refs(&self.bools, lits, "CP-SAT BoolAnd")?;
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; all literals in `lits` resolve to known bool variable
            // indices (verified by ensure_bool_literal_refs); the slice pointer is
            // valid for the duration of the call.
            unsafe {
                cpmodel_add_bool_and(self.ptr.as_ptr(), lits.as_ptr(), lits.len());
            }
            Ok(())
        }

        pub fn add_bool_xor(&mut self, lits: &[i32]) {
            self.try_add_bool_xor(lits)
                .expect("invalid CP-SAT BoolXor constraint");
        }

        pub fn try_add_bool_xor(&mut self, lits: &[i32]) -> Result<()> {
            ensure_bool_literal_refs(&self.bools, lits, "CP-SAT BoolXor")?;
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; all literals in `lits` resolve to known bool variable
            // indices (verified by ensure_bool_literal_refs); the slice pointer is
            // valid for the duration of the call.
            unsafe {
                cpmodel_add_bool_xor(self.ptr.as_ptr(), lits.as_ptr(), lits.len());
            }
            Ok(())
        }

        pub fn add_implication(&mut self, lhs: i32, rhs: i32) {
            self.try_add_implication(lhs, rhs)
                .expect("invalid CP-SAT implication constraint");
        }

        pub fn try_add_implication(&mut self, lhs: i32, rhs: i32) -> Result<()> {
            ensure_bool_literal_refs(&self.bools, &[lhs, rhs], "CP-SAT Implication")?;
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; both `lhs` and `rhs` resolve to known bool variable
            // indices (verified by ensure_bool_literal_refs above).
            unsafe {
                cpmodel_add_implication(self.ptr.as_ptr(), lhs, rhs);
            }
            Ok(())
        }

        pub fn add_at_most_one(&mut self, lits: &[i32]) {
            self.try_add_at_most_one(lits)
                .expect("invalid CP-SAT AtMostOne constraint");
        }

        pub fn try_add_at_most_one(&mut self, lits: &[i32]) -> Result<()> {
            ensure_bool_literal_refs(&self.bools, lits, "CP-SAT AtMostOne")?;
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; all literals in `lits` resolve to known bool variable
            // indices (verified by ensure_bool_literal_refs); the slice pointer is
            // valid for the duration of the call.
            unsafe {
                cpmodel_add_at_most_one(self.ptr.as_ptr(), lits.as_ptr(), lits.len());
            }
            Ok(())
        }

        pub fn add_exactly_one(&mut self, lits: &[i32]) {
            self.try_add_exactly_one(lits)
                .expect("invalid CP-SAT ExactlyOne constraint");
        }

        pub fn try_add_exactly_one(&mut self, lits: &[i32]) -> Result<()> {
            ensure_bool_literal_refs(&self.bools, lits, "CP-SAT ExactlyOne")?;
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; all literals in `lits` resolve to known bool variable
            // indices (verified by ensure_bool_literal_refs); the slice pointer is
            // valid for the duration of the call.
            unsafe {
                cpmodel_add_exactly_one(self.ptr.as_ptr(), lits.as_ptr(), lits.len());
            }
            Ok(())
        }

        pub fn add_allowed_assignments(&mut self, vars: &[i32], tuples: &[Vec<i64>]) {
            self.try_add_allowed_assignments(vars, tuples)
                .expect("invalid CP-SAT AllowedAssignments constraint");
        }

        pub fn try_add_allowed_assignments(
            &mut self,
            vars: &[i32],
            tuples: &[Vec<i64>],
        ) -> Result<()> {
            ensure_known_refs(&self.vars, vars, "CP-SAT AllowedAssignments")?;
            if let Some(tuple) = tuples.iter().find(|tuple| tuple.len() != vars.len()) {
                return Err(OrtoolsError::InvalidInput(format!(
                    "CP-SAT AllowedAssignments tuple arity {} does not match variable count {}",
                    tuple.len(),
                    vars.len()
                )));
            }
            let flat: Vec<i64> = tuples.iter().flatten().copied().collect();
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; all variable indices have been verified to exist in
            // self.vars; every tuple has been verified to have the same arity as
            // `vars`; `flat` is a contiguous Vec with tuples.len() * vars.len()
            // elements so `tuples.len()` correctly identifies the row count.
            unsafe {
                cpmodel_add_allowed_assignments(
                    self.ptr.as_ptr(),
                    vars.as_ptr(),
                    vars.len(),
                    flat.as_ptr(),
                    tuples.len(),
                );
            }
            Ok(())
        }

        pub fn minimize(&mut self, vars: &[i32], coeffs: &[i64]) {
            self.try_minimize(vars, coeffs)
                .expect("invalid CP-SAT objective");
        }

        pub fn try_minimize(&mut self, vars: &[i32], coeffs: &[i64]) -> Result<()> {
            self.validate_linear_terms(vars, coeffs, "CP-SAT objective")?;
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; vars and coeffs have been validated (equal length,
            // all indices known) by validate_linear_terms; slice pointers are valid
            // for the duration of the call.
            unsafe {
                cpmodel_minimize(
                    self.ptr.as_ptr(),
                    vars.as_ptr(),
                    coeffs.as_ptr(),
                    vars.len(),
                );
            }
            Ok(())
        }

        pub fn maximize(&mut self, vars: &[i32], coeffs: &[i64]) {
            self.try_maximize(vars, coeffs)
                .expect("invalid CP-SAT objective");
        }

        pub fn try_maximize(&mut self, vars: &[i32], coeffs: &[i64]) -> Result<()> {
            self.validate_linear_terms(vars, coeffs, "CP-SAT objective")?;
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; vars and coeffs have been validated (equal length,
            // all indices known) by validate_linear_terms; slice pointers are valid
            // for the duration of the call.
            unsafe {
                cpmodel_maximize(
                    self.ptr.as_ptr(),
                    vars.as_ptr(),
                    coeffs.as_ptr(),
                    vars.len(),
                );
            }
            Ok(())
        }

        pub fn new_fixed_interval_var(
            &mut self,
            start: i32,
            size: i64,
            end: i32,
            name: &str,
        ) -> i32 {
            self.try_new_fixed_interval_var(start, size, end, name)
                .expect("invalid CP-SAT fixed interval variable")
        }

        pub fn try_new_fixed_interval_var(
            &mut self,
            start: i32,
            size: i64,
            end: i32,
            name: &str,
        ) -> Result<i32> {
            ensure_known_ref(&self.vars, start, "CP-SAT interval start")?;
            ensure_known_ref(&self.vars, end, "CP-SAT interval end")?;
            ensure_non_negative_i64(size, "CP-SAT interval size")?;
            let c = cstring(name, "CP-SAT interval name")?;
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; start and end are verified known variable indices;
            // size is non-negative; c is a valid NUL-terminated CString that lives
            // for the duration of the call.
            let idx = unsafe {
                cpmodel_new_interval_var(self.ptr.as_ptr(), start, size, end, c.as_ptr())
            };
            if idx < 0 {
                return Err(OrtoolsError::NativeCallFailed("cpmodel_new_interval_var"));
            }
            self.intervals.insert(idx);
            Ok(idx)
        }

        pub fn new_optional_interval_var(
            &mut self,
            start: i32,
            size: i64,
            end: i32,
            lit: i32,
            name: &str,
        ) -> i32 {
            self.try_new_optional_interval_var(start, size, end, lit, name)
                .expect("invalid CP-SAT optional interval variable")
        }

        pub fn try_new_optional_interval_var(
            &mut self,
            start: i32,
            size: i64,
            end: i32,
            lit: i32,
            name: &str,
        ) -> Result<i32> {
            ensure_known_ref(&self.vars, start, "CP-SAT optional interval start")?;
            ensure_known_ref(&self.vars, end, "CP-SAT optional interval end")?;
            ensure_known_ref(&self.bools, lit, "CP-SAT optional interval literal")?;
            ensure_non_negative_i64(size, "CP-SAT optional interval size")?;
            let c = cstring(name, "CP-SAT optional interval name")?;
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; start and end are verified known variable indices;
            // lit is a verified known bool variable index; size is non-negative;
            // c is a valid NUL-terminated CString that lives for the duration of the call.
            let idx = unsafe {
                cpmodel_new_optional_interval_var(
                    self.ptr.as_ptr(),
                    start,
                    size,
                    end,
                    lit,
                    c.as_ptr(),
                )
            };
            if idx < 0 {
                return Err(OrtoolsError::NativeCallFailed(
                    "cpmodel_new_optional_interval_var",
                ));
            }
            self.intervals.insert(idx);
            Ok(idx)
        }

        /// Add a Hamiltonian circuit constraint.
        ///
        /// Each arc is `(tail, head, literal)`.  The arcs whose literal equals 1
        /// must form a single circuit covering all nodes that have no self-loop
        /// literal set to 1.  Use a self-loop `(i, i, lit)` to make node `i`
        /// optional — if `lit = 1` the node is skipped.
        pub fn add_circuit(&mut self, tails: &[i32], heads: &[i32], lits: &[i32]) {
            self.try_add_circuit(tails, heads, lits)
                .expect("invalid CP-SAT circuit constraint");
        }

        pub fn try_add_circuit(
            &mut self,
            tails: &[i32],
            heads: &[i32],
            lits: &[i32],
        ) -> Result<()> {
            if tails.len() != heads.len() || tails.len() != lits.len() {
                return Err(OrtoolsError::InvalidInput(
                    "CP-SAT circuit tails, heads, and literals must have equal length".to_string(),
                ));
            }
            for &tail in tails {
                ensure_non_negative(tail, "CP-SAT circuit tail")?;
            }
            for &head in heads {
                ensure_non_negative(head, "CP-SAT circuit head")?;
            }
            ensure_known_refs(&self.bools, lits, "CP-SAT circuit literal")?;
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; tails, heads, and lits are equal-length slices (checked
            // above), all tail/head node indices are non-negative, and all literal
            // indices are verified known bool variables; slice pointers are valid for
            // the duration of the call.
            unsafe {
                cpmodel_add_circuit(
                    self.ptr.as_ptr(),
                    tails.as_ptr(),
                    heads.as_ptr(),
                    lits.as_ptr(),
                    tails.len(),
                );
            }
            Ok(())
        }

        pub fn add_no_overlap(&mut self, intervals: &[i32]) {
            self.try_add_no_overlap(intervals)
                .expect("invalid CP-SAT NoOverlap constraint");
        }

        pub fn try_add_no_overlap(&mut self, intervals: &[i32]) -> Result<()> {
            ensure_known_refs(&self.intervals, intervals, "CP-SAT NoOverlap")?;
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; all interval indices are verified to exist in
            // self.intervals; the slice pointer is valid for the duration of the call.
            unsafe {
                cpmodel_add_no_overlap(self.ptr.as_ptr(), intervals.as_ptr(), intervals.len());
            }
            Ok(())
        }

        pub fn add_cumulative(&mut self, intervals: &[i32], demands: &[i64], capacity: i64) {
            self.try_add_cumulative(intervals, demands, capacity)
                .expect("invalid CP-SAT Cumulative constraint");
        }

        pub fn try_add_cumulative(
            &mut self,
            intervals: &[i32],
            demands: &[i64],
            capacity: i64,
        ) -> Result<()> {
            if intervals.len() != demands.len() {
                return Err(OrtoolsError::InvalidInput(
                    "CP-SAT Cumulative intervals and demands must have equal length".to_string(),
                ));
            }
            ensure_known_refs(&self.intervals, intervals, "CP-SAT Cumulative interval")?;
            ensure_non_negative_i64(capacity, "CP-SAT Cumulative capacity")?;
            for &demand in demands {
                ensure_non_negative_i64(demand, "CP-SAT Cumulative demand")?;
            }
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; all interval indices are verified to exist in
            // self.intervals; intervals and demands are equal-length slices (checked
            // above); capacity and all demands are non-negative; slice pointers are
            // valid for the duration of the call.
            unsafe {
                cpmodel_add_cumulative(
                    self.ptr.as_ptr(),
                    intervals.as_ptr(),
                    demands.as_ptr(),
                    intervals.len(),
                    capacity,
                );
            }
            Ok(())
        }

        pub fn add_no_overlap_2d(&mut self, x_intervals: &[i32], y_intervals: &[i32]) {
            self.try_add_no_overlap_2d(x_intervals, y_intervals)
                .expect("invalid CP-SAT NoOverlap2D constraint");
        }

        pub fn try_add_no_overlap_2d(
            &mut self,
            x_intervals: &[i32],
            y_intervals: &[i32],
        ) -> Result<()> {
            if x_intervals.len() != y_intervals.len() {
                return Err(OrtoolsError::InvalidInput(
                    "CP-SAT NoOverlap2D interval lists must have equal length".to_string(),
                ));
            }
            ensure_known_refs(
                &self.intervals,
                x_intervals,
                "CP-SAT NoOverlap2D x interval",
            )?;
            ensure_known_refs(
                &self.intervals,
                y_intervals,
                "CP-SAT NoOverlap2D y interval",
            )?;
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; all x and y interval indices are verified to exist in
            // self.intervals; the two slices are equal length (checked above);
            // slice pointers are valid for the duration of the call.
            unsafe {
                cpmodel_add_no_overlap_2d(
                    self.ptr.as_ptr(),
                    x_intervals.as_ptr(),
                    y_intervals.as_ptr(),
                    x_intervals.len(),
                );
            }
            Ok(())
        }

        pub fn solve(&self, time_limit_seconds: f64) -> CpSolution {
            self.try_solve(time_limit_seconds)
                .expect("invalid CP-SAT solve request")
        }

        pub fn try_solve(&self, time_limit_seconds: f64) -> Result<CpSolution> {
            if !time_limit_seconds.is_finite() || time_limit_seconds < 0.0 {
                return Err(OrtoolsError::InvalidInput(
                    "CP-SAT time limit must be finite and non-negative".to_string(),
                ));
            }
            // SAFETY: self.ptr is a live, non-null CpModelBuilder owned exclusively
            // by this struct; time_limit_seconds has been validated as finite and
            // non-negative; cpmodel_solve allocates a new CpSolverResponse on the
            // heap which we immediately wrap in NonNull, becoming its unique owner.
            unsafe {
                let ptr = NonNull::new(cpmodel_solve(self.ptr.as_ptr(), time_limit_seconds))
                    .ok_or(OrtoolsError::NativeCallFailed("cpmodel_solve"))?;
                Ok(CpSolution {
                    ptr,
                    vars: self.vars.clone(),
                })
            }
        }

        fn validate_linear_terms(
            &self,
            vars: &[i32],
            coeffs: &[i64],
            label: &'static str,
        ) -> Result<()> {
            if vars.len() != coeffs.len() {
                return Err(OrtoolsError::InvalidInput(format!(
                    "{label} variable and coefficient lists must have equal length"
                )));
            }
            ensure_known_refs(&self.vars, vars, label)
        }
    }

    impl Default for CpModel {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Drop for CpModel {
        fn drop(&mut self) {
            // SAFETY: self.ptr is a live, non-null CpModelBuilder; this is the
            // Drop impl so the struct is being destroyed and the pointer will
            // never be accessed again after this call.
            unsafe { cpmodel_free(self.ptr.as_ptr()) }
        }
    }

    pub struct CpSolution {
        ptr: NonNull<CpSolverResponse>,
        vars: HashSet<i32>,
    }

    impl CpSolution {
        pub fn status(&self) -> OrtoolsStatus {
            // SAFETY: self.ptr is a live, non-null CpSolverResponse allocated by
            // cpmodel_solve and owned exclusively by this struct; reading the
            // status is always safe after a successful solve.
            unsafe { cpresponse_status(self.ptr.as_ptr()) }
        }
        pub fn objective_value(&self) -> i64 {
            self.try_objective_value()
                .expect("CP-SAT solution objective is not available")
        }
        pub fn try_objective_value(&self) -> Result<i64> {
            if !self.status().is_success() {
                return Err(OrtoolsError::MissingSolution("cpresponse_objective_value"));
            }
            // SAFETY: self.ptr is a live, non-null CpSolverResponse owned exclusively
            // by this struct; the status check above guarantees a feasible/optimal
            // solution exists, so the objective value is populated by OR-Tools.
            Ok(unsafe { cpresponse_objective_value(self.ptr.as_ptr()) })
        }
        pub fn value(&self, var_index: i32) -> i64 {
            self.try_value(var_index)
                .expect("CP-SAT solution variable is not available")
        }
        pub fn try_value(&self, var_index: i32) -> Result<i64> {
            if !self.status().is_success() {
                return Err(OrtoolsError::MissingSolution("cpresponse_value"));
            }
            ensure_known_ref(&self.vars, var_index, "CP-SAT solution value")?;
            // SAFETY: self.ptr is a live, non-null CpSolverResponse owned exclusively
            // by this struct; the status check above guarantees a solution exists;
            // var_index has been verified to be a known variable index, so the
            // value array is populated for this index by OR-Tools.
            Ok(unsafe { cpresponse_value(self.ptr.as_ptr(), var_index) })
        }
        pub fn wall_time(&self) -> f64 {
            // SAFETY: self.ptr is a live, non-null CpSolverResponse owned exclusively
            // by this struct; wall time is always populated regardless of solve outcome.
            unsafe { cpresponse_wall_time(self.ptr.as_ptr()) }
        }
    }

    impl Drop for CpSolution {
        fn drop(&mut self) {
            // SAFETY: self.ptr is a live, non-null CpSolverResponse; this is the
            // Drop impl so the struct is being destroyed and the pointer will
            // never be accessed again after this call.
            unsafe { cpresponse_free(self.ptr.as_ptr()) }
        }
    }

    pub struct LinearSolver {
        ptr: NonNull<MpSolver>,
        var_count: i32,
        constraint_count: i32,
        last_status: Option<OrtoolsStatus>,
    }

    impl LinearSolver {
        pub fn new_glop(name: &str) -> Self {
            Self::try_new_glop(name).expect("mpsolver_new returned null")
        }

        pub fn try_new_glop(name: &str) -> Result<Self> {
            let c = cstring(name, "GLOP solver name")?;
            // SAFETY: `mpsolver_new` allocates a fresh MpSolver instance and returns
            // a non-null pointer on success; we wrap it in NonNull immediately and
            // the resulting LinearSolver becomes its unique owner.  c is a valid
            // NUL-terminated CString that lives for the duration of the call.
            unsafe {
                let ptr = NonNull::new(mpsolver_new(c.as_ptr(), LpSolverType::Glop))
                    .ok_or(OrtoolsError::NativeCallFailed("mpsolver_new"))?;
                Ok(Self {
                    ptr,
                    var_count: 0,
                    constraint_count: 0,
                    last_status: None,
                })
            }
        }

        pub fn num_var(&mut self, lb: f64, ub: f64, name: &str) -> i32 {
            self.try_num_var(lb, ub, name)
                .expect("invalid GLOP numeric variable")
        }

        pub fn try_num_var(&mut self, lb: f64, ub: f64, name: &str) -> Result<i32> {
            validate_float_domain(lb, ub, "GLOP numeric variable")?;
            let c = cstring(name, "GLOP numeric variable name")?;
            // SAFETY: self.ptr is a live, non-null MpSolver owned exclusively by
            // this struct; lb/ub have been validated (finite, lb <= ub); c is a
            // valid NUL-terminated CString that lives for the duration of the call.
            let idx = unsafe { mpsolver_num_var(self.ptr.as_ptr(), lb, ub, c.as_ptr()) };
            self.record_var_index(idx, "mpsolver_num_var")
        }

        pub fn int_var(&mut self, lb: f64, ub: f64, name: &str) -> i32 {
            self.try_int_var(lb, ub, name)
                .expect("invalid MPSolver integer variable")
        }

        pub fn try_int_var(&mut self, lb: f64, ub: f64, name: &str) -> Result<i32> {
            validate_float_domain(lb, ub, "MPSolver integer variable")?;
            let c = cstring(name, "MPSolver integer variable name")?;
            // SAFETY: self.ptr is a live, non-null MpSolver owned exclusively by
            // this struct; lb/ub have been validated (finite, lb <= ub); c is a
            // valid NUL-terminated CString that lives for the duration of the call.
            let idx = unsafe { mpsolver_int_var(self.ptr.as_ptr(), lb, ub, c.as_ptr()) };
            self.record_var_index(idx, "mpsolver_int_var")
        }

        pub fn bool_var(&mut self, name: &str) -> i32 {
            self.try_bool_var(name)
                .expect("invalid MPSolver bool variable")
        }

        pub fn try_bool_var(&mut self, name: &str) -> Result<i32> {
            let c = cstring(name, "MPSolver bool variable name")?;
            // SAFETY: self.ptr is a live, non-null MpSolver owned exclusively by
            // this struct; c is a valid NUL-terminated CString that lives for the
            // duration of the call.
            let idx = unsafe { mpsolver_bool_var(self.ptr.as_ptr(), c.as_ptr()) };
            self.record_var_index(idx, "mpsolver_bool_var")
        }

        pub fn add_constraint(&mut self, lb: f64, ub: f64, name: &str) -> i32 {
            self.try_add_constraint(lb, ub, name)
                .expect("invalid GLOP constraint")
        }

        pub fn try_add_constraint(&mut self, lb: f64, ub: f64, name: &str) -> Result<i32> {
            validate_float_domain(lb, ub, "GLOP constraint")?;
            let c = cstring(name, "GLOP constraint name")?;
            // SAFETY: self.ptr is a live, non-null MpSolver owned exclusively by
            // this struct; lb/ub have been validated (no NaN, lb <= ub); c is a
            // valid NUL-terminated CString that lives for the duration of the call.
            let idx = unsafe { mpsolver_add_constraint(self.ptr.as_ptr(), lb, ub, c.as_ptr()) };
            if idx < 0 {
                return Err(OrtoolsError::NativeCallFailed("mpsolver_add_constraint"));
            }
            if idx != self.constraint_count {
                return Err(OrtoolsError::NativeCallFailed("mpsolver_add_constraint"));
            }
            self.constraint_count += 1;
            Ok(idx)
        }

        pub fn set_constraint_coeff(&mut self, ci: i32, vi: i32, coeff: f64) {
            self.try_set_constraint_coeff(ci, vi, coeff)
                .expect("invalid GLOP constraint coefficient");
        }

        pub fn try_set_constraint_coeff(&mut self, ci: i32, vi: i32, coeff: f64) -> Result<()> {
            self.validate_constraint_index(ci)?;
            self.validate_var_index(vi)?;
            validate_finite(coeff, "GLOP constraint coefficient")?;
            // SAFETY: self.ptr is a live, non-null MpSolver owned exclusively by
            // this struct; ci and vi have been validated as in-bounds indices;
            // coeff is a finite f64.
            unsafe { mpsolver_set_constraint_coeff(self.ptr.as_ptr(), ci, vi, coeff) }
            Ok(())
        }

        pub fn set_objective_coeff(&mut self, vi: i32, coeff: f64) {
            self.try_set_objective_coeff(vi, coeff)
                .expect("invalid GLOP objective coefficient");
        }

        pub fn try_set_objective_coeff(&mut self, vi: i32, coeff: f64) -> Result<()> {
            self.validate_var_index(vi)?;
            validate_finite(coeff, "GLOP objective coefficient")?;
            // SAFETY: self.ptr is a live, non-null MpSolver owned exclusively by
            // this struct; vi has been validated as an in-bounds index; coeff is finite.
            unsafe { mpsolver_set_objective_coeff(self.ptr.as_ptr(), vi, coeff) }
            Ok(())
        }

        pub fn maximize(&mut self) {
            // SAFETY: self.ptr is a live, non-null MpSolver owned exclusively by
            // this struct; setting the objective direction has no preconditions.
            unsafe { mpsolver_maximize(self.ptr.as_ptr()) }
        }
        pub fn minimize(&mut self) {
            // SAFETY: self.ptr is a live, non-null MpSolver owned exclusively by
            // this struct; setting the objective direction has no preconditions.
            unsafe { mpsolver_minimize(self.ptr.as_ptr()) }
        }

        pub fn solve(&mut self) -> OrtoolsStatus {
            self.try_solve()
                .expect("GLOP solve request failed before native solve")
        }

        pub fn try_solve(&mut self) -> Result<OrtoolsStatus> {
            // SAFETY: self.ptr is a live, non-null MpSolver owned exclusively by
            // this struct; no aliasing occurs because `&mut self` is held.
            let status = unsafe { mpsolver_solve(self.ptr.as_ptr()) };
            self.last_status = Some(status);
            Ok(status)
        }

        pub fn objective_value(&self) -> f64 {
            self.try_objective_value()
                .expect("GLOP objective value is not available")
        }

        pub fn try_objective_value(&self) -> Result<f64> {
            self.ensure_solution("mpsolver_objective_value")?;
            // SAFETY: self.ptr is a live, non-null MpSolver owned exclusively by
            // this struct; ensure_solution above guarantees a successful solve has
            // completed, so the objective value is populated.
            Ok(unsafe { mpsolver_objective_value(self.ptr.as_ptr()) })
        }

        pub fn var_value(&self, vi: i32) -> f64 {
            self.try_var_value(vi)
                .expect("GLOP variable value is not available")
        }

        pub fn try_var_value(&self, vi: i32) -> Result<f64> {
            self.validate_var_index(vi)?;
            self.ensure_solution("mpsolver_var_value")?;
            // SAFETY: self.ptr is a live, non-null MpSolver owned exclusively by
            // this struct; vi has been validated as an in-bounds index; ensure_solution
            // above guarantees a successful solve has completed, so variable values
            // are populated.
            Ok(unsafe { mpsolver_var_value(self.ptr.as_ptr(), vi) })
        }

        fn record_var_index(&mut self, idx: i32, operation: &'static str) -> Result<i32> {
            if idx < 0 {
                return Err(OrtoolsError::NativeCallFailed(operation));
            }
            if idx != self.var_count {
                return Err(OrtoolsError::NativeCallFailed(operation));
            }
            self.var_count += 1;
            Ok(idx)
        }

        fn validate_var_index(&self, vi: i32) -> Result<()> {
            if (0..self.var_count).contains(&vi) {
                Ok(())
            } else {
                Err(OrtoolsError::InvalidInput(format!(
                    "GLOP variable index {vi} is outside 0..{}",
                    self.var_count
                )))
            }
        }

        fn validate_constraint_index(&self, ci: i32) -> Result<()> {
            if (0..self.constraint_count).contains(&ci) {
                Ok(())
            } else {
                Err(OrtoolsError::InvalidInput(format!(
                    "GLOP constraint index {ci} is outside 0..{}",
                    self.constraint_count
                )))
            }
        }

        fn ensure_solution(&self, operation: &'static str) -> Result<()> {
            if self.last_status.is_some_and(OrtoolsStatus::is_success) {
                Ok(())
            } else {
                Err(OrtoolsError::MissingSolution(operation))
            }
        }
    }

    impl Drop for LinearSolver {
        fn drop(&mut self) {
            // SAFETY: self.ptr is a live, non-null MpSolver; this is the Drop impl
            // so the struct is being destroyed and the pointer will never be
            // accessed again after this call.
            unsafe { mpsolver_free(self.ptr.as_ptr()) }
        }
    }

    pub struct SimpleMinCostFlow {
        ptr: NonNull<MinCostFlow>,
        arc_count: i32,
        last_status: Option<MinCostFlowStatus>,
    }

    impl SimpleMinCostFlow {
        pub fn new(reserve_num_nodes: i32, reserve_num_arcs: i32) -> Self {
            Self::try_new(reserve_num_nodes, reserve_num_arcs)
                .expect("mincostflow_new returned null")
        }

        pub fn try_new(reserve_num_nodes: i32, reserve_num_arcs: i32) -> Result<Self> {
            ensure_non_negative(reserve_num_nodes, "min-cost flow node reserve")?;
            ensure_non_negative(reserve_num_arcs, "min-cost flow arc reserve")?;
            // SAFETY: `mincostflow_new` allocates a fresh MinCostFlow and returns
            // a non-null pointer on success; both reserve arguments are non-negative
            // (checked above); we wrap the result in NonNull immediately and the
            // resulting SimpleMinCostFlow becomes its unique owner.
            unsafe {
                let ptr = NonNull::new(mincostflow_new(reserve_num_nodes, reserve_num_arcs))
                    .ok_or(OrtoolsError::NativeCallFailed("mincostflow_new"))?;
                Ok(Self {
                    ptr,
                    arc_count: 0,
                    last_status: None,
                })
            }
        }

        pub fn add_arc_with_capacity_and_unit_cost(
            &mut self,
            tail: i32,
            head: i32,
            capacity: i64,
            unit_cost: i64,
        ) -> i32 {
            self.try_add_arc_with_capacity_and_unit_cost(tail, head, capacity, unit_cost)
                .expect("invalid min-cost flow arc")
        }

        pub fn try_add_arc_with_capacity_and_unit_cost(
            &mut self,
            tail: i32,
            head: i32,
            capacity: i64,
            unit_cost: i64,
        ) -> Result<i32> {
            ensure_non_negative(tail, "min-cost flow arc tail")?;
            ensure_non_negative(head, "min-cost flow arc head")?;
            ensure_non_negative_i64(capacity, "min-cost flow arc capacity")?;
            // SAFETY: self.ptr is a live, non-null MinCostFlow owned exclusively by
            // this struct; tail and head are non-negative node indices; capacity is
            // non-negative; unit_cost is unrestricted (may be negative to model gain).
            let idx =
                unsafe { mincostflow_add_arc(self.ptr.as_ptr(), tail, head, capacity, unit_cost) };
            if idx < 0 {
                return Err(OrtoolsError::NativeCallFailed("mincostflow_add_arc"));
            }
            if idx != self.arc_count {
                return Err(OrtoolsError::NativeCallFailed("mincostflow_add_arc"));
            }
            self.arc_count += 1;
            Ok(idx)
        }

        pub fn set_node_supply(&mut self, node: i32, supply: i64) {
            self.try_set_node_supply(node, supply)
                .expect("invalid min-cost flow node supply");
        }

        pub fn try_set_node_supply(&mut self, node: i32, supply: i64) -> Result<()> {
            ensure_non_negative(node, "min-cost flow supply node")?;
            // SAFETY: self.ptr is a live, non-null MinCostFlow owned exclusively by
            // this struct; node is a non-negative index; supply may be any i64
            // (positive = source, negative = sink per OR-Tools convention).
            unsafe { mincostflow_set_node_supply(self.ptr.as_ptr(), node, supply) }
            Ok(())
        }

        pub fn solve(&mut self) -> MinCostFlowStatus {
            self.try_solve()
                .expect("min-cost flow solve request failed before native solve")
        }

        pub fn try_solve(&mut self) -> Result<MinCostFlowStatus> {
            // SAFETY: self.ptr is a live, non-null MinCostFlow owned exclusively by
            // this struct; no aliasing occurs because `&mut self` is held.
            let status = unsafe { mincostflow_solve(self.ptr.as_ptr()) };
            self.last_status = Some(status);
            Ok(status)
        }

        pub fn solve_max_flow_with_min_cost(&mut self) -> MinCostFlowStatus {
            self.try_solve_max_flow_with_min_cost()
                .expect("min-cost flow solve request failed before native solve")
        }

        pub fn try_solve_max_flow_with_min_cost(&mut self) -> Result<MinCostFlowStatus> {
            // SAFETY: self.ptr is a live, non-null MinCostFlow owned exclusively by
            // this struct; no aliasing occurs because `&mut self` is held.
            let status = unsafe { mincostflow_solve_max_flow_with_min_cost(self.ptr.as_ptr()) };
            self.last_status = Some(status);
            Ok(status)
        }

        pub fn optimal_cost(&self) -> i64 {
            self.try_optimal_cost()
                .expect("min-cost flow optimal cost is not available")
        }

        pub fn try_optimal_cost(&self) -> Result<i64> {
            self.ensure_solution("mincostflow_optimal_cost")?;
            // SAFETY: self.ptr is a live, non-null MinCostFlow owned exclusively by
            // this struct; ensure_solution above guarantees a successful solve, so
            // the optimal cost is populated.
            Ok(unsafe { mincostflow_optimal_cost(self.ptr.as_ptr()) })
        }

        pub fn maximum_flow(&self) -> i64 {
            self.try_maximum_flow()
                .expect("min-cost flow maximum flow is not available")
        }

        pub fn try_maximum_flow(&self) -> Result<i64> {
            self.ensure_solution("mincostflow_maximum_flow")?;
            // SAFETY: self.ptr is a live, non-null MinCostFlow owned exclusively by
            // this struct; ensure_solution above guarantees a successful solve, so
            // the maximum flow value is populated.
            Ok(unsafe { mincostflow_maximum_flow(self.ptr.as_ptr()) })
        }

        pub fn flow(&self, arc: i32) -> i64 {
            self.try_flow(arc)
                .expect("min-cost flow arc flow is not available")
        }

        pub fn try_flow(&self, arc: i32) -> Result<i64> {
            validate_arc_index(arc, self.arc_count)?;
            self.ensure_solution("mincostflow_flow")?;
            // SAFETY: self.ptr is a live, non-null MinCostFlow owned exclusively by
            // this struct; arc is a validated in-bounds index; ensure_solution above
            // guarantees a successful solve, so per-arc flow values are populated.
            Ok(unsafe { mincostflow_flow(self.ptr.as_ptr(), arc) })
        }

        pub fn num_arcs(&self) -> i32 {
            self.try_num_arcs()
                .expect("min-cost flow arc count is not available")
        }

        pub fn try_num_arcs(&self) -> Result<i32> {
            // SAFETY: self.ptr is a live, non-null MinCostFlow owned exclusively by
            // this struct; querying the arc count is always safe after construction.
            let native = unsafe { mincostflow_num_arcs(self.ptr.as_ptr()) };
            if native == self.arc_count {
                Ok(native)
            } else {
                Err(OrtoolsError::NativeCallFailed("mincostflow_num_arcs"))
            }
        }

        pub fn tail(&self, arc: i32) -> i32 {
            self.try_tail(arc)
                .expect("min-cost flow arc tail is not available")
        }

        pub fn try_tail(&self, arc: i32) -> Result<i32> {
            validate_arc_index(arc, self.arc_count)?;
            // SAFETY: self.ptr is a live, non-null MinCostFlow owned exclusively by
            // this struct; arc has been validated as an in-bounds index above.
            Ok(unsafe { mincostflow_tail(self.ptr.as_ptr(), arc) })
        }

        pub fn head(&self, arc: i32) -> i32 {
            self.try_head(arc)
                .expect("min-cost flow arc head is not available")
        }

        pub fn try_head(&self, arc: i32) -> Result<i32> {
            validate_arc_index(arc, self.arc_count)?;
            // SAFETY: self.ptr is a live, non-null MinCostFlow owned exclusively by
            // this struct; arc has been validated as an in-bounds index above.
            Ok(unsafe { mincostflow_head(self.ptr.as_ptr(), arc) })
        }

        pub fn capacity(&self, arc: i32) -> i64 {
            self.try_capacity(arc)
                .expect("min-cost flow arc capacity is not available")
        }

        pub fn try_capacity(&self, arc: i32) -> Result<i64> {
            validate_arc_index(arc, self.arc_count)?;
            // SAFETY: self.ptr is a live, non-null MinCostFlow owned exclusively by
            // this struct; arc has been validated as an in-bounds index above.
            Ok(unsafe { mincostflow_capacity(self.ptr.as_ptr(), arc) })
        }

        pub fn unit_cost(&self, arc: i32) -> i64 {
            self.try_unit_cost(arc)
                .expect("min-cost flow arc unit cost is not available")
        }

        pub fn try_unit_cost(&self, arc: i32) -> Result<i64> {
            validate_arc_index(arc, self.arc_count)?;
            // SAFETY: self.ptr is a live, non-null MinCostFlow owned exclusively by
            // this struct; arc has been validated as an in-bounds index above.
            Ok(unsafe { mincostflow_unit_cost(self.ptr.as_ptr(), arc) })
        }

        fn ensure_solution(&self, operation: &'static str) -> Result<()> {
            if self.last_status.is_some_and(MinCostFlowStatus::is_success) {
                Ok(())
            } else {
                Err(OrtoolsError::MissingSolution(operation))
            }
        }
    }

    impl Drop for SimpleMinCostFlow {
        fn drop(&mut self) {
            // SAFETY: self.ptr is a live, non-null MinCostFlow; this is the Drop
            // impl so the struct is being destroyed and the pointer will never be
            // accessed again after this call.
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
        fn cpsat_rejects_unknown_var_reference() {
            let mut model = CpModel::new();
            assert!(model.try_add_linear_eq(&[123], &[1], 0).is_err());
        }

        #[test]
        fn cpsat_rejects_unknown_bool_literal() {
            let mut model = CpModel::new();
            let x = model.new_int_var(0, 1, "x");
            assert!(model.try_add_bool_or(&[x]).is_err());
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
        fn glop_rejects_unknown_coeff_ref() {
            let mut s = LinearSolver::new_glop("test");
            let c = s.add_constraint(f64::NEG_INFINITY, 1.0, "c");
            assert!(s.try_set_constraint_coeff(c, 99, 1.0).is_err());
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

        #[test]
        fn min_cost_flow_rejects_unknown_arc_read() {
            let flow = SimpleMinCostFlow::new(0, 0);
            assert!(flow.try_flow(0).is_err());
        }
    }
}
