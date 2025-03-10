use rusty_xinput::{XInputHandle, XInputError};
use crate::device_registry::{Controller, ControllerEvent, Axis, Button, DeviceMetadata};
use std::error::Error;

const JOYSTICK_DEADZONE: i16 = 2500;
const TRIGGER_DEADZONE: u8 = 5;

/// XInput device specification for the registry
pub struct XInputDeviceSpec;

impl super::InputDevice for XInputDeviceSpec {
    fn is_compatible(&self, device: &DeviceMetadata) -> bool {
        device.vid == 0x045E && device.pid == 0x028E
    }

    fn connect(&self, _device: HidDevice) -> Result<Box<dyn Controller>, HidError> {
        XInputController::try_create()
            .map(|c| Box::new(c) as Box<dyn Controller>)
            .map_err(|e| HidError::OpenHidDeviceError {
                message: format!("XInput error: {}", e),
            })
    }

    fn device_name(&self) -> &'static str {
        "XInput-Compatible Controller"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// XInput controller implementation
pub struct XInputController {
    handle: XInputHandle,
    port: u32,
    last_state: rusty_xinput::XInputState,
}

impl XInputController {
     /// Attempt to create an XInput controller on any available port
     pub fn try_create() -> Result<Self, XInputError> {
        let handle = XInputHandle::load_default()?;
        for port in 0..4 {
            if handle.get_state(port).is_ok() {
                return Ok(Self {
                    handle,
                    port,
                    last_state: Default::default(),
                });
            }
        }
        Err(XInputError::NoController)
    }

    /// Refresh controller state and return new events
    fn poll_state(&mut self) -> Result<Vec<ControllerEvent>, XInputError> {
        let mut events = Vec::new();
        let new_state = self.handle.get_state(self.port)?;

        // Process button changes
        let button_changes = new_state.raw.Gamepad.wButtons ^ self.last_state.raw.Gamepad.wButtons;
        if button_changes != 0 {
            for &(mask, button) in BUTTON_MAPPINGS.iter() {
                if button_changes & mask != 0 {
                    let pressed = (new_state.raw.Gamepad.wButtons & mask) != 0;
                    events.push(ControllerEvent::ButtonPress { button, pressed });
                }
            }
        }

        // Process analog inputs
        self.process_axis(
            new_state.raw.Gamepad.sThumbLX,
            self.last_state.raw.Gamepad.sThumbLX,
            Axis::LeftStickX,
            &mut events,
        );
        self.process_axis(
            new_state.raw.Gamepad.sThumbLY,
            self.last_state.raw.Gamepad.sThumbLY,
            Axis::LeftStickY,
            &mut events,
        );
        self.process_axis(
            new_state.raw.Gamepad.sThumbRX,
            self.last_state.raw.Gamepad.sThumbRX,
            Axis::RightStickX,
            &mut events,
        );
        self.process_axis(
            new_state.raw.Gamepad.sThumbRY,
            self.last_state.raw.Gamepad.sThumbRY,
            Axis::RightStickY,
            &mut events,
        );

        // Process triggers
        self.process_trigger(
            new_state.raw.Gamepad.bLeftTrigger,
            self.last_state.raw.Gamepad.bLeftTrigger,
            Axis::L2,
            &mut events,
        );
        self.process_trigger(
            new_state.raw.Gamepad.bRightTrigger,
            self.last_state.raw.Gamepad.bRightTrigger,
            Axis::R2,
            &mut events,
        );

        self.last_state = new_state;
        Ok(events)
    }

    fn process_axis(
        &mut self,
        new_value: i16,
        last_value: i16,
        axis: Axis,
        events: &mut Vec<ControllerEvent>,
    ) {
        if new_value != last_value {
            let normalized = self.normalize_axis(new_value, 32767);
            events.push(ControllerEvent::AxisMove { axis, value: normalized });
        }
    }

    fn process_trigger(
        &mut self,
        new_value: u8,
        last_value: u8,
        axis: Axis,
        events: &mut Vec<ControllerEvent>,
    ) {
        if new_value != last_value {
            let normalized = self.normalize_trigger(new_value);
            events.push(ControllerEvent::AxisMove { axis, value: normalized });
        }
    }

    fn normalize_axis(&self, value: i16, max: i16) -> f32 {
        let deadzone_adjusted = if value.abs() < JOYSTICK_DEADZONE {
            0.0
        } else {
            value as f32 / max as f32
        };
        deadzone_adjusted.clamp(-1.0, 1.0)
    }

    fn normalize_trigger(&self, value: u8) -> f32 {
        let deadzone_adjusted = if value < TRIGGER_DEADZONE {
            0.0
        } else {
            value as f32 / 255.0
        };
        deadzone_adjusted.clamp(0.0, 1.0)
    }
}

impl Controller for XInputController {
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn Error>> {
        self.poll_state().map_err(|e| Box::new(e) as Box<dyn Error>)
    }

    fn get_metadata(&self) -> DeviceMetadata {
        DeviceMetadata {
            vid: 0x045E,
            pid: 0x028E,
            version: 0,
            manufacturer: "Microsoft".into(),
            product: "XInput Controller".into(),
            serial: "".into(),
        }
    }
}

const BUTTON_MAPPINGS: [(u16, Button); 14] = [
    (0x1000, Button::Cross),
    (0x2000, Button::Circle),
    (0x4000, Button::Square),
    (0x8000, Button::Triangle),
    (0x0020, Button::Share),
    (0x0010, Button::Options),
    (0x0001, Button::DpadUp),
    (0x0002, Button::DpadDown),
    (0x0004, Button::DpadLeft),
    (0x0008, Button::DpadRight),
    (0x0100, Button::L1),
    (0x0200, Button::R1),
    (0x0040, Button::L3),
    (0x0080, Button::R3),
];