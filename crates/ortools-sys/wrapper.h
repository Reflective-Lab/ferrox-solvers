#pragma once
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    ORTOOLS_UNKNOWN       = 0,
    ORTOOLS_OPTIMAL       = 1,
    ORTOOLS_FEASIBLE      = 2,
    ORTOOLS_INFEASIBLE    = 3,
    ORTOOLS_UNBOUNDED     = 4,
    ORTOOLS_MODEL_INVALID = 5,
    ORTOOLS_ERROR         = 6,
} OrtoolsStatus;

typedef enum {
    MIN_COST_FLOW_NOT_SOLVED         = 0,
    MIN_COST_FLOW_OPTIMAL            = 1,
    MIN_COST_FLOW_FEASIBLE           = 2,
    MIN_COST_FLOW_INFEASIBLE         = 3,
    MIN_COST_FLOW_UNBALANCED         = 4,
    MIN_COST_FLOW_BAD_RESULT         = 5,
    MIN_COST_FLOW_BAD_COST_RANGE     = 6,
    MIN_COST_FLOW_BAD_CAPACITY_RANGE = 7,
    MIN_COST_FLOW_ERROR              = 8,
} MinCostFlowStatus;

/* ── CP-SAT ─────────────────────────────────────────────────────────────── */
typedef struct CpModelBuilder   CpModelBuilder;
typedef struct CpSolverResponse CpSolverResponse;

CpModelBuilder*   cpmodel_new(void);
void              cpmodel_free(CpModelBuilder* m);
int32_t           cpmodel_new_int_var(CpModelBuilder* m, int64_t lb, int64_t ub, const char* name);
int32_t           cpmodel_new_bool_var(CpModelBuilder* m, const char* name);
void              cpmodel_add_linear_le(CpModelBuilder* m, const int32_t* idx,
                                        const int64_t* c, size_t n, int64_t rhs);
void              cpmodel_add_linear_ge(CpModelBuilder* m, const int32_t* idx,
                                        const int64_t* c, size_t n, int64_t rhs);
void              cpmodel_add_linear_eq(CpModelBuilder* m, const int32_t* idx,
                                        const int64_t* c, size_t n, int64_t rhs);
void              cpmodel_add_all_different(CpModelBuilder* m, const int32_t* idx, size_t n);
void              cpmodel_add_bool_or(CpModelBuilder* m, const int32_t* lits, size_t n);
void              cpmodel_add_bool_and(CpModelBuilder* m, const int32_t* lits, size_t n);
void              cpmodel_add_bool_xor(CpModelBuilder* m, const int32_t* lits, size_t n);
void              cpmodel_add_implication(CpModelBuilder* m, int32_t lhs, int32_t rhs);
void              cpmodel_add_at_most_one(CpModelBuilder* m, const int32_t* lits, size_t n);
void              cpmodel_add_exactly_one(CpModelBuilder* m, const int32_t* lits, size_t n);
void              cpmodel_add_allowed_assignments(CpModelBuilder* m, const int32_t* idx,
                                                 size_t var_count, const int64_t* tuples,
                                                 size_t tuple_count);
int32_t           cpmodel_new_interval_var(CpModelBuilder* m, int32_t start, int64_t size,
                                           int32_t end, const char* name);
int32_t           cpmodel_new_optional_interval_var(CpModelBuilder* m, int32_t start,
                                                    int64_t size, int32_t end, int32_t lit,
                                                    const char* name);
void              cpmodel_add_circuit(CpModelBuilder* m, const int32_t* tails,
                                      const int32_t* heads, const int32_t* lits, size_t n);
void              cpmodel_add_no_overlap(CpModelBuilder* m, const int32_t* idx, size_t n);
void              cpmodel_add_cumulative(CpModelBuilder* m, const int32_t* intervals,
                                         const int64_t* demands, size_t n, int64_t capacity);
void              cpmodel_add_no_overlap_2d(CpModelBuilder* m, const int32_t* x_intervals,
                                            const int32_t* y_intervals, size_t n);
void              cpmodel_minimize(CpModelBuilder* m, const int32_t* idx,
                                   const int64_t* c, size_t n);
void              cpmodel_maximize(CpModelBuilder* m, const int32_t* idx,
                                   const int64_t* c, size_t n);
CpSolverResponse* cpmodel_solve(CpModelBuilder* m, double time_limit);
OrtoolsStatus     cpresponse_status(const CpSolverResponse* r);
int64_t           cpresponse_objective_value(const CpSolverResponse* r);
int64_t           cpresponse_value(const CpSolverResponse* r, int32_t var_index);
double            cpresponse_wall_time(const CpSolverResponse* r);
void              cpresponse_free(CpSolverResponse* r);

/* ── GLOP / MP linear solver ─────────────────────────────────────────────── */
typedef enum { LP_GLOP = 0 } LpSolverType;
typedef struct MpSolver MpSolver;

MpSolver*     mpsolver_new(const char* name, LpSolverType type);
void          mpsolver_free(MpSolver* s);
int32_t       mpsolver_num_var(MpSolver* s, double lb, double ub, const char* name);
int32_t       mpsolver_int_var(MpSolver* s, double lb, double ub, const char* name);
int32_t       mpsolver_bool_var(MpSolver* s, const char* name);
int32_t       mpsolver_add_constraint(MpSolver* s, double lb, double ub, const char* name);
void          mpsolver_set_constraint_coeff(MpSolver* s, int32_t ci, int32_t vi, double coeff);
void          mpsolver_set_objective_coeff(MpSolver* s, int32_t vi, double coeff);
void          mpsolver_minimize(MpSolver* s);
void          mpsolver_maximize(MpSolver* s);
OrtoolsStatus mpsolver_solve(MpSolver* s);
double        mpsolver_objective_value(const MpSolver* s);
double        mpsolver_var_value(const MpSolver* s, int32_t vi);

/* ── SimpleMinCostFlow ───────────────────────────────────────────────────── */
typedef struct MinCostFlow MinCostFlow;

MinCostFlow*      mincostflow_new(int32_t reserve_num_nodes, int32_t reserve_num_arcs);
void              mincostflow_free(MinCostFlow* f);
int32_t           mincostflow_add_arc(MinCostFlow* f, int32_t tail, int32_t head,
                                      int64_t capacity, int64_t unit_cost);
void              mincostflow_set_node_supply(MinCostFlow* f, int32_t node, int64_t supply);
MinCostFlowStatus mincostflow_solve(MinCostFlow* f);
MinCostFlowStatus mincostflow_solve_max_flow_with_min_cost(MinCostFlow* f);
int64_t           mincostflow_optimal_cost(const MinCostFlow* f);
int64_t           mincostflow_maximum_flow(const MinCostFlow* f);
int64_t           mincostflow_flow(const MinCostFlow* f, int32_t arc);
int32_t           mincostflow_num_arcs(const MinCostFlow* f);
int32_t           mincostflow_tail(const MinCostFlow* f, int32_t arc);
int32_t           mincostflow_head(const MinCostFlow* f, int32_t arc);
int64_t           mincostflow_capacity(const MinCostFlow* f, int32_t arc);
int64_t           mincostflow_unit_cost(const MinCostFlow* f, int32_t arc);

#ifdef __cplusplus
}
#endif
