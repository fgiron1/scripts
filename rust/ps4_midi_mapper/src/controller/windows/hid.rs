use hidapi::{HidApi, HidDevice};
use std::error::Error;
use std::collections::HashMap;
use std::any::Any;
use crate::controller::{Controller, types::{ControllerEvent, Button, Axis, DeviceInfo}};

// Constants for touchpad processing
const TOUCHPAD_UPDATE_THRESHOLD: i32 = 5; // Lower threshold for more responsive touchpad
const DS4_TOUCHPAD_X_MAX: i32 = 1920;
const DS4_TOUCHPAD_Y_MAX: i32 = 942;

// Default deadzones
const DEFAULT_STICK_DEADZONE: f32 = 0.10;
const DEFAULT_TRIGGER_DEADZONE: f32 = 0.05;

pub struct HidController {
    device: HidDevice,
    device_info: DeviceInfo,
    button_states: HashMap<Button, bool>,
    axis_values: HashMap<Axis, f32>,
    last_report: Vec<u8>,
    
    // Touchpad state
    touchpad_tracking: bool,
    touchpad_last_x: i32,
    touchpad_last_y: i32,
    touchpad_device: Option<HidDevice>,
    touchpad_format_detected: bool,
    touchpad_format: TouchpadFormat,
    
    // Controller properties
    is_dualshock: bool,
    is_bluetooth: bool,
    debug_mode: bool,
}

// Define various touchpad data formats
#[derive(Debug, Clone, Copy, PartialEq)]
enum TouchpadFormat {
    // Separate HID touchpad device with different formats
    HIDTouchpad1 { x_offset: usize, y_offset: usize, touch_byte: usize, touch_mask: u8 },
    HIDTouchpad2 { x_offset: usize, y_offset: usize, touch_byte: usize, touch_mask: u8 },
    Unknown
}

impl Controller for HidController {
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn Error>> {
        let mut events = Vec::new();
        
        // First poll the separate touchpad device if available
        if let Some(touchpad) = &mut self.touchpad_device {
            let mut buf = [0u8; 64];
            match touchpad.read_timeout(&mut buf, 0) { // Use 0 timeout for immediate return
                Ok(size) if size > 0 => {
                    // Process touchpad data more aggressively - don't wait for changes
                    self.process_touchpad_data(&buf[..size], &mut events)?;
                    
                    // Print debug info if needed
                    if self.debug_mode {
                        println!("⌨️ Touchpad data received: {} bytes", size);
                        print!("  Data: ");
                        for i in 0..min(8, size) {
                            print!("{:02X} ", buf[i]);
                        }
                        println!();
                    }
                },
                Err(e) => {
                    if !e.to_string().contains("timed out") && 
                       !e.to_string().contains("temporarily unavailable") {
                        // Only report serious errors
                        if self.debug_mode {
                            println!("❌ Touchpad error: {}", e);
                        }
                    }
                    // No sleep here to keep polling aggressive
                },
                _ => { /* No data available, continue */ }
            }
        }
        
        // Poll the main controller with zero timeout for instant response
        let mut buf = [0u8; 64];
        match self.device.read_timeout(&mut buf, 0) {
            Ok(size) if size > 0 => {
                // Process all data, not just changed data
                self.last_report = buf[..size].to_vec();
                
                // Parse controller events
                self.parse_hid_report(&buf[..size], &mut events);
            },
            Err(e) => {
                if !e.to_string().contains("timed out") && 
                   !e.to_string().contains("temporarily unavailable") {
                    return Err(format!("Error reading controller: {}", e).into());
                }
                // No sleep here to keep polling aggressive
            },
            _ => { /* No data available, continue */ }
        }
        
        Ok(events)
    }
    
    fn get_device_info(&self) -> DeviceInfo {
        self.device_info.clone()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

}

impl HidController {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        // Initialize HID API
        let api = HidApi::new()?;
        
        // Try to find a compatible controller
        let (device, device_info) = Self::find_controller(&api)?;
        
        // Determine if this is a DualShock controller
        let product_lower = device_info.product.to_lowercase();
        let is_dualshock = product_lower.contains("dualshock") || 
                          product_lower.contains("wireless controller");
        
        // Always look for the separate touchpad device, regardless of controller type
        let touchpad_device = Self::find_touchpad_device(&api)?;
        
