//
// Created by man2706kum on 10/13/25.
//

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

void send_data_internal(CountNetIoWrapper *io, const void *data, size_t len);

void recv_data_internal(CountNetIoWrapper *io, void *data, size_t len);

#ifdef __cplusplus
}
#endif