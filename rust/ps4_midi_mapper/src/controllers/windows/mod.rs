//! Windows-specific controller implementations

mod hid;
mod xinput;
mod direct_input;

pub use self::{
    hid::HidDeviceSpec,
    xinput::XInputDeviceSpec,
    direct_input::DirectInputDeviceSpec
};

use crate::device_registry::InputDevice;

/// Common Windows controller features
#[cfg(target_os = "windows")]
pub trait WindowsControllerExt {
    /// Check if controller requires elevated privileges
    fn requires_elevation(&self) -> bool;
}

#[cfg(target_os = "windows")]
impl<T: InputDevice> WindowsControllerExt for T {
    default fn requires_elevation(&self) -> bool {
        // HID devices typically need admin rights for raw input
        matches!(self.device_name(), "HID-compliant game controller")
    }
}