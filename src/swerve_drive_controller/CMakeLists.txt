cmake_minimum_required(VERSION 3.20)
project(swerve_drive_controller)

if(CMAKE_COMPILER_IS_GNUCXX OR CMAKE_CXX_COMPILER_ID MATCHES "Clang")
  add_compile_options(
    -Wall -Wextra -Werror=conversion -Werror=unused-but-set-variable
    -Werror=return-type -Werror=shadow -Werror=format -Werror=range-loop-construct
    -Werror=missing-braces)
endif()

# find dependencies
set(THIS_PACKAGE_INCLUDE_DEPENDS
  controller_interface
  generate_parameter_library
  geometry_msgs
  hardware_interface
  nav_msgs
  pluginlib
  rclcpp
  rclcpp_lifecycle
  rcpputils
  realtime_tools
  tf2
  tf2_msgs
  Eigen3
)

# Tell CMake to get the deps
find_package(ament_cmake REQUIRED)
foreach(Dependency IN ITEMS ${THIS_PACKAGE_INCLUDE_DEPENDS})
  find_package(${Dependency} REQUIRED)
endforeach()

add_compile_definitions(RCPPUTILS_VERSION_MAJOR=${rcpputils_VERSION_MAJOR})
add_compile_definitions(RCPPUTILS_VERSION_MINOR=${rcpputils_VERSION_MINOR})
generate_parameter_library(swerve_drive_controller_parameters
  src/swerve_drive_controller_parameters.yaml
)

add_library(swerve_drive_controller SHARED
  src/swerve_drive_controller.cpp
)
target_compile_features(swerve_drive_controller PUBLIC cxx_std_23)
target_include_directories(swerve_drive_controller PUBLIC
  $<BUILD_INTERFACE:${PROJECT_SOURCE_DIR}/include>
  $<INSTALL_INTERFACE:include/swerve_drive_controller>
)

target_link_libraries(swerve_drive_controller PUBLIC 
  swerve_drive_controller_parameters
  controller_interface::controller_interface
  hardware_interface::hardware_interface
  pluginlib::pluginlib
  rclcpp::rclcpp
  rclcpp_lifecycle::rclcpp_lifecycle
  rcpputils::rcpputils
  realtime_tools::realtime_tools
  tf2::tf2
  ${tf2_msgs_TARGETS}
  ${geometry_msgs_TARGETS}
  ${nav_msgs_TARGETS}
  ${control_msgs_TARGETS}
)
pluginlib_export_plugin_description_file(controller_interface swerve_drive_plugin.xml)

if(BUILD_TESTING)
  find_package(ament_cmake_gmock REQUIRED)
  find_package(controller_manager REQUIRED)
  find_package(ros2_control_test_assets REQUIRED)

  add_definitions(-DTEST_FILES_DIRECTORY="${CMAKE_CURRENT_SOURCE_DIR}/test")
  ament_add_gmock(test_load_swerve_drive_controller test/test_load_swerve_drive_controller.cpp)
  target_link_libraries(test_load_swerve_drive_controller
    controller_manager::controller_manager
    ros2_control_test_assets::ros2_control_test_assets
  )

  find_package(ament_lint_auto REQUIRED)
  # the following line skips the linter which checks for copyrights
  set(ament_cmake_copyright_FOUND TRUE)
  set(ament_cmake_cpplint_FOUND FALSE)
  ament_lint_auto_find_test_dependencies()
endif()

# Install the header files
install(
  DIRECTORY include/
  DESTINATION include/swerve_drive_controller
)

install(TARGETS swerve_drive_controller swerve_drive_controller_parameters
  EXPORT export_swerve_drive_controller
  RUNTIME DESTINATION bin
  ARCHIVE DESTINATION lib
  LIBRARY DESTINATION lib
)

ament_export_targets(export_swerve_drive_controller HAS_LIBRARY_TARGET)
ament_export_dependencies(${THIS_PACKAGE_INCLUDE_DEPENDS})
ament_package()
