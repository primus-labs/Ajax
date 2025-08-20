cmake_minimum_required(VERSION 4.0)
project(otls)
set(NAME "otls")

set(CMAKE_CXX_COMPILER clang++)

find_path(CMAKE_FOLDER NAMES cmake/emp-tool-config.cmake)
include(${CMAKE_FOLDER}/cmake/emp-base.cmake)

find_package(emp-ot REQUIRED)
include_directories(${EMP-OT_INCLUDE_DIRS})
include_directories(${CMAKE_CURRENT_SOURCE_DIR})

install(FILES cmake/otls-config.cmake DESTINATION cmake/)
install(DIRECTORY ole DESTINATION include)

add_library(wrapper SHARED
        ole/wrapper/wrapper.hpp
        ole/wrapper/wrapper.cpp
        ole/wrapper/io.hpp
        ole/wrapper/io.cpp
        ole/wrapper/utils.hpp
        ole/wrapper/utils.cpp
        ole/wrapper/constants.hpp
        ole/wrapper/constants.cpp
)

target_compile_options(wrapper PRIVATE -Wall -Wpedantic)
install(TARGETS wrapper LIBRARY DESTINATION lib)

ENABLE_TESTING()
ADD_SUBDIRECTORY(test)