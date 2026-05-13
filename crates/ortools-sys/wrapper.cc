// Intentionally does NOT include wrapper.h — the C typedef for CpModelBuilder
// would collide with operations_research::sat::CpModelBuilder from cp_model.h.
// The extern "C" block here redeclares all exported functions with their
// concrete struct types.
//
// IntVar(int, CpModelBuilder*) is private in v9.15, so we cannot reconstruct
// vars from indices alone. Instead the storage struct caches every IntVar/BoolVar
// (as IntVar) keyed by its proto index so make_var() can look them up.

#include <cstring>
#include <memory>
#include <thread>
#include <unordered_map>
#include <vector>

#include "ortools/sat/cp_model.h"
#include "ortools/sat/cp_model_solver.h"
#include "ortools/sat/sat_parameters.pb.h"
#include "ortools/linear_solver/linear_solver.h"
#include "ortools/graph/min_cost_flow.h"

// ── Internal storage types ────────────────────────────────────────────────────

struct CpModelBuilder {
    operations_research::sat::CpModelBuilder builder;
    // Every variable (int or bool) stored as IntVar, keyed by proto index.
    std::unordered_map<int32_t, operations_research::sat::IntVar> vars;
    // Bool vars kept separately so NewOptionalIntervalVar can receive a BoolVar.
    std::unordered_map<int32_t, operations_research::sat::BoolVar> bools;
    // Interval vars keyed by their proto index.
    std::unordered_map<int32_t, operations_research::sat::IntervalVar> intervals;
};

struct CpSolverResponse {
    operations_research::sat::CpSolverResponse response;
};

struct MpSolver {
    std::unique_ptr<operations_research::MPSolver> solver;
    std::vector<operations_research::MPVariable*>   vars;
    std::vector<operations_research::MPConstraint*> constraints;
};

struct MinCostFlow {
    operations_research::SimpleMinCostFlow solver;

    MinCostFlow(int32_t reserve_num_nodes, int32_t reserve_num_arcs)
        : solver(reserve_num_nodes, reserve_num_arcs) {}
};

// ── Helpers ───────────────────────────────────────────────────────────────────

static operations_research::sat::IntVar make_var(int32_t idx, const CpModelBuilder* m) {
    return m->vars.at(idx);
}

static int32_t negated_lit_ref(int32_t lit) {
    return -lit - 1;
}

static operations_research::sat::LinearExpr make_expr(
    const CpModelBuilder* m, const int32_t* idx, const int64_t* c, size_t n)
{
    std::vector<operations_research::sat::IntVar> vars;
    std::vector<int64_t> coeffs(c, c + n);
    vars.reserve(n);
    for (size_t i = 0; i < n; ++i)
        vars.push_back(make_var(idx[i], m));
    return operations_research::sat::LinearExpr::WeightedSum(vars, coeffs);
}

static int32_t map_min_cost_flow_status(operations_research::MinCostFlowBase::Status status) {
    using S = operations_research::MinCostFlowBase;
    switch (status) {
        case S::NOT_SOLVED:          return 0;
        case S::OPTIMAL:             return 1;
        case S::FEASIBLE:            return 2;
        case S::INFEASIBLE:          return 3;
        case S::UNBALANCED:          return 4;
        case S::BAD_RESULT:          return 5;
        case S::BAD_COST_RANGE:      return 6;
        case S::BAD_CAPACITY_RANGE:  return 7;
        default:                     return 8;
    }
}

// ── CP-SAT extern "C" ─────────────────────────────────────────────────────────

