mod xinput;
mod hid;

use crate::controller::Controller;
use std::error::Error;

/// Create the best available controller on Windows
pub fn create_controller() -> Result<Box<dyn Controller>, Box<dyn Error>> {
    // First try XInput, which works well for Xbox controllers
    if let Ok(controller) = xinput::XInputController::new() {
        println!("Controller connected via XInput");
        return Ok(Box::new(controller));
    }
    
    // Then try HID, which should work for PS4 controllers and others
    match hid::HidController::new() {
        Ok(controller) => {
            println!("Controller connected via HID");
            return Ok(Box::new(controller));
        }
        Err(e) => {
            return Err(format!("Could not find compatible controller: {}", e).into());
        }
    }
}