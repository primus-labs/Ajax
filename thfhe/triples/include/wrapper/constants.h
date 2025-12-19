//
// Created by hdvanegasm on 8/19/25.
//

#ifndef OTLS_CONSTANTS_H
#define OTLS_CONSTANTS_H

#include <stdlib.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct PrimalLpnParameterWrapper PrimalLpnParameterWrapper;

/*
 * ==============================
 * Primal LPN Parameter functions
 * ==============================
 */

PrimalLpnParameterWrapper *new_primal_lpn_parameter(const int64_t n, const int64_t t, const int64_t k,
                                                    const int64_t log_bin_sz, const int64_t n_pre,
                                                    const int64_t t_pre, const int64_t k_pre,
                                                    const int64_t log_bin_sz_pre);

void delete_primal_lpn_parameter(const PrimalLpnParameterWrapper *primal);

#ifdef __cplusplus
}
#endif

#endif //OTLS_CONSTANTS_H
