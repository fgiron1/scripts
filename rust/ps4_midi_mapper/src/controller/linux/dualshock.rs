// Improvements for the Linux touchpad support in src/controller/linux/dualshock.rs

use gilrs::{Gilrs, Event, Button as GilrsButton, Axis as GilrsAxis, EventType};
use evdev::{Device, AbsoluteAxisType, InputEventKind, EventType as EvdevEventType};
use std::{fs, path::Path};
use crate::controller::{Controller, types::{ControllerEvent, Button, Axis, DeviceInfo}};
use std::error::Error;
use std::collections::HashMap;

// Constants for the touchpad
const TOUCHPAD_X_MAX: i32 = 1920;
const TOUCHPAD_Y_MAX: i32 = 942;

pub struct DualShockController {
    gilrs: Gilrs,
    touchpad_device: Option<Device>,
    gamepad_id: usize,
    touchpad_x: i32,
    touchpad_y: i32,
    touchpad_active: bool,
}

impl DualShockController {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let gilrs = Gilrs::new()?;
        
        // Find a compatible game controller
        let gamepad_id = gilrs.gamepads()
            .find(|(_, gamepad)| {
                let name = gamepad.name();
                name.contains("DualShock") || name.contains("Wireless Controller")
            })
            .map(|(id, _)| id.into_usize())
            .ok_or("No compatible controller found")?;
        
        // Find the touchpad device if available
        let touchpad_device = Self::find_touchpad_device()?;
        
        Ok(Self {
            gilrs,
            touchpad_device,
            gamepad_id,
            touchpad_x: 0,
            touchpad_y: 0,
            touchpad_active: false,
        })
    }
        
    fn find_touchpad_device() -> Result<Option<Device>, Box<dyn Error>> {
        println!("Searching for touchpad device...");
        
        for entry in fs::read_dir("/dev/input")? {
            let path = entry?.path();
            
            if let Ok(device) = Device::open(&path) {
                // Check if this is the touchpad for a DualShock controller
                if let Some(name) = device.name() {
                    println!("Found input device: {}", name);
                    
                    // Check if it's a touchpad
                    if name.contains("Touchpad") || 
                    name.contains("Touch") || 
                    name.contains("SONY") || 
                    name.contains("Sony") {
                        
                        if let Some(abs_info) = device.supported_absolute_axes() {
                            // Check for typical touchpad axes
                            if abs_info.contains(AbsoluteAxisType::ABS_MT_POSITION_X) ||
                            abs_info.contains(AbsoluteAxisType::ABS_MT_TRACKING_ID) ||
                            abs_info.contains(AbsoluteAxisType::ABS_X) {
                                
                                println!("Found touchpad device: {}", name);
                                
                                // Try to grab the device - if it fails, we'll still try to use it
                                let _ = device.grab();
                                return Ok(Some(device));
                            }
                        }
                    }
                }
            }
        }
        
        println!("No touchpad device found");
        Ok(None)
    }
}

