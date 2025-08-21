//
// Created by hdvanegasm on 8/21/25.
//

#ifndef OTLS_IO_INTERNAL_HPP
#define OTLS_IO_INTERNAL_HPP

#include "wrapper/io.h"
#include "ole_f2k.h"

struct NetIoWrapper {
    NetIO *inner_net;
};

#endif //OTLS_IO_INTERNAL_HPP
