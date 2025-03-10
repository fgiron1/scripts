mod dualshock;

use super::{Controller, types::DeviceInfo};
use std::error::Error;

/// Create the best available controller on Linux
pub fn create_controller() -> Result<Box<dyn Controller>, Box<dyn Error>> {
    // For Linux, we primarily support DualShock controllers
    dualshock::DualShockController::new()
        .map(|controller| Box::new(controller) as Box<dyn Controller>)
}