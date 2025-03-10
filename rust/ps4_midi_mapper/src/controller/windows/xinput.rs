use rusty_xinput::{XInputHandle, XInputState, XInputError};
use crate::controller::{Controller, types::{ControllerEvent, Button, Axis, DeviceInfo}};
use std::error::Error;

const JOYSTICK_DEADZONE: i16 = 2500; // About ~7.5% of i16::MAX
const TRIGGER_DEADZONE: u8 = 5;      // About ~2% of u8::MAX

pub struct XInputController {
    handle: XInputHandle,
    port: u32,
    last_state: XInputState,
}

impl XInputController {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let handle = XInputHandle::load_default()?;
        
        // Try all four possible XInput ports
        for port in 0..4 {
            if let Ok(state) = handle.get_state(port) {
                return Ok(Self {
                    handle,
                    port,
                    last_state: state,
                });
            }
        }
        
        Err("No XInput controller found".into())
    }
    
    fn normalize_stick(&self, value: i16) -> f32 {
        if value.abs() < JOYSTICK_DEADZONE {
            return 0.0;
        }
        
        // Normalize to -1.0 to 1.0 range
        (value as f32 / 32767.0).clamp(-1.0, 1.0)
    }
    
    fn normalize_trigger(&self, value: u8) -> f32 {
        if value < TRIGGER_DEADZONE {
            return 0.0;
        }
        
        // Normalize to 0.0 to 1.0 range
        (value as f32 / 255.0).clamp(0.0, 1.0)
    }
}

impl Controller for XInputController {
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn Error>> {
        let mut events = Vec::new();
        
        // Get current state
        let new_state = match self.handle.get_state(self.port) {
            Ok(state) => state,
            Err(XInputError::DeviceNotConnected) => {
                return Err("Controller disconnected".into());
            }
            Err(e) => return Err(Box::new(e)),
        };
        
        // Process button changes
        let old_buttons = self.last_state.gamepad.buttons.0;
        let new_buttons = new_state.gamepad.buttons.0;
        
        if old_buttons != new_buttons {
            // Check each button
            process_button_change(Button::DpadUp, 0x0001, old_buttons, new_buttons, &mut events);
            process_button_change(Button::DpadDown, 0x0002, old_buttons, new_buttons, &mut events);
            process_button_change(Button::DpadLeft, 0x0004, old_buttons, new_buttons, &mut events);
            process_button_change(Button::DpadRight, 0x0008, old_buttons, new_buttons, &mut events);
            process_button_change(Button::Options, 0x0010, old_buttons, new_buttons, &mut events);
            process_button_change(Button::Share, 0x0020, old_buttons, new_buttons, &mut events);
            process_button_change(Button::L3, 0x0040, old_buttons, new_buttons, &mut events);
            process_button_change(Button::R3, 0x0080, old_buttons, new_buttons, &mut events);
            process_button_change(Button::L1, 0x0100, old_buttons, new_buttons, &mut events);
            process_button_change(Button::R1, 0x0200, old_buttons, new_buttons, &mut events);
            process_button_change(Button::PS, 0x0400, old_buttons, new_buttons, &mut events);
            process_button_change(Button::Cross, 0x1000, old_buttons, new_buttons, &mut events);
            process_button_change(Button::Circle, 0x2000, old_buttons, new_buttons, &mut events);
            process_button_change(Button::Square, 0x4000, old_buttons, new_buttons, &mut events);
            process_button_change(Button::Triangle, 0x8000, old_buttons, new_buttons, &mut events);
        }
        
        // Process left stick
        if new_state.gamepad.thumb_lx != self.last_state.gamepad.thumb_lx {
            events.push(ControllerEvent::AxisMove {
                axis: Axis::LeftStickX,
                value: self.normalize_stick(new_state.gamepad.thumb_lx),
            });
        }
        
        if new_state.gamepad.thumb_ly != self.last_state.gamepad.thumb_ly {
            events.push(ControllerEvent::AxisMove {
                axis: Axis::LeftStickY,
                // Invert Y axis to match expected behavior (up is positive)
                value: self.normalize_stick(new_state.gamepad.thumb_ly) * -1.0,
            });
        }
        
        // Process right stick
        if new_state.gamepad.thumb_rx != self.last_state.gamepad.thumb_rx {
            events.push(ControllerEvent::AxisMove {
                axis: Axis::RightStickX,
                value: self.normalize_stick(new_state.gamepad.thumb_rx),
            });
        }
        
        if new_state.gamepad.thumb_ry != self.last_state.gamepad.thumb_ry {
            events.push(ControllerEvent::AxisMove {
                axis: Axis::RightStickY,
                // Invert Y axis to match expected behavior (up is positive)
                value: self.normalize_stick(new_state.gamepad.thumb_ry) * -1.0,
            });
        }
        
        // Process triggers
        if new_state.gamepad.left_trigger != self.last_state.gamepad.left_trigger {
            events.push(ControllerEvent::AxisMove {
                axis: Axis::L2,
                value: self.normalize_trigger(new_state.gamepad.left_trigger),
            });
        }
        
        if new_state.gamepad.right_trigger != self.last_state.gamepad.right_trigger {
            events.push(ControllerEvent::AxisMove {
                axis: Axis::R2,
                value: self.normalize_trigger(new_state.gamepad.right_trigger),
            });
        }
        
        self.last_state = new_state;
        Ok(events)
    }
    
    fn get_device_info(&self) -> DeviceInfo {
        DeviceInfo {
            vid: 0x045E, // Microsoft
            pid: 0x028E, // Xbox Controller
            manufacturer: "Microsoft".to_string(),
            product: "Xbox Controller".to_string(),
        }
    }
}

fn process_button_change(
    button: Button,
    mask: u16,
    old_buttons: u16,
    new_buttons: u16,
    events: &mut Vec<ControllerEvent>
) {
    let old_state = (old_buttons & mask) != 0;
    let new_state = (new_buttons & mask) != 0;
    
    if old_state != new_state {
        events.push(ControllerEvent::ButtonPress {
            button,
            pressed: new_state,
        });
    }
}