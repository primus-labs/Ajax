//
// Created by man2706kum on 10/13/25.
//

#ifndef OTLS_COUNT_IO_INTERNAL_HPP
#define OTLS_COUNT_IO_INTERNAL_HPP

#include "wrapper/countio.h"
#include "internal/countio.h"

struct CountNetIoWrapper {
    emp::CountNetIO *inner;
};

#endif