//
// Created by hdvanegasm on 8/19/25.
//

#include "utils.hpp"

/*
 * =======================
 * Block wrapper functions
 * =======================
 */

Block *new_block(const M128i *block_data) {
    const auto block = new Block;
    block->inner_block = block_data->inner_m128i;
    return block;
}

/*
 * =========================
 * __m128i wrapper functions
 * =========================
 */

M128i *new_m128i(__m128i *inner_value) {
    const auto wrapped_value = new M128i;
    wrapped_value->inner_m128i = inner_value;
    return wrapped_value;
}
