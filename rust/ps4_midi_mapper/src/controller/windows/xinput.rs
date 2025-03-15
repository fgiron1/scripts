// Updated file: src/controller/windows/xinput.rs

use rusty_xinput::{XInputHandle, XInputState};
use crate::controller::{Controller, types::{ControllerEvent, Button, Axis, DeviceInfo}};
use std::{any::Any, error::Error};
use std::collections::HashMap;

// XInput constants
const XINPUT_GAMEPAD_DPAD_UP: u16 = 0x0001;
const XINPUT_GAMEPAD_DPAD_DOWN: u16 = 0x0002;
const XINPUT_GAMEPAD_DPAD_LEFT: u16 = 0x0004;
const XINPUT_GAMEPAD_DPAD_RIGHT: u16 = 0x0008;
const XINPUT_GAMEPAD_START: u16 = 0x0010;
const XINPUT_GAMEPAD_BACK: u16 = 0x0020;
const XINPUT_GAMEPAD_LEFT_THUMB: u16 = 0x0040;
const XINPUT_GAMEPAD_RIGHT_THUMB: u16 = 0x0080;
const XINPUT_GAMEPAD_LEFT_SHOULDER: u16 = 0x0100;
const XINPUT_GAMEPAD_RIGHT_SHOULDER: u16 = 0x0200;
const XINPUT_GAMEPAD_GUIDE: u16 = 0x0400;
const XINPUT_GAMEPAD_A: u16 = 0x1000;
const XINPUT_GAMEPAD_B: u16 = 0x2000;
const XINPUT_GAMEPAD_X: u16 = 0x4000;
const XINPUT_GAMEPAD_Y: u16 = 0x8000;

// Stick deadzone
const XINPUT_GAMEPAD_LEFT_THUMB_DEADZONE: i16 = 7849;
const XINPUT_GAMEPAD_RIGHT_THUMB_DEADZONE: i16 = 8689;
const XINPUT_GAMEPAD_TRIGGER_THRESHOLD: u8 = 30;

pub struct XInputController {
    handle: XInputHandle,
    port: u32,
    last_state: XInputState,
    button_states: HashMap<Button, bool>,
    axis_values: HashMap<Axis, f32>,
}

impl XInputController {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        // Try to load XInput
        let handle = match XInputHandle::load_default() {
            Ok(handle) => handle,
            Err(_) => return Err("Failed to load XInput".into()),
        };
        
        // Try all four possible XInput ports
        for port in 0..4 {
            if let Ok(state) = handle.get_state(port) {
                println!("Found Xbox-compatible controller on port {}", port);
                
                return Ok(Self {
                    handle,
                    port,
                    last_state: state,
                    button_states: HashMap::new(),
                    axis_values: HashMap::new(),
                });
            }
        }
        
        Err("No XInput controller found".into())
    }
    
    // Normalize stick input with deadzone handling
    fn normalize_stick(&self, value: i16, deadzone: i16) -> f32 {
        if value.abs() < deadzone {
            return 0.0;
        }
        
        // Determine direction
        let value_f = value as f32;
        let sign = if value_f < 0.0 { -1.0 } else { 1.0 };
        
        // Get absolute value and apply deadzone compensation
        let abs_value = value_f.abs();
        let normalized = ((abs_value - deadzone as f32) / (32767.0 - deadzone as f32)).min(1.0);
        
        // Return signed normalized value
        sign * normalized
    }
    
    // Normalize trigger input
    fn normalize_trigger(&self, value: u8) -> f32 {
        if value < XINPUT_GAMEPAD_TRIGGER_THRESHOLD {
            return 0.0;
        }
        
        // Map from threshold-255 to 0.0-1.0
        let normalized = (value - XINPUT_GAMEPAD_TRIGGER_THRESHOLD) as f32 / 
                         (255 - XINPUT_GAMEPAD_TRIGGER_THRESHOLD) as f32;
        
        normalized.min(1.0)
    }
    
    // Process button change for more consistency with other controllers
    fn process_button_change(&mut self, button: Button, is_pressed: bool, events: &mut Vec<ControllerEvent>) {
        let prev_state = self.button_states.get(&button).copied().unwrap_or(false);
        
        if prev_state != is_pressed {
            events.push(ControllerEvent::ButtonPress {
                button,
                pressed: is_pressed,
            });
            
            self.button_states.insert(button, is_pressed);
        }
    }
    
    // Process axis change for more consistency with other controllers
    fn process_axis_change(&mut self, axis: Axis, value: f32, events: &mut Vec<ControllerEvent>) {
        let prev_value = self.axis_values.get(&axis).copied().unwrap_or(0.0);
        
        // Use different sensitivity thresholds based on axis type
        let threshold = match axis {
            Axis::L2 | Axis::R2 => 0.005, // More sensitive for triggers
            _ => 0.01,           // Default for sticks
        };
        
        if (value - prev_value).abs() > threshold {
            events.push(ControllerEvent::AxisMove {
                axis,
                value,
            });
            
            self.axis_values.insert(axis, value);
        }
    }
}

