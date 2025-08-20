//
// Created by hdvanegasm on 8/19/25.
//

#include "constants.hpp"


/*
 * ==============================
 * Primal LPN Parameter functions
 * ==============================
 */

PrimalLpnParameterWrapper *new_primal_lpn_parameter(const int64_t n, const int64_t t, const int64_t k,
                                                    const int64_t log_bin_sz, const int64_t n_pre,
                                                    const int64_t t_pre, const int64_t k_pre,
                                                    const int64_t log_bin_sz_pre) {
    auto *inner = new PrimalLpnParameterWrapper;
    inner->inner_param = new PrimalLPNParameter(n, t, k, log_bin_sz, n_pre, t_pre, k_pre, log_bin_sz_pre);
    return inner;
}

void delete_primal_lpn_parameter(const PrimalLpnParameterWrapper *primal) {
    if (primal) {
        delete primal->inner_param;
        delete primal;
    }
}
