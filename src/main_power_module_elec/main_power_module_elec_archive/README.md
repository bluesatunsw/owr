# Main Power Hub (MPH)

A device which monitors and distributes battery power onto four subcircuits (intended to be either computing power hubs or subsystem power hubs). It complies with the common device specification 2025-04.

Power input from the battery is provided via a male XT90 connector and each of the subcircuits has a female XT60 power output connector.

## Switching
Power to each of the subcircuits can be toggled by the MCU. Triggering the estop, a normally closed contact wired to the device, removes power from all subcircuits bypassing the control of the MCU. The MCU is always powered. As implied by the connector name, each channel is able to output 60A peak and 40A continuous.

Switching on ramps slowly (soft pullup resistor + gate capacitance), switching off is instantaneous.

Power MOSFET: C2902884
https://www.lcsc.com/datasheet/C2902884.pdf

BJT on pullup: BC846s
https://diotec.com/request/datasheet/bc846s.pdf

Gate drive voltage is provided by a charge pump circuit that sums the battery voltage + the 12V rail.
The gate driver is a simple BJT pullup configuration.

Charge pump MOSFET: SSM3K333R
https://toshiba.semicon-storage.com/info/SSM3K333R_datasheet_en_20250220.pdf?did=6736&prodName=SSM3K333R

## Monitoring
The bus voltage is monitored through a voltage divider into an ADC (10k / (10k + 220k)).
The output current of each channel is monitored.
This is done with a 1m shunt resistor on each channel.
The voltage across the resistor is fed into one internal STM32 op-amp per channel.
A compensation capacitor is placed on the output of each op-amp to introduce a pole near 1kHz.
The op-amp pins should be connected internally to the ADCs using software.

