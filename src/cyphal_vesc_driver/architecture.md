# Cyphal BESC Driver

# Future

# v1.1 (current)

ros2_control hardware interface.

Exposes state and command interfaces for each BESC on the drivebase.

## Decision to make it a system

ros2_control has [three kinds of hardware interface](https://control.ros.org/rolling/doc/getting_started/getting_started.html#overview-hardware-components), two of which (actuator and system) would be possible for this software component.

However, actuator only allows the exposition of one ROS2 joint, which is a problem as we have four wheels. We then have a choice between exposing a different cyphal node for each actuator (undesirable because too many cyphal nodes) or having some kind of cooked separate process that muxes one node between multiple hardware interfaces (attempted this, it was ugly).

Hence, system is the only sensible choice.

## Gaslighting ROS2

Due to firmware constraints we have no way of supplying the BESCs with a velocity setpoint. Hence, we need to translate the supplied velocity command to cyphal duty cycle setpoints. This does not need to be precise (so long as it's monotonic LGTM).

## Future Extension

Stepper driver drivers will eventually be added. It remains to be seen whether this will be in the same or a different hardware interface.
