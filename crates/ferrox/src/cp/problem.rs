use serde::{Deserialize, Serialize};

/// A single integer variable in a CP-SAT model.
/// Set `is_bool = true` for binary (0/1) variables that may serve as
/// optional-interval literals; the solver treats them as `BoolVar` internally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpVariable {
    pub name: String,
    pub lb: i64,
    pub ub: i64,
    #[serde(default)]
    pub is_bool: bool,
}

/// A fixed-duration interval variable.  The solver enforces `end == start + duration`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntervalVarDef {
    pub name: String,
    pub start_var: String,
    pub duration: i64,
    pub end_var: String,
}

/// An optional interval that is active only when `lit_var` (a bool variable) equals 1.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionalIntervalVarDef {
    pub name: String,
    pub start_var: String,
    pub duration: i64,
    pub end_var: String,
    pub lit_var: String,
}

/// One term in a linear expression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpTerm {
    pub var: String,
    pub coeff: i64,
}

/// A Boolean literal in a CP-SAT model.
///
/// `negated = true` represents `not var`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpBoolLiteral {
    pub var: String,
    #[serde(default)]
    pub negated: bool,
}

/// One fixed-demand task in a cumulative resource constraint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CumulativeDemand {
    pub interval: String,
    pub demand: i64,
}

/// One rectangle in a two-dimensional no-overlap constraint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoOverlap2DRectangle {
    pub x_interval: String,
    pub y_interval: String,
}

/// A constraint over variables.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConstraintKind {
    LinearLe {
        terms: Vec<CpTerm>,
        rhs: i64,
    },
    LinearGe {
        terms: Vec<CpTerm>,
        rhs: i64,
    },
    LinearEq {
        terms: Vec<CpTerm>,
        rhs: i64,
    },
    AllDifferent {
        vars: Vec<String>,
    },
    /// At least one literal must be true.
    BoolOr {
        literals: Vec<CpBoolLiteral>,
    },
    /// Every literal must be true.
    BoolAnd {
        literals: Vec<CpBoolLiteral>,
    },
    /// An odd number of literals must be true.
    BoolXor {
        literals: Vec<CpBoolLiteral>,
    },
    /// If `antecedent` is true, `consequent` must be true.
    Implication {
        antecedent: CpBoolLiteral,
        consequent: CpBoolLiteral,
    },
    /// At most one literal may be true.
    AtMostOne {
        literals: Vec<CpBoolLiteral>,
    },
    /// Exactly one literal must be true.
    ExactlyOne {
        literals: Vec<CpBoolLiteral>,
    },
    /// The listed variables must take one of the allowed tuples.
    AllowedAssignments {
        vars: Vec<String>,
        tuples: Vec<Vec<i64>>,
    },
    /// None of the listed interval variables may overlap in time.
    NoOverlap {
        intervals: Vec<String>,
    },
    /// Intervals consume a fixed amount of a cumulative resource.
    Cumulative {
        demands: Vec<CumulativeDemand>,
        capacity: i64,
    },
    /// Rectangles, represented by x/y interval pairs, may not overlap.
    NoOverlap2D {
        rectangles: Vec<NoOverlap2DRectangle>,
    },
}

/// Seeded into `ContextKey::Seeds` with id prefix `"cpsat-request:"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpSatRequest {
    pub id: String,
    pub variables: Vec<CpVariable>,
    #[serde(default)]
    pub interval_vars: Vec<IntervalVarDef>,
    #[serde(default)]
    pub optional_interval_vars: Vec<OptionalIntervalVarDef>,
    pub constraints: Vec<ConstraintKind>,
    pub objective_terms: Option<Vec<CpTerm>>,
    pub minimize: bool,
    pub time_limit_seconds: Option<f64>,
}

/// Written to `ContextKey::Strategies` with id prefix `"cpsat-plan:"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpSatPlan {
    pub request_id: String,
    pub status: String,
    pub assignments: Vec<(String, i64)>,
    pub objective_value: Option<i64>,
    pub wall_time_seconds: f64,
    pub solver: String,
}
