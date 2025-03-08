// src/controller/directinput.rs
use super::types::{Button, Axis, ControllerEvent};
use super::Controller;
use hidapi::HidApi;
use std::error::Error;

pub struct DirectInputController {
    device: hidapi::HidDevice,
}

impl DirectInputController {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let api = HidApi::new()?;
        let device = api.open(0x054C, 0x09CC)?; // Sony DS4 Vendor/Product IDs

        Ok(Self { device })
    }
}

impl Controller for DirectInputController {
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn Error>> {
        let mut events = Vec::new();
        let mut buffer = [0u8; 64];
    
        if self.device.read(&mut buffer)? > 0 {
            // Buttons (byte 5)
            let buttons = buffer[5];
            events.extend(
                [
                    (0x10, Button::Cross), (0x20, Button::Circle),
                    (0x40, Button::Square), (0x80, Button::Triangle),
                    (0x01, Button::L1), (0x02, Button::R1),
                    (0x04, Button::Share), (0x08, Button::Options),
                    (0x10, Button::L3), (0x20, Button::R3)
                ].iter().filter_map(|(mask, btn)| {
                    (buttons & mask != 0).then(|| ControllerEvent::ButtonPress {
                        button: *btn,
                        pressed: true
                    })
                })
            );
    
            // Triggers (bytes 8-9)
            events.push(ControllerEvent::AxisMove {
                axis: Axis::L2,
                value: buffer[8] as f32 / 255.0
            });
            events.push(ControllerEvent::AxisMove {
                axis: Axis::R2,
                value: buffer[9] as f32 / 255.0
            });
    
            // Sticks (bytes 1-4)
            events.push(ControllerEvent::AxisMove {
                axis: Axis::LeftStickX,
                value: (buffer[1] as i16 - 128) as f32 / 128.0
            });
            events.push(ControllerEvent::AxisMove {
                axis: Axis::LeftStickY,
                value: (buffer[2] as i16 - 128) as f32 / 128.0
            });
            events.push(ControllerEvent::AxisMove {
                axis: Axis::RightStickX,
                value: (buffer[3] as i16 - 128) as f32 / 128.0
            });
            events.push(ControllerEvent::AxisMove {
                axis: Axis::RightStickY,
                value: (buffer[4] as i16 - 128) as f32 / 128.0
            });
        }
    
        Ok(events)
    }
}