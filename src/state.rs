use async_hid::{AsyncHidRead, DeviceReader};
use futures_lite::FutureExt;
use std::{iter::zip, sync::Arc, time::Duration};
use tokio::{sync::Mutex, time};

use crate::{error::MirajazzError, types::DeviceInput};

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
/// You can only have one active reader per device at a time
pub struct DeviceStateReader {
    pub reader: Arc<Mutex<DeviceReader>>,
    pub states: Mutex<DeviceState>,
    pub process_input: fn(u8, u8) -> Result<DeviceInput, MirajazzError>,
    pub supports_both_states: bool,
}

impl DeviceStateReader {
    /// Reads data from device
    pub async fn raw_read_data(&self, length: usize) -> Result<Vec<u8>, MirajazzError> {
        let mut buf = vec![0u8; length];

        let _size = self.reader.lock().await.read_input_report(&mut buf).await?;

        Ok(buf)
    }

    /// Reads data from device with specified timeout
    ///
    /// Returns [Some] if there was any data, returns [None] if timeout was reached before reading something
    pub async fn raw_read_data_with_timeout(
        &self,
        length: usize,
        timeout: Duration,
    ) -> Result<Option<Vec<u8>>, MirajazzError> {
        let mut buf = vec![0u8; length];

        let size = self
            .reader
            .lock()
            .await
            .read_input_report(&mut buf)
            .or(async {
                time::sleep(timeout).await;
                Ok(0)
            })
            .await?;

        if size == 0 {
            return Ok(None);
        }

        Ok(Some(buf))
    }

    /// Reads current input state from the device and calls provided function for processing
    pub async fn read_input(
        &self,
        timeout: Option<Duration>,
        process_input: fn(u8, u8) -> Result<DeviceInput, MirajazzError>,
    ) -> Result<DeviceInput, MirajazzError> {
        let data = if timeout.is_some() {
            self.raw_read_data_with_timeout(512, timeout.unwrap())
                .await?
        } else {
            Some(self.raw_read_data(512).await?)
        };

        if data.is_none() {
            return Ok(DeviceInput::NoData);
        }

        let data = data.unwrap();

        if data[0] == 0 {
            return Ok(DeviceInput::NoData);
        }

        let state = if self.supports_both_states {
            data[10]
        } else {
            0x1u8
        };

        Ok(process_input(data[9], state)?)
    }

    /// Reads states and returns updates
    pub async fn read(
        &self,
        timeout: Option<Duration>,
    ) -> Result<Vec<DeviceStateUpdate>, MirajazzError> {
        let input = self.read_input(timeout, self.process_input).await?;

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
                    if !self.supports_both_states {
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
                    if !self.supports_both_states {
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
