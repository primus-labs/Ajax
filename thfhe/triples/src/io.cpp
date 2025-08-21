//
// Created by hdvanegasm on 8/19/25.
//


/*
 * =========================
 * NetIO wrapper functions
 * =========================
 */

#include "wrapper/io.h"
#include "internal/io_internal.hpp"

NetIoWrapper *new_net_io(const char *address, const int32_t port, const size_t quiet) {
    const auto io = new NetIoWrapper;
    io->inner_net = new NetIO(address, port, quiet);
    return io;
}

void delete_net_io(const NetIoWrapper *io) {
    if (io) {
        delete io->inner_net;
        delete io;
    }
}
