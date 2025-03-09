#[cfg(target_os = "linux")]
use super::super::{Controller, Button, Axis, ControllerEvent};
#[cfg(target_os = "linux")]
use gilrs::{Gilrs, Event, Button as GilrsButton, Axis as GilrsAxis};
#[cfg(target_os = "linux")]
use evdev::{Device, AbsoluteAxisCode, InputEventKind};
#[cfg(target_os = "linux")]
use std::{fs, path::Path};

#[cfg(target_os = "linux")]
pub struct DualShockController {
    gilrs: Gilrs,
    touchpad_device: Option<Device>,
}

#[cfg(target_os = "linux")]
impl DualShockController {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let gilrs = Gilrs::new()?;
        let touchpad_device = Self::find_touchpad_device()?;
        Ok(Self { gilrs, touchpad_device })
    }

    fn find_touchpad_device() -> Result<Option<Device>, Box<dyn std::error::Error>> {
        for entry in fs::read_dir("/dev/input")? {
            let path = entry?.path();
            if let Ok(device) = Device::open(&path) {
                if Self::is_dualshock_touchpad(&device) {
                    device.set_nonblocking(true)?;
                    return Ok(Some(device));
                }
            }
        }
        Ok(None)
    }

    fn is_dualshock_touchpad(device: &Device) -> bool {
        device.name().unwrap_or_default().contains("Wireless Controller") &&
        device.supported_absolute_axes()
            .map(|axes| axes.contains(AbsoluteAxisCode::ABS_MT_POSITION_X))
            .unwrap_or(false)
    }
}

#[cfg(target_os = "linux")]
impl Controller for DualShockController {
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn std::error::Error>> {
        let mut events = Vec::new();

        // Process gamepad events
        while let Some(Event { event, .. }) = self.gilrs.next_event() {
            match event {
                gilrs::EventType::ButtonPressed(b, _) => events.push(
                    ControllerEvent::ButtonPress {
                        button: b.into(),
                        pressed: true
                    }
                ),
                gilrs::EventType::ButtonReleased(b, _) => events.push(
                    ControllerEvent::ButtonPress {
                        button: b.into(),
                        pressed: false
                    }
                ),
                gilrs::EventType::AxisChanged(a, v, _) => events.push(
                    ControllerEvent::AxisMove {
                        axis: a.into(),
                        value: v
                    }
                ),
                _ => {}
            }
        }

        // Process touchpad events
        if let Some(device) = &mut self.touchpad_device {
            for event in device.fetch_events()? {
                if let InputEventKind::Absolute(axis) = event.kind() {
                    match axis {
                        AbsoluteAxisCode::ABS_MT_POSITION_X => events.push(
                            ControllerEvent::TouchpadMove {
                                x: event.value(),
                                y: None
                            }
                        ),
                        AbsoluteAxisCode::ABS_MT_POSITION_Y => events.push(
                            ControllerEvent::TouchpadMove {
                                x: None,
                                y: event.value()
                            }
                        ),
                        _ => {}
                    }
                }
            }
        }

        Ok(events)
    }
}

#[cfg(target_os = "linux")]
impl From<GilrsButton> for Button {
    fn from(btn: GilrsButton) -> Self {
        match btn {
            GilrsButton::South => Button::Cross,
            GilrsButton::East => Button::Circle,
            GilrsButton::North => Button::Triangle,
            GilrsButton::West => Button::Square,
            GilrsButton::LeftTrigger => Button::L1,
            GilrsButton::RightTrigger => Button::R1,
            GilrsButton::LeftTrigger2 => Button::L2,
            GilrsButton::RightTrigger2 => Button::R2,
            GilrsButton::Select => Button::Share,
            GilrsButton::Start => Button::Options,
            GilrsButton::Mode => Button::PS,
            GilrsButton::LeftThumb => Button::L3,
            GilrsButton::RightThumb => Button::R3,
            _ => Button::Unknown,
        }
    }
}

#[cfg(target_os = "linux")]
impl From<GilrsAxis> for Axis {
    fn from(axis: GilrsAxis) -> Self {
        match axis {
            GilrsAxis::LeftStickX => Axis::LeftStickX,
            GilrsAxis::LeftStickY => Axis::LeftStickY,
            GilrsAxis::RightStickX => Axis::RightStickX,
            GilrsAxis::RightStickY => Axis::RightStickY,
            GilrsAxis::LeftZ => Axis::L2,
            GilrsAxis::RightZ => Axis::R2,
            _ => Axis::Unknown,
        }
    }
}