pub mod dualshock;

use super::Controller;
use std::error::Error;

/// Create the best available controller on Linux
pub fn create_controller() -> Result<Box<dyn Controller>, Box<dyn Error>> {
    // For Linux, we primarily support DualShock controllers
    match dualshock::DualShockController::new() {
        Ok(controller) => {
            println!("Controller connected via gilrs/evdev");
            Ok(Box::new(controller) as Box<dyn Controller>)
        },
        Err(e) => {
            Err(format!("Could not find compatible controller: {}", e).into())
        }
    }
}