//
// Created by hdvanegasm on 8/19/25.
//

#ifndef OTLS_UTILS_HPP
#define OTLS_UTILS_HPP
#include <emmintrin.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct BlockWrapper BlockWrapper;

typedef struct M128iWrapper M128iWrapper;

/*
 * =======================
 * Block wrapper functions
 * =======================
 */

BlockWrapper *new_block(const M128iWrapper *block_data);

void delete_block(const BlockWrapper *block);

/*
* =========================
* __m128i wrapper functions
* =========================
*/

M128iWrapper *new_m128i_wrapper(__m128i *inner_value);

void delete_m128i(const M128iWrapper *value);

#ifdef __cplusplus
}
#endif

#endif //OTLS_UTILS_HPP
