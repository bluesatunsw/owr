# There was an attempt to copy https://github.com/XiangpengHao/cxx-cmake-example
if (CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(CARGO_CMD cargo build --verbose)
    set(CARGO_PROFILE "debug")
else ()
    set(CARGO_CMD cargo build --release --verbose)
    set(CARGO_PROFILE "release")
endif ()


set(RUST_PART_LIB "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_PROFILE}/libcyphal_vesc_driver_impl.a")
set(RUST_PART_CXX "${CMAKE_CURRENT_BINARY_DIR}/target/cxxbridge/cyphal_vesc_driver_impl/src/lib.rs.cc")

add_library(cyphal_vesc_driver_impl STATIC ${RUST_PART_CXX})

add_custom_command(
    OUTPUT  ${RUST_PART_CXX}
    COMMAND RUSTFLAGS="${RUST_FLAGS}" ${CARGO_CMD}
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)

include_directories(${CMAKE_CURRENT_BINARY_DIR}/cxxbridge/cyphal_vesc_driver_impl/src/)
target_link_libraries(cyphal_vesc_driver_impl ${RUST_PART_LIB})