extern "C" {

CpModelBuilder* cpmodel_new() {
    return new CpModelBuilder{};
}

void cpmodel_free(CpModelBuilder* m) {
    delete m;
}

int32_t cpmodel_new_int_var(CpModelBuilder* m, int64_t lb, int64_t ub, const char* name) {
    auto v = m->builder.NewIntVar(operations_research::Domain(lb, ub));
    if (name && name[0]) v = v.WithName(name);
    int32_t idx = v.index();
    m->vars[idx] = v;
    return idx;
}

int32_t cpmodel_new_bool_var(CpModelBuilder* m, const char* name) {
    auto bv = m->builder.NewBoolVar();
    if (name && name[0]) bv = bv.WithName(name);
    int32_t idx = bv.index();
    // BoolVar is implicitly convertible to IntVar (takes value 0 or 1)
    m->vars[idx] = operations_research::sat::IntVar(bv);
    // Also store as BoolVar for optional interval creation.
    m->bools[idx] = bv;
    return idx;
}

void cpmodel_add_linear_le(CpModelBuilder* m, const int32_t* idx,
                            const int64_t* c, size_t n, int64_t rhs) {
    m->builder.AddLessOrEqual(make_expr(m, idx, c, n), rhs);
}

void cpmodel_add_linear_ge(CpModelBuilder* m, const int32_t* idx,
                            const int64_t* c, size_t n, int64_t rhs) {
    m->builder.AddGreaterOrEqual(make_expr(m, idx, c, n), rhs);
}

void cpmodel_add_linear_eq(CpModelBuilder* m, const int32_t* idx,
                            const int64_t* c, size_t n, int64_t rhs) {
    m->builder.AddEquality(make_expr(m, idx, c, n), rhs);
}

void cpmodel_add_all_different(CpModelBuilder* m, const int32_t* idx, size_t n) {
    std::vector<operations_research::sat::IntVar> vars;
    vars.reserve(n);
    for (size_t i = 0; i < n; ++i)
        vars.push_back(make_var(idx[i], m));
    m->builder.AddAllDifferent(vars);
}

void cpmodel_add_bool_or(CpModelBuilder* m, const int32_t* lits, size_t n) {
    auto* proto = m->builder.MutableProto()->add_constraints();
    auto* bool_or = proto->mutable_bool_or();
    for (size_t i = 0; i < n; ++i)
        bool_or->add_literals(lits[i]);
}

void cpmodel_add_bool_and(CpModelBuilder* m, const int32_t* lits, size_t n) {
    auto* proto = m->builder.MutableProto()->add_constraints();
    auto* bool_and = proto->mutable_bool_and();
    for (size_t i = 0; i < n; ++i)
        bool_and->add_literals(lits[i]);
}

void cpmodel_add_bool_xor(CpModelBuilder* m, const int32_t* lits, size_t n) {
    auto* proto = m->builder.MutableProto()->add_constraints();
    auto* bool_xor = proto->mutable_bool_xor();
    for (size_t i = 0; i < n; ++i)
        bool_xor->add_literals(lits[i]);
}

void cpmodel_add_implication(CpModelBuilder* m, int32_t lhs, int32_t rhs) {
    auto* proto = m->builder.MutableProto()->add_constraints();
    auto* bool_or = proto->mutable_bool_or();
    bool_or->add_literals(negated_lit_ref(lhs));
    bool_or->add_literals(rhs);
}

void cpmodel_add_at_most_one(CpModelBuilder* m, const int32_t* lits, size_t n) {
    auto* proto = m->builder.MutableProto()->add_constraints();
    auto* at_most_one = proto->mutable_at_most_one();
    for (size_t i = 0; i < n; ++i)
        at_most_one->add_literals(lits[i]);
}

void cpmodel_add_exactly_one(CpModelBuilder* m, const int32_t* lits, size_t n) {
    auto* proto = m->builder.MutableProto()->add_constraints();
    auto* exactly_one = proto->mutable_exactly_one();
    for (size_t i = 0; i < n; ++i)
        exactly_one->add_literals(lits[i]);
}

void cpmodel_add_allowed_assignments(CpModelBuilder* m, const int32_t* idx,
                                      size_t var_count, const int64_t* tuples,
                                      size_t tuple_count) {
    std::vector<operations_research::sat::IntVar> vars;
    vars.reserve(var_count);
    for (size_t i = 0; i < var_count; ++i)
        vars.push_back(make_var(idx[i], m));

    auto table = m->builder.AddAllowedAssignments(vars);
    for (size_t t = 0; t < tuple_count; ++t) {
        const int64_t* begin = tuples + t * var_count;
        std::vector<int64_t> tuple(begin, begin + var_count);
        table.AddTuple(tuple);
    }
}

void cpmodel_minimize(CpModelBuilder* m, const int32_t* idx, const int64_t* c, size_t n) {
    m->builder.Minimize(make_expr(m, idx, c, n));
}

void cpmodel_maximize(CpModelBuilder* m, const int32_t* idx, const int64_t* c, size_t n) {
    m->builder.Maximize(make_expr(m, idx, c, n));
}

CpSolverResponse* cpmodel_solve(CpModelBuilder* m, double time_limit) {
    operations_research::sat::SatParameters params;
    if (time_limit > 0.0) params.set_max_time_in_seconds(time_limit);
    // Use all available hardware threads for large-neighbourhood search.
    params.set_num_search_workers(std::max(1u, std::thread::hardware_concurrency()));
    auto* r = new CpSolverResponse{};
    r->response = operations_research::sat::SolveWithParameters(m->builder.Build(), params);
    return r;
}

// Status enum values must match OrtoolsStatus in lib.rs:
//   Unknown=0, Optimal=1, Feasible=2, Infeasible=3, Unbounded=4, ModelInvalid=5, Error=6
// CP-SAT proto uses different values (OPTIMAL=4, FEASIBLE=2, etc.), so we map here.
int32_t cpresponse_status(const CpSolverResponse* r) {
    using S = operations_research::sat::CpSolverStatus;
    switch (r->response.status()) {
        case S::OPTIMAL:        return 1;
        case S::FEASIBLE:       return 2;
        case S::INFEASIBLE:     return 3;
        case S::MODEL_INVALID:  return 5;
        default:                return 0; // UNKNOWN
    }
}

int64_t cpresponse_objective_value(const CpSolverResponse* r) {
    return static_cast<int64_t>(r->response.objective_value());
}

// Direct proto access — no need to reconstruct IntVar to read a value.
int64_t cpresponse_value(const CpSolverResponse* r, int32_t var_index) {
    return r->response.solution(var_index);
}

double cpresponse_wall_time(const CpSolverResponse* r) {
    return r->response.wall_time();
}

void cpresponse_free(CpSolverResponse* r) {
    delete r;
}

// ── Interval vars + NoOverlap ────────────────────────────────────────────────

// Fixed-duration interval: solver enforces end == start + size.
int32_t cpmodel_new_interval_var(CpModelBuilder* m, int32_t start_idx,
                                  int64_t size, int32_t end_idx,
                                  const char* /*name*/) {
    auto start = make_var(start_idx, m);
    auto end   = make_var(end_idx,   m);
    auto iv = m->builder.NewIntervalVar(start, size, end);
    int32_t idx = iv.index();
    m->intervals[idx] = iv;
    return idx;
}

// Optional interval: active only when `bool_idx` variable is 1.
int32_t cpmodel_new_optional_interval_var(CpModelBuilder* m, int32_t start_idx,
                                           int64_t size, int32_t end_idx,
                                           int32_t bool_idx, const char* /*name*/) {
    auto start = make_var(start_idx, m);
    auto end   = make_var(end_idx,   m);
    auto lit   = m->bools.at(bool_idx);
    auto iv = m->builder.NewOptionalIntervalVar(start, size, end, lit);
    int32_t idx = iv.index();
    m->intervals[idx] = iv;
    return idx;
}

// Circuit (Hamiltonian): exactly the arcs whose literal is 1 form a circuit
// covering all visited nodes.  Pass self-loop arcs (tail == head) to make a
// node optional — if its self-loop literal is 1 the node is skipped.
void cpmodel_add_circuit(CpModelBuilder* m,
                          const int32_t* tails, const int32_t* heads,
                          const int32_t* lits, size_t n) {
    auto circuit = m->builder.AddCircuitConstraint();
    for (size_t i = 0; i < n; ++i)
        circuit.AddArc(tails[i], heads[i], m->bools.at(lits[i]));
}

// NoOverlap: no two of the listed intervals may overlap in time.
void cpmodel_add_no_overlap(CpModelBuilder* m, const int32_t* idx, size_t n) {
    std::vector<operations_research::sat::IntervalVar> ivs;
    ivs.reserve(n);
    for (size_t i = 0; i < n; ++i)
        ivs.push_back(m->intervals.at(idx[i]));
    m->builder.AddNoOverlap(ivs);
}

// Cumulative: at every integer point, active interval demands must not exceed capacity.
void cpmodel_add_cumulative(CpModelBuilder* m, const int32_t* intervals,
                             const int64_t* demands, size_t n, int64_t capacity) {
    auto cumulative = m->builder.AddCumulative(capacity);
    for (size_t i = 0; i < n; ++i)
        cumulative.AddDemand(m->intervals.at(intervals[i]), demands[i]);
}

// NoOverlap2D: no pair of x/y rectangles may overlap on both axes.
void cpmodel_add_no_overlap_2d(CpModelBuilder* m, const int32_t* x_intervals,
                                const int32_t* y_intervals, size_t n) {
    auto no_overlap_2d = m->builder.AddNoOverlap2D();
    for (size_t i = 0; i < n; ++i)
        no_overlap_2d.AddRectangle(m->intervals.at(x_intervals[i]),
                                   m->intervals.at(y_intervals[i]));
}

// ── GLOP ──────────────────────────────────────────────────────────────────────

// LP_GLOP = 0, matching LpSolverType in lib.rs
MpSolver* mpsolver_new(const char* /*name*/, int /*type*/) {
    auto* s = new MpSolver{};
    s->solver.reset(operations_research::MPSolver::CreateSolver("GLOP"));
    if (!s->solver) { delete s; return nullptr; }
    return s;
}

void mpsolver_free(MpSolver* s) { delete s; }

int32_t mpsolver_num_var(MpSolver* s, double lb, double ub, const char* name) {
    auto* v = s->solver->MakeNumVar(lb, ub, name ? name : "");
    auto idx = static_cast<int32_t>(s->vars.size());
    s->vars.push_back(v);
    return idx;
}

int32_t mpsolver_int_var(MpSolver* s, double lb, double ub, const char* name) {
    auto* v = s->solver->MakeIntVar(lb, ub, name ? name : "");
    auto idx = static_cast<int32_t>(s->vars.size());
    s->vars.push_back(v);
    return idx;
}

int32_t mpsolver_bool_var(MpSolver* s, const char* name) {
    auto* v = s->solver->MakeBoolVar(name ? name : "");
    auto idx = static_cast<int32_t>(s->vars.size());
    s->vars.push_back(v);
    return idx;
}

int32_t mpsolver_add_constraint(MpSolver* s, double lb, double ub, const char* name) {
    auto* c = s->solver->MakeRowConstraint(lb, ub, name ? name : "");
    auto idx = static_cast<int32_t>(s->constraints.size());
    s->constraints.push_back(c);
    return idx;
}

void mpsolver_set_constraint_coeff(MpSolver* s, int32_t ci, int32_t vi, double coeff) {
    s->constraints[ci]->SetCoefficient(s->vars[vi], coeff);
}

void mpsolver_set_objective_coeff(MpSolver* s, int32_t vi, double coeff) {
    s->solver->MutableObjective()->SetCoefficient(s->vars[vi], coeff);
}

void mpsolver_minimize(MpSolver* s) { s->solver->MutableObjective()->SetMinimization(); }
void mpsolver_maximize(MpSolver* s) { s->solver->MutableObjective()->SetMaximization(); }

int32_t mpsolver_solve(MpSolver* s) {
    // Return values match OrtoolsStatus: Optimal=1, Feasible=2, Infeasible=3, ...
    switch (s->solver->Solve()) {
        case operations_research::MPSolver::OPTIMAL:    return 1;
        case operations_research::MPSolver::FEASIBLE:   return 2;
        case operations_research::MPSolver::INFEASIBLE: return 3;
        case operations_research::MPSolver::UNBOUNDED:  return 4;
        default:                                        return 6; // Error
    }
}

double mpsolver_objective_value(const MpSolver* s) {
    return s->solver->Objective().Value();
}

double mpsolver_var_value(const MpSolver* s, int32_t vi) {
    return s->vars[vi]->solution_value();
}

// ── SimpleMinCostFlow ────────────────────────────────────────────────────────

MinCostFlow* mincostflow_new(int32_t reserve_num_nodes, int32_t reserve_num_arcs) {
    return new MinCostFlow(reserve_num_nodes, reserve_num_arcs);
}

void mincostflow_free(MinCostFlow* f) {
    delete f;
}

int32_t mincostflow_add_arc(MinCostFlow* f, int32_t tail, int32_t head,
                             int64_t capacity, int64_t unit_cost) {
    return f->solver.AddArcWithCapacityAndUnitCost(tail, head, capacity, unit_cost);
}

void mincostflow_set_node_supply(MinCostFlow* f, int32_t node, int64_t supply) {
    f->solver.SetNodeSupply(node, supply);
}

int32_t mincostflow_solve(MinCostFlow* f) {
    return map_min_cost_flow_status(f->solver.Solve());
}

int32_t mincostflow_solve_max_flow_with_min_cost(MinCostFlow* f) {
    return map_min_cost_flow_status(f->solver.SolveMaxFlowWithMinCost());
}

int64_t mincostflow_optimal_cost(const MinCostFlow* f) {
    return f->solver.OptimalCost();
}

int64_t mincostflow_maximum_flow(const MinCostFlow* f) {
    return f->solver.MaximumFlow();
}

int64_t mincostflow_flow(const MinCostFlow* f, int32_t arc) {
    return f->solver.Flow(arc);
}

int32_t mincostflow_num_arcs(const MinCostFlow* f) {
    return f->solver.NumArcs();
}

int32_t mincostflow_tail(const MinCostFlow* f, int32_t arc) {
    return f->solver.Tail(arc);
}

int32_t mincostflow_head(const MinCostFlow* f, int32_t arc) {
    return f->solver.Head(arc);
}

int64_t mincostflow_capacity(const MinCostFlow* f, int32_t arc) {
    return f->solver.Capacity(arc);
}

int64_t mincostflow_unit_cost(const MinCostFlow* f, int32_t arc) {
    return f->solver.UnitCost(arc);
}

} // extern "C"
