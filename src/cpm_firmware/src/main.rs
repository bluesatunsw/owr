/* CPM firmware. Currently in the bringup stage. Uses the G474 template. */

#![no_std]
#![no_main]

extern crate alloc;
use embedded_alloc::LlffHeap as Heap;
use embedded_common::{
    argb::{self, Colour},
    can::CanDriver,
    clock,
};
use heapless::Vec;

use cortex_m_rt::entry;

use stm32g4xx_hal::{
    adc::{self, AdcClaim, AdcCommonExt, config::SampleTime}, gpio::{GpioExt, PinState}, opamp::*, pac::{self, i2c1::timeoutr::TIDLE_R}, prelude::*, pwr::{PwrExt, VoltageScale}, rcc::*, time::{ExtU32, RateExtU32}
};

use canadensis::{
    core::{
        time::MicrosecondDuration32, transfer::MessageTransfer, transport::Transport, Priority, SubjectId,
    },
    encoding::Deserialize,
    node::{
        data_types::{GetInfoResponse, Version},
        BasicNode, CoreNode,
    },
    requester::TransferIdFixedMap,
    Node, TransferHandler,
};
use canadensis_can::{CanNodeId, CanReceiver, CanTransmitter, CanTransport, Mtu};

// NOTE: import the data types you use
use canadensis_data_types::uavcan::{node::execute_command_1_3, primitive::scalar::natural8_1_0};

use canadensis_data_types::uavcan::node::execute_command_1_3::SERVICE as EXECUTE_COMMAND_SERVICE;
use canadensis_data_types::uavcan::node::execute_command_1_3::{
    ExecuteCommandRequest, ExecuteCommandResponse,
};

// Cyphal constants
const NODE_ID: u8 = 3;
const CYPHAL_CONCURRENT_TRANSFERS: usize = 4;
const CYPHAL_NUM_TOPICS: usize = 8;
const CYPHAL_NUM_SERVICES: usize = 8;

const HEARTBEAT_PERIOD_US: u32 = 1_000_000;
const TELEM_PERIOD_US: u32 = 50_000;
const TID_TIMEOUT_US: u32 = 100_000;

// Cyphal IDs
// NOTE: remove these ones and add the ones you use
const LED_TELEM_SUBJECT: SubjectId = SubjectId::from_truncating(3000);
const LED_UPDATE_SUBJECT: SubjectId = SubjectId::from_truncating(3001);

use panic_probe as _;
use defmt_rtt as _;

const NUM_LEDS: usize = 300;
const RED: Colour = Colour {r: 255, b: 0, g: 0};
const GREEN: Colour = Colour {r: 0, b: 0, g: 255};
const BLUE: Colour = Colour {r: 0, b: 255, g: 0};
const MAGENTA: Colour = Colour {r: 255, b: 255, g: 0};
const YELLOW: Colour = Colour {r: 255, b: 0, g: 255};
const CYAN: Colour = Colour {r: 0, b: 255, g: 255};
const DEFAULT_BRIGHTNESS: u8 = 64;

mod controller;
use controller::{PowerCtrl, ExtARGBCtrl};

// Global allocator -- required by canadensis.
#[global_allocator]
static G_HEAP: Heap = Heap::empty();

fn initialise_allocator() {
    use core::mem::MaybeUninit;
    // NOTE: You need to calculate HEAP_SIZE such that you don't crash when messages are reassembled
    // You may also want to shrink this if there's not so much CAN traffic
    const HEAP_SIZE: usize = 0x8000; // 32 KiB
    static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
    unsafe { G_HEAP.init(&raw mut HEAP_MEM as usize, HEAP_SIZE) }
}

///////////
// Main. //
///////////

