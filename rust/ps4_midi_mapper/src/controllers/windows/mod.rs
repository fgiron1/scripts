use crate::device_registry::{Controller, DeviceMetadata, InputDevice};
use hidapi::{HidDevice, HidError};
use std::any::Any;
use std::error::Error;

pub struct DirectInputDeviceSpec;
pub struct XInputDeviceSpec;

impl InputDevice for DirectInputDeviceSpec {
    fn is_compatible(&self, device: &hidapi::DeviceInfo) -> bool {
        // DirectInput compatibility check
        device.product_string().map_or(false, |s| 
            s.contains("Joystick") || s.contains("Gamepad"))
    }

    fn connect(&self, device: HidDevice) -> Result<Box<dyn Controller>, HidError> {
        // Convert HidDevice to HANDLE if needed
        let handle = /* conversion logic */;
        let controller = direct_input::DirectInputController::new(handle)?;
        Ok(Box::new(controller))
    }

    fn device_name(&self) -> &'static str {
        "DirectInput-Compatible Controller"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl InputDevice for XInputDeviceSpec {
    fn is_compatible(&self, device: &hidapi::DeviceInfo) -> bool {
        // XInput compatibility check
        device.vendor_id() == 0x045E && device.product_id() == 0x028E
    }

    fn connect(&self, _device: HidDevice) -> Result<Box<dyn Controller>, HidError> {
        let controller = xinput::XInputController::try_create()?;
        Ok(Box::new(controller))
    }

    fn device_name(&self) -> &'static str {
        "XInput-Compatible Controller"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}