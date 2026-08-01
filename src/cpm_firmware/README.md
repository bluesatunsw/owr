# Compute Power Module (CPM)

On-board LEDs from top to bottom:
- Status light
    - Amber for initialisation, green if all good, flashing red to indicate a fault (OCP on any channel)
- CAN activity light
    - Blue for RX, green for TX

Due to ADC routing limitations, the VBUS sense is managed by ExtARGBCtrl rather
than CorePowerCtrl