        if touchpad_device.is_some() {
            println!("✅ Found separate HID-compliant touchpad device!");
        } else {
            println!("⚠️ No touchpad device found! Touchpad X/Y inputs won't work.");
        }
        
        // Create and return the controller
        Ok(Self {
            device,
            device_info,
            button_states: HashMap::new(),
            axis_values: HashMap::new(),
            last_report: Vec::with_capacity(64),
            touchpad_tracking: false,
            touchpad_last_x: 0,
            touchpad_last_y: 0,
            touchpad_device,
            touchpad_format_detected: false,
            touchpad_format: TouchpadFormat::Unknown,
            is_dualshock,
            is_bluetooth: false, // Will be detected on first report
            debug_mode: false,   // Set to true for debugging
        })
    }

    // Find any compatible controller
    fn find_controller(api: &HidApi) -> Result<(HidDevice, DeviceInfo), Box<dyn Error>> {
        println!("Searching for game controllers...");
        
        for device_info in api.device_list() {
            // Get product name
            let product_name = match device_info.product_string() {
                Some(name) => name,
                None => continue,
            };
            
            // Look for controllers - avoid picking up the touchpad device here
            if (product_name.contains("Controller") || 
                product_name.contains("Gamepad") || 
                product_name.contains("DualShock") ||
                product_name.contains("Xbox")) && 
                !product_name.contains("Touchpad") {
                
                // For controllers, we want the main input interface
                let is_input_interface = 
                    (device_info.usage_page() == 0x01 && device_info.usage() == 0x05) ||
                    (device_info.interface_number() == 0);
                
                if is_input_interface {
                    if let Ok(device) = api.open_path(device_info.path()) {
                        // Get device info
                        let manufacturer = match device.get_manufacturer_string() {
                            Ok(Some(s)) => s,
                            _ => "Unknown".to_string(),
                        };
                            
                        let dev_info = DeviceInfo {
                            vid: device_info.vendor_id(),
                            pid: device_info.product_id(),
                            manufacturer,
                            product: product_name.to_string(),
                        };
                        
                        println!("Found controller: {} (VID: 0x{:04X}, PID: 0x{:04X})", 
                            product_name, device_info.vendor_id(), device_info.product_id());
                        
                        // Set non-blocking mode
                        let _ = device.set_blocking_mode(false);
                        
                        return Ok((device, dev_info));
                    }
                }
            }
        }
        
        Err("No compatible controller found via HID. Make sure your controller is connected and powered on.".into())
    }
    
