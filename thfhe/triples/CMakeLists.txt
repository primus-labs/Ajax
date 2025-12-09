cmake_minimum_required(VERSION 3.31...4.0)
project(otls LANGUAGES C CXX)
set(NAME "otls")

# Compiler and standard setup
set(CMAKE_CXX_COMPILER clang++)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

# ===== EMP dependency setup =====
find_path(CMAKE_FOLDER NAMES cmake/emp-tool-config.cmake)
include(${CMAKE_FOLDER}/cmake/emp-base.cmake)

find_package(emp-ot REQUIRED)
include_directories(${EMP-OT_INCLUDE_DIRS})
include_directories(${CMAKE_CURRENT_SOURCE_DIR})

# ===== Find and link OpenSSL (needed for BN_new etc.) =====
find_package(OpenSSL REQUIRED)
if (NOT OpenSSL_FOUND)
    message(FATAL_ERROR "OpenSSL not found. Install it via: sudo apt-get install -y libssl-dev")
endif()

# ===== Library definition =====
add_library(wrapper SHARED
    src/wrapper.cpp
    src/utils.cpp
    src/constants.cpp
    src/countio.cpp
    internal/wrapper_internal.hpp
)

target_include_directories(wrapper
    PUBLIC
        "${CMAKE_SOURCE_DIR}/include"
    PRIVATE
        "${CMAKE_SOURCE_DIR}/internal"
)

# ===== Link dependencies =====
target_link_libraries(wrapper
    PRIVATE
        ${EMP-OT_LIBRARIES}
        OpenSSL::Crypto
)

# ===== RPATH fix: ensure binaries find libwrapper.so relative to themselves =====
# $ORIGIN means "directory where the binary is located"
set_target_properties(wrapper PROPERTIES
    INSTALL_RPATH "$ORIGIN"
    BUILD_WITH_INSTALL_RPATH ON
)

# ===== Installation rules =====
install(FILES cmake/otls-config.cmake DESTINATION cmake/)
install(DIRECTORY src DESTINATION include)
install(TARGETS wrapper LIBRARY DESTINATION lib)

# ===== Enable and add tests =====
enable_testing()
add_subdirectory(test)