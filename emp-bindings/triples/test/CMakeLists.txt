# This macro creates a test executable and links it with required libs
macro(add_test_executable_with_lib _name libs)
    add_executable(test_${_name} "${_name}.cpp")

    # Link with EMP and wrapper libraries
    target_link_libraries(test_${_name}
        PRIVATE
            ${EMP-TOOL_LIBRARIES}
            wrapper
    )

    # RPATH fix: tell the binary to look for libwrapper.so in its parent directory
    set_target_properties(test_${_name} PROPERTIES
        INSTALL_RPATH "$ORIGIN/../"
        BUILD_WITH_INSTALL_RPATH ON
    )
endmacro()

# Macro for adding simple test cases
macro(add_test_case _name)
    add_test_executable_with_lib(${_name} "")
    add_test(NAME ${_name}
        COMMAND "${CMAKE_RUNTIME_OUTPUT_DIRECTORY}/test_${_name}"
        WORKING_DIRECTORY "${CMAKE_SOURCE_DIR}/"
    )
endmacro()

# Macro for tests that use the "run" wrapper script
macro(add_test_case_with_run _name)
    add_test_executable_with_lib(${_name} "")
    add_test(NAME ${_name}
        COMMAND "./run" "${CMAKE_RUNTIME_OUTPUT_DIRECTORY}/test_${_name}"
        WORKING_DIRECTORY "${CMAKE_SOURCE_DIR}/"
    )
endmacro()

# ===== Test cases =====
add_test_case_with_run(ole_f2k)
add_test_case_with_run(ole_z2k)
add_test_case_with_run(triples)