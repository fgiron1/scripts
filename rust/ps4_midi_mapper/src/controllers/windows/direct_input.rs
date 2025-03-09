use windows::{
    core::{Result, Error},
    Win32::{
        Devices::{
            HumanInterfaceDevice::{
                IDirectInput8A, IDirectInputDevice8A, 
                GUID_Joystick, c_dfDIJoystick,
                DISCL_BACKGROUND, DISCL_NONEXCLUSIVE
            },
            DeviceAndDriverInstallation::GUID
        },
        Foundation::HANDLE,
    },
};
use crate::device_registry::{Controller, ControllerEvent, Axis, Button, DeviceMetadata};
use std::{ffi::c_void, mem::size_of, ptr::null_mut};

const JOYSTICK_DEADZONE: i16 = 7840;  // ~25% of i16 range
const TRIGGER_DEADZONE: u8 = 30;      // ~12% of u8 range

/// DirectInput device specification for the registry
pub struct DirectInputDeviceSpec;

impl super::InputDevice for DirectInputDeviceSpec {
    fn is_compatible(&self, guid: &GUID) -> bool {
        // Match joystick-class devices
        guid.data1 == 0x4D1E55B2 &&  // GUID_Joystick
        guid.data2 == 0xF16F &&
        guid.data3 == 0x11CF
    }

    fn connect(&self, handle: HANDLE) -> Result<Box<dyn Controller>, Box<dyn std::error::Error>> {
        let di_device = unsafe { create_di_device(handle)? };
        Ok(Box::new(DirectInputController::new(di_device)?)) // Added closing )
    }

    fn device_name(&self) -> &'static str {
        "DirectInput-Compatible Joystick"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// DirectInput controller implementation
struct DirectInputController {
    device: Arc<IDirectInputDevice8A>,
    metadata: DeviceMetadata,
}

// Implement Send/Sync safely
unsafe impl Send for DirectInputController {}
unsafe impl Sync for DirectInputController {}

impl DirectInputController {
    fn new(device: IDirectInputDevice8A) -> Result<Self, Error> {
        let metadata = unsafe { get_device_metadata(&device)? };
        Ok(Self { device, metadata })
    }

    fn poll_state(&self) -> Result<DIJOYSTATE, Error> {
        let mut state = DIJOYSTATE::default();
        unsafe {
            self.device.GetDeviceState(
                size_of::<DIJOYSTATE>() as u32,
                &mut state as *mut _ as *mut c_void
            )?;
        }
        Ok(state)
    }
}

impl Controller for DirectInputController {
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn std::error::Error>> {
        let state = self.poll_state()?;
        Ok(process_di_state(&state))
    }

    fn get_metadata(&self) -> DeviceMetadata {
        self.metadata.clone()
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

/// Gets device metadata from DirectInput device
unsafe fn get_device_metadata(device: &IDirectInputDevice8A) -> Result<DeviceMetadata, Error> {
    let mut di_info = DIDEVICEINSTANCEA::default();
    di_info.dwSize = size_of::<DIDEVICEINSTANCEA>() as u32;
    
    device.GetDeviceInfo(&mut di_info)?;

    Ok(DeviceMetadata {
        vid: (di_info.guidProduct.Data1 >> 16) as u16,
        pid: (di_info.guidProduct.Data1 & 0xFFFF) as u16,
        version: di_info.dwDevType as u16,
        manufacturer: String::from_utf8_lossy(
            &di_info.tszInstanceName[..di_info.tszInstanceName.iter()
                .position(|&c| c == 0).unwrap_or(0)]
        ).into(),
        product: String::from_utf8_lossy(
            &di_info.tszProductName[..di_info.tszProductName.iter()
                .position(|&c| c == 0).unwrap_or(0)]
        ).into(),
        serial: "".into()
    })
}

/// Helper to create DirectInput device from handle
unsafe fn create_di_device(handle: HANDLE) -> Result<IDirectInputDevice8A> {
    let di = IDirectInput8A::new()?;
    let mut device = None;
    di.CreateDevice(&GUID_Joystick, &mut device)?;
    device.SetDataFormat(&c_dfDIJoystick)?;
    device.SetCooperativeLevel(handle, DISCL_BACKGROUND | DISCL_NONEXCLUSIVE)?;
    device.Acquire()?;
    Ok(device)
}