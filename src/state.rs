use std::{iter::zip, sync::Arc, time::Duration};

use tokio::sync::Mutex;

use crate::{device::Device, error::MirajazzError, types::DeviceInput};

/// Tells what changed in button states
#[derive(Copy, Clone, Debug, Hash)]
pub enum DeviceStateUpdate {
    /// Button got pressed down
    ButtonDown(u8),

    /// Button got released
    ButtonUp(u8),

    /// Encoder got pressed down
    EncoderDown(u8),

    /// Encoder was released from being pressed down
    EncoderUp(u8),

    /// Encoder was twisted
    EncoderTwist(u8, i8),
}

#[derive(Default)]
pub struct DeviceState {
    /// Buttons include Touch Points state
    pub buttons: Vec<bool>,
    pub encoders: Vec<bool>,
}

/// Button reader that keeps state of the device and returns events instead of full states
pub struct DeviceStateReader {
    pub device: Arc<Device>,
    pub states: Mutex<DeviceState>,
    pub process_input: fn(u8, u8) -> Result<DeviceInput, MirajazzError>,
}

impl DeviceStateReader {
    /// Reads states and returns updates
    pub async fn read(
        &self,
        timeout: Option<Duration>,
    ) -> Result<Vec<DeviceStateUpdate>, MirajazzError> {
        let input = self.device.read_input(timeout, self.process_input).await?;

        Ok(self.input_to_updates(input).await)
    }

    async fn input_to_updates(&self, input: DeviceInput) -> Vec<DeviceStateUpdate> {
        let mut my_states = self.states.lock().await;
        let mut updates = vec![];

        match input {
            DeviceInput::ButtonStateChange(buttons) => {
                for (index, (their, mine)) in
                    zip(buttons.iter(), my_states.buttons.iter()).enumerate()
                {
                    if !self.device.supports_both_states() {
                        if *their {
                            updates.push(DeviceStateUpdate::ButtonDown(index as u8));
                            updates.push(DeviceStateUpdate::ButtonUp(index as u8));
                        }
                    } else if their != mine {
                        if *their {
                            updates.push(DeviceStateUpdate::ButtonDown(index as u8));
                        } else {
                            updates.push(DeviceStateUpdate::ButtonUp(index as u8));
                        }
                    }
                }

                my_states.buttons = buttons;
            }

            DeviceInput::EncoderStateChange(encoders) => {
                for (index, (their, mine)) in
                    zip(encoders.iter(), my_states.encoders.iter()).enumerate()
                {
                    if !self.device.supports_both_states() {
                        if *their {
                            updates.push(DeviceStateUpdate::EncoderDown(index as u8));
                            updates.push(DeviceStateUpdate::EncoderUp(index as u8));
                        }
                    } else if *their != *mine {
                        if *their {
                            updates.push(DeviceStateUpdate::EncoderDown(index as u8));
                        } else {
                            updates.push(DeviceStateUpdate::EncoderUp(index as u8));
                        }
                    }
                }

                my_states.encoders = encoders;
            }

            DeviceInput::EncoderTwist(twist) => {
                for (index, change) in twist.iter().enumerate() {
                    if *change != 0 {
                        updates.push(DeviceStateUpdate::EncoderTwist(index as u8, *change));
                    }
                }
            }
            _ => {}
        }

        drop(my_states);

        updates
    }
}
