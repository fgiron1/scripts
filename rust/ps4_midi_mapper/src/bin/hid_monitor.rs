use hidapi::{HidApi, HidDevice};
use std::io::{self, Write};
use std::thread;
use std::time::Duration;
use std::error::Error;

// Constants for display
const RESET: &str = "\x1B[0m";
const YELLOW: &str = "\x1B[33m";
const GREEN: &str = "\x1B[32m";
const CYAN: &str = "\x1B[36m";
const CLEAR_LINE: &str = "\x1B[2K\r";
const CURSOR_UP: &str = "\x1B[1A";

/// Main function
fn main() -> Result<(), Box<dyn Error>> {
    println!("\n{}===== Simple HID Device Monitor ====={}",
             GREEN, RESET);
    println!("This tool helps identify and monitor HID devices");
    
    // Create HidApi instance
    let api = HidApi::new()?;
    
    // List all devices with detailed info
    println!("\nAll detected HID devices:");
    println!("{:<4} {:<6} {:<6} {:<24} {:<10} {:<10} {:<8}", 
             "Idx", "VID", "PID", "Product", "UsagePage", "Usage", "Interface");
    println!("{:-<75}", "");

    let devices: Vec<_> = api.device_list().collect();
    if devices.is_empty() {
        println!("No HID devices detected!");
        return Err("No devices found".into());
    }

    for (i, device_info) in devices.iter().enumerate() {
        let product = device_info.product_string().unwrap_or("Unknown");
        println!("{:<4} {:<6} {:<6} {:<24} {:<10} {:<10} {:<8}", 
                 i, 
                 format!("{:04x}", device_info.vendor_id()),
                 format!("{:04x}", device_info.product_id()),
                 product,
                 format!("0x{:04X}", device_info.usage_page()),
                 format!("0x{:04X}", device_info.usage()),
                 device_info.interface_number());
    }
    
    // Offer to list all PS4 controller related devices first
    println!("\nPotential touchpad or controller devices:");
    println!("{:-<75}", "");
    
    let ps4_devices: Vec<_> = devices.iter().enumerate()
        .filter(|(_, d)| {
            let product = d.product_string().unwrap_or("").to_lowercase();
            let is_sony = d.vendor_id() == 0x054C; // Sony VID
            let is_touchpad = product.contains("touch") || 
                             (d.usage_page() == 0x000D) || // Digitizer
                             (product.contains("pad") && !product.contains("gamepad"));
            let is_controller = product.contains("controller") || 
                               product.contains("dualshock") ||
                               product.contains("wireless");
            
            is_sony || is_touchpad || is_controller
        })
        .collect();
    
    if ps4_devices.is_empty() {
        println!("No PS4 controller or touchpad-like devices found.");
    } else {
        for (idx, (i, device_info)) in ps4_devices.iter().enumerate() {
            let product = device_info.product_string().unwrap_or("Unknown");
            println!("{:<4} {:<6} {:<6} {:<24} {:<10} {:<10} {:<8}", 
                     idx, 
                     format!("{:04x}", device_info.vendor_id()),
                     format!("{:04x}", device_info.product_id()),
                     product,
                     format!("0x{:04X}", device_info.usage_page()),
                     format!("0x{:04X}", device_info.usage()),
                     device_info.interface_number());
        }
    }
    
    // Get user selection
    print!("\nEnter device index to monitor (from the complete list, or 'q' to quit): ");
    io::stdout().flush()?;
    
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    let input = input.trim();
    if input.eq_ignore_ascii_case("q") {
        return Ok(());
    }
    
    let device_index = match input.parse::<usize>() {
        Ok(idx) if idx < devices.len() => idx,
        _ => {
            println!("Invalid selection!");
            return Err("Invalid selection".into());
        }
    };
    
    // Get the selected device info
    let device_info = &devices[device_index];
    println!("\nSelected device: {} (VID: 0x{:04X}, PID: 0x{:04X})", 
             device_info.product_string().unwrap_or("Unknown"), 
             device_info.vendor_id(), 
             device_info.product_id());
    
    // Try the direct approach first (might fail for some devices)
    println!("Attempting to monitor device...");
    if let Err(e) = monitor_device(device_info, &api) {
        println!("Failed to access device: {}", e);
        #[cfg(target_os = "windows")]
        println!("This is normal for some devices on Windows, especially touchpads which may be locked by the system.");
        println!("\nAdvice for DualShock touchpad access:");
        println!("1. Try DS4Windows or similar driver that exposes touchpad data");
        println!("2. Consider using lower-level access methods in your main application");
        println!("3. For development, try adding touchpad support to your app conditionally");
    }
    
    Ok(())
}

