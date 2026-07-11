#ifndef CYPHAL_VESC_DRIVER_HPP
#define CYPHAL_VESC_DRIVER_HPP

#include "hardware_interface/system_interface.hpp"



namespace cyphal_vesc_driver 
{

// FIXME: This is inspired by 
// https://github.com/ros-controls/ros2_control_demos/blob/master/example_1/hardware/include/ros2_control_demo_example_1/rrbot.hpp
// Are we sure that on_shutdown (ready for dtor/mem free) and on_cleanup (idk) aren't necessary????

using CallbackReturn = rclcpp_lifecycle::node_interfaces::LifecycleNodeInterface::CallbackReturn;


class CyphalVescDriver : public hardware_interface::SystemInterface {
private:
  std::vector<CommandInterface> commands;
  std::vector<StateInterface> states;
public:
  CyphalVescDriver() {}

  CallbackReturn on_init(const hardware_interface::HardwareComponentInterfaceParams & params) override;

  CallbackReturn on_configure(const rclcpp_lifecycle::State & previous_state) override;

  CallbackReturn on_activate(const rclcpp_lifecycle::State & previous_state) override;

  CallbackReturn on_deactivate(const rclcpp_lifecycle::State & previous_state) override;

  hardware_interface::return_type read(const rclcpp::Time & time, const rclcpp::Duration & period) override;

  hardware_interface::return_type write(const rclcpp::Time & time, const rclcpp::Duration & period) override;

};



/*
 * UNCONFIGURED (on_init, on_cleanup):
 *   Hardware is initialized but communication is not started and therefore no interface is
 *available.
 *
 * INACTIVE (on_configure, on_deactivate):
 *   Communication with the hardware is started and it is configured.
 *   States can be read, but command interfaces are not available.
 *
 * FINALIZED (on_shutdown):
 *   Hardware interface is ready for unloading/destruction.
 *   Allocated memory is cleaned up.
 *
 * ACTIVE (on_activate):
 *   Power circuits of hardware are active and hardware can be moved, e.g., brakes are disabled.
 *   Command interfaces are available.
*/








}

#endif
