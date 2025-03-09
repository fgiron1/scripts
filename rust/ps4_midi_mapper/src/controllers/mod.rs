//! Controller implementations for various platforms and protocols

#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod linux;

use crate::device_registry::{Controller, DeviceMetadata};
use std::error::Error;

/// Trait for platform-specific controller extensions
pub trait PlatformControllerExt {
    /// Check if the controller is currently connected
    fn is_connected(&self) -> bool;

    /// Get the battery level (if supported)
    fn battery_level(&self) -> Option<f32>;
}

/// Default implementation for platform extensions
impl<T: Controller> PlatformControllerExt for T {
    default fn is_connected(&self) -> bool {
        // Assume connected unless platform-specific logic says otherwise
        true
    }

    default fn battery_level(&self) -> Option<f32> {
        // Default to no battery info
        None
    }
}

/// Common controller functionality
pub trait InputDevice: Send + Sync {
    /// Check if a device is compatible with this controller type
    fn is_compatible(&self, device: &DeviceMetadata) -> bool;

    /// Connect to a device
    fn connect(&self, device: DeviceHandle) -> Result<Box<dyn Controller>, Box<dyn Error>>;

    /// Get a human-readable name for the device type
    fn device_name(&self) -> &'static str;

    /// For downcasting
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Platform-agnostic device handle
#[derive(Debug)]
pub enum DeviceHandle {
    #[cfg(target_os = "windows")]
    Windows(HANDLE),
    #[cfg(target_os = "linux")]
    Linux(std::path::PathBuf),
    Hid(hidapi::HidDevice),
}

/// Register all available controller types
pub fn register_controllers(registry: &mut crate::device_registry::DeviceRegistry) {
    #[cfg(target_os = "windows")]
    {
        registry.register_device(windows::HidDeviceSpec);
        registry.register_device(windows::XInputDeviceSpec);
        registry.register_device(windows::DirectInputDeviceSpec);
    }

    #[cfg(target_os = "linux")]
    {
        registry.register_device(linux::DualShockDeviceSpec);
    }
}