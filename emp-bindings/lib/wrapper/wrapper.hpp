#pragma once
#include "../ole_f2k.h"
#include "../ole_z2k.h"
#include "../utils.h"
#include "io.hpp"
#include "utils.hpp"
#include "constants.hpp"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct OleF2kWrapper {
    OLEF2K<NetIO> *inner_ole;
} OleF2kWrapper;

typedef struct FerretCot {
    FerretCOT<NetIO> *inner_ferret_cot;
} FerretCotWrapper;

/*
 * =========================
 * OLE F2k wrapper functions
 * =========================
 */

OleF2kWrapper *new_ole_f2k(const NetIoWrapper *io, const FerretCotWrapper *ot);

void inner_prod_ole_f2k(const OleF2kWrapper *ole, const BlockWrapper *res, const BlockWrapper *a, const BlockWrapper *b,
                        int sz);

void compute_ole_f2k(const OleF2kWrapper *ole, const BlockWrapper *out, const BlockWrapper *in, int length);

void delete_ole_f2k(const OleF2kWrapper *ole);

/*
 * =========================
 * Ferret COT wrapper functions
 * =========================
 */

FerretCotWrapper *new_ferret_cot(const int party, const int threads, const NetIoWrapper **ios, const size_t n_ios,
                                 const bool malicious,
                                 const bool run_setup,
                                 const PrimalLpnParameterWrapper *param, const char *pre_file);

void delete_ferret_cot(const FerretCotWrapper *cot);


#ifdef __cplusplus
}
#endif



