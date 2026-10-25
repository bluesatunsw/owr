use core::cell::UnsafeCell;

use alloc::vec::Vec;
use canadensis::core::transfer::{MessageTransfer, ServiceTransfer};
use canadensis::core::transport::Transport;
use canadensis::encoding::Deserialize;
use canadensis::{ResponseToken, TransferHandler};
use canadensis_data_types::reg::udral::service::actuator;
use cortex_m::interrupt::Mutex;
use embedded_common::dprintln;

use crate::config::CommsConfig;
use crate::state::{CommutationMode, CommutationState};

pub const CYPHAL_CONCURRENT_TRANSFERS: usize = 4;
pub const CYPHAL_NUM_TOPICS: usize = 8;
pub const CYPHAL_NUM_SERVICES: usize = 8;

pub struct CommSystem {
    pub config: CommsConfig,
    pub commutation_state: &'static Mutex<UnsafeCell<CommutationState>>,
}

impl CommSystem {
    // TODO implement sending for the periodic data we need to send to comply with UDRAL Servo
    //pub fn tick(node: &mut Node) {}
}

impl<T: Transport> TransferHandler<T> for CommSystem {
    fn handle_message<N>(&mut self, _node: &mut N, transfer: &MessageTransfer<Vec<u8>, T>) -> bool
    where
        N: canadensis::Node<Transport = T>,
    {
        match transfer.header.subject {
            _ if self.config.ctrl_duty == transfer.header.subject => {
                // TODO this deadband is stupid, should have Readiness heartbeat
                let Ok(duty) = actuator::common::sp::scalar_0_1::Scalar::deserialize_from_bytes(
                    &transfer.payload,
                ) else {
                    return false;
                };

                cortex_m::interrupt::free(|cs| unsafe {
                    if f32::from(duty.value).abs() < 0.02 {
                        (*self.commutation_state.borrow(cs).get()).mode = CommutationMode::Disabled;
                    } else {
                        (*self.commutation_state.borrow(cs).get()).mode =
                            CommutationMode::DutyCycle {
                                duty: f32::from(duty.value),
                            };
                    }
                });
                true
            }
            _ => {
                dprintln!(0, "[WARN] Unknown message RECV'd");
                false
            }
        }
    }

    fn handle_request<N>(
        &mut self,
        _node: &mut N,
        _token: ResponseToken<T>,
        _transfer: &ServiceTransfer<Vec<u8>, T>,
    ) -> bool
    where
        N: canadensis::Node<Transport = T>,
    {
        dprintln!(0, "[INFO] COMMS.REQ");
        false
    }

    fn handle_response<N>(&mut self, _node: &mut N, _transfer: &ServiceTransfer<Vec<u8>, T>) -> bool
    where
        N: canadensis::Node<Transport = T>,
    {
        dprintln!(0, "[INFO] COMMS.RESP");
        false
    }
}