    // Enhanced touchpad device detection for Windows systems
    fn find_touchpad_device(api: &HidApi) -> Result<Option<HidDevice>, Box<dyn Error>> {
        println!("🔍 Searching for HID-compliant touchpad device...");
        
        // DEBUG: List all HID devices to help identify the touchpad
        println!("\n⚠️ DEBUG: Listing potential touchpad devices:");
        for device_info in api.device_list() {
            if let Some(product) = device_info.product_string() {
                let product_lower = product.to_lowercase();
                
                // Filter to only show devices that might be touchpads
                if product_lower.contains("touch") || 
                   product_lower.contains("pad") || 
                   product_lower.contains("hid-compliant") || 
                   device_info.usage_page() == 0x0D || // Digitizer
                   device_info.usage_page() == 0x01 && device_info.usage() == 0x04 {  // Joystick
                    println!("  - {}", product);
                    println!("    VID: 0x{:04X}, PID: 0x{:04X}, Path: {}", 
                             device_info.vendor_id(), device_info.product_id(),
                             device_info.path().to_string_lossy());
                    println!("    Usage Page: 0x{:04X}, Usage: 0x{:04X}",
                             device_info.usage_page(), device_info.usage());
                }
            }
        }
        println!();
        
        // Focus specifically on finding devices named "HID-compliant touchpad"
        for device_info in api.device_list() {
            if let Some(product) = device_info.product_string() {
                let product_lower = product.to_lowercase();
                
                // Strong check for touchpad devices
                if product_lower == "hid-compliant touchpad" || 
                   product_lower.contains("touchpad") {
                    println!("🎯 Found touchpad device: {}", product);
                    println!("   VID: 0x{:04X}, PID: 0x{:04X}", 
                             device_info.vendor_id(), device_info.product_id());
                    println!("   Usage Page: 0x{:04X}, Usage: 0x{:04X}",
                             device_info.usage_page(), device_info.usage());
                    
                    // Try to open the device
                    match api.open_path(device_info.path()) {
                        Ok(device) => {
                            println!("✅ Successfully opened touchpad device!");
                            
                            // Set to non-blocking mode with more aggressive settings
                            let _ = device.set_blocking_mode(false);
                            
                            return Ok(Some(device));
                        },
                        Err(e) => {
                            println!("⚠️ Failed to open device: {}", e);
                            // Continue to try other devices
                        }
                    }
                }
            }
        }
        
        // If we didn't find a device specifically named touchpad,
        // look for devices with touchpad usage pages
        for device_info in api.device_list() {
            // Check for touchpad usage pages and usages (broader range)
            let is_touchpad_by_usage = 
                device_info.usage_page() == 0x0D || // Digitizer
                device_info.usage_page() == 0x04 || // Touch screen
                device_info.usage_page() == 0x0B || // Haptic page (sometimes used)
                (device_info.usage_page() == 0x01 && // Generic Desktop
                    (device_info.usage() == 0x04 || // Joystick (sometimes touchpads register as this)
                     device_info.usage() == 0x05 || // Game Pad
                     device_info.usage() == 0x06 || // Keyboard (DS4 sometimes registers as this)
                     device_info.usage() == 0x07 || // Keypad
                     device_info.usage() == 0x08)); // Multi-axis Controller
            
            if is_touchpad_by_usage {
                if let Some(product) = device_info.product_string() {
                    // Filter out main controller devices to avoid duplication
                    let product_lower = product.to_lowercase();
                    let is_likely_controller =
                        product_lower.contains("controller") ||
                        product_lower.contains("gamepad");
                    
                    // Skip if it's likely the main controller
                    if is_likely_controller {
                        continue;
                    }
                    
                    println!("🔍 Potential touchpad by usage: {}", product);
                    println!("   VID: 0x{:04X}, PID: 0x{:04X}", 
                             device_info.vendor_id(), device_info.product_id());
                    println!("   Usage Page: 0x{:04X}, Usage: 0x{:04X}",
                             device_info.usage_page(), device_info.usage());
                    
                    // Try to open the device
                    if let Ok(device) = api.open_path(device_info.path()) {
                        println!("✅ Opened potential touchpad device");
                        let _ = device.set_blocking_mode(false);
                        return Ok(Some(device));
                    }
                }
            }
        }
        
        // Last resort - try ANY device that's not the main controller
        // and test if it provides touchpad-like data
        println!("\n⚠️ Trying fallback method for touchpad detection...");
        for device_info in api.device_list() {
            if let Some(product) = device_info.product_string() {
                let product_lower = product.to_lowercase();
                
                // Skip the main controller and devices we already know aren't touchpads
                if product_lower.contains("wireless controller") ||
                   product_lower.contains("camera") ||
                   product_lower.contains("audio") ||
                   product_lower.contains("mouse") ||
                   product_lower.contains("keyboard") {
                    continue;
                }
                
                println!("🔍 Trying device: {}", product);
                if let Ok(device) = api.open_path(device_info.path()) {
                    println!("✅ Opened potential touchpad device - will test at runtime");
                    let _ = device.set_blocking_mode(false);
                    return Ok(Some(device));
                }
            }
        }
        
        println!("❌ No separate touchpad device found. Touchpad X/Y inputs won't work.");
        Ok(None)
    }

