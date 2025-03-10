mod xinput;
mod direct_input;

use crate::controller::Controller;
use std::error::Error;

/// Create the best available controller on Windows
/// Create the best available controller on Windows
pub fn create_controller() -> Result<Box<dyn Controller>, Box<dyn Error>> {
    // Try XInput first, as it's the most reliable for modern controllers
    match xinput::XInputController::new() {
        Ok(controller) => return Ok(Box::new(controller)),
        Err(_) => {
            // Don't output errors here - they'll be too verbose during polling
            // Fall through to try DirectInput
        }
    }
    
    // Fall back to DirectInput if XInput fails
    match std::panic::catch_unwind(|| direct_input::DirectInputController::new()) {
        Ok(result) => {
            match result {
                Ok(controller) => return Ok(Box::new(controller)),
                Err(_) => {
                    // Don't output errors here - they'll be too verbose during polling
                    return Err("No compatible controller found".into());
                }
            }
        },
        Err(_) => {
            // Don't output errors here - they'll be too verbose during polling
            return Err("DirectInput initialization caused a panic".into());
        }
    }
}