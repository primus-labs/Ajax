#include "wrapper.hpp"
#include "io.hpp"
#include "utils.hpp"

/*
 * =========================
 * OLE F2k wrapper functions
 * =========================
 */

OleF2kWrapper *new_ole_f2k(const NetIoWrapper *io, const FerretCotWrapper *ot) {
    auto *ole = new OleF2kWrapper;
    ole->inner_ole = new OLEF2K<NetIO>(io->inner_net, ot->inner_ferret_cot);
    return ole;
}

void inner_prod_ole_f2k(const OleF2kWrapper *ole, const BlockWrapper *res, const BlockWrapper *a, const BlockWrapper *b,
                        const int sz) {
    ole->inner_ole->inner_prod(res->inner_block, a->inner_block, b->inner_block, sz);
}

void compute_ole_f2k(const OleF2kWrapper *ole, const BlockWrapper *out, const BlockWrapper *in, const int length) {
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

FerretCotWrapper *new_ferret_cot(const int party, const int threads, const NetIoWrapper **ios, const size_t n_ios,
                                 const bool malicious,
                                 const bool run_setup,
                                 const PrimalLpnParameterWrapper *param, const char *pre_file) {
    auto *cot = new FerretCotWrapper;

    auto **net_ios = new NetIO *[n_ios];
    for (size_t i = 0; i < n_ios; i++) {
        net_ios[i] = ios[i]->inner_net;
    }
    cot->inner_ferret_cot = new FerretCOT<NetIO>(party, threads, net_ios, malicious, run_setup, *param->inner_param,
                                                 std::string(pre_file));
    delete[] net_ios;
    return cot;
}

void delete_ferret_cot(const FerretCotWrapper *cot) {
    if (cot) {
        delete cot->inner_ferret_cot;
        delete cot;
    }
}
