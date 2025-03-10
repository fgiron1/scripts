use super::Controller;
use hidapi::{HidApi, HidDevice, HidError};
use std::any::Any;

/// Registry for managing connected devices
pub struct DeviceRegistry {
    device_types: Vec<Box<dyn InputDevice>>,
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self {
            device_types: Vec::new(),
        }
    }

    /// Registers a new type of input device
    pub fn register_device<T: InputDevice + 'static>(&mut self, device: T) {
        self.device_types.push(Box::new(device));
    }

    /// Detects all connected devices
    pub fn detect_devices(&self) -> Result<Vec<Box<dyn Controller>>, HidError> {
        let api = HidApi::new()?;
        let mut devices = Vec::new();

        for device in api.device_list() {
            for device_type in &self.device_types {
                if device_type.is_compatible(device) {
                    if let Ok(hid_device) = api.open(device.vendor_id(), device.product_id()) {
                        match device_type.connect(hid_device) {
                            Ok(ctrl) => devices.push(ctrl),
                            Err(e) => eprintln!("Connection failed: {}", e),
                        }
                    }
                }
            }
        }

        Ok(devices)
    }
}

/// Trait for input device types
pub trait InputDevice: Any + Send + Sync {
    /// Checks if a device is compatible with this type
    fn is_compatible(&self, device: &hidapi::DeviceInfo) -> bool;

    /// Connects to a device
    fn connect(&self, device: HidDevice) -> Result<Box<dyn Controller>, HidError>;

    /// Returns a human-readable name for the device type
    fn device_name(&self) -> &'static str;

    /// For downcasting
    fn as_any(&self) -> &dyn Any;
}