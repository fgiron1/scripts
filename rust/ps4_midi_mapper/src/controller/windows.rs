// src/controller/windows.rs
use super::types::{Button, Axis, ControllerEvent};
use super::Controller;
use rusty_xinput::XInputHandle;
use std::error::Error;

pub struct XInputController {
    xinput: XInputHandle,
    controller_id: u32,
    last_buttons: u16,
    last_left_trigger: u8,
    last_right_trigger: u8,
    last_thumb_lx: i16,
    last_thumb_ly: i16,
    last_thumb_rx: i16,
    last_thumb_ry: i16,
}

impl XInputController {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let xinput = XInputHandle::load_default()
            .map_err(|e| format!("XInput failed to load: {:?}\nEnsure ViGEmBus is installed", e))?;

        println!("Searching for controllers...");

        for id in 0..4 {
            if xinput.get_state(id).is_ok() {
                println!("✓ Controller found on port {}", id);
                return Ok(Self {
                    xinput,
                    controller_id: id,
                    last_buttons: 0,
                    last_left_trigger: 0,
                    last_right_trigger: 0,
                    last_thumb_lx: 0,
                    last_thumb_ly: 0,
                    last_thumb_rx: 0,
                    last_thumb_ry: 0,
                });
            }
        }

        Err("No controller found. Ensure:\n1. DS4Windows is RUNNING\n2. Controller is connected via USB/BT\n3. Controller is in 'XInput' mode in DS4Windows".into())
    }
}

impl Controller for XInputController {
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn Error>> {
        let mut events = Vec::new();

        match self.xinput.get_state(self.controller_id) {
            Ok(state) => {
                // Process buttons
                let buttons = state.raw.Gamepad.wButtons;
                let button_change = buttons ^ self.last_buttons;

                if button_change != 0 {
                    for &(xinput_button, button) in BUTTON_MAPPINGS.iter() {
                        if button_change & xinput_button != 0 {
                            let pressed = (buttons & xinput_button) != 0;
                            events.push(ControllerEvent::ButtonPress {
                                button,
                                pressed,
                            });
                        }
                    }
                }

                // Process analog sticks
                if state.raw.Gamepad.sThumbLX != self.last_thumb_lx {
                    events.push(ControllerEvent::AxisMove {
                        axis: Axis::LeftStickX,
                        value: normalize_axis(state.raw.Gamepad.sThumbLX, 32767),
                    });
                    self.last_thumb_lx = state.raw.Gamepad.sThumbLX;
                }

                if state.raw.Gamepad.sThumbLY != self.last_thumb_ly {
                    events.push(ControllerEvent::AxisMove {
                        axis: Axis::LeftStickY,
                        value: normalize_axis(state.raw.Gamepad.sThumbLY, 32767),
                    });
                    self.last_thumb_ly = state.raw.Gamepad.sThumbLY;
                }

                if state.raw.Gamepad.sThumbRX != self.last_thumb_rx {
                    events.push(ControllerEvent::AxisMove {
                        axis: Axis::RightStickX,
                        value: normalize_axis(state.raw.Gamepad.sThumbRX, 32767),
                    });
                    self.last_thumb_rx = state.raw.Gamepad.sThumbRX;
                }

                if state.raw.Gamepad.sThumbRY != self.last_thumb_ry {
                    events.push(ControllerEvent::AxisMove {
                        axis: Axis::RightStickY,
                        value: normalize_axis(state.raw.Gamepad.sThumbRY, 32767),
                    });
                    self.last_thumb_ry = state.raw.Gamepad.sThumbRY;
                }

                // Process triggers
                if state.raw.Gamepad.bLeftTrigger != self.last_left_trigger {
                    events.push(ControllerEvent::AxisMove {
                        axis: Axis::L2,
                        value: normalize_axis(state.raw.Gamepad.bLeftTrigger as i16, 255),
                    });
                    self.last_left_trigger = state.raw.Gamepad.bLeftTrigger;
                }

                if state.raw.Gamepad.bRightTrigger != self.last_right_trigger {
                    events.push(ControllerEvent::AxisMove {
                        axis: Axis::R2,
                        value: normalize_axis(state.raw.Gamepad.bRightTrigger as i16, 255),
                    });
                    self.last_right_trigger = state.raw.Gamepad.bRightTrigger;
                }

                self.last_buttons = buttons;
            }
            Err(_) => {
                // No error, just return empty events if controller disconnected
            }
        }

        Ok(events)
    }
}

// Normalize an axis value to the range -1.0 to 1.0
fn normalize_axis(value: i16, max: i16) -> f32 {
    value as f32 / max as f32
}

// Button mapping
const BUTTON_MAPPINGS: [(u16, Button); 14] = [
    (0x1000, Button::Cross),    // A
    (0x2000, Button::Circle),   // B
    (0x4000, Button::Square),   // X
    (0x8000, Button::Triangle), // Y
    (0x0020, Button::Share),    // Back
    (0x0010, Button::Options),  // Start
    (0x0001, Button::DpadUp),   // D-pad Up
    (0x0002, Button::DpadDown), // D-pad Down
    (0x0004, Button::DpadLeft), // D-pad Left
    (0x0008, Button::DpadRight),// D-pad Right
    (0x0100, Button::L1),       // Left Bumper
    (0x0200, Button::R1),       // Right Bumper
    (0x0040, Button::L3),       // Left Stick Click
    (0x0080, Button::R3),       // Right Stick Click
];