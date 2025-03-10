mod xinput;
mod direct_input;

use crate::controller::Controller;
use std::error::Error;

/// Create the best available controller on Windows
pub fn create_controller() -> Result<Box<dyn Controller>, Box<dyn Error>> {
    // Try XInput first, as it's the most reliable for modern controllers
    if let Ok(controller) = xinput::XInputController::new() {
        return Ok(Box::new(controller));
    }
    
    // Fall back to DirectInput if XInput fails
    if let Ok(controller) = direct_input::DirectInputController::new() {
        return Ok(Box::new(controller));
    }
    
    Err("No compatible controller found".into())
}