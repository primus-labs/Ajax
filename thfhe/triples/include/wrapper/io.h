//
// Created by hdvanegasm on 8/19/25.
//

#pragma once

#include <stdlib.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct NetIoWrapper NetIoWrapper;

/*
 * =========================
 * NetIO wrapper functions
 * =========================
 */

NetIoWrapper *new_net_io(const char *address, const int32_t port, const size_t quiet);

void delete_net_io(const NetIoWrapper *io);

#ifdef __cplusplus
}
#endif