    // Enhanced process_touchpad_data with more aggressive format detection
    fn process_touchpad_data(&mut self, data: &[u8], events: &mut Vec<ControllerEvent>) -> Result<(), Box<dyn Error>> {
        if data.len() < 3 {
            return Ok(());
        }
        
        // Print debug data to help identify touchpad format
        if self.debug_mode {
            print!("📱 Touchpad data: ");
            for i in 0..min(24, data.len()) {
                print!("{:02X} ", data[i]);
            }
            println!();
        }
        
        // Auto-detect touchpad format - check more aggressively
        if !self.touchpad_format_detected || self.touchpad_format == TouchpadFormat::Unknown {
            self.detect_touchpad_format(data);
        }
        
        // BRUTE FORCE APPROACH: Try all possible combinations of touchpad data
        // This is less efficient but more likely to find the data somewhere
        
        // Try to locate potential 2-byte X/Y coordinates throughout the buffer
        for offset in 0..data.len().saturating_sub(4) {
            // Look for values that could be valid coordinates
            let x1 = (data[offset] as i32) | ((data[offset + 1] as i32) << 8);
            let y1 = if offset + 3 < data.len() {
                (data[offset + 2] as i32) | ((data[offset + 3] as i32) << 8)
            } else {
                0
            };
            
            // Check if these look like valid touchpad coordinates
            if x1 > 0 && x1 < DS4_TOUCHPAD_X_MAX && y1 > 0 && y1 < DS4_TOUCHPAD_Y_MAX {
                if self.debug_mode && !self.touchpad_format_detected {
                    println!("👆 Potential touchpad coordinates at offset {}: X={}, Y={}", offset, x1, y1);
                }
                
                // Found something that looks like touchpad data!
                if !self.touchpad_format_detected {
                    // Save this format for future use
                    self.touchpad_format = TouchpadFormat::HIDTouchpad1 { 
                        x_offset: offset, 
                        y_offset: offset + 2, 
                        touch_byte: 0,  // Assume touch is active since we found coords
                        touch_mask: 0x01
                    };
                    self.touchpad_format_detected = true;
                    
                    println!("✅ Detected touchpad format: X at offset {}, Y at offset {}", 
                           offset, offset + 2);
                }
                
                // Update position with these coordinates
                self.update_touchpad_position(x1, y1, events);
                return Ok(());
            }
        }
        
        // Also try single-byte coordinates if 2-byte detection failed
        if !self.touchpad_tracking {
            for offset in 0..data.len().saturating_sub(2) {
                // Try single-byte coordinates
                let x2 = data[offset] as i32 * DS4_TOUCHPAD_X_MAX / 255;
                let y2 = if offset + 1 < data.len() { 
                    data[offset + 1] as i32 * DS4_TOUCHPAD_Y_MAX / 255
                } else {
                    0
                };
                
                // Check if these look like valid coordinates
                if x2 > 0 && x2 < DS4_TOUCHPAD_X_MAX && y2 > 0 && y2 < DS4_TOUCHPAD_Y_MAX {
                    if self.debug_mode && !self.touchpad_format_detected {
                        println!("👆 Potential single-byte coordinates at offset {}: X={}, Y={}", 
                               offset, x2, y2);
                    }
                    
                    // Save this format
                    if !self.touchpad_format_detected {
                        self.touchpad_format = TouchpadFormat::HIDTouchpad2 { 
                            x_offset: offset, 
                            y_offset: offset + 1, 
                            touch_byte: 0,
                            touch_mask: 0x01
                        };
                        self.touchpad_format_detected = true;
                        
                        println!("✅ Detected single-byte touchpad format: X at offset {}, Y at offset {}", 
                               offset, offset + 1);
                    }
                    
                    // Update position
                    self.update_touchpad_position(x2, y2, events);
                    return Ok(());
                }
            }
        }
        
        // If we haven't found anything yet, try the detected format if we have one
        match self.touchpad_format {
            TouchpadFormat::HIDTouchpad1 { x_offset, y_offset, .. } => {
                if x_offset + 1 < data.len() && y_offset + 1 < data.len() {
                    let x = ((data[x_offset] as i32) | ((data[x_offset + 1] as i32) << 8)).min(DS4_TOUCHPAD_X_MAX);
                    let y = ((data[y_offset] as i32) | ((data[y_offset + 1] as i32) << 8)).min(DS4_TOUCHPAD_Y_MAX);
                    
                    // Only process if coordinates are valid
                    if x > 0 && y > 0 && x < DS4_TOUCHPAD_X_MAX && y < DS4_TOUCHPAD_Y_MAX {
                        self.update_touchpad_position(x, y, events);
                    } else if self.touchpad_tracking {
                        // If we have tracking but invalid coords, consider it a release
                        self.end_touch(events);
                    }
                }
            },
            
            TouchpadFormat::HIDTouchpad2 { x_offset, y_offset, .. } => {
                if x_offset < data.len() && y_offset < data.len() {
                    // Single-byte coordinates that need scaling
                    let x = (data[x_offset] as i32 * DS4_TOUCHPAD_X_MAX / 255).min(DS4_TOUCHPAD_X_MAX);
                    let y = (data[y_offset] as i32 * DS4_TOUCHPAD_Y_MAX / 255).min(DS4_TOUCHPAD_Y_MAX);
                    
                    if x > 0 && y > 0 {
                        self.update_touchpad_position(x, y, events);
                    } else if self.touchpad_tracking {
                        self.end_touch(events);
                    }
                }
            },
            
            TouchpadFormat::Unknown => {
                // Already tried brute force above, no need to try again
            }
        }
        
        Ok(())
    }
    
