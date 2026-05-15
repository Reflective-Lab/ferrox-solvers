#pragma once
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    HIGHS_kOk      = 0,
    HIGHS_kWarning = 1,
    HIGHS_kError   = 2,
} HighsReturnStatus;

typedef enum {
    HIGHS_MODEL_STATUS_NOT_SET         = 0,
    HIGHS_MODEL_STATUS_LOAD_ERROR      = 1,
    HIGHS_MODEL_STATUS_MODEL_ERROR     = 2,
    HIGHS_MODEL_STATUS_INFEASIBLE      = 8,
    HIGHS_MODEL_STATUS_OPTIMAL         = 7,
    HIGHS_MODEL_STATUS_UNBOUNDED       = 9,
    HIGHS_MODEL_STATUS_SOLUTION_LIMIT  = 11,
    HIGHS_MODEL_STATUS_TIME_LIMIT      = 12,
} HighsModelStatus;

typedef struct HighsHandle HighsHandle;

HighsHandle*       highs_create(void);
void               highs_destroy(HighsHandle* h);
HighsReturnStatus  highs_add_col(HighsHandle* h, double cost, double lb, double ub);
HighsReturnStatus  highs_add_row(HighsHandle* h, double lb, double ub,
                                  int num_nz, const int* idx, const double* val);
HighsReturnStatus  highs_change_col_integer_type(HighsHandle* h, int col, int is_integer);
HighsReturnStatus  highs_set_time_limit(HighsHandle* h, double seconds);
HighsReturnStatus  highs_set_mip_rel_gap(HighsHandle* h, double gap);
HighsReturnStatus  highs_run(HighsHandle* h);
HighsModelStatus   highs_get_model_status(HighsHandle* h);
double             highs_get_objective_value(HighsHandle* h);
double             highs_get_col_value(HighsHandle* h, int col);
double             highs_get_mip_gap(HighsHandle* h);
HighsReturnStatus  highs_get_col_value_checked(HighsHandle* h, int col, double* out);
HighsReturnStatus  highs_get_mip_gap_checked(HighsHandle* h, double* out);
int                highs_solution_value_valid(HighsHandle* h);

#ifdef __cplusplus
}
#endif
