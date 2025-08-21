//
// Created by hdvanegasm on 8/21/25.
//

#ifndef OTLS_WRAPPER_INTERNAL_HPP
#define OTLS_WRAPPER_INTERNAL_HPP

#include "wrapper/wrapper.h"
#include "ole_f2k.h"
#include "ole_z2k.h"

struct OleF2kWrapper {
    OLEF2K<NetIO> *inner_ole;
};

struct FerretCot {
    FerretCOT<NetIO> *inner_ferret_cot;
};

struct OleZ2kWrapper {
    OLEZ2K<NetIO> *inner_ole;
};

#endif //OTLS_WRAPPER_INTERNAL_HPP
