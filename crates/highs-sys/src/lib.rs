#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case)]

#[cfg(feature = "link")]
use std::os::raw::c_int;

pub const HIGHS_NAME: &str = "HiGHS";
pub const HIGHS_VERSION: &str = "1.14.0";
pub const HIGHS_TAG: &str = "v1.14.0";
pub const HIGHS_SOURCE_URL: &str = "https://github.com/ERGO-Code/HiGHS";
pub const HIGHS_EXPECTED_COMMIT: &str = "7df0786de3088c832297e5ed821db236d8fab281";
pub const HIGHS_BUILD_CONFIG: &str =
    "cmake:CMAKE_BUILD_TYPE=Release;BUILD_SHARED_LIBS=ON;FAST_BUILD=ON";

pub fn highs_source_mode() -> &'static str {
    option_env!("FERROX_HIGHS_SOURCE_MODE").unwrap_or("native-link-disabled")
}

pub fn highs_source_commit() -> &'static str {
    option_env!("FERROX_HIGHS_SOURCE_COMMIT").unwrap_or("native-link-disabled")
}

// ── Status ────────────────────────────────────────────────────────────────────

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighsReturnStatus {
    Ok = 0,
    Warning = 1,
    Error = 2,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighsModelStatus {
    NotSet = 0,
    LoadError = 1,
    ModelError = 2,
    Infeasible = 8,
    Optimal = 7,
    Unbounded = 9,
    SolutionLimit = 11,
    TimeLimit = 12,
}

impl HighsModelStatus {
    pub fn is_success(self) -> bool {
        self == Self::Optimal
    }
    pub fn is_optimal(self) -> bool {
        self == Self::Optimal
    }
    pub fn may_have_incumbent(self) -> bool {
        matches!(self, Self::Optimal | Self::SolutionLimit | Self::TimeLimit)
    }
}

// ── Opaque C type ─────────────────────────────────────────────────────────────

#[repr(C)]
pub struct HighsHandle {
    _p: [u8; 0],
}

// ── FFI declarations ──────────────────────────────────────────────────────────

#[cfg(feature = "link")]
unsafe extern "C" {
    pub fn highs_create() -> *mut HighsHandle;
    pub fn highs_destroy(h: *mut HighsHandle);
    pub fn highs_add_col(h: *mut HighsHandle, cost: f64, lb: f64, ub: f64) -> HighsReturnStatus;
    pub fn highs_add_row(
        h: *mut HighsHandle,
        lb: f64,
        ub: f64,
        num_nz: c_int,
        idx: *const c_int,
        val: *const f64,
    ) -> HighsReturnStatus;
    pub fn highs_change_col_integer_type(
        h: *mut HighsHandle,
        col: c_int,
        is_integer: c_int,
    ) -> HighsReturnStatus;
    pub fn highs_set_time_limit(h: *mut HighsHandle, seconds: f64) -> HighsReturnStatus;
    pub fn highs_set_mip_rel_gap(h: *mut HighsHandle, gap: f64) -> HighsReturnStatus;
    pub fn highs_run(h: *mut HighsHandle) -> HighsReturnStatus;
    pub fn highs_get_model_status(h: *mut HighsHandle) -> HighsModelStatus;
    pub fn highs_get_objective_value(h: *mut HighsHandle) -> f64;
    pub fn highs_get_col_value(h: *mut HighsHandle, col: c_int) -> f64;
    pub fn highs_get_mip_gap(h: *mut HighsHandle) -> f64;
    pub fn highs_get_col_value_checked(
        h: *mut HighsHandle,
        col: c_int,
        out: *mut f64,
    ) -> HighsReturnStatus;
    pub fn highs_get_mip_gap_checked(h: *mut HighsHandle, out: *mut f64) -> HighsReturnStatus;
    pub fn highs_solution_value_valid(h: *mut HighsHandle) -> c_int;
}

// ── Safe wrapper ──────────────────────────────────────────────────────────────

#[cfg(feature = "link")]
pub mod safe {
    #[allow(clippy::wildcard_imports)]
    use super::*;
    use std::fmt;
    use std::ptr::NonNull;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum HighsError {
        InvalidInput(String),
        NativeCallFailed(&'static str),
        MissingSolution(&'static str),
    }

    impl fmt::Display for HighsError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::InvalidInput(message) => write!(f, "invalid HiGHS input: {message}"),
                Self::NativeCallFailed(op) => write!(f, "HiGHS native call failed: {op}"),
                Self::MissingSolution(op) => write!(f, "HiGHS solution unavailable: {op}"),
            }
        }
    }

    impl std::error::Error for HighsError {}

    pub type Result<T> = std::result::Result<T, HighsError>;

    pub struct HighsSolver {
        ptr: NonNull<HighsHandle>,
        num_cols: i32,
    }

    impl HighsSolver {
        pub fn new() -> Self {
            unsafe {
                Self {
                    ptr: NonNull::new(highs_create()).expect("highs_create returned null"),
                    num_cols: 0,
                }
            }
        }

        /// Add a continuous variable. Returns the column index.
        pub fn add_col(&mut self, cost: f64, lb: f64, ub: f64) -> i32 {
            self.try_add_col(cost, lb, ub)
                .expect("highs_add_col failed")
        }

        /// Add a continuous variable. Returns the column index.
        pub fn try_add_col(&mut self, cost: f64, lb: f64, ub: f64) -> Result<i32> {
            validate_col(cost, lb, ub)?;
            ensure_ok(
                unsafe { highs_add_col(self.ptr.as_ptr(), cost, lb, ub) },
                "highs_add_col",
            )?;
            let idx = self.num_cols;
            self.num_cols += 1;
            Ok(idx)
        }

        /// Add an integer variable. Returns the column index.
        pub fn add_int_col(&mut self, cost: f64, lb: f64, ub: f64) -> i32 {
            self.try_add_int_col(cost, lb, ub)
                .expect("highs_add_int_col failed")
        }

        /// Add an integer variable. Returns the column index.
        pub fn try_add_int_col(&mut self, cost: f64, lb: f64, ub: f64) -> Result<i32> {
            let col = self.try_add_col(cost, lb, ub)?;
            ensure_ok(
                unsafe { highs_change_col_integer_type(self.ptr.as_ptr(), col, 1) },
                "highs_change_col_integer_type",
            )?;
            Ok(col)
        }

        /// Add a binary (0/1) variable. Returns the column index.
        pub fn add_bin_col(&mut self, cost: f64) -> i32 {
            self.try_add_bin_col(cost)
                .expect("highs_add_bin_col failed")
        }

        /// Add a binary (0/1) variable. Returns the column index.
        pub fn try_add_bin_col(&mut self, cost: f64) -> Result<i32> {
            self.try_add_int_col(cost, 0.0, 1.0)
        }

        /// Add a row constraint: lb <= sum(idx[i] * val[i]) <= ub
        pub fn add_row(&mut self, lb: f64, ub: f64, indices: &[i32], coeffs: &[f64]) {
            self.try_add_row(lb, ub, indices, coeffs)
                .expect("highs_add_row failed");
        }

        /// Add a row constraint: lb <= sum(idx[i] * val[i]) <= ub
        pub fn try_add_row(
            &mut self,
            lb: f64,
            ub: f64,
            indices: &[i32],
            coeffs: &[f64],
        ) -> Result<()> {
            validate_row(lb, ub, indices, coeffs, self.num_cols)?;
            let nnz = i32::try_from(indices.len()).map_err(|_| {
                HighsError::InvalidInput("row non-zero count exceeds i32".to_string())
            })?;
            ensure_ok(
                unsafe {
                    highs_add_row(
                        self.ptr.as_ptr(),
                        lb,
                        ub,
                        nnz,
                        indices.as_ptr(),
                        coeffs.as_ptr(),
                    )
                },
                "highs_add_row",
            )
        }

        pub fn set_time_limit(&mut self, secs: f64) {
            self.try_set_time_limit(secs)
                .expect("highs_set_time_limit failed");
        }

        pub fn try_set_time_limit(&mut self, secs: f64) -> Result<()> {
            if !secs.is_finite() || secs <= 0.0 {
                return Err(HighsError::InvalidInput(
                    "time limit must be finite and > 0".to_string(),
                ));
            }
            ensure_ok(
                unsafe { highs_set_time_limit(self.ptr.as_ptr(), secs) },
                "highs_set_time_limit",
            )
        }

        pub fn set_mip_rel_gap(&mut self, gap: f64) {
            self.try_set_mip_rel_gap(gap)
                .expect("highs_set_mip_rel_gap failed");
        }

        pub fn try_set_mip_rel_gap(&mut self, gap: f64) -> Result<()> {
            if !gap.is_finite() || gap < 0.0 {
                return Err(HighsError::InvalidInput(
                    "MIP relative gap must be finite and >= 0".to_string(),
                ));
            }
            ensure_ok(
                unsafe { highs_set_mip_rel_gap(self.ptr.as_ptr(), gap) },
                "highs_set_mip_rel_gap",
            )
        }

        pub fn run(&mut self) -> HighsModelStatus {
            self.try_run().expect("highs_run failed")
        }

        pub fn try_run(&mut self) -> Result<HighsModelStatus> {
            ensure_ok(unsafe { highs_run(self.ptr.as_ptr()) }, "highs_run")?;
            Ok(unsafe { highs_get_model_status(self.ptr.as_ptr()) })
        }

        pub fn objective_value(&self) -> f64 {
            unsafe { highs_get_objective_value(self.ptr.as_ptr()) }
        }

        pub fn col_value(&self, col: i32) -> f64 {
            self.try_col_value(col).expect("highs_get_col_value failed")
        }

        pub fn try_col_value(&self, col: i32) -> Result<f64> {
            if col < 0 || col >= self.num_cols {
                return Err(HighsError::InvalidInput(format!(
                    "column index {col} is out of bounds for {} columns",
                    self.num_cols
                )));
            }

            let mut value = 0.0;
            ensure_ok(
                unsafe {
                    highs_get_col_value_checked(
                        self.ptr.as_ptr(),
                        col,
                        std::ptr::addr_of_mut!(value),
                    )
                },
                "highs_get_col_value",
            )?;
            Ok(value)
        }

        pub fn mip_gap(&self) -> f64 {
            self.try_mip_gap().expect("highs_get_mip_gap failed")
        }

        pub fn try_mip_gap(&self) -> Result<f64> {
            let mut value = 1.0;
            ensure_ok(
                unsafe {
                    highs_get_mip_gap_checked(self.ptr.as_ptr(), std::ptr::addr_of_mut!(value))
                },
                "highs_get_mip_gap",
            )?;
            Ok(value)
        }

        pub fn has_solution(&self) -> bool {
            unsafe { highs_solution_value_valid(self.ptr.as_ptr()) != 0 }
        }
    }

    impl Default for HighsSolver {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Drop for HighsSolver {
        fn drop(&mut self) {
            unsafe { highs_destroy(self.ptr.as_ptr()) }
        }
    }

    fn ensure_ok(status: HighsReturnStatus, operation: &'static str) -> Result<()> {
        if status == HighsReturnStatus::Ok {
            Ok(())
        } else {
            Err(HighsError::NativeCallFailed(operation))
        }
    }

    fn validate_col(cost: f64, lb: f64, ub: f64) -> Result<()> {
        if !cost.is_finite() {
            return Err(HighsError::InvalidInput(
                "objective coefficient must be finite".to_string(),
            ));
        }
        validate_bounds(lb, ub, "column")
    }

    fn validate_row(
        lb: f64,
        ub: f64,
        indices: &[i32],
        coeffs: &[f64],
        num_cols: i32,
    ) -> Result<()> {
        validate_bounds(lb, ub, "row")?;
        if indices.len() != coeffs.len() {
            return Err(HighsError::InvalidInput(
                "row indices and coefficients must have the same length".to_string(),
            ));
        }
        for &idx in indices {
            if idx < 0 || idx >= num_cols {
                return Err(HighsError::InvalidInput(format!(
                    "row references column {idx}, but model has {num_cols} columns"
                )));
            }
        }
        if coeffs.iter().any(|coeff| !coeff.is_finite()) {
            return Err(HighsError::InvalidInput(
                "row coefficients must be finite".to_string(),
            ));
        }
        Ok(())
    }

    fn validate_bounds(lb: f64, ub: f64, label: &'static str) -> Result<()> {
        if lb.is_nan() || ub.is_nan() {
            return Err(HighsError::InvalidInput(format!(
                "{label} bounds must not be NaN"
            )));
        }
        if lb > ub {
            return Err(HighsError::InvalidInput(format!(
                "{label} lower bound > upper bound"
            )));
        }
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_values() {
        assert_eq!(HighsModelStatus::Optimal as i32, 7);
        assert!(HighsModelStatus::Optimal.is_success());
        assert!(!HighsModelStatus::TimeLimit.is_success());
        assert!(HighsModelStatus::TimeLimit.may_have_incumbent());
        assert!(!HighsModelStatus::Infeasible.is_success());
    }

    #[cfg(feature = "link")]
    mod integration {
        use super::super::safe::*;

        #[test]
        fn mip_binary_knapsack() {
            // Maximize 5x + 4y + 3z  subject to  2x + 3y + 2z <= 5,  x,y,z in {0,1}
            let mut s = HighsSolver::new();
            let x = s.add_bin_col(-5.0); // minimize negated costs
            let y = s.add_bin_col(-4.0);
            let z = s.add_bin_col(-3.0);
            s.add_row(f64::NEG_INFINITY, 5.0, &[x, y, z], &[2.0, 3.0, 2.0]);
            let status = s.run();
            assert!(status.is_success());
            // Optimal: x=1,y=1 → capacity=5, value=9 (beats x=1,z=1 → value=8)
            assert!((-s.objective_value() - 9.0).abs() < 0.5);
        }
    }
}
