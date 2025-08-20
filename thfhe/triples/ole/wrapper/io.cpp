//
// Created by hdvanegasm on 8/19/25.
//

#include "io.hpp"

/*
 * =========================
 * NetIO wrapper functions
 * =========================
 */

NetIo *new_net_io(const char *address, const int port, const bool quiet) {
    const auto io = new NetIo;
    io->inner_net = new NetIO(address, port, quiet);
    return io;
}
