#include "cyphal_bus_driver/cyphal_bus_driver.hpp"
#include "cyphal_bus_driver_rs.h"
#include <cstdint>
#include <hardware_interface/hardware_info.hpp>
#include <optional>
#include <rclcpp/logging.hpp>
#include <string>

namespace cyphal_bus_driver {

CallbackReturn CyphalBusDriver::on_init(
    const hardware_interface::HardwareComponentInterfaceParams &params) {
  if (hardware_interface::SystemInterface::on_init(params) !=
      CallbackReturn::SUCCESS) {
    return CallbackReturn::ERROR;
  }
    
  // Did we get a can bus?
  if (!params.hardware_info.hardware_parameters.contains("can_bus")) {
      RCLCPP_FATAL(get_logger(), "No CANbus for driver specified!");
      return CallbackReturn::ERROR;
  }
  auto can_bus{params.hardware_info.hardware_parameters.at("can_bus")};
    
  // Did we get a cyphal node id?
  if (!params.hardware_info.hardware_parameters.contains("node_id")) {
      RCLCPP_FATAL(get_logger(), "No Cyphal NodeID for driver specified!");
      return CallbackReturn::ERROR;
  }
  auto node_id{static_cast<uint8_t>(std::stoi(
    params.hardware_info.hardware_parameters.at("node_id")
  ))};

  // Check that all of the joints have reasonable configs!
  rust::Vec<NamedBescConfig> besc_configs{};
  rust::Vec<NamedStepperConfig> stepper_configs{};
  for (auto joint : params.hardware_info.joints) {
    RCLCPP_WARN(get_logger(), "joint: %s", joint.name.c_str());


    auto device_id{static_cast<uint16_t>(std::stoi(
        joint.parameters.at("device_id")
    ))};

    if (joint.parameters.at("type") == "besc") {
        auto invert{joint.parameters.at("invert") == "True"};

        besc_configs.push_back(NamedBescConfig {
            .name = joint.name,
            .config = BescConfig { .id = device_id, .invert = invert }
        });
        if (joint.command_interfaces.size() !=  1 || joint.command_interfaces[0].name != "velocity" ||
            joint.state_interfaces.size() != 1 || joint.state_interfaces[0].name != "velocity") {
            RCLCPP_FATAL(
                get_logger(),
                "Wrong command/state interface specified, you may only use velocity!");
            return CallbackReturn::ERROR;
        }
    } else if (joint.parameters.at("type") == "stepper") {
        stepper_configs.push_back(NamedStepperConfig {
            .name = joint.name,
            .config = StepperConfig { .id = device_id }
        });
        if (joint.command_interfaces.size() !=  1 || joint.command_interfaces[0].name != "position" ||
            joint.state_interfaces.size() != 1 || joint.state_interfaces[0].name != "position") {
            RCLCPP_FATAL(
                get_logger(),
                "Wrong command/state interface specified, you may only use position!");
            return CallbackReturn::ERROR;
        }
    }

    RCLCPP_WARN(get_logger(), "\twith device_id: %d", device_id);
  }
    
  driver = {BusDriver::create(can_bus, node_id, besc_configs, stepper_configs)};

  RCLCPP_INFO(get_logger(), "Successfully initialized!");
  return CallbackReturn::SUCCESS;
}

CallbackReturn
CyphalBusDriver::on_activate(const rclcpp_lifecycle::State &previous_state) {
  (*driver)->activate();

  RCLCPP_INFO(get_logger(), "Successfully activated!");
  return CallbackReturn::SUCCESS;
}

CallbackReturn
CyphalBusDriver::on_deactivate(const rclcpp_lifecycle::State &previous_state) {
  (*driver)->deactivate();

  // TODO: later deassert UDRAL
  RCLCPP_INFO(get_logger(), "Successfully deactivated!");
  return CallbackReturn::SUCCESS;
}

CallbackReturn
CyphalBusDriver::on_shutdown(const rclcpp_lifecycle::State &previous_state) {
  BusDriver::destroy(std::move(*driver));
  driver = std::nullopt;

  RCLCPP_INFO(get_logger(), "Successfully shutdown!");
  return CallbackReturn::SUCCESS;
}

hardware_interface::return_type
CyphalBusDriver::read(const rclcpp::Time & /* time */,
                       const rclcpp::Duration & /* period */) {
  auto res{(*driver)->read()};
  for (auto& besc : res.bescs) {
      set_state(std::string(besc.device_name) + "/velocity", besc.measured_velocity);
  }
  for (auto& stepper : res.steppers) {
      set_state(std::string(stepper.device_name) + "/position", stepper.measured_position);
  }

  return hardware_interface::return_type::OK;
}

hardware_interface::return_type
CyphalBusDriver::write(const rclcpp::Time &time,
                        const rclcpp::Duration &period) {  
  rust::Vec<BescCommand> besc_commands{};
  rust::Vec<StepperCommand> stepper_commands{};

  for (auto& joint : info_.joints) {
      if (joint.parameters.at("type") == "besc") {
        besc_commands.push_back(BescCommand {
            .device_name = joint.name,
            .target_velocity = get_command(joint.name + "/velocity"),
        });
      } else if (joint.parameters.at("type") == "stepper") {
        stepper_commands.push_back(StepperCommand {
            .device_name = joint.name,
            .target_position = get_command(joint.name + "/position"),
        });
      }
  }
  (*driver)->write(besc_commands, stepper_commands);

  return hardware_interface::return_type::OK;
}
} // namespace cyphal_bus_driver

#include "pluginlib/class_list_macros.hpp"
PLUGINLIB_EXPORT_CLASS(cyphal_bus_driver::CyphalBusDriver,
                       hardware_interface::SystemInterface)
