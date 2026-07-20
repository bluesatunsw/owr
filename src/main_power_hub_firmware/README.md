# Main Power Module Firmware

See `main.rs`. This firmware is to support current sensing, channel control and
status LEDs. This is not yet fully implemented (bringup stage).

LED states for the top IDC connector LED:
- Amber: System initialisation.
- Green: Active; E-Stop is disengaged (all systems go!)
- Yellow, flashing (unused, can't implement directly): Standby; E-Stop is engaged.
- Red, flashing (unused): Critical error.

LED states for the bottom IDC connector LED:
- Amber: System initialisation.
- Blue: CAN FD activity indicator. (TX)
- Green: CAN FD activity indicator. (RX)

LED states for different channels:
- Amber: System initialisation.
- Green: Channel is enabled.
- Yellow: Channel is disabled.
- Red, flashing (unused): Power fault on this channel (e.g. current exceeded); disabled.
