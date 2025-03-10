use windows::{
    core::{Result as WindowsResult},
    Win32::{
        Devices::{
            HumanInterfaceDevice::{
                IDirectInput8W, IDirectInputDevice8W, 
                GUID_Joystick, DISCL_BACKGROUND, DISCL_NONEXCLUSIVE, 
                DIJOYSTATE, DIDEVICEINSTANCEW
            },
        },
        Foundation::HWND,
        System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER}
    },
};
use std::{
    ffi::c_void, 
    mem::size_of,
    sync::Arc,
    error::Error
};
use crate::controller::{Controller, types::{ControllerEvent, Button, Axis, DeviceInfo}};

const JOYSTICK_DEADZONE: i16 = 7840;  // ~25% of i16 range
const TRIGGER_DEADZONE: u8 = 30;      // ~12% of u8 range

/// DirectInput controller implementation
pub struct DirectInputController {
    // For now, we'll keep a simplified structure
    // When implementing for real, this would hold device handles and state
    device_info: DeviceInfo,
}

impl DirectInputController {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        // For testing purposes, this can create a fake controller
        // In production, you would implement the real DirectInput initialization here
        
        #[cfg(debug_assertions)]
        {
            // Only in debug builds, create a test device for development
            Ok(Self {
                device_info: DeviceInfo {
                    vid: 0x1234,
                    pid: 0x5678,
                    manufacturer: "Development".to_string(),
                    product: "DirectInput Test Controller".to_string(),
                }
            })
        }
        
        #[cfg(not(debug_assertions))]
        {
            // In release builds, we need to implement the real DirectInput code
            // This is just a stub for now
            Err("DirectInput support not fully implemented in release mode yet".into())
        }
    }
    
    /// When implementing the full version, this function will initialize DirectInput
    /// Below is a sketch of how it would work (commented out to avoid compilation errors)
    fn _init_directinput() -> Result<(), Box<dyn Error>> {
        // The general approach would be:
        
        // 1. Initialize COM if needed
        // 2. Create the DirectInput interface (IDirectInput8W)
        //    let di = CoCreateInstance(&CLSID_DirectInput8, None, CLSCTX_INPROC_SERVER)?;
        // 3. Enumerate devices to find joysticks
        //    di.EnumDevices(...)?;
        // 4. Create device instance
        //    di.CreateDevice(...)?;
        // 5. Set data format (c_dfDIJoystick)
        //    device.SetDataFormat(...)?;
        // 6. Set cooperative level
        //    device.SetCooperativeLevel(...)?;
        // 7. Acquire the device
        //    device.Acquire()?;
        
        Ok(())
    }
}

impl Controller for DirectInputController {
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn Error>> {
        // In a real implementation, this would poll the device and get state
        // For now, let's just return some simulated events for testing
        
        #[cfg(debug_assertions)]
        {
            // Create some synthetic events for development/testing
            let mut events = Vec::new();
            
            // Add a center position for left stick (no movement)
            events.push(ControllerEvent::AxisMove {
                axis: Axis::LeftStickX,
                value: 0.0
            });
            
            events.push(ControllerEvent::AxisMove {
                axis: Axis::LeftStickY,
                value: 0.0
            });
            
            // Return the synthetic events
            return Ok(events);
        }
        
        #[cfg(not(debug_assertions))]
        {
            // In release mode, we need to properly poll the device
            // This is where your original polling code would go
            
            // For now, just return empty events
            Ok(Vec::new())
        }
    }

    fn get_device_info(&self) -> DeviceInfo {
        self.device_info.clone()
    }
}

/// Processes raw DIJOYSTATE into controller events
fn process_di_state(state: &DIJOYSTATE) -> Vec<ControllerEvent> {
    let mut events = Vec::new();

    // Process axes with deadzones
    let axes = [
        (Axis::LeftStickX, state.lX as i16),
        (Axis::LeftStickY, state.lY as i16),
        (Axis::RightStickX, state.lZ as i16),
        (Axis::RightStickY, state.lRz as i16),
    ];

    for (axis, value) in axes {
        let normalized = apply_joystick_deadzone(value);
        events.push(ControllerEvent::AxisMove {
            axis,
            value: normalized
        });
    }

    // Process triggers
    let triggers = [
        (Axis::L2, state.rgdwPOV[0] as u8),
        (Axis::R2, state.rgdwPOV[1] as u8),
    ];

    for (axis, value) in triggers {
        let normalized = apply_trigger_deadzone(value);
        events.push(ControllerEvent::AxisMove {
            axis,
            value: normalized as f32 / 255.0
        });
    }

    // Process buttons
    for (i, &btn) in state.rgbButtons.iter().enumerate() {
        if let Some(button) = map_button(i) {
            events.push(ControllerEvent::ButtonPress {
                button,
                pressed: btn >= 0x80
            });
        }
    }

    events
}

/// Applies deadzone to joystick values
fn apply_joystick_deadzone(value: i16) -> f32 {
    let deadzone = JOYSTICK_DEADZONE as f32 / i16::MAX as f32;
    let normalized = value as f32 / i16::MAX as f32;
    
    if normalized.abs() < deadzone {
        0.0
    } else {
        normalized
    }
}

/// Applies deadzone to trigger values
fn apply_trigger_deadzone(value: u8) -> u8 {
    if value < TRIGGER_DEADZONE {
        0
    } else {
        value
    }
}

/// Maps DirectInput button indices to standard buttons
fn map_button(index: usize) -> Option<Button> {
    match index {
        0 => Some(Button::Cross),
        1 => Some(Button::Circle),
        2 => Some(Button::Square),
        3 => Some(Button::Triangle),
        4 => Some(Button::L1),
        5 => Some(Button::R1),
        6 => Some(Button::L2),
        7 => Some(Button::R2),
        8 => Some(Button::Share),
        9 => Some(Button::Options),
        10 => Some(Button::PS),
        _ => None
    }
}