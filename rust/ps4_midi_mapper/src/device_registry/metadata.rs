use windows::Win32::Devices::HumanInterfaceDevice::DIDEVICEINSTANCEA;
use windows::core::HSTRING;
use std::ffi::CStr;

#[derive(Debug, Clone)]
pub struct DeviceMetadata {
    pub vid: u16,
    pub pid: u16,
    pub version: u16,
    pub manufacturer: String,
    pub product: String,
    pub serial: String,
}

impl DeviceMetadata {
    /// Creates metadata from a HID device
    pub fn from_hid(device: &hidapi::DeviceInfo) -> Self {
        Self {
            vid: device.vendor_id(),
            pid: device.product_id(),
            version: device.release_number(),
            manufacturer: device.manufacturer_string()
                .unwrap_or_default()
                .to_owned(),
            product: device.product_string()
                .unwrap_or_default()
                .to_owned(),
            serial: device.serial_number()
                .unwrap_or_default()
                .to_owned(),
        }
    }

    /// Creates metadata from a DirectInput device instance
    pub fn from_di(device: &DIDEVICEINSTANCEA) -> Self {
        Self {
            vid: (device.guidProduct.data1 >> 16) as u16,  // Lowercase data1
            pid: (device.guidProduct.data1 & 0xFFFF) as u16,  // Lowercase data1
            version: (device.dwDevType >> 8) as u16,
            manufacturer: unsafe {
                CStr::from_ptr(device.tszInstanceName.as_ptr() as *const i8)  // Cast to i8 pointer
                    .to_string_lossy()
                    .into_owned()
            },
            product: unsafe {
                CStr::from_ptr(device.tszProductName.as_ptr() as *const i8)  // Cast to i8 pointer
                    .to_string_lossy()
                    .into_owned()
            },
            serial: String::new(),
        }
    }

    /// Creates metadata from Windows device properties
    pub fn from_windows(device: &windows::Devices::Enumeration::DeviceInformation) -> Self {
        Self {
            vid: 0,
            pid: 0,
            version: 0,
            manufacturer: device.properties()
                .get(&HSTRING::from("System.Devices.Manufacturer"))
                .and_then(|v| v.as_string().ok())
                .unwrap_or_default(),
            product: device.properties()
                .get(&HSTRING::from("System.Devices.Product"))
                .and_then(|v| v.as_string().ok())
                .unwrap_or_default(),
            serial: device.properties()
                .get(&HSTRING::from("System.Devices.SerialNumber"))
                .and_then(|v| v.as_string().ok())
                .unwrap_or_default(),
        }
    }
}