#[entry]
fn main() -> ! {
    initialise_allocator();

    let dp = stm32g4xx_hal::pac::Peripherals::take().unwrap();
    let cp = stm32g4xx_hal::pac::CorePeripherals::take().unwrap();
    let pwr = dp
        .PWR
        .constrain()
        .vos(VoltageScale::Range1 { enable_boost: true })
        .freeze();
    let mut rcc = dp.RCC.freeze(
        Config::pll()
            .pll_cfg(PllConfig {
                mux: PllSrc::HSE(24.MHz()),
                m: PllMDiv::DIV_2, // 12 MHz
                n: PllNMul::MUL_28, // 336 MHz
                r: Some(PllRDiv::DIV_2), // 168 MHz
                q: Some(PllQDiv::DIV_2), // 168 MHz
                p: Some(PllPDiv::DIV_20), // 16.8 MHz
            })
            .fdcan_src(FdCanClockSource::PLLQ),
        pwr,
    );
    defmt::debug!("Initialised memory allocator and configured clock");

    defmt::trace!("Setting up pins...");
    let gpioa = dp.GPIOA.split(&mut rcc);
    let gpiob = dp.GPIOB.split(&mut rcc);
    let gpioc = dp.GPIOC.split(&mut rcc);
    
    let (isense24v, isense19v, isense5a, isense5b) = unsafe {
        let stolen_gpioa = pac::GPIOA::steal().split(&mut rcc);
        let stolen_gpiob = pac::GPIOB::steal().split(&mut rcc);
        (
            stolen_gpioa.pa6.into_analog(),
            stolen_gpiob.pb1.into_analog(),
            stolen_gpiob.pb12.into_analog(),
            stolen_gpioa.pa8.into_analog()
        )
    };

    let mut delay = cp.SYST.delay(&rcc.clocks);

    defmt::debug!("Setting up ARGB LED control (on-board)...");
    let mut onboard_leds = argb::Controller::new(
        dp.UART4,
        gpioc.pc10.into_alternate(),
        DEFAULT_BRIGHTNESS,
        &mut rcc
    );
    delay.delay(1.secs());
    onboard_leds.display(&[Colour::AMBER; 3]);
    delay.delay(1.secs());

    // r2p0 errata: need to bit-bang I2C
    /*defmt::debug!("Testing EEPROM...");
    defmt::debug!("Setting up I2C EEPROM...");
    const EEPROM_ADDR: u8 = 0b1010_000;
    let sda = gpioa.pa15.into_open_drain_output();
    let sda = sda.internal_pull_up(true);
    let scl = gpiob.pb7.into_alternate_open_drain();
    let scl = scl.internal_pull_up(true);
    let mut i2c_bus = dp.I2C1.i2c((sda, scl), 200.kHz(), &mut rcc);

    defmt::debug!("Reading 8 bytes from address 0x000...");
    let address: u16 = 0x000;
    let mut data: [u8; 8] = [0; 8];
    match i2c_bus.write_read(EEPROM_ADDR, &[((address & 0x3F00) >> 8) as u8, (address & 0xFF) as u8], &mut data) {
        Ok(_) => defmt::debug!("Got {}", data),
        Err(e) => defmt::warn!("Some error reading {:?}", defmt::Debug2Format(&e)),
    }

    defmt::debug!("Incrementing first four bytes at 0x000...");
    let address = 0x000;
    let addr_words = [((address & 0x3F00) >> 8) as u8, (address & 0xFF) as u8];
    let send_bytes: [u8; 6] = [addr_words[0], addr_words[1], data[0] + 1, data[1] + 1, data[2] + 1, data[3] + 1];
    match i2c_bus.write(EEPROM_ADDR, &send_bytes) {
        Ok(_) => defmt::debug!("No error writing data"),
        Err(e) => defmt::warn!("Error writing: {:?}", defmt::Debug2Format(&e)),
    };

    // clear write cycle
    delay.delay(5.millis());

    defmt::debug!("Reading 8 bytes from address 0x000...");
    let address: u16 = 0x000;
    for i in 0..8 {
        data[i] = 0;
    }
    match i2c_bus.write_read(EEPROM_ADDR, &[((address & 0x3F00) >> 8) as u8, (address & 0xFF) as u8], &mut data) {
        Ok(_) => defmt::debug!("Got {}", data),
        Err(e) => defmt::warn!("Error writing: {:?}", defmt::Debug2Format(&e)),
    }*/

    // ADC setup
    defmt::debug!("Configuring ADC12...");
    let mut adc12_common = dp.ADC12_COMMON
        .claim(adc::config::ClockMode::AdcKerCk {
            prescaler: (adc::config::Prescaler::Div_4),
            src: (adc::config::ClockSource::PllP)
        }, 
        &mut rcc
    );
    adc12_common.enable_vref();

    defmt::debug!("Setting up ADC1...");
    let adc1 = adc12_common
        .claim_and_configure(dp.ADC1, adc::config::AdcConfig::default(), &mut delay);
    defmt::debug!("Setting up ADC2...");
    let adc2 = adc12_common
        .claim_and_configure(dp.ADC2, adc::config::AdcConfig::default(), &mut delay);

    defmt::debug!("Configuring ADC3...");
    let mut adc345_common = dp.ADC345_COMMON
        .claim(adc::config::ClockMode::AdcKerCk {
            prescaler: (adc::config::Prescaler::Div_4),
            src: (adc::config::ClockSource::PllP)
        }, 
        &mut rcc
    );
    adc345_common.enable_vref();

    defmt::debug!("Setting up ADC3...");
    let adc3 = adc345_common
        .claim_and_configure(dp.ADC3, adc::config::AdcConfig::default(), &mut delay);
    defmt::debug!("Setting up ADC4...");
    let mut adc4 = adc345_common
        .claim_and_configure(dp.ADC4, adc::config::AdcConfig::default(), &mut delay);
    defmt::debug!("Setting up ADC5...");
    let adc5 = adc345_common
        .claim_and_configure(dp.ADC5, adc::config::AdcConfig::default(), &mut delay);
    
    let (_, opamp2, opamp3, opamp4, opamp5, ..) = dp.OPAMP.split(&mut rcc);
    // 24V draws 1A max -- so 0.1 V drop across sense-resistor full-scale
    // so gain of 16 should be comfortable full-scale
    let i24v_opamp = opamp2
        .pga_external_filter(gpiob.pb0.into_analog(), gpioa.pa5.into_analog(), Gain::Gain16)
        .enable_output(gpioa.pa6.into_analog());
    // 19V draws 10A max -- so 0.2 V drop across sense-resistor full-scale
    // so gain of 8 should be comfortable full-scale (accounting for OCP)
    let i19v_opamp = opamp3
        .pga_external_filter(gpiob.pb13.into_analog(), gpiob.pb2.into_analog(), Gain::Gain8)
        .enable_output(gpiob.pb1.into_analog());
    // LEDs draw max 4A per channel pair, so 0.2 drop across sense resistor full-scale
    // so gain of 8 should be comfortable (w OCP)
    let i5a_opamp = opamp4
        .pga_external_filter(gpiob.pb11.into_analog(), gpiob.pb10.into_analog(), Gain::Gain8)
        .enable_output(gpiob.pb12.into_analog());
    let i5b_opamp = opamp5
        .pga_external_filter(gpioc.pc3.into_analog(), gpiob.pb15.into_analog(), Gain::Gain8)
        .enable_output(gpioa.pa8.into_analog());

    defmt::debug!("Setting up 24V and 19V power control...");
    // initialise in disabled state
    let en24v = gpioc.pc6.into_push_pull_output_in_state(PinState::Low);
    let en19v = gpioc.pc7.into_push_pull_output_in_state(PinState::Low);
    let vsense24v = gpioc.pc0.into_analog();
    let vsense19v = gpioc.pc1.into_analog();
    let temp24v = gpioa.pa1.into_analog();
    let temp19v = gpioa.pa0.into_analog();
    let temp5v = gpioa.pa7.into_analog();
    let mut power_ctrl = PowerCtrl::new(
        en24v, en19v,
        adc2, adc3, vsense24v, vsense19v,
        temp24v, temp19v, temp5v,
        i24v_opamp, i19v_opamp, isense24v, isense19v
    );

    let vbus_sense = gpiob.pb14.into_analog();

    defmt::debug!("Setting up ARGB LED control (external)...");
    let nen5a = gpioc.pc8.into_push_pull_output_in_state(PinState::High);
    let nen5b = gpioc.pc9.into_push_pull_output_in_state(PinState::High);
    let lc0 = argb::Controller::new(dp.UART5, gpioc.pc12.into_alternate(), 255, &mut rcc);
    let lc1 = argb::Controller::new(dp.USART2, gpioa.pa2.into_alternate(), 255, &mut rcc);
    let lc2 = argb::Controller::new(dp.USART3, gpiob.pb9.into_alternate(), 255, &mut rcc);
    let lc3 = argb::Controller::new(dp.USART1, gpioc.pc4.into_alternate(), 255, &mut rcc);
    let vsense5v = gpioc.pc2.into_analog();
    let temp12v = gpioa.pa3.into_analog();
    let mut ext_led_ctrl = ExtARGBCtrl::new(
        lc0, lc1, lc2, lc3,
        nen5a, nen5b,
        adc1, adc5,
        vsense5v,
        isense5a, isense5b,
        temp12v,
        i5a_opamp, i5b_opamp,
    );

    defmt::debug!("Setting up microsecond clock...");
    let clock = clock::MicrosecondClock::new(dp.TIM2, &mut rcc);
    defmt::debug!("Setting up FDCAN driver...");
    let can = CanDriver::new(
        // NOTE: change these to the pins and FDCAN that the FDCAN transceiver is on
        dp.FDCAN1,
        gpioa.pa11.into_alternate(),
        gpioa.pa12.into_alternate(),
        &mut rcc,
    );
    
    defmt::debug!("Setting up core Cyphal node...");
    let id = CanNodeId::from_truncating(NODE_ID);
    let transmitter = CanTransmitter::new(Mtu::CanFd64);
    let receiver = CanReceiver::new(id);
    let core_node: CoreNode<
        _,
        _,
        _,
        TransferIdFixedMap<CanTransport, CYPHAL_CONCURRENT_TRANSFERS>,
        _,
        CYPHAL_NUM_TOPICS,
        CYPHAL_NUM_SERVICES,
    > = CoreNode::new(clock, id, transmitter, receiver, can);

    // Node initialisation is a non-recoverable error and should only happen if we run out of
    // memory or the hardware is completely broken, hence all the unwrapping.
    defmt::debug!("Setting up basic Cyphal node...");
    let mut node = BasicNode::new(
        core_node,
        GetInfoResponse {
            // NOTE: Update these appropriately
            protocol_version: Version { major: 1, minor: 0 },
            hardware_version: Version { major: 0, minor: 1 },
            software_version: Version { major: 0, minor: 1 },
            software_vcs_revision_id: 0,
            unique_id: embedded_common::debug::uuid(),
            name: Vec::from_slice(b"org.bluesat.template.UPDATEME").unwrap(),
            software_image_crc: Vec::new(),
            certificate_of_authenticity: Vec::new(),
        },
    )
    .unwrap();

    // NOTE: add calls like the following if you want to listen for specific messages
    node.subscribe_message(
        LED_UPDATE_SUBJECT,
        size_of::<natural8_1_0::Natural8>(),
        MicrosecondDuration32::from_ticks(TID_TIMEOUT_US),
    )
    .unwrap();

    node.subscribe_request(
        EXECUTE_COMMAND_SERVICE,
        size_of::<execute_command_1_3::ExecuteCommandRequest>(),
        MicrosecondDuration32::from_ticks(TID_TIMEOUT_US),
    ).unwrap();

    // NOTE: add calls like the following to publish specific messages
    defmt::trace!("Starting publication of LED messages...");
    node.start_publishing(LED_TELEM_SUBJECT, 10.millis(), Priority::Nominal).unwrap();

    defmt::debug!("Turning on 24V and 19V rails...");
    power_ctrl.enable_24v();
    power_ctrl.enable_19v();
    defmt::debug!("Turning on all LED rails...");
    ext_led_ctrl.enable_a();
    ext_led_ctrl.enable_b();
    delay.delay(100.millis());

    // all good!
    onboard_leds.display(&[GREEN; 3]);

    let cpwrdata = power_ctrl.tick();
    let mut min_24v_centivolts: u64 = cpwrdata.v24v_centivolts as u64;
    let mut max_24v_centivolts: u64 = cpwrdata.v24v_centivolts as u64;
    let mut rolling_24v_sum_centivolts: u64 = min_24v_centivolts;
    let mut min_19v_centivolts: u64 = cpwrdata.v19v_centivolts as u64;
    let mut max_19v_centivolts: u64 = cpwrdata.v19v_centivolts as u64;
    let mut rolling_19v_sum_centivolts: u64 = min_19v_centivolts;
    let mut num_samples: u64 = 1;

    // Start the superloop.
    let mut tim_heartbeat = node.clock().now_const();
    let mut tim_telem = node.clock().now_const();
    let mut tim_argb = node.clock().now_const();
  
    let mut comms_state = CommsState {};
    // NOTE: You almost certainly want to replace the Subsystem with an actual subsystem
    let mut subsystem = Subsystem { hue: 0 };

    defmt::info!("System initialised. Entering superloop...");
    loop {
        // Handle Cyphal tasks
        node.receive(&mut CommsHandler { state: &mut comms_state, subsystem: &mut subsystem } ).unwrap();

        // You can set the health of the node to represent the state of the subsystem to be
        // signalled over Cyphal/CAN in the heartbeat messages:
        // node.set_health(...); node.set_status_code(...);
        
        if node
            .clock()
            .advance_if_elapsed(&mut tim_heartbeat, HEARTBEAT_PERIOD_US.micros())
        {
            defmt::debug!("Publishing node heartbeat...");
            node.run_per_second_tasks().unwrap();
        }
        
        if node
            .clock()
            .advance_if_elapsed(&mut tim_telem, TELEM_PERIOD_US.micros())
        {
            defmt::trace!("Publishing LED telemetry...");
            node.publish(
                LED_TELEM_SUBJECT,
                &natural8_1_0::Natural8 {
                    value: subsystem.hue as u8
                },
            )
            .unwrap();
        }

    // brownout test
    /*loop {
        let cpwrdata = power_ctrl.tick();
        defmt::println!("+24V line\t{}.{:02} V\tdrawing {} mA",
            cpwrdata.v24v_centivolts / 100,
            cpwrdata.v24v_centivolts % 100,
            cpwrdata.i24v_milliamps
        );
        defmt::println!("+19V line\t{}.{:02} V\tdrawing {} mA",
            cpwrdata.v19v_centivolts / 100,
            cpwrdata.v19v_centivolts % 100,
            cpwrdata.i19v_milliamps
        );
        const VBUS_DIVIDER_DENOM: u32 = 23; // 10k - 220k divider
        let vbus_sample = adc4.convert(&vbus_sense, SampleTime::Cycles_640_5) as u32;
        let mut vbus_millivolts = vbus_sample * 3200 * VBUS_DIVIDER_DENOM / (1 << 12);
        defmt::println!("VBUS = {} mV", vbus_millivolts);
        /*if vbus_millivolts < 20000 {
            power_ctrl.disable_19v();
            power_ctrl.disable_24v();
            defmt::println!("BROWNOUT!!!");
            onboard_leds.display(&[RED; 3]);
            while vbus_millivolts < 20500 {
                let vbus_sample = adc4.convert(&vbus_sense, SampleTime::Cycles_640_5) as u32;
                vbus_millivolts = vbus_sample * 3200 * VBUS_DIVIDER_DENOM / (1 << 12);
                delay.delay(1.millis());
            }
            defmt::println!("back to normal");
            onboard_leds.display(&[Colour {r: 0, g: 255, b: 0}; 3]);
            power_ctrl.enable_19v();
            power_ctrl.enable_24v();
        }*/
        delay.delay(1.millis());
    }*/

        let cpwrdata = power_ctrl.tick();
        ext_led_ctrl.ctrl.0.display(&[RED; 100]);
        ext_led_ctrl.ctrl.1.display(&[BLUE; 100]);
        ext_led_ctrl.ctrl.2.display(&[MAGENTA; 100]);
        ext_led_ctrl.ctrl.3.display(&[YELLOW; 100]);
        let v24v_cv = cpwrdata.v24v_centivolts as u64;
        rolling_24v_sum_centivolts += v24v_cv;
        num_samples += 1;
        if v24v_cv < min_24v_centivolts { min_24v_centivolts = v24v_cv }
        if v24v_cv > max_24v_centivolts { max_24v_centivolts = v24v_cv }
        let avg_24v_centivolts: u32 = (rolling_24v_sum_centivolts / num_samples) as u32;
        defmt::println!("+24V line: min {}.{:02} V, curr {}.{:02} V, max {}.{:02} V\tdrawing {} mA",
            min_24v_centivolts / 100,
            min_24v_centivolts % 100,
            v24v_cv / 100,
            v24v_cv % 100,
            max_24v_centivolts / 100,
            max_24v_centivolts % 100,
            cpwrdata.i24v_milliamps
        );
        defmt::println!("+24V temperature\t{} degrees C", cpwrdata.t24v_celsius);
        let v19v_cv = cpwrdata.v19v_centivolts as u64;
        rolling_19v_sum_centivolts += v19v_cv;
        if v19v_cv < min_19v_centivolts { min_19v_centivolts = v19v_cv }
        if v19v_cv > max_19v_centivolts { max_19v_centivolts = v19v_cv }
        let avg_19v_centivolts: u32 = (rolling_19v_sum_centivolts / num_samples) as u32;
        defmt::println!("+19V line: min {}.{:02} V, curr {}.{:02} V, max {}.{:02} V\tdrawing {} mA",
            min_19v_centivolts / 100,
            min_19v_centivolts % 100,
            v19v_cv / 100,
            v19v_cv % 100,
            max_19v_centivolts / 100,
            max_19v_centivolts % 100,
            cpwrdata.i19v_milliamps
        );
        defmt::println!("+19V temperature\t{} degrees C", cpwrdata.t19v_celsius);
        defmt::println!("+5V temperature\t{} degrees C", cpwrdata.t5v_celsius);
        let vbus_sample = adc4.convert(&vbus_sense, SampleTime::Cycles_640_5) as u32;
        const VBUS_DIVIDER_DENOM: u32 = 23; // 10k - 220k divider
        defmt::println!("VBUS = {} mV", vbus_sample * 3200 * VBUS_DIVIDER_DENOM / (1 << 12));
        let ledpwrdata = ext_led_ctrl.tick();
        defmt::println!("+5V line: curr {}.{:02} V", ledpwrdata.v5v_centivolts / 100, ledpwrdata.v5v_centivolts % 100);
        defmt::println!("+5V A: {} mA\tB: {} mA", ledpwrdata.ia_milliamps, ledpwrdata.ib_milliamps);
        defmt::println!("+12V temperature\t{} degrees C", ledpwrdata.t12v_celsius);
    }
}

