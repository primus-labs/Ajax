//
// Created by hdvanegasm on 8/19/25.
//


#ifndef OTLS_IO_HPP
#define OTLS_IO_HPP

#include "../countio.h"
#include "../ole_f2k.h"
#include "../ole_z2k.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct NetIoWrapper {
    NetIO *inner_net;
} NetIoWrapper;

/*
 * =========================
 * NetIO wrapper functions
 * =========================
 */

NetIoWrapper *new_net_io(char *address, int port, bool quiet);

void delete_net_io(const NetIoWrapper *io);

#ifdef __cplusplus
}
#endif

#endif //OTLS_IO_HPP
