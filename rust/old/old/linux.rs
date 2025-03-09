
use super::types::{Button, Axis, ControllerEvent};
use super::Controller;
use gilrs::{Gilrs, Event, Button as GilrsButton, Axis as GilrsAxis};
use evdev::{Device, AbsoluteAxisCode};
use std::fs;
use std::error::Error;

pub struct LinuxController {
    gilrs: Gilrs,
    touchpad_device: Option<Device>,
}

impl LinuxController {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let gilrs = Gilrs::new()?;
        let touchpad_device = Self::find_touchpad_device()?;
        Ok(Self { gilrs, touchpad_device })
    }

    fn find_touchpad_device() -> Result<Option<Device>, Box<dyn Error>> {
        for entry in fs::read_dir("/dev/input")? {
            let path = entry?.path();
            if let Ok(device) = Device::open(&path) {
                if device.name().unwrap_or_default().contains("Wireless Controller") &&
                    device.supported_absolute_axes().map(|a| 
                        a.contains(AbsoluteAxisCode::ABS_MT_POSITION_X)).unwrap_or(false) 
                {
                    device.set_nonblocking(true)?;
                    return Ok(Some(device));
                }
            }
        }
        Ok(None)
    }
}

impl Controller for LinuxController {
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn Error>> {
        let mut events = Vec::new();

        while let Some(Event { event, .. }) = self.gilrs.next_event() {
            match event {
                gilrs::EventType::ButtonPressed(b, _) => events.push(
                    ControllerEvent::ButtonPress { button: b.into(), pressed: true }
                ),
                gilrs::EventType::ButtonReleased(b, _) => events.push(
                    ControllerEvent::ButtonPress { button: b.into(), pressed: false }
                ),
                gilrs::EventType::AxisChanged(a, v, _) => events.push(
                    ControllerEvent::AxisMove { axis: a.into(), value: v }
                ),
                _ => {}
            }
        }

        if let Some(device) = &mut self.touchpad_device {
            for event in device.fetch_events()? {
                match AbsoluteAxisCode::from(event.code()) {
                    AbsoluteAxisCode::ABS_MT_POSITION_X => events.push(
                        ControllerEvent::TouchpadEvent { x: event.value(), y: 0 }
                    ),
                    AbsoluteAxisCode::ABS_MT_POSITION_Y => events.push(
                        ControllerEvent::TouchpadEvent { x: 0, y: event.value() }
                    ),
                    _ => {}
                }
            }
        }

        Ok(events)
    }
}

impl From<GilrsButton> for Button {
    fn from(btn: GilrsButton) -> Self {
        match btn {
            GilrsButton::South => Button::South,
            GilrsButton::East => Button::East,
            GilrsButton::North => Button::North,
            GilrsButton::West => Button::West,
            GilrsButton::LeftTrigger => Button::LeftTrigger,
            GilrsButton::RightTrigger => Button::RightTrigger,
            GilrsButton::LeftTrigger2 => Button::LeftTrigger2,
            GilrsButton::RightTrigger2 => Button::RightTrigger2,
            GilrsButton::Select => Button::Select,
            GilrsButton::Start => Button::Start,
            GilrsButton::Mode => Button::Mode,
            GilrsButton::LeftThumb => Button::LeftThumb,
            GilrsButton::RightThumb => Button::RightThumb,
            _ => Button::Unknown,
        }
    }
}

impl From<GilrsAxis> for Axis {
    fn from(axis: GilrsAxis) -> Self {
        match axis {
            GilrsAxis::LeftStickX => Axis::LeftStickX,
            GilrsAxis::LeftStickY => Axis::LeftStickY,
            GilrsAxis::RightStickX => Axis::RightStickX,
            GilrsAxis::RightStickY => Axis::RightStickY,
            GilrsAxis::LeftZ => Axis::LeftZ,
            GilrsAxis::RightZ => Axis::RightZ,
            _ => unreachable!(),
        }
    }
}