    // Helper to detect the touchpad data format from a sample
    fn detect_touchpad_format(&mut self, data: &[u8]) {
        if data.len() < 5 {
            return;
        }
        
        // Print debug info
        if self.debug_mode {
            println!("Analyzing touchpad format. Data sample:");
            for i in 0..min(16, data.len()) {
                print!("{:02X} ", data[i]);
            }
            println!();
        }
        
        // Strategy 1: Try to find any non-zero bytes that might be coordinates
        // Most touchpad formats have data within the first 8 bytes
        let mut non_zero_indices = Vec::new();
        for i in 0..min(16, data.len()) {
            if data[i] != 0 {
                non_zero_indices.push(i);
            }
        }
        
        if non_zero_indices.len() >= 2 {
            if self.debug_mode {
                println!("Non-zero bytes found at indices: {:?}", non_zero_indices);
            }
            
            // Heuristic: If we have at least two non-zero bytes, assume they might be X/Y
            // Check some common touchpad formats based on the data pattern
            
            // Format 1: Common format with 2-byte X/Y values
            if data.len() >= 5 && non_zero_indices.contains(&1) && non_zero_indices.contains(&3) {
                self.touchpad_format = TouchpadFormat::HIDTouchpad1 {
                    x_offset: 1, y_offset: 3, touch_byte: 0, touch_mask: 0x01
                };
                self.touchpad_format_detected = true;
                println!("Detected touchpad format: Format 1 (2-byte X/Y with touch at byte 0)");
                return;
            }
            
            // Format 2: Single-byte X/Y coordinates
            if data.len() >= 3 && non_zero_indices.contains(&1) && non_zero_indices.contains(&2) {
                self.touchpad_format = TouchpadFormat::HIDTouchpad2 {
                    x_offset: 1, y_offset: 2, touch_byte: 0, touch_mask: 0x01
                };
                self.touchpad_format_detected = true;
                println!("Detected touchpad format: Format 2 (single-byte X/Y)");
                return;
            }
            
            // Format 3: Common multi-touch format with contact ID
            if data.len() >= 6 && non_zero_indices.contains(&2) && non_zero_indices.contains(&4) {
                self.touchpad_format = TouchpadFormat::HIDTouchpad1 {
                    x_offset: 2, y_offset: 4, touch_byte: 1, touch_mask: 0x7F
                };
                self.touchpad_format_detected = true;
                println!("Detected touchpad format: Format 3 (multi-touch with contact ID)");
                return;
            }
        }
        
        // If we can't determine the format, set a fallback that will try various formats
        if self.debug_mode {
            println!("Could not determine touchpad format automatically. Will try various formats.");
        }
    }
    
    // Parse HID report from main controller
    fn parse_hid_report(&mut self, data: &[u8], events: &mut Vec<ControllerEvent>) {
        // First report - detect if this is Bluetooth mode
        if !self.last_report.is_empty() && self.is_dualshock && !self.is_bluetooth {
            self.is_bluetooth = data[0] == 0x11; // Common BT report ID
        }
        
        // Process controller based on type
        if self.is_dualshock {
            self.parse_dualshock_report(data, events);
        } else {
            self.parse_generic_report(data, events);
        }
    }
    
