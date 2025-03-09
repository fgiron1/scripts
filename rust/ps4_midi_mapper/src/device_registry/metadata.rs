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
            version: device.version_number(),
            manufacturer: device.manufacturer_string().unwrap_or_default().to_owned(),
            product: device.product_string().unwrap_or_default().to_owned(),
            serial: device.serial_number().unwrap_or_default().to_owned(),
        }
    }
}