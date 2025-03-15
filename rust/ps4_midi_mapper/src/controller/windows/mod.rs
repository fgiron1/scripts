// src/controller/windows/mod.rs
mod xinput;
mod hid;
mod rawio;

use crate::controller::Controller;
use std::error::Error;

/// Create the best available controller on Windows
pub fn create_controller() -> Result<Box<dyn Controller>, Box<dyn Error>> {
    // First try the Raw Input controller which has the best touchpad support
    match rawio::WindowsRawIOController::new() {
        Ok(controller) => {
            println!("Controller connected via Windows Raw Input");
            return Ok(Box::new(controller));
        }
        Err(e) => {
            println!("Raw Input controller not available: {}", e);
            println!("Trying alternative methods...");
        }
    }
    
    // Then try XInput, which works well for Xbox controllers
    if let Ok(controller) = xinput::XInputController::new() {
        println!("Controller connected via XInput");
        return Ok(Box::new(controller));
    }
    
    // Finally try HID, which should work for PS4 controllers and others
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