use crate::controllers::linux::dualshock::DualShockController;
use crate::device_registry::{Controller, DeviceMetadata};

pub struct PlatformController {
    controller: DualShockController,
}

impl PlatformController {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            controller: DualShockController::new()?
        })
    }
}

impl Controller for PlatformController {
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn std::error::Error>> {
        self.controller.poll_events()
    }

    fn get_metadata(&self) -> DeviceMetadata {
        DeviceMetadata {
            vid: 0x054C, // Sony
            pid: 0x09CC, // DualShock 4
            version: 0,
            manufacturer: "Sony Interactive Entertainment".into(),
            product: "Wireless Controller".into(),
            serial: "".into(),
        }
    }
}