    // Parse DualShock 4 reports
    fn parse_dualshock_report(&mut self, data: &[u8], events: &mut Vec<ControllerEvent>) {
        // Handle BT vs USB format - offset buttons in BT mode
        let offset = if self.is_bluetooth { 2 } else { 0 };
        
        if data.len() < 10 + offset {
            return;
        }
        
        // Process buttons - bytes 5-7 for USB, 7-9 for BT
        let btn_offset = 5 + offset;
        
        // Face buttons (byte 5/7)
        self.update_button(Button::Square, (data[btn_offset] & 0x10) != 0, events);
        self.update_button(Button::Cross, (data[btn_offset] & 0x20) != 0, events);
        self.update_button(Button::Circle, (data[btn_offset] & 0x40) != 0, events);
        self.update_button(Button::Triangle, (data[btn_offset] & 0x80) != 0, events);
        
        // Shoulder buttons (byte 6/8)
        self.update_button(Button::L1, (data[btn_offset + 1] & 0x01) != 0, events);
        self.update_button(Button::R1, (data[btn_offset + 1] & 0x02) != 0, events);
        self.update_button(Button::L2, (data[btn_offset + 1] & 0x04) != 0, events);
        self.update_button(Button::R2, (data[btn_offset + 1] & 0x08) != 0, events);
        self.update_button(Button::Share, (data[btn_offset + 1] & 0x10) != 0, events);
        self.update_button(Button::Options, (data[btn_offset + 1] & 0x20) != 0, events);
        self.update_button(Button::L3, (data[btn_offset + 1] & 0x40) != 0, events);
        self.update_button(Button::R3, (data[btn_offset + 1] & 0x80) != 0, events);
        
        // Special buttons (byte 7/9)
        self.update_button(Button::PS, (data[btn_offset + 2] & 0x01) != 0, events);
        self.update_button(Button::Touchpad, (data[btn_offset + 2] & 0x02) != 0, events);
        
        // Process D-pad (lower nibble of byte 5/7)
        let dpad = data[btn_offset] & 0x0F;
        self.process_dpad(dpad, events);
        
        // Process sticks and triggers
        let stick_offset = 1 + offset;
        
        // Analog sticks - normalize to -1.0..1.0 range
        let left_x = self.normalize_stick(data[stick_offset]);
        let left_y = -self.normalize_stick(data[stick_offset + 1]); // Y axis is inverted
        let right_x = self.normalize_stick(data[stick_offset + 2]);
        let right_y = -self.normalize_stick(data[stick_offset + 3]); // Y axis is inverted
        
        // Triggers - normalize to 0.0..1.0 range
        let l2 = self.normalize_trigger(data[stick_offset + 7]);
        let r2 = self.normalize_trigger(data[stick_offset + 8]);
        
        // Update all axis values with reduced frequency to minimize MIDI traffic
        self.update_axis(Axis::LeftStickX, left_x, events);
        self.update_axis(Axis::LeftStickY, left_y, events);
        self.update_axis(Axis::RightStickX, right_x, events);
        self.update_axis(Axis::RightStickY, right_y, events);
        self.update_axis(Axis::L2, l2, events);
        self.update_axis(Axis::R2, r2, events);
    }
    
    // Process the D-pad based on DualShock 4 encoding
    fn process_dpad(&mut self, dpad: u8, events: &mut Vec<ControllerEvent>) {
        let (up, right, down, left) = match dpad {
            0 => (true, false, false, false),   // Up
            1 => (true, true, false, false),    // Up+Right
            2 => (false, true, false, false),   // Right
            3 => (false, true, true, false),    // Down+Right
            4 => (false, false, true, false),   // Down
            5 => (false, false, true, true),    // Down+Left
            6 => (false, false, false, true),   // Left
            7 => (true, false, false, true),    // Up+Left
            _ => (false, false, false, false),  // Released
        };
        
        self.update_button(Button::DpadUp, up, events);
        self.update_button(Button::DpadRight, right, events);
        self.update_button(Button::DpadDown, down, events);
        self.update_button(Button::DpadLeft, left, events);
    }
    
