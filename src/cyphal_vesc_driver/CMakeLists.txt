cmake_minimum_required(VERSION 3.8)
project(cyphal_vesc_driver LANGUAGES CXX)

if(CMAKE_COMPILER_IS_GNUCXX OR CMAKE_CXX_COMPILER_ID MATCHES "Clang")
  add_compile_options(
    -Wall -Wextra -Werror=conversion -Werror=unused-but-set-variable
    -Werror=return-type -Werror=shadow -Werror=format -Werror=range-loop-construct
    -Werror=missing-braces)
endif()

# find dependencies
set(THIS_PACKAGE_INCLUDE_DEPENDS
  hardware_interface
  pluginlib
  rclcpp
  rclcpp_lifecycle
)

# find dependencies
find_package(ament_cmake REQUIRED)
foreach(Dependency IN ITEMS ${THIS_PACKAGE_INCLUDE_DEPENDS})
  find_package(${Dependency} REQUIRED)
endforeach()


add_library(cyphal_vesc_driver SHARED
  src/cyphal_vesc_driver.cpp
)
target_compile_features(cyphal_vesc_driver PUBLIC cxx_std_23)
target_include_directories(cyphal_vesc_driver PUBLIC
  $<BUILD_INTERFACE:${PROJECT_SOURCE_DIR}/include>
  $<INSTALL_INTERFACE:include/cyphal_vesc_driver>
)

include_directories(${CMAKE_CURRENT_BINARY_DIR}/cyphal_vesc_driver_impl)
add_subdirectory(cyphal_vesc_driver_impl)

target_link_libraries(cyphal_vesc_driver PUBLIC
  hardware_interface::hardware_interface
  pluginlib::pluginlib
  rclcpp::rclcpp
  rclcpp_lifecycle::rclcpp_lifecycle
)





# Export hardware plugins
pluginlib_export_plugin_description_file(hardware_interface cyphal_vesc_driver_plugin.xml)

# INSTALL
install(
  DIRECTORY include/
  DESTINATION include/cyphal_vesc_driver
)
install(TARGETS cyphal_vesc_driver
  EXPORT export_cyphal_vesc_driver
  ARCHIVE DESTINATION lib
  LIBRARY DESTINATION lib
  RUNTIME DESTINATION bin
)



if(BUILD_TESTING)
  find_package(ament_lint_auto REQUIRED)
  # the following line skips the linter which checks for copyrights
  # comment the line when a copyright and license is added to all source files
  set(ament_cmake_copyright_FOUND TRUE)
  # the following line skips cpplint (only works in a git repo)
  # comment the line when this package is in a git repo and when
  # a copyright and license is added to all source files
  set(ament_cmake_cpplint_FOUND TRUE)
  ament_lint_auto_find_test_dependencies()
endif()



ament_export_targets(export_cyphal_vesc_driver HAS_LIBRARY_TARGET)
ament_export_dependencies(${THIS_PACKAGE_INCLUDE_DEPENDS})
ament_package()
