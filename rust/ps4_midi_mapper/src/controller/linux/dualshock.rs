use gilrs::{Gilrs, Event, Button as GilrsButton, Axis as GilrsAxis, EventType};
use evdev::{Device, AbsoluteAxisType, InputEventKind};
use std::{fs, path::Path};
use crate::controller::{Controller, types::{ControllerEvent, Button, Axis, DeviceInfo}};
use std::error::Error;

const TOUCHPAD_VENDOR: &str = "Sony";
const TOUCHPAD_PRODUCT: &str = "Wireless Controller";

pub struct DualShockController {
    gilrs: Gilrs,
    touchpad_device: Option<Device>,
    gamepad_id: usize,
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
        })
    }
    
    fn find_touchpad_device() -> Result<Option<Device>, Box<dyn Error>> {
        for entry in fs::read_dir("/dev/input")? {
            let path = entry?.path();
            
            if let Ok(device) = Device::open(&path) {
                // Check if this is the touchpad for a DualShock controller
                if let Some(name) = device.name() {
                    if name.contains(TOUCHPAD_PRODUCT) {
                        if let Some(abs_info) = device.supported_absolute_axes() {
                            if abs_info.contains(AbsoluteAxisType::ABS_MT_POSITION_X) {
                                // Found the touchpad
                                device.grab()?;
                                return Ok(Some(device));
                            }
                        }
                    }
                }
            }
        }
        
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
            for ev in touchpad.fetch_events()? {
                if let InputEventKind::Absolute(abs_axis) = ev.kind() {
                    match abs_axis.0 {
                        0x35 => { // ABS_MT_POSITION_X
                            events.push(ControllerEvent::TouchpadMove {
                                x: Some(ev.value()),
                                y: None,
                            });
                        }
                        0x36 => { // ABS_MT_POSITION_Y
                            events.push(ControllerEvent::TouchpadMove {
                                x: None,
                                y: Some(ev.value()),
                            });
                        }
                        _ => {}
                    }
                }
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