/// Monitor data from a specific device
fn monitor_device(device_info: &hidapi::DeviceInfo, api: &HidApi) -> Result<(), Box<dyn Error>> {
    // Convert path to CString for hidapi
    use std::ffi::CString;
    let path_cstr = CString::new(device_info.path().to_string_lossy().to_string())?;
    
    // Open the device
    let device = api.open_path(path_cstr.as_ref())?;
    println!("✓ Successfully opened device!");
    
    // Set device to non-blocking mode
    if let Err(e) = device.set_blocking_mode(false) {
        println!("Warning: Could not set non-blocking mode: {}", e);
    }
    
    // Prepare display
    println!("\n{}===== HID Data Monitor ====={}",
             GREEN, RESET);
    println!("Interact with the device to see data changes");
    println!("Press Ctrl+C to exit\n");
    
    // Prepare display area with blank lines to create a static view
    for _ in 0..25 {
        println!();
    }
    
    // Move cursor back up
    for _ in 0..25 {
        print!("{}", CURSOR_UP);
    }
    io::stdout().flush()?;
    
    // Monitor loop
    monitor_loop(&device)?;
    
    Ok(())
}

/// Main monitoring loop for device data
fn monitor_loop(device: &HidDevice) -> Result<(), Box<dyn Error>> {
    let mut last_data = Vec::new();
    let mut report_counter = 0;
    let mut change_counter = 0;
    let mut buf = [0u8; 64]; // 64 bytes is common for HID reports
    
    while report_counter < 1000 { // Limit to avoid infinite loop
        match device.read_timeout(&mut buf, 5) {
            Ok(size) if size > 0 => {
                let data = &buf[..size];
                report_counter += 1;
                
                // Check if data changed
                if data != last_data.as_slice() {
                    change_counter += 1;
                    update_display(data, &last_data, report_counter, change_counter)?;
                    last_data = data.to_vec();
                }
            },
            Ok(_) => {
                // No data, just wait
                thread::sleep(Duration::from_millis(5));
            },
            Err(e) => {
                // Only report non-timeout errors
                if !e.to_string().contains("timed out") && 
                   !e.to_string().contains("no data available") {
                    // Clear line and print error
                    print!("{}", CLEAR_LINE);
                    println!("Error reading device: {}", e);
                    thread::sleep(Duration::from_secs(1));
                    return Err(e.into());
                }
                thread::sleep(Duration::from_millis(5));
            }
        }
    }
    
    Ok(())
}

