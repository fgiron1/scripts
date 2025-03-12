pub mod types;
pub mod profiles;
#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod linux;

use types::{ControllerEvent, DeviceInfo};
use std::error::Error;

/// The main controller interface that all platform-specific implementations must provide
pub trait Controller: Send {
    /// Poll for new events from the controller
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn Error>>;
    
    /// Get controller device information
    fn get_device_info(&self) -> DeviceInfo;
}

/// Create a controller instance appropriate for the current platform
pub fn create_controller() -> Result<Box<dyn Controller>, Box<dyn Error>> {
    #[cfg(target_os = "windows")]
    {
        windows::create_controller()
    }
    
    #[cfg(target_os = "linux")]
    {
        linux::create_controller()
    }
    
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        Err("Unsupported platform".into())
    }
}