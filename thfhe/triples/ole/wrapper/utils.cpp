//
// Created by hdvanegasm on 8/19/25.
//

#include "utils.hpp"

/*
 * =======================
 * Block wrapper functions
 * =======================
 */

BlockWrapper *new_block(const M128iWrapper *block_data) {
    const auto block = new BlockWrapper;
    block->inner_block = block_data->inner_m128i;
    return block;
}

void delete_net_io(const BlockWrapper *block) {
    delete block->inner_block;
    delete block;
}

/*
 * =========================
 * __m128i wrapper functions
 * =========================
 */

M128iWrapper *new_m128i_wrapper(__m128i *inner_value) {
    const auto wrapped_value = new M128iWrapper;
    wrapped_value->inner_m128i = inner_value;
    return wrapped_value;
}

void delete_m128i(const M128iWrapper *value) {
    if (value) {
        delete value->inner_m128i;
        delete value;
    }
}