    // Generic HID gamepad parsing - best effort
    fn parse_generic_report(&mut self, data: &[u8], events: &mut Vec<ControllerEvent>) {
        // Basic gamepad typically has:
        // - Byte 0: usually report ID
        // - Byte 1-2: X/Y for left stick
        // - Byte 3-4: X/Y for right stick (if present)
        // - Byte 5+: Buttons as bit fields
        
        if data.len() < 6 {
            return;
        }
        
        // Try to process sticks (assuming standard layout)
        if data.len() >= 5 {
            let left_x = self.normalize_stick(data[1]);
            let left_y = self.normalize_stick(data[2]);
            let right_x = self.normalize_stick(data[3]);
            let right_y = self.normalize_stick(data[4]);
            
            self.update_axis(Axis::LeftStickX, left_x, events);
            self.update_axis(Axis::LeftStickY, -left_y, events); // Inverted for consistency
            self.update_axis(Axis::RightStickX, right_x, events);
            self.update_axis(Axis::RightStickY, -right_y, events); // Inverted for consistency
        }
        
        // Try to detect button state changes in bytes 5-8
        for byte_idx in 0..min(4, data.len() - 5) {
            let byte = data[byte_idx + 5];
            for bit in 0..8 {
                let mask = 1 << bit;
                let pressed = (byte & mask) != 0;
                
                // Convert to abstract button (implementation-specific)
                // This is just a guess and may not work for all controllers
                let button = match (byte_idx, bit) {
                    (0, 0) => Button::Cross,
                    (0, 1) => Button::Circle,
                    (0, 2) => Button::Square,
                    (0, 3) => Button::Triangle,
                    (0, 4) => Button::L1,
                    (0, 5) => Button::R1,
                    (0, 6) => Button::L2,
                    (0, 7) => Button::R2,
                    (1, 0) => Button::Share,
                    (1, 1) => Button::Options,
                    (1, 2) => Button::L3,
                    (1, 3) => Button::R3,
                    (1, 4) => Button::PS,
                    (1, 5) => Button::Touchpad,
                    _ => continue, // Unknown mapping
                };
                
                self.update_button(button, pressed, events);
            }
        }
    }
    
    // Helper function to update button state and generate events on change
    fn update_button(&mut self, button: Button, pressed: bool, events: &mut Vec<ControllerEvent>) {
        let prev_state = self.button_states.get(&button).copied().unwrap_or(false);
        
        if pressed != prev_state {
            events.push(ControllerEvent::ButtonPress {
                button,
                pressed,
            });
            
            self.button_states.insert(button, pressed);
        }
    }
    
    // Helper function to update axis value and generate events on change
    fn update_axis(&mut self, axis: Axis, value: f32, events: &mut Vec<ControllerEvent>) {
        let previous = self.axis_values.get(&axis).copied().unwrap_or(0.0);
        
        // Use different sensitivity thresholds based on axis type
        let min_change = match axis {
            Axis::L2 | Axis::R2 => 0.05,               // Triggers
            Axis::TouchpadX | Axis::TouchpadY => 0.01, // Touchpad (more sensitive)
            _ => 0.02,                                 // Sticks
        };
        
        // Only emit events if change is significant
        if (value - previous).abs() > min_change {
            events.push(ControllerEvent::AxisMove {
                axis,
                value,
            });
            
            self.axis_values.insert(axis, value);
        }
    }
    
    // Helper to update touchpad position and generate events
    fn update_touchpad_position(&mut self, x: i32, y: i32, events: &mut Vec<ControllerEvent>) {
        // Check if position has changed at all - much more sensitive
        let x_diff = (x - self.touchpad_last_x).abs();
        let y_diff = (y - self.touchpad_last_y).abs();
        
        if !self.touchpad_tracking || x_diff > 0 || y_diff > 0 {
            // Send touchpad event every time
            events.push(ControllerEvent::TouchpadMove {
                x: Some(x),
                y: Some(y),
            });
            
            // Normalize coordinates for MIDI mapping
            let x_norm = (x as f32 / DS4_TOUCHPAD_X_MAX as f32) * 2.0 - 1.0;
            let y_norm = -((y as f32 / DS4_TOUCHPAD_Y_MAX as f32) * 2.0 - 1.0); // Invert Y
            
            // More sensitive axis events
            if !self.touchpad_tracking || x_diff > 0 {
                events.push(ControllerEvent::AxisMove {
                    axis: Axis::TouchpadX,
                    value: x_norm,
                });
            }
            
            if !self.touchpad_tracking || y_diff > 0 {
                events.push(ControllerEvent::AxisMove {
                    axis: Axis::TouchpadY,
                    value: y_norm,
                });
            }
            
            // Update state
            self.touchpad_last_x = x;
            self.touchpad_last_y = y;
            self.touchpad_tracking = true;
            
            if self.debug_mode {
                println!("👆 Touchpad: X={}, Y={} (normalized: {:.2}, {:.2})", 
                    x, y, x_norm, y_norm);
            }
        }
    }
    
