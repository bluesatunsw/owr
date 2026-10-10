#include "cyphal_vesc_driver/cyphal_vesc_driver.hpp"
#include "cyphal_vesc_driver_impl.h"

namespace cyphal_vesc_driver {

CallbackReturn CyphalVescDriver::on_init(
    const hardware_interface::HardwareComponentInterfaceParams &params) {
  if (hardware_interface::SystemInterface::on_init(params) !=
      CallbackReturn::SUCCESS) {
    return CallbackReturn::ERROR;
  }

  for (auto joint : params.hardware_info.joints) {
    RCLCPP_WARN(get_logger(), "joint: %s", joint.name.c_str());

    if (joint.command_interfaces.size() != 1 ||
        joint.command_interfaces[0].name != "velocity") {
      RCLCPP_FATAL(
          get_logger(),
          "Wrong command interfaces specified, you may only use velocity!");
    }

    if (joint.state_interfaces.size() != 1 ||
        joint.state_interfaces[0].name != "velocity") {
      RCLCPP_FATAL(
          get_logger(),
          "Wrong state interface specified, you may only use velocity!");
    }

    RCLCPP_WARN(get_logger(), "\twith subject_id: %s",
                joint.command_interfaces[0].parameters["subject_id"].c_str());
  }

  RCLCPP_INFO(get_logger(), "Successfully initialized!");
  return CallbackReturn::SUCCESS;
}

CallbackReturn
CyphalVescDriver::on_configure(const rclcpp_lifecycle::State &previous_state) {
  RCLCPP_INFO(get_logger(), "Successfully configured");
  return CallbackReturn::SUCCESS;
}

CallbackReturn
CyphalVescDriver::on_activate(const rclcpp_lifecycle::State &previous_state) {
  RCLCPP_INFO(get_logger(), "Successfully activated!");
  return CallbackReturn::SUCCESS;
}

CallbackReturn
CyphalVescDriver::on_deactivate(const rclcpp_lifecycle::State &previous_state) {
  // TODO: set all the VESC's to 0% duty cycle for now (later deassert UDRAL
  // readiness)
  RCLCPP_INFO(get_logger(), "Successfully deactivated!");
  return CallbackReturn::SUCCESS;
}

hardware_interface::return_type
CyphalVescDriver::read(const rclcpp::Time & /* time */,
                       const rclcpp::Duration & /* period */) {
  // TODO: We aren't reading anything from the CANbus at this point
  //
  // We probably *should* be seeing whether node heartbeats are transferred to
  // check whether they exist

  return hardware_interface::return_type::OK;
}

hardware_interface::return_type
CyphalVescDriver::write(const rclcpp::Time &time,
                        const rclcpp::Duration &period) {
  return hardware_interface::return_type::OK;
}

} // namespace cyphal_vesc_driver

#include "pluginlib/class_list_macros.hpp"
PLUGINLIB_EXPORT_CLASS(cyphal_vesc_driver::CyphalVescDriver,
                       hardware_interface::SystemInterface)
