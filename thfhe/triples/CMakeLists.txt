cmake_minimum_required(VERSION 3.3)
project(otls)
set(NAME "otls")

set(CMAKE_CXX_COMPILER clang++)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_path(CMAKE_FOLDER NAMES cmake/emp-tool-config.cmake)
include(${CMAKE_FOLDER}/cmake/emp-base.cmake)

find_package(emp-ot REQUIRED)
include_directories(${EMP-OT_INCLUDE_DIRS})
include_directories(${CMAKE_CURRENT_SOURCE_DIR})

install(FILES cmake/otls-config.cmake DESTINATION cmake/)
install(DIRECTORY src DESTINATION include)

add_library(wrapper SHARED
        src/wrapper.cpp
        src/io.cpp
        src/utils.cpp
        src/constants.cpp
        internal/wrapper_internal.hpp
)

target_include_directories(wrapper PUBLIC "${CMAKE_SOURCE_DIR}/include" PRIVATE "${CMAKE_SOURCE_DIR}/internal" })

install(TARGETS wrapper LIBRARY DESTINATION lib)

ENABLE_TESTING()
ADD_SUBDIRECTORY(test)