// NOTE: fill this with any state that the comms (CAN FD) handler needs that isn't part of the
// subsystem
struct CommsState {

}

// NOTE: replace with your actual subsystem
struct Subsystem {
    hue: u8,
}

struct CommsHandler<'a> {
    state: &'a mut CommsState,
    subsystem: &'a mut Subsystem,
}

impl<T: Transport> TransferHandler<T> for CommsHandler<'_> {
    fn handle_message<N: Node<Transport = T>>(
        &mut self,
        _node: &mut N,
        transfer: &MessageTransfer<alloc::vec::Vec<u8>, T>,
    ) -> bool {
        // NOTE: add your message handler here
        defmt::debug!("Received a message with subject ID {}", u16::from(transfer.header.subject));
        match transfer.header.subject {
            // TODO: add a new case for each subject you want to handle
            LED_UPDATE_SUBJECT => {
                let new_hue = natural8_1_0::Natural8::deserialize_from_bytes(&transfer.payload).unwrap();
                self.subsystem.hue = new_hue.value;
            },
            _ => {},
        }
        true
    }

    fn handle_request<N: Node<Transport = T>>(
        &mut self,
        node: &mut N,
        token: canadensis::ResponseToken<T>,
        transfer: &canadensis::core::transfer::ServiceTransfer<alloc::vec::Vec<u8>, T>,
    ) -> bool {
        if transfer.header.service != EXECUTE_COMMAND_SERVICE {
            return false;
        }

        let req =
            ExecuteCommandRequest::deserialize_from_bytes(transfer.payload.as_slice()).unwrap();
        match req.command {
            // handle COMMAND_RESTART
            ExecuteCommandRequest::COMMAND_RESTART => {
                defmt::println!("bazinga!");
                stm32g4xx_hal::stm32g4::stm32g474::SCB::sys_reset();
            }
            // NOTE: add any other request handlers here
            _ => {
                node.send_response(
                    token,
                    1000.millis(),
                    &ExecuteCommandResponse {
                        status: ExecuteCommandResponse::STATUS_BAD_COMMAND,
                        output: Vec::new(),
                    },
                )
                .unwrap();
            }
        }

        true
    }
}