    // Helper to handle touch release
    fn end_touch(&mut self, events: &mut Vec<ControllerEvent>) {
        if self.touchpad_tracking {
            self.touchpad_tracking = false;
            
            // Reset axis values to center
            events.push(ControllerEvent::AxisMove {
                axis: Axis::TouchpadX,
                value: 0.0,
            });
            
            events.push(ControllerEvent::AxisMove {
                axis: Axis::TouchpadY,
                value: 0.0,
            });
            
            if self.debug_mode {
                println!("📱 Touchpad: Touch released");
            }
        }
    }
    
    // Normalize analog stick value from 0-255 to -1.0-1.0
    fn normalize_stick(&self, value: u8) -> f32 {
        // Center is at 128
        let centered = (value as f32) - 128.0;
        let normalized = centered / 128.0;
        
        // Apply deadzone
        if normalized.abs() < DEFAULT_STICK_DEADZONE {
            return 0.0;
        }
        
        // Rescale values outside deadzone to use full range (-1.0 to 1.0)
        let sign = if normalized < 0.0 { -1.0 } else { 1.0 };
        let rescaled = sign * ((normalized.abs() - DEFAULT_STICK_DEADZONE) / (1.0 - DEFAULT_STICK_DEADZONE));
        
        // Clamp to valid range
        rescaled.max(-1.0).min(1.0)
    }
    
    // Normalize trigger value from 0-255 to 0.0-1.0
    fn normalize_trigger(&self, value: u8) -> f32 {
        let normalized = value as f32 / 255.0;
        
        // Apply deadzone
        if normalized < DEFAULT_TRIGGER_DEADZONE {
            return 0.0;
        }
        
        // Rescale to use full range
        ((normalized - DEFAULT_TRIGGER_DEADZONE) / (1.0 - DEFAULT_TRIGGER_DEADZONE)).min(1.0)
    }
    
    // Print detailed debug information about the touchpad data format
    pub fn print_touchpad_debug(&self) {
        println!("\nTouchpad Debug Information:");
        println!("-------------------------");
        println!("Separate touchpad device present: {}", self.touchpad_device.is_some());
        println!("Format detected: {}", self.touchpad_format_detected);
        
        match self.touchpad_format {
            TouchpadFormat::HIDTouchpad1 { x_offset, y_offset, touch_byte, touch_mask } => {
                println!("Format type: HID Touchpad Type 1");
                println!("X offset: {}", x_offset);
                println!("Y offset: {}", y_offset);
                println!("Touch state byte: {} (mask: 0x{:02X})", touch_byte, touch_mask);
            },
            TouchpadFormat::HIDTouchpad2 { x_offset, y_offset, touch_byte, touch_mask } => {
                println!("Format type: HID Touchpad Type 2");
                println!("X offset: {}", x_offset);
                println!("Y offset: {}", y_offset);
                println!("Touch state byte: {} (mask: 0x{:02X})", touch_byte, touch_mask);
            },
            TouchpadFormat::Unknown => {
                println!("Format type: Unknown (still detecting)");
            }
        }
        
        println!("Current tracking state: {}", self.touchpad_tracking);
        println!("Last tracked position: ({}, {})", self.touchpad_last_x, self.touchpad_last_y);
        println!("-------------------------");
    }
    
    // Enable touchpad debugging
    pub fn enable_touchpad_debug(&mut self) {
        self.debug_mode = true;
        println!("\n📱 TOUCHPAD DEBUGGING ENABLED");
        println!("============================");
        
        // Force format to Unknown to trigger re-detection
        self.touchpad_format = TouchpadFormat::Unknown;
        self.touchpad_format_detected = false;
        
        // Print current state
        println!("Is DualShock: {}", self.is_dualshock);
        println!("Has separate touchpad device: {}", self.touchpad_device.is_some());
        println!("Current tracking state: {}", self.touchpad_tracking);
        println!("Bluetooth mode: {}", self.is_bluetooth);
        println!("");
        println!("HINT: Try swiping on the touchpad in different patterns");
        println!("============================\n");
    }
}

// Simple helper for min
fn min(a: usize, b: usize) -> usize {
    if a < b { a } else { b }
}