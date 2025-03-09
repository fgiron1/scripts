use windows::Win32::Devices::HumanInterfaceDevice::{IDirectInputDevice8A, DIJOYSTATE};
use super::Controller;
use windows::core::Result;
use crate::controller::types::{ControllerEvent, Button, InputEvent, Input, Axis, Joystick};
use std::ffi::c_void;
use std::error::Error;

pub struct DirectInputController {
    device: IDirectInputDevice8A,
}

impl DirectInputController {
    pub fn new(device: IDirectInputDevice8A) -> Result<Self> {
        Ok(Self { device })
    }

    fn poll(&self) -> Result<Vec<InputEvent>> {
        let mut state = DIJOYSTATE::default();
        unsafe {
            self.device.GetDeviceState(
                std::mem::size_of::<DIJOYSTATE>() as u32,
                &mut state as *mut _ as *mut c_void
            )?;
        }
        Ok(process_directinput_state(&state))
    }
}

impl Controller for DirectInputController {
    fn poll_events(&mut self) -> std::result::Result<Vec<ControllerEvent>, Box<dyn Error>> {
        let raw_events = self.poll()?;
        Ok(raw_events.into_iter().map(|e| match e {
            InputEvent::Axis(input, value) => ControllerEvent::AxisMove {
                axis: match input {
                    Input::LeftStickX => Axis::LeftStickX,
                    Input::LeftStickY => Axis::LeftStickY,
                    Input::RightStickX => Axis::RightStickX,
                    Input::RightStickY => Axis::RightStickY,
                    Input::LeftTrigger => Axis::L2,
                    Input::RightTrigger => Axis::R2,
                    _ => unreachable!(),
                },
                value,
            },
            InputEvent::Button(input, pressed) => ControllerEvent::ButtonPress {
                button: match input {
                    Input::Button(0) => Button::Cross,
                    Input::Button(1) => Button::Circle,
                    // Add other button mappings...
                    _ => Button::Unknown,
                },
                pressed,
            },
        }).collect())
    }
}


fn process_directinput_state(state: &DIJOYSTATE) -> Vec<InputEvent> {
    let mut events = Vec::new();

    // Process trackpad
    let trackpad_x = state.rglSlider[0] as i16;
    let trackpad_y = state.rglSlider[1] as i16;

    events.push(InputEvent::Axis(
        Input::TrackpadX,
        Joystick::with_deadzone(trackpad_x, super::win_xinput::JOYSTICK_DEADZONE),
    ));

    events.push(InputEvent::Axis(
        Input::TrackpadY,
        Joystick::with_deadzone(trackpad_y, super::win_xinput::JOYSTICK_DEADZONE),
    ));

    // Process triggers
    let left_trigger = state.rgdwPOV[0] as u8;
    let right_trigger = state.rgdwPOV[1] as u8;

    let left_trigger = if left_trigger < super::win_xinput::TRIGGER_DEADZONE {
        0
    } else {
        left_trigger
    };
    let right_trigger = if right_trigger < super::win_xinput::TRIGGER_DEADZONE {
        0
    } else {
        right_trigger
    };

    events.push(InputEvent::Axis(
        Input::LeftTrigger,
        left_trigger as f32 / 255.0,
    ));
    events.push(InputEvent::Axis(
        Input::RightTrigger,
        right_trigger as f32 / 255.0,
    ));

    // Process joysticks
    let lx = Joystick::with_deadzone(state.lX as i16, super::win_xinput::JOYSTICK_DEADZONE);
    let ly = Joystick::with_deadzone(state.lY as i16, super::win_xinput::JOYSTICK_DEADZONE);
    let rx = Joystick::with_deadzone(state.lZ as i16, super::win_xinput::JOYSTICK_DEADZONE);
    let ry = Joystick::with_deadzone(state.lRz as i16, super::win_xinput::JOYSTICK_DEADZONE);

    events.push(InputEvent::Axis(Input::LeftStickX, lx));
    events.push(InputEvent::Axis(Input::LeftStickY, ly));
    events.push(InputEvent::Axis(Input::RightStickX, rx));
    events.push(InputEvent::Axis(Input::RightStickY, ry));

    // Process buttons
    for (i, &btn) in state.rgbButtons.iter().enumerate() {
        let pressed = btn >= 0x80;  // Buttons are 0x00-0xFF where >= 0x80 is pressed
        events.push(InputEvent::Button(Input::Button(i as u8), pressed));
    }

    events
}