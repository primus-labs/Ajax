#include "wrapper.hpp"
#include "io.hpp"
#include "utils.hpp"

/*
 * =========================
 * OLE F2k wrapper functions
 * =========================
 */

OleF2k *new_ole_f2k(const NetIo *io, const FerretCot *ot) {
    auto *ole = new OleF2k;
    ole->inner_ole = new OLEF2K<NetIO>(io->inner_net, ot->inner_ferret_cot);
    return ole;
}

void inner_prod_ole_f2k(const OleF2k *ole, const Block *res, const Block *a, const Block *b, const int sz) {
    ole->inner_ole->inner_prod(res->inner_block, a->inner_block, b->inner_block, sz);
}

void compute_ole_f2k(const OleF2k *ole, const Block *out, const Block *in, const int length) {
    ole->inner_ole->compute(out->inner_block, in->inner_block, length);
}
