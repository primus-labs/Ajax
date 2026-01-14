//
// Created by hdvanegasm on 8/21/25.
//

#ifndef OTLS_UTILS_INTERNAL_HPP
#define OTLS_UTILS_INTERNAL_HPP

#include "wrapper/utils.h"
#include "ole_f2k.h"

struct BlockWrapper {
    block *inner_block;
};

struct M128iWrapper {
    __m128i *inner_m128i;
};

#endif //OTLS_UTILS_INTERNAL_HPP
