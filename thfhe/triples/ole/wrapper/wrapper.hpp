#pragma once
#include "../ole_f2k.h"
#include "../ole_z2k.h"
#include "../utils.h"
#include "io.hpp"
#include "utils.hpp"
#include "constants.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct OleF2k {
    OLEF2K<NetIO> *inner_ole;
} OleF2k;

typedef struct FerretCot {
    FerretCOT<NetIO> *inner_ferret_cot;
} FerretCot;

/*
 * =========================
 * OLE F2k wrapper functions
 * =========================
 */

OleF2k *new_ole_f2k(const NetIo *io, const FerretCot *ot);

void inner_prod_ole_f2k(const OleF2k *ole, const Block *res, const Block *a, const Block *b, int sz);

void compute_ole_f2k(const OleF2k *ole, const Block *out, const Block *in, int length);

/*
 * =========================
 * Ferret COT wrapper functions
 * =========================
 */

FerretCot new_ferret_cot(int party, int threads, NetIo **ios, bool malicious, bool run_setup,
                         PrimalLpnParameter param, std::string pre_file);


#ifdef __cplusplus
}
#endif



