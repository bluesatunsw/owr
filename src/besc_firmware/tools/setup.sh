sudo ip link set down can0
sudo ip link set up can0 type can bitrate 1000000 fd off

export UAVCAN__CAN__IFACE='socketcan:can0'
export UAVCAN__CAN__MTU=8
export UAVCAN__CAN__BITRATE=1000000
export UAVCAN__NODE__ID=$(yakut accommodate)  # Pick an unoccupied node-ID automatically for this shell session.

