#include "wrapper/wrapper.h"

#include "internal/wrapper_internal.hpp"
#include "internal/countio_internal.hpp"
#include "internal/utils_internal.hpp"
#include "internal/constants_internal.hpp"


/*
 * =========================
 * OLE F2k wrapper functions
 * =========================
 */

OleF2kWrapper *new_ole_f2k(const CountNetIoWrapper *io, const FerretCotWrapper *ot) {
    auto *ole = new OleF2kWrapper;
    ole->inner_ole = new OLEF2K<CountNetIO>(io->inner, ot->inner_ferret_cot);
    return ole;
}

void inner_prod_ole_f2k(const OleF2kWrapper *ole, const BlockWrapper *res, const BlockWrapper *a,
                        const BlockWrapper *b,
                        const int sz) {
    ole->inner_ole->inner_prod(res->inner_block, a->inner_block, b->inner_block, sz);
}

void compute_ole_f2k(const OleF2kWrapper *ole, const BlockWrapper *out, const BlockWrapper *in,
                     const int length) {
    ole->inner_ole->compute(out->inner_block, in->inner_block, length);
}

void delete_ole_f2k(const OleF2kWrapper *ole) {
    if (ole) {
        delete ole->inner_ole;
        delete ole;
    }
}

/*
 * =========================
 * Ferret COT wrapper functions
 * =========================
 */

FerretCotWrapper *new_ferret_cot(const int party, const int threads, const CountNetIoWrapper **ios,
                                 const size_t n_ios,
                                 const bool malicious,
                                 const bool run_setup,
                                 const PrimalLpnParameterWrapper *param, const char *pre_file) {
    auto *cot = new FerretCotWrapper;

    auto **net_ios = new CountNetIO *[n_ios];
    for (size_t i = 0; i < n_ios; i++) {
        net_ios[i] = ios[i]->inner;
    }
    cot->inner_ferret_cot = new FerretCOT<CountNetIO>(party, threads, net_ios, malicious, run_setup, *param->inner_param,
                                                 std::string(pre_file));
    return cot;
}

void delete_ferret_cot(const FerretCotWrapper *cot) {
    if (cot) {
        delete cot->inner_ferret_cot;
        delete cot;
    }
}

/*
 * =========================
 * OLE Z2k wrapper functions
 * =========================
 */

OleZ2kWrapper *new_ole_z2k(const CountNetIoWrapper *io, const FerretCotWrapper *cot, const size_t bitlength) {
    const auto ole_z2k_wrapper = new OleZ2kWrapper;
    ole_z2k_wrapper->inner_ole = new OLEZ2K<CountNetIO>(io->inner, cot->inner_ferret_cot, bitlength);
    return ole_z2k_wrapper;
}

void delete_ole_z2k(const OleZ2kWrapper *ole) {
    if (ole) {
        delete ole->inner_ole;
        delete ole;
    }
}

void compute_ole_z2k(const OleZ2kWrapper *ole, uint64_t *out, const uint64_t *in, const size_t length,
                     const size_t cot_batch_size) {
    ole->inner_ole->compute(out, in, length, cot_batch_size);
}

