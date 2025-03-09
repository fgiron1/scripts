use crate::device_registry::{Controller, DeviceMetadata};
use crate::controllers::windows::{HidController, XInputController, DirectInputController};
use std::error::Error;

/// Platform-specific controller for Windows
pub struct WindowsController {
    controller: Box<dyn Controller>,
}

impl WindowsController {
    pub fn new() -> Result<Self, Box<dyn Error>> {
    // Try XInput first for Microsoft controllers
    if let Some(controller) = XInputController::try_create() {
        return Ok(Self { controller });
    }
    // Then fall back to HID
    else if let Some(controller) = HidController::try_create() {
        return Ok(Self { controller });
    }
    // Finally try DirectInput
    else if let Some(controller) = DirectInputController::try_create() {
        return Ok(Self { controller });
    }
        Err("No compatible controller found".into())
    }
}

impl Controller for WindowsController {
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn Error>> {
        self.controller.poll_events()
    }

    fn get_metadata(&self) -> DeviceMetadata {
        self.controller.get_metadata()
    }
}