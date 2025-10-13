#pragma once

#include <stdlib.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct CountNetIoWrapper CountNetIoWrapper;

/*
 * =========================
 * CountNetIO wrapper functions
 * =========================
 */

CountNetIoWrapper *new_count_net_io(const char *address, int32_t port, size_t quiet);

void delete_count_net_io(const CountNetIoWrapper *io);

size_t count_net_io_get_bytes_sent(const CountNetIoWrapper *io);

size_t count_net_io_get_bytes_recv(const CountNetIoWrapper *io);

#ifdef __cplusplus
}
#endif