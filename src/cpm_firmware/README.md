# Compute Power Module (CPM)

On-board LEDs from top to bottom:
- Status light
    - Amber for initialisation, green if all good, flashing red to indicate a fault (OCP on any channel)
- CAN activity light
    - Blue for RX, green for TX

TEMP0 (PA7) is for 5V regulator -- ADC2 (led, but has to be core)
TEMP1 (PA3) is for 12V regulator -- ADC1 (core, but has to be led)
TEMP2 (PA1) is for 24V regulator -- ADC12 (core)
TEMP3 (PA0) is for 19V regulator -- ADC12 (core)
