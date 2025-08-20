//
// Created by hdvanegasm on 8/19/25.
//

#ifndef OTLS_UTILS_HPP
#define OTLS_UTILS_HPP

#include <ole/utils.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct Block {
    block *inner_block;
} Block;

typedef struct M128i {
    __m128i *inner_m128i;
} M128i;

/*
 * =======================
 * Block wrapper functions
 * =======================
 */

Block *new_block(M128i *block_data);

/*
* =========================
* __m128i wrapper functions
* =========================
*/

M128i *new_m128i(__m128i *inner_value);

#ifdef __cplusplus
}
#endif

#endif //OTLS_UTILS_HPP
