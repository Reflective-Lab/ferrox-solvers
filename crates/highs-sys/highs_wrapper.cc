// Intentionally does NOT include highs_wrapper.h — the C typedef for
// HighsModelStatus would collide with HiGHS's own enum class of the same name.
// All exported functions are redeclared here with int32_t return types.
// Integer constants must match highs_wrapper.h / lib.rs definitions.

#include <cstdint>
#include <cstddef>
#include "Highs.h"

// ── Integer return constants (must match highs_wrapper.h) ────────────────────
// HighsReturnStatus
static constexpr int32_t FX_kOk      = 0;
static constexpr int32_t FX_kError   = 2;

// HighsModelStatus — values must match what lib.rs HighsModelStatus expects.
// Our header pins these; we map from HiGHS enum class values here.
static constexpr int32_t FX_MS_NOT_SET        = 0;
static constexpr int32_t FX_MS_OPTIMAL        = 7;
static constexpr int32_t FX_MS_INFEASIBLE     = 8;
static constexpr int32_t FX_MS_UNBOUNDED      = 9;
static constexpr int32_t FX_MS_SOLUTION_LIMIT = 11;
static constexpr int32_t FX_MS_TIME_LIMIT     = 12;

// ── Storage struct ────────────────────────────────────────────────────────────

struct HighsHandle {
    Highs highs;
    int   num_cols = 0;
};

// ── extern "C" ───────────────────────────────────────────────────────────────

extern "C" {

HighsHandle* highs_create() {
    auto* h = new HighsHandle{};
    h->highs.setOptionValue("output_flag", false);
    return h;
}

void highs_destroy(HighsHandle* h) { delete h; }

int32_t highs_add_col(HighsHandle* h, double cost, double lb, double ub) {
    auto s = h->highs.addCol(cost, lb, ub, 0, nullptr, nullptr);
    if (s == HighsStatus::kOk) h->num_cols++;
    return s == HighsStatus::kOk ? FX_kOk : FX_kError;
}

int32_t highs_add_row(HighsHandle* h, double lb, double ub,
                      int num_nz, const int* idx, const double* val) {
    auto s = h->highs.addRow(lb, ub, num_nz, idx, val);
    return s == HighsStatus::kOk ? FX_kOk : FX_kError;
}

int32_t highs_change_col_integer_type(HighsHandle* h, int col, int is_integer) {
    auto vtype = is_integer ? HighsVarType::kInteger : HighsVarType::kContinuous;
    auto s = h->highs.changeColIntegrality(col, vtype);
    return s == HighsStatus::kOk ? FX_kOk : FX_kError;
}

int32_t highs_set_time_limit(HighsHandle* h, double seconds) {
    auto s = h->highs.setOptionValue("time_limit", seconds);
    return s == HighsStatus::kOk ? FX_kOk : FX_kError;
}

int32_t highs_set_mip_rel_gap(HighsHandle* h, double gap) {
    auto s = h->highs.setOptionValue("mip_rel_gap", gap);
    return s == HighsStatus::kOk ? FX_kOk : FX_kError;
}

int32_t highs_run(HighsHandle* h) {
    auto s = h->highs.run();
    return s == HighsStatus::kOk ? FX_kOk : FX_kError;
}

int32_t highs_get_model_status(HighsHandle* h) {
    switch (h->highs.getModelStatus()) {
        case HighsModelStatus::kOptimal:               return FX_MS_OPTIMAL;
        case HighsModelStatus::kInfeasible:            return FX_MS_INFEASIBLE;
        case HighsModelStatus::kUnbounded:             return FX_MS_UNBOUNDED;
        case HighsModelStatus::kUnboundedOrInfeasible: return FX_MS_UNBOUNDED;
        case HighsModelStatus::kObjectiveBound:        return FX_MS_SOLUTION_LIMIT;
        case HighsModelStatus::kObjectiveTarget:       return FX_MS_SOLUTION_LIMIT;
        case HighsModelStatus::kSolutionLimit:         return FX_MS_SOLUTION_LIMIT;
        case HighsModelStatus::kTimeLimit:             return FX_MS_TIME_LIMIT;
        case HighsModelStatus::kIterationLimit:        return FX_MS_TIME_LIMIT;
        default:                                       return FX_MS_NOT_SET;
    }
}

double highs_get_objective_value(HighsHandle* h) {
    return h->highs.getObjectiveValue();
}

double highs_get_col_value(HighsHandle* h, int col) {
    return h->highs.getSolution().col_value[col];
}

double highs_get_mip_gap(HighsHandle* h) {
    double val = 1.0;
    h->highs.getInfoValue("mip_gap", val);
    return val;
}

int32_t highs_get_col_value_checked(HighsHandle* h, int col, double* out) {
    if (out == nullptr) return FX_kError;
    const auto& values = h->highs.getSolution().col_value;
    if (col < 0 || static_cast<size_t>(col) >= values.size()) return FX_kError;
    *out = values[static_cast<size_t>(col)];
    return FX_kOk;
}

int32_t highs_get_mip_gap_checked(HighsHandle* h, double* out) {
    if (out == nullptr) return FX_kError;
    double val = 1.0;
    auto s = h->highs.getInfoValue("mip_gap", val);
    if (s != HighsStatus::kOk) return FX_kError;
    *out = val;
    return FX_kOk;
}

int highs_solution_value_valid(HighsHandle* h) {
    return h->highs.getSolution().value_valid ? 1 : 0;
}

} // extern "C"
