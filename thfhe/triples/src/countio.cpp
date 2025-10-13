#include "wrapper/countio.h"
#include "internal/countio_internal.hpp"

extern "C" {

CountNetIoWrapper *new_count_net_io(const char *address, int32_t port, size_t quiet) {
    auto wrapper = new CountNetIoWrapper;
    wrapper->inner = new emp::CountNetIO(address, port, quiet != 0);
    return wrapper;
}

void delete_count_net_io(const CountNetIoWrapper *io) {
    if (!io) return;
    delete io->inner;
    delete io;
}

size_t count_net_io_get_bytes_sent(const CountNetIoWrapper *io) {
    return io ? io->inner->get_total_bytes_sent() : 0;
}

size_t count_net_io_get_bytes_recv(const CountNetIoWrapper *io) {
    return io ? io->inner->get_total_bytes_recv() : 0;
}

}