/// Update the display with current data
fn update_display(data: &[u8], last_data: &[u8], reports: u32, changes: u32) -> Result<(), Box<dyn Error>> {
    // Header section
    print!("{}", CLEAR_LINE);
    println!("{}HID Report Monitor{}", CYAN, RESET);
    print!("{}", CLEAR_LINE);
    println!("Reports: {}, Changes: {}, Report Size: {} bytes", reports, changes, data.len());
    
    // Raw data section
    print!("{}", CLEAR_LINE);
    println!("{}Raw Data:{}", CYAN, RESET);
    
    // Header row for byte indices
    print!("{}", CLEAR_LINE);
    print!("     ");
    for i in 0..16 {
        print!("{:02X} ", i);
    }
    println!();
    
    // Horizontal line
    print!("{}", CLEAR_LINE);
    print!("    +");
    for _ in 0..16 {
        print!("---");
    }
    println!("+");
    
    // Data rows
    for row in 0..((data.len() + 15) / 16) {
        print!("{}", CLEAR_LINE);
        print!("{:02X} | ", row * 16);
        
        for col in 0..16 {
            let idx = row * 16 + col;
            if idx < data.len() {
                // Check if byte changed
                let changed = idx >= last_data.len() || data[idx] != last_data[idx];
                
                if changed {
                    print!("{}{:02X}{} ", YELLOW, data[idx], RESET);
                } else {
                    print!("{:02X} ", data[idx]);
                }
            } else {
                print!("   ");
            }
        }
        println!("|");
    }
    
    // Horizontal line
    print!("{}", CLEAR_LINE);
    print!("    +");
    for _ in 0..16 {
        print!("---");
    }
    println!("+");
    
    // Add ASCII representation
    print!("{}", CLEAR_LINE);
    println!("{}ASCII:{}", CYAN, RESET);
    
    print!("{}", CLEAR_LINE);
    print!("     ");
    for i in 0..16 {
        print!(" {:X} ", i);
    }
    println!();
    
    for row in 0..((data.len() + 15) / 16) {
        print!("{}", CLEAR_LINE);
        print!("{:02X} | ", row * 16);
        
        for col in 0..16 {
            let idx = row * 16 + col;
            if idx < data.len() {
                let c = data[idx];
                let changed = idx >= last_data.len() || data[idx] != last_data[idx];
                
                if c >= 32 && c <= 126 {
                    // Printable ASCII
                    if changed {
                        print!("{}{}{}  ", YELLOW, c as char, RESET);
                    } else {
                        print!("{}  ", c as char);
                    }
                } else {
                    // Non-printable
                    if changed {
                        print!("{}·{}  ", YELLOW, RESET);
                    } else {
                        print!("·  ");
                    }
                }
            } else {
                print!("   ");
            }
        }
        println!("|");
    }
    
    // Pattern detection section
    print!("{}", CLEAR_LINE);
    println!("{}Pattern Detection:{}", CYAN, RESET);
    
    // Try to detect touchpad coordinates (common ranges for PS4 touchpad)
    let mut found_coords = false;
    for i in 0..data.len().saturating_sub(4) {
        // Look for potential 16-bit coordinates (little-endian)
        let x = (data[i] as u16) | ((data[i + 1] as u16) << 8);
        let y = (data[i + 2] as u16) | ((data[i + 3] as u16) << 8);
        
        // PS4 touchpad is typically 1920×942, but check for reasonable ranges
        if (x > 0 && x < 2000 && y > 0 && y < 1000) {
            print!("{}", CLEAR_LINE);
            println!("Potential touchpad coordinates at offset {}: X={}, Y={}", i, x, y);
            found_coords = true;
            
            // Show how these values changed
            if i < last_data.len().saturating_sub(4) {
                let last_x = (last_data[i] as u16) | ((last_data[i + 1] as u16) << 8);
                let last_y = (last_data[i + 2] as u16) | ((last_data[i + 3] as u16) << 8);
                
                if x != last_x || y != last_y {
                    print!("{}", CLEAR_LINE);
                    println!("  Change: ΔX={}, ΔY={}", 
                             (x as i32) - (last_x as i32), 
                             (y as i32) - (last_y as i32));
                }
            }
            
            // Implementation hint
            print!("{}", CLEAR_LINE);
            println!("  Implementation: self.touchpad_format = TouchpadFormat::HIDTouchpad1 {{");
            print!("{}", CLEAR_LINE);
            println!("    x_offset: {},", i);
            print!("{}", CLEAR_LINE);
            println!("    y_offset: {},", i + 2);
            print!("{}", CLEAR_LINE);
            println!("    touch_byte: 0,  // Try different values");
            print!("{}", CLEAR_LINE);
            println!("    touch_mask: 0x01");
            print!("{}", CLEAR_LINE);
            println!("  }};");
            
            break;
        }
    }
    
    if !found_coords {
        print!("{}", CLEAR_LINE);
        println!("No potential touchpad coordinates detected yet.");
        print!("{}", CLEAR_LINE);
        println!("Try moving your finger on the touchpad.");
    }
    
    // Fill remaining lines to maintain stable display area
    let used_lines = 17 + (data.len() + 15) / 16 * 2 + (if found_coords { 8 } else { 2 });
    for _ in used_lines..25 {
        print!("{}", CLEAR_LINE);
        println!();
    }
    
    // Move cursor back up
    for _ in 0..25 {
        print!("{}", CURSOR_UP);
    }
    io::stdout().flush()?;
    
    Ok(())
}