# BESC Firmware 

"The firmware for the BESC"

## Running and debugging the firmware

To run the firmware in a debugging/development context with an ST-LINK
connected:

```
openocd -f ../embedded_common/tools/openocd-stm32g4.cfg
cargo run --release
```

It's pretty much necessary to run in release mode due to space optimisations
being necessary to fit onto the dozens of KB of device flash.

To flash the firmware *and have it continue to run after you disconnect the
debugger* (for integration testing, or the competition (!)), do *not* run
OpenOCD. Instead, with the debugger connected, run:

```
cargo flash --release --chip stm32fg474rbtx
```

(You may need to install `cargo flash`.)

## Message IDs for Cyphal

For the BESCs, subject-IDs for drive commands are in the form `30xy`:

- `30` means the message is intended for the drivebase
- `x` is the channel identifier:
    - 5 to 8 refer to the front-left (FL), front-right (FR), back-left (BL) and
      back-right (BR) motors respectively (in the context of the drivebase).
- `y` is the "topic", which currently has underspecified semantics. Just use `0`.

A message with subject-ID 3070, for example, is a drive command targeting the
back-left BESC.

## Firmware organization

The `bsp` directory contains board/device specific structs (e.g. a driver for
the TIM1 peripheral to get it to produce 6 PWM outputs, or a driver to
communicate with the DRV8301). However, be mindful that this firmware also uses
drivers from `embedded_common`; you won't find the Cyphal microsecond clock in
`bsp`, for instance.

The `config` directory contains structs (and presets for specific motors, e.g.
the front left drivebase motor) that may require a firmware restart to apply
properly. These currently have no method to change them but in the future they
should all be exposed as Cyphal registers, e.g. PID constants.

The `state` directory contains structs that store the current state of the
motor, e.g. what control mode it is currently executing (voltage, velocity,
position, etc.) or the current state of the PID controller (numerical
integrator and differentiator values). This should be considered to be reset to
default when the firmware is restarted.

The `comms` directory contains implementations of handler functions for Cyphal
(canadensis).

The `util` directory contains utility operations, mostly for motor control
(e.g. inverse Park transform).

## Implementation details

### Timer allocation

- TIM1
    - Commutation timer to generate 6 PWM signals
- TIM2
    - Cyphal microsecond clock (as assigned by `embedded_common`)
- TIM5
    - Hall effect interfacing timer
- TIM6
    - Motion control timer

### TIM1/PWM details

TIM1 is the PWM clock source, with different duty cycles set for all three
phases (OCx) independently. The G4s we're using have a neat feature that the
PWM of the advanced-control timers can generate complementary outputs *with
programmable dead time between them going high*, which is exactly what we need
for this motor control application. (We ideally want to switch either the
high-side or the low-side MOSFET for a phase, but we need to allow them time to
turn on/switch off so they are never both on (even partially) as this
effectively shorts power rails and kills the hardware. This is
known as avoiding shoot-through.)

Despite the PWM configuration being largely encapsulated by the SixPwmDriver,
we use the **TIM1 update event as the interrupt tick used to handle commutation
and current sensing**. This interrupt is configured to fire when the TIM1 CNT
register hits 0.

### Current sensing (note: not yet implemented)

See [NXP's AN14164 "Current Sensing Techniques in Motor Control
Applications"](https://www.nxp.com/docs/en/application-note/AN14164.pdf). Just
section 4 ("Low-side current sensing") should be sufficient.

Current is sensed via shunt resistors buffered by the DRV8301. Current is
measured on the low-side MOSFET and therefore must be sensed when that MOSFET
is on. We need to synchronise the ADC sample with the PWM -- for each PWM cycle
we want to sense the middle of the complementary (low-side) output pulse.

We have assigned pins to use the injected channels on ADC1, since these
channels have their own data registers, more suitable trigger sources for motor
control, faster triggers and run at higher priority. However, the STM32 Rust
HAL doesn't support these right now, so we're going to implement them ourselves.

### Hall Effect Sensor Working Method

PA0/1/2 --> TIM5CH1/2/3 --[XOR]--> TI1 --[Edge Detector]--> TI1F_ED --> TRC
--[Slave Controller]--> RESET TIM5.CNT
--[TIM5_CCMR1.CC1S == IC1 mapped to TRC]--> IC1

TIM5 capture compare register 1 is set up in *capture* mode, it captures
TIM5.CNT *when IC1 is asserted*. The counter is set to run in One Pulse Mode
(OPM) as it overflows ~100sec and so `CNT == 0` is used as a sentinel value.

### Configured Interrupts

In order of (priority, position in ivt):

- 31, 29 `TIM5` (variable, motor frequency [0, 8.4kHz] with our battery)
- 32, 25 `TIM1_UP_TIM10` (fixed, 28kHz)
<!-- - 61, 54 `TIM6_DAC` (fixed, 1kHz) -->

### Duty cycle limit

-> 1 clk = 47.6 ns
DRV8301 is config'd thru resistor for 80ns deadtime
50 dts ticks @ 168MHz --> 298ns

TIM1 is on APB2, TIM1 CLK -> 168MHz, TIM1.ARR -> 6000

TIM1.CNT      0 -----up to------>  TIM.ARR   -----down to----> 0

              <------------------  35.71μs  ------------------->
TIM1.OCxREF   ⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺\____________/⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺
TIM1.OCx      ⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺\______________/⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺
TIM1.OCxN     ____________________/⎺⎺⎺⎺⎺⎺⎺⎺⎺⎺\__________________
                                 <> deadtime  <> 298ns
               at least 595ns   <------------> max duty limit 
TIM1.UEV      \________________________________________________/
^^^ commutation (+current PID control) interrupt (TIM1.RCR = 1, so every cycle)


ADCCLK = APB2CLK (84MHz) / 4 == 21MHz (max 36MHz)
                          vvvvv NOT TO SCALE vvvvv
ADC Samp      ______________________⎺___________________________
                                   ^ sample time 3 clks == 143ns
                                    ^ inj group trigger latency 3 clks == 143ns
ADC Conv      _______________________/⎺⎺\_______________________
               12 clks == 571ns      <-->           (doesn't affect timing 😸)

We need to limit the duty cycle to ensure channel A,B always have some low time
(to allow for current sensing). Assuming 0 settling time after switch

Tcurrsense = deadtime + ADC inj trig latency + ADC samp time
= 442ns (75 TIM cnt) 
~= 100 TIM1 counts (w/ settling time) (595ns) => 98.3% max duty


Aside: rust stm32f4xx-hal uses output-compare mode 1
```
PWM mode 1 - In upcounting, channel 1 is active as long as TIMx_CNT<TIMx_CCR1
else inactive. In downcounting, channel 1 is inactive (OC1REF=0) as long as
TIMx_CNT>TIMx_CCR1 else active (OC1REF=’1’).
```
