//! Linux-specific controller implementations

mod dualshock;

pub use self::dualshock::DualShockDeviceSpec;

use crate::device_registry::InputDevice;

/// Common Linux controller features
#[cfg(target_os = "linux")]
pub trait LinuxControllerExt {
    /// Check if the controller supports force feedback
    fn supports_force_feedback(&self) -> bool;

    /// Check if the controller has a touchpad
    fn has_touchpad(&self) -> bool;
}

#[cfg(target_os = "linux")]
impl<T: InputDevice> LinuxControllerExt for T {
    default fn supports_force_feedback(&self) -> bool {
        // Most modern controllers support force feedback
        true
    }

    default fn has_touchpad(&self) -> bool {
        // Only DualShock controllers have touchpads
        matches!(self.device_name(), "DualShock 4" | "DualSense")
    }
}