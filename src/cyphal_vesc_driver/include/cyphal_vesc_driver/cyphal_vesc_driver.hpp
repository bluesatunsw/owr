#ifndef CYPHAL_VESC_DRIVER_HPP
#define CYPHAL_VESC_DRIVER_HPP

#include "cyphal_vesc_driver_impl.h"
#include "hardware_interface/system_interface.hpp"
#include <optional>



namespace cyphal_vesc_driver 
{
// https://github.com/ros-controls/ros2_control_demos/blob/master/example_1/hardware/include/ros2_control_demo_example_1/rrbot.hpp

using CallbackReturn = rclcpp_lifecycle::node_interfaces::LifecycleNodeInterface::CallbackReturn;


class CyphalVescDriver : public hardware_interface::SystemInterface {
private:
    std::optional<rust::Box<BescDriver>> driver;
public:
  CallbackReturn on_init(const hardware_interface::HardwareComponentInterfaceParams & params) override;
  CallbackReturn on_activate(const rclcpp_lifecycle::State & previous_state) override;
  CallbackReturn on_deactivate(const rclcpp_lifecycle::State & previous_state) override;
  CallbackReturn on_shutdown(const rclcpp_lifecycle::State & previous_state) override;

  hardware_interface::return_type read(const rclcpp::Time & time, const rclcpp::Duration & period) override;
  hardware_interface::return_type write(const rclcpp::Time & time, const rclcpp::Duration & period) override;
};
}

#endif
