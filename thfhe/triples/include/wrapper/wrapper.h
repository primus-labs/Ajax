#pragma once

#include "wrapper/countio.h"
#include "wrapper/utils.h"
#include "wrapper/constants.h"
#include <stdint.h>


#ifdef __cplusplus
extern "C" {
#endif

typedef struct OleF2kWrapper OleF2kWrapper;

typedef struct FerretCot FerretCotWrapper;

typedef struct OleZ2kWrapper OleZ2kWrapper;

/*
 * =========================
 * OLE F2k wrapper functions
 * =========================
 */

OleF2kWrapper *new_ole_f2k(const CountNetIoWrapper *io, const FerretCotWrapper *ot);

void inner_prod_ole_f2k(const OleF2kWrapper *ole, const BlockWrapper *res, const BlockWrapper *a, const BlockWrapper *b,
                        int sz);

void compute_ole_f2k(const OleF2kWrapper *ole, const BlockWrapper *out, const BlockWrapper *in, int length);

void delete_ole_f2k(const OleF2kWrapper *ole);

/*
 * =========================
 * Ferret COT wrapper functions
 * =========================
 */

FerretCotWrapper *new_ferret_cot(const int party, const int threads, const CountNetIoWrapper **ios, const size_t n_ios,
                                 const bool malicious,
                                 const bool run_setup,
                                 const PrimalLpnParameterWrapper *param, const char *pre_file);

void delete_ferret_cot(const FerretCotWrapper *cot);


/*
 * =========================
 * OLE Z2k wrapper functions
 * =========================
 */

OleZ2kWrapper *new_ole_z2k(const CountNetIoWrapper *io, const FerretCotWrapper *cot, const size_t bitlength);

void delete_ole_z2k(const OleZ2kWrapper *ole);

void compute_ole_z2k(const OleZ2kWrapper *ole, uint64_t *out, const uint64_t *in, const size_t length,
                     const size_t cot_batch_size);

#ifdef __cplusplus
}
#endif




