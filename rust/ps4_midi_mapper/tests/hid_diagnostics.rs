use hidapi::HidApi;
use serial_test::serial;

#[test]
#[serial]  // Ensures tests run sequentially with hardware access
fn list_connected_hid_devices() {
    let api = HidApi::new().expect("Failed to initialize HID API");
    
    println!("\nConnected HID Devices:");
    println!("{:-<60}", "");
    println!(
        "{:<6} {:<6} {:<25} {:<25} {:<15}",
        "VID", "PID", "Manufacturer", "Product", "Serial"
    );
    println!("{:-<60}", "");

    for device in api.device_list() {
        let vid = format!("{:04x}", device.vendor_id());
        let pid = format!("{:04x}", device.product_id());
        let manufacturer = device.manufacturer_string().unwrap_or("N/A");
        let product = device.product_string().unwrap_or("N/A");
        let serial = device.serial_number().unwrap_or("N/A");

        println!(
            "{:<6} {:<6} {:<25} {:<25} {:<15}",
            vid, pid, manufacturer, product, serial
        );
    }
    
    // Fail test if no devices found (optional)
    let devices: Vec<_> = api.device_list().collect();
    assert!(!devices.is_empty(), "No HID devices detected");
}