# There was an attempt to copy https://github.com/XiangpengHao/cxx-cmake-example
if (CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(CARGO_CMD cargo build)
    set(CARGO_PROFILE "debug")
else ()
    set(CARGO_CMD cargo build --release)
    set(CARGO_PROFILE "release")
endif ()


set(RUST_PART_LIB "${CMAKE_CURRENT_BINARY_DIR}/${CARGO_PROFILE}/libcyphal_bus_driver_rs.a")
set(RUST_PART_CXX "${CMAKE_CURRENT_BINARY_DIR}/cyphal_bus_driver_rs.cc")

add_library(cyphal_bus_driver_rs STATIC ${RUST_PART_CXX})

add_custom_command(
    OUTPUT ${RUST_PART_CXX}
    COMMAND CARGO_TARGET_DIR=${CMAKE_CURRENT_BINARY_DIR} ${CARGO_CMD}
    COMMAND cp ${CMAKE_CURRENT_BINARY_DIR}/cxxbridge/cyphal_bus_driver_rs/src/lib.rs.cc  ${RUST_PART_CXX}
    COMMAND cp ${CMAKE_CURRENT_BINARY_DIR}/cxxbridge/cyphal_bus_driver_rs/src/lib.rs.h   ${CMAKE_CURRENT_BINARY_DIR}/cyphal_bus_driver_rs.h
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
    DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/src/lib.rs
)

target_link_libraries(cyphal_bus_driver_rs ${RUST_PART_LIB})