impl Controller for DualShockController {
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn Error>> {
        let mut events = Vec::new();
        
        // Process gilrs events
        while let Some(Event { id, event, .. }) = self.gilrs.next_event() {
            // Only process events from our selected gamepad
            if id.into_usize() != self.gamepad_id {
                continue;
            }
            
            match event {
                EventType::ButtonPressed(button, _) => {
                    if let Some(mapped_button) = map_button(button) {
                        events.push(ControllerEvent::ButtonPress {
                            button: mapped_button,
                            pressed: true,
                        });
                    }
                }
                
                EventType::ButtonReleased(button, _) => {
                    if let Some(mapped_button) = map_button(button) {
                        events.push(ControllerEvent::ButtonPress {
                            button: mapped_button,
                            pressed: false,
                        });
                    }
                }
                
                EventType::AxisChanged(axis, value, _) => {
                    if let Some(mapped_axis) = map_axis(axis) {
                        events.push(ControllerEvent::AxisMove {
                            axis: mapped_axis,
                            value,
                        });
                    }
                }
                
                _ => {}
            }
        }
        
        // Process touchpad events if available
        if let Some(touchpad) = &mut self.touchpad_device {
            // Track if we've seen X or Y changes in this batch
            let mut x_updated = false;
            let mut y_updated = false;
            let mut touch_started = false;
            let mut touch_ended = false;
            
            // Process all pending events from the touchpad
            for ev in touchpad.fetch_events()? {
                match ev.kind() {
                    // Handle multitouch events
                    InputEventKind::Absolute(AbsoluteAxisType::ABS_MT_POSITION_X) => {
                        self.touchpad_x = ev.value();
                        x_updated = true;
                    }
                    InputEventKind::Absolute(AbsoluteAxisType::ABS_MT_POSITION_Y) => {
                        self.touchpad_y = ev.value();
                        y_updated = true;
                    }
                    // Handle tracking ID for touch start/end
                    InputEventKind::Absolute(AbsoluteAxisType::ABS_MT_TRACKING_ID) => {
                        if ev.value() == -1 {
                            touch_ended = true;
                        } else {
                            touch_started = true;
                            self.touchpad_active = true;
                        }
                    }
                    // Handle normal absolute events (single touch)
                    InputEventKind::Absolute(AbsoluteAxisType::ABS_X) => {
                        self.touchpad_x = ev.value();
                        x_updated = true;
                    }
                    InputEventKind::Absolute(AbsoluteAxisType::ABS_Y) => {
                        self.touchpad_y = ev.value();
                        y_updated = true;
                    }
                    // Key events for touch/release
                    InputEventKind::Key(code) => {
                        // Key code 330 is typically BTN_TOUCH
                        if code.0 == 330 {
                            if ev.value() == 1 {
                                touch_started = true;
                                self.touchpad_active = true;
                            } else if ev.value() == 0 {
                                touch_ended = true;
                            }
                        }
                    }
                    // Sync event indicates the end of an event packet
                    InputEventKind::Synchronization(_) => {
                        // If we have new coordinates and touch is active, send them
                        if self.touchpad_active && (x_updated || y_updated) {
                            events.push(ControllerEvent::TouchpadMove {
                                x: if x_updated { Some(self.touchpad_x) } else { None },
                                y: if y_updated { Some(self.touchpad_y) } else { None },
                            });
                            
                            // Also map to axes for MIDI mapping
                            if x_updated {
                                let x_norm = (self.touchpad_x as f32 / TOUCHPAD_X_MAX as f32) * 2.0 - 1.0;
                                events.push(ControllerEvent::AxisMove {
                                    axis: Axis::TouchpadX,
                                    value: x_norm,
                                });
                            }
                            
                            if y_updated {
                                // Invert Y since touchpad coordinates are top-to-bottom
                                let y_norm = -((self.touchpad_y as f32 / TOUCHPAD_Y_MAX as f32) * 2.0 - 1.0);
                                events.push(ControllerEvent::AxisMove {
                                    axis: Axis::TouchpadY,
                                    value: y_norm,
                                });
                            }
                        }
                        
                        // Reset trackers
                        x_updated = false;
                        y_updated = false;
                    }
                    _ => {}
                }
            }
            
            // Update touchpad active state
            if touch_ended && !touch_started {
                self.touchpad_active = false;
            }
        }
        
        Ok(events)
    }
    
    fn get_device_info(&self) -> DeviceInfo {
        DeviceInfo {
            vid: 0x054C, // Sony
            pid: 0x09CC, // DualShock 4
            manufacturer: "Sony".to_string(),
            product: "DualShock 4".to_string(),
        }
    }
}

// Mapping functions remain the same
fn map_button(button: GilrsButton) -> Option<Button> {
    match button {
        GilrsButton::South => Some(Button::Cross),
        GilrsButton::East => Some(Button::Circle),
        GilrsButton::West => Some(Button::Square),
        GilrsButton::North => Some(Button::Triangle),
        GilrsButton::LeftTrigger => Some(Button::L1),
        GilrsButton::RightTrigger => Some(Button::R1),
        GilrsButton::LeftTrigger2 => Some(Button::L2),
        GilrsButton::RightTrigger2 => Some(Button::R2),
        GilrsButton::Select => Some(Button::Share),
        GilrsButton::Start => Some(Button::Options),
        GilrsButton::Mode => Some(Button::PS),
        GilrsButton::LeftThumb => Some(Button::L3),
        GilrsButton::RightThumb => Some(Button::R3),
        GilrsButton::DPadUp => Some(Button::DpadUp),
        GilrsButton::DPadDown => Some(Button::DpadDown),
        GilrsButton::DPadLeft => Some(Button::DpadLeft),
        GilrsButton::DPadRight => Some(Button::DpadRight),
        _ => None,
    }
}

fn map_axis(axis: GilrsAxis) -> Option<Axis> {
    match axis {
        GilrsAxis::LeftStickX => Some(Axis::LeftStickX),
        GilrsAxis::LeftStickY => Some(Axis::LeftStickY),
        GilrsAxis::RightStickX => Some(Axis::RightStickX),
        GilrsAxis::RightStickY => Some(Axis::RightStickY),
        GilrsAxis::LeftZ => Some(Axis::L2),
        GilrsAxis::RightZ => Some(Axis::R2),
        _ => None,
    }
}