impl Controller for XInputController {
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn Error>> {
        let mut events = Vec::new();
        
        // Get current state
        let new_state = match self.handle.get_state(self.port) {
            Ok(state) => state,
            Err(_) => {
                return Err("Controller disconnected".into());
            }
        };
        
        // Process button changes
        let old_buttons = self.last_state.raw.Gamepad.wButtons;
        let new_buttons = new_state.raw.Gamepad.wButtons;
        
        if old_buttons != new_buttons {
            // Process D-Pad
            self.process_button_change(Button::DpadUp, 
                (new_buttons & XINPUT_GAMEPAD_DPAD_UP) != 0, &mut events);
            self.process_button_change(Button::DpadDown, 
                (new_buttons & XINPUT_GAMEPAD_DPAD_DOWN) != 0, &mut events);
            self.process_button_change(Button::DpadLeft, 
                (new_buttons & XINPUT_GAMEPAD_DPAD_LEFT) != 0, &mut events);
            self.process_button_change(Button::DpadRight, 
                (new_buttons & XINPUT_GAMEPAD_DPAD_RIGHT) != 0, &mut events);
            
            // Process face buttons
            self.process_button_change(Button::Cross, 
                (new_buttons & XINPUT_GAMEPAD_A) != 0, &mut events);
            self.process_button_change(Button::Circle, 
                (new_buttons & XINPUT_GAMEPAD_B) != 0, &mut events);
            self.process_button_change(Button::Square, 
                (new_buttons & XINPUT_GAMEPAD_X) != 0, &mut events);
            self.process_button_change(Button::Triangle, 
                (new_buttons & XINPUT_GAMEPAD_Y) != 0, &mut events);
            
            // Process shoulder buttons
            self.process_button_change(Button::L1, 
                (new_buttons & XINPUT_GAMEPAD_LEFT_SHOULDER) != 0, &mut events);
            self.process_button_change(Button::R1, 
                (new_buttons & XINPUT_GAMEPAD_RIGHT_SHOULDER) != 0, &mut events);
            
            // Process special buttons
            self.process_button_change(Button::Options, 
                (new_buttons & XINPUT_GAMEPAD_START) != 0, &mut events);
            self.process_button_change(Button::Share, 
                (new_buttons & XINPUT_GAMEPAD_BACK) != 0, &mut events);
            self.process_button_change(Button::PS, 
                (new_buttons & XINPUT_GAMEPAD_GUIDE) != 0, &mut events);
            
            // Process thumbstick clicks
            self.process_button_change(Button::L3, 
                (new_buttons & XINPUT_GAMEPAD_LEFT_THUMB) != 0, &mut events);
            self.process_button_change(Button::R3, 
                (new_buttons & XINPUT_GAMEPAD_RIGHT_THUMB) != 0, &mut events);
        }
        
        // Process left stick
        let left_x = self.normalize_stick(
            new_state.raw.Gamepad.sThumbLX, 
            XINPUT_GAMEPAD_LEFT_THUMB_DEADZONE
        );
        self.process_axis_change(Axis::LeftStickX, left_x, &mut events);
        
        let left_y = -self.normalize_stick(
            new_state.raw.Gamepad.sThumbLY, 
            XINPUT_GAMEPAD_LEFT_THUMB_DEADZONE
        );
        self.process_axis_change(Axis::LeftStickY, left_y, &mut events);
        
        // Process right stick
        let right_x = self.normalize_stick(
            new_state.raw.Gamepad.sThumbRX, 
            XINPUT_GAMEPAD_RIGHT_THUMB_DEADZONE
        );
        self.process_axis_change(Axis::RightStickX, right_x, &mut events);
        
        let right_y = -self.normalize_stick(
            new_state.raw.Gamepad.sThumbRY, 
            XINPUT_GAMEPAD_RIGHT_THUMB_DEADZONE
        );
        self.process_axis_change(Axis::RightStickY, right_y, &mut events);
        
        // Process triggers
        let left_trigger = self.normalize_trigger(new_state.raw.Gamepad.bLeftTrigger);
        self.process_axis_change(Axis::L2, left_trigger, &mut events);
        
        let right_trigger = self.normalize_trigger(new_state.raw.Gamepad.bRightTrigger);
        self.process_axis_change(Axis::R2, right_trigger, &mut events);
        
        // Update last state
        self.last_state = new_state;
        
        Ok(events)
    }
    
    fn get_device_info(&self) -> DeviceInfo {
        DeviceInfo {
            vid: 0x045E, // Microsoft
            pid: 0x028E, // Xbox Controller (generic)
            manufacturer: "Microsoft".to_string(),
            product: "Xbox Controller".to_string(),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}