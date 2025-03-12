use hidapi::{HidApi, HidDevice};
use std::error::Error;
use std::collections::HashMap;
use std::time::Duration;
use std::thread;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use crate::config::{AXIS_MAPPINGS, BUTTON_MAPPINGS};
use crate::controller::{Controller, types::{ControllerEvent, Button, Axis, DeviceInfo}};
use crate::controller::profiles::{self};

// Static to track if display has been initialized
static DISPLAY_INITIALIZED: AtomicBool = AtomicBool::new(false);

// Constants for touchpad processing
const DS4_TOUCHPAD_X_MAX: i32 = 1920;
const DS4_TOUCHPAD_Y_MAX: i32 = 942; 
const TOUCHPAD_UPDATE_THRESHOLD: i32 = 20; // Minimum difference to register a touchpad movement

pub struct HidController {
    device: HidDevice,
    device_info: DeviceInfo,
    button_states: HashMap<Button, bool>,
    axis_values: HashMap<Axis, f32>,
    last_report: Vec<u8>,
    debug_mode: bool,
    touchpad_tracking: bool,
    touchpad_last_x: i32,
    touchpad_last_y: i32,
    touchpad_device: Option<HidDevice>,
    touchpad_device_path: Option<String>,
}

impl Controller for HidController {
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn Error>> {
        let mut events = Vec::new();
        
        // First poll the touchpad device if available
        if self.touchpad_device.is_some() {
            self.poll_touchpad(&mut events)?;
        }
        
        // Then poll the main controller as before
        // Buffer to read HID report
        let mut buf = [0u8; 64];
        
        // Read with timeout (1ms for low latency)
        match self.device.read_timeout(&mut buf, 1) {
            Ok(size) if size > 0 => {
                // Only process if data has changed
                if buf[..size] != self.last_report[..size.min(self.last_report.len())] {
                    // Save report for comparison next time
                    self.last_report = buf[..size].to_vec();
                    
                    // Parse and process the main controller events
                    let mut controller_events = self.parse_hid_report(&buf[..size]);
                    events.append(&mut controller_events);
                }
            },
            Ok(_) => {
                // No data or no change, just continue
            },
            Err(e) => {
                // Handle "Resource temporarily unavailable" error by waiting
                if e.to_string().contains("temporarily unavailable") {
                    thread::sleep(Duration::from_millis(1)); // Minimal sleep for latency
                } else {
                    return Err(format!("Error reading controller: {}", e).into());
                }
            }
        }
        
        Ok(events)
    }
    
    fn get_device_info(&self) -> DeviceInfo {
        self.device_info.clone()
    }
}

impl HidController {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        // Initialize HID API
        let api = HidApi::new()?;
        
        // Try to find a PlayStation 4 controller
        let (device, device_info) = Self::find_controller(&api)?;
        
        // Disable mapper's display by setting an environment variable
        std::env::set_var("PS4_DISABLE_MAPPER_DISPLAY", "1");

        // Get a suitable profile for this controller
        let profiles = profiles::create_profiles();
        let profile = match profiles::get_profile_for_device(&device_info, &profiles) {
            Some(profile) => profile.clone(),
            None => profiles::create_generic_profile(),
        };
        println!("Using controller profile: {}", profile.name);
        println!("Found controller: {} (VID: 0x{:04X}, PID: 0x{:04X})", 
            device_info.product, device_info.vid, device_info.pid);
        
        // Try to find the separate touchpad device
        let (touchpad_device, touchpad_device_path) = Self::find_touchpad_device(&api, device_info.vid);
        
        // Create with default states
        Ok(Self {
            device,
            device_info,
            button_states: HashMap::new(),
            axis_values: HashMap::new(),
            last_report: vec![0; 64],  // Default buffer size
            debug_mode: true,          // Enable debug mode by default
            touchpad_tracking: false,
            touchpad_last_x: 0,
            touchpad_last_y: 0,
            touchpad_device,
            touchpad_device_path,
        })
    }

    fn find_touchpad_device(api: &HidApi, controller_vid: u16) -> (Option<HidDevice>, Option<String>) {
        println!("Searching for touchpad device...");
        
        // Search for a device with the same VID as the controller but possibly different PID
        // and with "touch" or "pad" in the product name
        for device_info in api.device_list() {
            if device_info.vendor_id() == controller_vid {
                if let Some(product) = device_info.product_string() {
                    let product_lower = product.to_lowercase();
                    if product_lower.contains("touch") || product_lower.contains("pad") {
                        println!("Possible touchpad device found: {}", product);
                        
                        // Try to open the device
                        if let Ok(device) = api.open_path(device_info.path()) {
                            println!("Successfully opened touchpad device: {} (VID: 0x{:04X}, PID: 0x{:04X})",
                                product, device_info.vendor_id(), device_info.product_id());
                            
                            // Set non-blocking mode
                            let _ = device.set_blocking_mode(false);
                            
                            // Return the device and its path
                            return (Some(device), Some(device_info.path().to_string_lossy().to_string()));
                        }
                    }
                }
            }
        }
        
        println!("No separate touchpad device found");
        (None, None)
    }

    // Add a method to poll the touchpad device
    fn poll_touchpad(&mut self, events: &mut Vec<ControllerEvent>) -> Result<(), Box<dyn Error>> {
        if let Some(touchpad) = &mut self.touchpad_device {
            let mut buf = [0u8; 64];
            
            // Try to read from the touchpad device
            match touchpad.read_timeout(&mut buf, 1) {
                Ok(size) if size > 0 => {
                    // Process the touchpad data
                    self.process_touchpad_data(&buf[..size], events);
                },
                Ok(_) => {
                    // No data available, that's fine
                },
                Err(e) => {
                    // Only report errors if they're not just "no data available"
                    if !e.to_string().contains("temporarily unavailable") {
                        eprintln!("Error reading from touchpad device: {}", e);
                    }
                }
            }
        }
        
        Ok(())
    }

    fn process_touchpad_data(&mut self, data: &[u8], events: &mut Vec<ControllerEvent>) {
        if data.len() < 5 {
            return; // Not enough data
        }
        
        // Debug output for the touchpad data
        if self.debug_mode {
            print!("\x1B[23;0HTouchpad Data: ");
            for i in 0..std::cmp::min(data.len(), 16) {
                print!("{:02X} ", data[i]);
            }
            print!("                                            ");
            io::stdout().flush().unwrap();
        }
        
        // The touchpad device may use a different format
        // Most common format for touch devices:
        // Byte 0: Report ID or contact count
        // Byte 1-2: X coordinate (little-endian)
        // Byte 3-4: Y coordinate (little-endian)
        // Byte 5: Might contain touch state (pressed/released)
        
        // Extract potential X and Y coordinates (try different byte offsets)
        let potential_formats = [
            // Format 1: X at bytes 1-2, Y at bytes 3-4
            (1, 3),
            // Format 2: X at bytes 2-3, Y at bytes 4-5
            (2, 4),
            // Format 3: Direct coordinates at bytes 0-3
            (0, 2)
        ];
        
        for (x_offset, y_offset) in potential_formats.iter() {
            if *x_offset + 1 < data.len() && *y_offset + 1 < data.len() {
                let x = ((data[*x_offset] as i32) | ((data[*x_offset + 1] as i32) << 8)).min(DS4_TOUCHPAD_X_MAX);
                let y = ((data[*y_offset] as i32) | ((data[*y_offset + 1] as i32) << 8)).min(DS4_TOUCHPAD_Y_MAX);
                
                // Look for a touch state byte
                let touch_state_byte = if data.len() > 5 { data[5] } else { 0 };
                let is_touching = 
                    // Common bit patterns for "touching" state
                    touch_state_byte != 0 || 
                    // Or detect based on coordinates (if not zero, likely touching)
                    (x > 0 && y > 0 && x < DS4_TOUCHPAD_X_MAX && y < DS4_TOUCHPAD_Y_MAX);
                
                if is_touching {
                    // We have touchpad activity!
                    
                    // Only send events if position has changed significantly
                    let x_diff = (x - self.touchpad_last_x).abs();
                    let y_diff = (y - self.touchpad_last_y).abs();
                    
                    if !self.touchpad_tracking || 
                       x_diff > TOUCHPAD_UPDATE_THRESHOLD || 
                       y_diff > TOUCHPAD_UPDATE_THRESHOLD {
                        
                        // Push touchpad event
                        events.push(ControllerEvent::TouchpadMove {
                            x: Some(x),
                            y: Some(y),
                        });
                        
                        // Normalize coordinates and send axis events
                        let x_norm = (x as f32 / DS4_TOUCHPAD_X_MAX as f32) * 2.0 - 1.0;
                        let y_norm = -((y as f32 / DS4_TOUCHPAD_Y_MAX as f32) * 2.0 - 1.0); // Invert Y
                        
                        self.check_axis_changed(Axis::TouchpadX, x_norm, events);
                        self.check_axis_changed(Axis::TouchpadY, y_norm, events);
                        
                        // Update state
                        self.touchpad_last_x = x;
                        self.touchpad_last_y = y;
                        self.touchpad_tracking = true;
                        
                        // Debug output
                        if self.debug_mode {
                            print!("\x1B[24;0HTouchpad: X={:<4} Y={:<4} | Format: x_off={}, y_off={} | CC: {}, {}", 
                                 x, y, *x_offset, *y_offset,
                                 ((x_norm + 1.0) / 2.0 * 127.0) as u8,
                                 ((y_norm + 1.0) / 2.0 * 127.0) as u8);
                            io::stdout().flush().unwrap();
                        }
                        
                        return; // Successfully processed touchpad data
                    }
                } else if self.touchpad_tracking {
                    // Touch ended
                    self.touchpad_tracking = false;
                    
                    // Reset axis values
                    self.check_axis_changed(Axis::TouchpadX, 0.0, events);
                    self.check_axis_changed(Axis::TouchpadY, 0.0, events);
                    
                    if self.debug_mode {
                        print!("\x1B[24;0HTouchpad: Released                                              ");
                        io::stdout().flush().unwrap();
                    }
                    
                    return;
                }
            }
        }
        
        // If we get here, we couldn't find valid touchpad data in any of the formats
        if self.debug_mode {
            print!("\x1B[24;0HTouchpad: Could not determine data format                      ");
            io::stdout().flush().unwrap();
        }
    }

    // Find any compatible controller
    fn find_controller(api: &HidApi) -> Result<(HidDevice, DeviceInfo), Box<dyn Error>> {
        println!("Searching for controllers...");
        
        // Check all connected HID devices
        for device_info in api.device_list() {
            // Get product name from the device info
            let product_name = match device_info.product_string() {
                Some(name) => name,
                None => continue,
            };
            
            // Look for devices that are likely to be controllers
            if product_name.contains("Controller") || 
               product_name.contains("Gamepad") || 
               product_name.contains("DualShock") ||
               product_name.contains("Xbox") {
                
                // For controllers, we want the interface that provides full controller data
                // This condition may need to be adjusted for different controller types
                let is_input_interface = 
                    (device_info.usage_page() == 0x01 && device_info.usage() == 0x05) ||
                    (device_info.interface_number() == 0);
                
                if is_input_interface {
                    // Try to open the device
                    if let Ok(device) = api.open_path(device_info.path()) {
                        // Get string info
                        let manufacturer = match device.get_manufacturer_string() {
                            Ok(Some(s)) => s,
                            _ => "Unknown".to_string(),
                        };
                            
                        let product = product_name.to_string();
                        
                        // Create device info
                        let dev_info = DeviceInfo {
                            vid: device_info.vendor_id(),
                            pid: device_info.product_id(),
                            manufacturer,
                            product,
                        };
                        
                        // Try to set non-blocking mode for lower latency
                        let _ = device.set_blocking_mode(false);
                        
                        return Ok((device, dev_info));
                    }
                }
            }
        }
        
        Err("No compatible controller found via HID. Make sure your controller is connected and powered on.".into())
    }
    
    // Debug information display - first time setup
    // Replace your current setup_debug_display function with this one
    fn setup_debug_display(&self) {
        // Clear screen
        print!("\x1B[2J\x1B[H");
        
        // Print static headers
        println!("DualShock Controller Debug - Wireless Controller");
        println!("Press buttons on your controller to see input events");
        println!("Press Ctrl+C to exit\n");
        
        // Static labels for input values
        println!("ANALOG INPUTS                | MIDI MAPPING");
        println!("------------------------------|----------------------------");
        println!("Left Stick X:                | CC");
        println!("Left Stick Y:                | CC");
        println!("Right Stick X:               | CC");
        println!("Right Stick Y:               | CC");
        println!("Left Trigger:                | CC");
        println!("Right Trigger:               | CC");
        println!("D-Pad:                       | No notes ON");
        println!("");
        println!("BUTTONS                      | NOTE MAPPING");
        println!("------------------------------|----------------------------\n");
        println!("                 □:off  ×:off  ○:off  △:off  | □:0  ×:0  ○:0  △:0");
        println!("                 L1:off  R1:off  L2:off  R2:off  | L1:0  R1:0  L2:0  R2:0");
        println!("                 L3:off  R3:off                | L3:0  R3:0");
        println!("                 SHARE:off  OPTIONS:off  PS:off  | SH:0  OPT:0  PS:0  TP:0");
        println!("");
        println!("");
        println!("");
        println!("");
        
        io::stdout().flush().unwrap();
    }
    
    fn update_debug_data(&mut self, data: &[u8]) {
        if !self.debug_mode || data.len() < 10 {
            return;
        }
        
        // Get raw values from the actual data locations
        let left_x_raw = data[1];
        let left_y_raw = data[2];
        let right_x_raw = data[3];
        let right_y_raw = data[4];
        let l2_raw = data[8];
        let r2_raw = data[9];
        
        // Use a fixed deadzone of 0.05 for debug display
        let left_x_norm = self.normalize_stick_value(left_x_raw, 0.05);
        let left_y_norm = -self.normalize_stick_value(left_y_raw, 0.05);
        let right_x_norm = self.normalize_stick_value(right_x_raw, 0.05);
        let right_y_norm = -self.normalize_stick_value(right_y_raw, 0.05);
        let l2_norm = l2_raw as f32 / 255.0;
        let r2_norm = r2_raw as f32 / 255.0;
        
        // Extract D-pad value (lower 4 bits of byte 5)
        let dpad = data[5] & 0x0F;
        let dpad_dir = match dpad {
            0 => "Up",
            1 => "Up+Right",
            2 => "Right",
            3 => "Down+Right",
            4 => "Down",
            5 => "Down+Left",
            6 => "Left",
            7 => "Up+Left",
            8 => "Released",
            _ => "Invalid",
        };
        
        // Extract button states
        let square   = (data[5] & 0x10) != 0;
        let cross    = (data[5] & 0x20) != 0;
        let circle   = (data[5] & 0x40) != 0;
        let triangle = (data[5] & 0x80) != 0;
        
        let l1      = (data[6] & 0x01) != 0;
        let r1      = (data[6] & 0x02) != 0;
        let l2_btn  = (data[6] & 0x04) != 0;
        let r2_btn  = (data[6] & 0x08) != 0;
        
        let l3      = (data[6] & 0x40) != 0;
        let r3      = (data[6] & 0x80) != 0;
        
        let share   = (data[6] & 0x10) != 0;
        let options = (data[6] & 0x20) != 0;
        let ps_btn  = (data[7] & 0x01) != 0;
        let touchpad = (data[7] & 0x02) != 0;
        
        // Find MIDI CC mappings for axes
        let left_x_cc = AXIS_MAPPINGS.iter().find(|m| m.axis == Axis::LeftStickX).map(|m| m.cc).unwrap_or(0);
        let left_y_cc = AXIS_MAPPINGS.iter().find(|m| m.axis == Axis::LeftStickY).map(|m| m.cc).unwrap_or(0);
        let right_x_cc = AXIS_MAPPINGS.iter().find(|m| m.axis == Axis::RightStickX).map(|m| m.cc).unwrap_or(0);
        let right_y_cc = AXIS_MAPPINGS.iter().find(|m| m.axis == Axis::RightStickY).map(|m| m.cc).unwrap_or(0);
        let l2_cc = AXIS_MAPPINGS.iter().find(|m| m.axis == Axis::L2).map(|m| m.cc).unwrap_or(0);
        let r2_cc = AXIS_MAPPINGS.iter().find(|m| m.axis == Axis::R2).map(|m| m.cc).unwrap_or(0);
        
        // Calculate MIDI CC values
        let left_x_midi = ((left_x_norm + 1.0) / 2.0 * 127.0) as u8;
        let left_y_midi = ((left_y_norm + 1.0) / 2.0 * 127.0) as u8;
        let right_x_midi = ((right_x_norm + 1.0) / 2.0 * 127.0) as u8;
        let right_y_midi = ((right_y_norm + 1.0) / 2.0 * 127.0) as u8;
        let l2_midi = (l2_norm * 127.0) as u8;
        let r2_midi = (r2_norm * 127.0) as u8;
        
        // Find button note mappings
        let square_note = BUTTON_MAPPINGS.iter().find(|m| m.button == Button::Square).map(|m| m.note).unwrap_or(0);
        let cross_note = BUTTON_MAPPINGS.iter().find(|m| m.button == Button::Cross).map(|m| m.note).unwrap_or(0);
        let circle_note = BUTTON_MAPPINGS.iter().find(|m| m.button == Button::Circle).map(|m| m.note).unwrap_or(0);
        let triangle_note = BUTTON_MAPPINGS.iter().find(|m| m.button == Button::Triangle).map(|m| m.note).unwrap_or(0);
        
        let l1_note = BUTTON_MAPPINGS.iter().find(|m| m.button == Button::L1).map(|m| m.note).unwrap_or(0);
        let r1_note = BUTTON_MAPPINGS.iter().find(|m| m.button == Button::R1).map(|m| m.note).unwrap_or(0);
        let l2_note = BUTTON_MAPPINGS.iter().find(|m| m.button == Button::L2).map(|m| m.note).unwrap_or(0);
        let r2_note = BUTTON_MAPPINGS.iter().find(|m| m.button == Button::R2).map(|m| m.note).unwrap_or(0);
        
        let l3_note = BUTTON_MAPPINGS.iter().find(|m| m.button == Button::L3).map(|m| m.note).unwrap_or(0);
        let r3_note = BUTTON_MAPPINGS.iter().find(|m| m.button == Button::R3).map(|m| m.note).unwrap_or(0);
        
        let share_note = BUTTON_MAPPINGS.iter().find(|m| m.button == Button::Share).map(|m| m.note).unwrap_or(0);
        let options_note = BUTTON_MAPPINGS.iter().find(|m| m.button == Button::Options).map(|m| m.note).unwrap_or(0);
        let ps_note = BUTTON_MAPPINGS.iter().find(|m| m.button == Button::PS).map(|m| m.note).unwrap_or(0);
        let touchpad_note = BUTTON_MAPPINGS.iter().find(|m| m.button == Button::Touchpad).map(|m| m.note).unwrap_or(0);
        
        let dpad_up_note = BUTTON_MAPPINGS.iter().find(|m| m.button == Button::DpadUp).map(|m| m.note).unwrap_or(0);
        let dpad_down_note = BUTTON_MAPPINGS.iter().find(|m| m.button == Button::DpadDown).map(|m| m.note).unwrap_or(0);
        let dpad_left_note = BUTTON_MAPPINGS.iter().find(|m| m.button == Button::DpadLeft).map(|m| m.note).unwrap_or(0);
        let dpad_right_note = BUTTON_MAPPINGS.iter().find(|m| m.button == Button::DpadRight).map(|m| m.note).unwrap_or(0);
    
        print!("\x1B[7;28H{:3} ({:+.2}) | CC {:2} = {:3}", left_x_raw, left_x_norm, left_x_cc, left_x_midi);
        print!("\x1B[8;28H{:3} ({:+.2}) | CC {:2} = {:3}", left_y_raw, left_y_norm, left_y_cc, left_y_midi);
        print!("\x1B[9;28H{:3} ({:+.2}) | CC {:2} = {:3}", right_x_raw, right_x_norm, right_x_cc, right_x_midi);
        print!("\x1B[10;28H{:3} ({:+.2}) | CC {:2} = {:3}", right_y_raw, right_y_norm, right_y_cc, right_y_midi);
        print!("\x1B[11;28H{:3} ({:.2})  | CC {:2} = {:3}", l2_raw, l2_norm, l2_cc, l2_midi);
        print!("\x1B[12;28H{:3} ({:.2})  | CC {:2} = {:3}", r2_raw, r2_norm, r2_cc, r2_midi);
        
        // D-pad - show active notes based on direction
        let dpad_notes = match dpad {
            0 => format!("Note {} ON", dpad_up_note),
            1 => format!("Notes {}, {} ON", dpad_up_note, dpad_right_note),
            2 => format!("Note {} ON", dpad_right_note),
            3 => format!("Notes {}, {} ON", dpad_down_note, dpad_right_note),
            4 => format!("Note {} ON", dpad_down_note),
            5 => format!("Notes {}, {} ON", dpad_down_note, dpad_left_note),
            6 => format!("Note {} ON", dpad_left_note),
            7 => format!("Notes {}, {} ON", dpad_up_note, dpad_left_note),
            _ => format!("No notes ON"),
        };
        
        print!("\x1B[13;28H{:<10} | {}", dpad_dir, dpad_notes);
        
        // Face buttons with active state and note mapping
        print!("\x1B[16;17H□:{:<4} ×:{:<4} ○:{:<4} △:{:<4}", 
            if square   { "ON" } else { "off" },
            if cross    { "ON" } else { "off" },
            if circle   { "ON" } else { "off" },
            if triangle { "ON" } else { "off" });
            
        // Show note mappings for face buttons
        print!(" | □:{:<2}  ×:{:<2}  ○:{:<2}  △:{:<2}", 
            if square { square_note } else { 0 },
            if cross { cross_note } else { 0 },
            if circle { circle_note } else { 0 },
            if triangle { triangle_note } else { 0 });
        
        // Shoulder buttons with active state and note mapping
        print!("\x1B[17;17HL1:{:<4} R1:{:<4} L2:{:<4} R2:{:<4}", 
            if l1      { "ON" } else { "off" },
            if r1      { "ON" } else { "off" },
            if l2_btn  { "ON" } else { "off" },
            if r2_btn  { "ON" } else { "off" });
            
        // Show note mappings for shoulder buttons
        print!(" | L1:{:<2}  R1:{:<2}  L2:{:<2}  R2:{:<2}", 
            if l1 { l1_note } else { 0 },
            if r1 { r1_note } else { 0 },
            if l2_btn { l2_note } else { 0 },
            if r2_btn { r2_note } else { 0 });
            
        // Stick press buttons with active state and note mapping
        print!("\x1B[18;17HL3:{:<4} R3:{:<4}               ", 
            if l3 { "ON" } else { "off" },
            if r3 { "ON" } else { "off" });
            
        // Show note mappings for stick presses
        print!(" | L3:{:<2}  R3:{:<2}", 
            if l3 { l3_note } else { 0 },
            if r3 { r3_note } else { 0 });
            
        // Special buttons with active state and note mapping
        print!("\x1B[19;17HSHARE:{:<4} OPTIONS:{:<4} PS:{:<4}", 
            if share    { "ON" } else { "off" },
            if options  { "ON" } else { "off" },
            if ps_btn   { "ON" } else { "off" });
            
        // Show note mappings for special buttons
        print!(" | SH:{:<2}  OPT:{:<2}  PS:{:<2}  TP:{:<2}", 
            if share { share_note } else { 0 },
            if options { options_note } else { 0 },
            if ps_btn { ps_note } else { 0 },
            if touchpad { touchpad_note } else { 0 });
            
        // Raw Button Bits and Raw HID Report
        print!("\x1B[21;15HRaw Button Bits: {:08b} {:08b} {:08b}", data[5], data[6], data[7]);
        
        print!("\x1B[22;15HRaw HID Report:  ");
        for i in 0..std::cmp::min(data.len(), 10) {
            print!("{:02X} ", data[i]);
        }
        
        // Show touchpad info
        print!("\x1B[23;15H");
        
        // Try to find touchpad data
        if data.len() >= 35 && self.device_info.product.to_lowercase().contains("dualshock") {
            let is_active = (data[35] & 0x80) != 0;
            
            if is_active && data.len() >= 39 {
                // Extract touchpad coordinates
                let x_low = data[36] as i32;
                let x_high = (data[37] & 0x0F) as i32;
                let x = (x_high << 8) | x_low;
                
                let y_high = ((data[37] & 0xF0) >> 4) as i32;
                let y_low = data[38] as i32;
                let y = (y_high << 8) | y_low;
                
                print!("Touchpad: X={:<4} Y={:<4} (Active)", x, y);
            } else {
                print!("Touchpad: Inactive (No touch data)");
            }
        } else {
            print!("Touchpad: Not detected or insufficient data");
        }
        
        io::stdout().flush().unwrap();
    }

    // Parse HID report based on controller profile
    fn parse_hid_report(&mut self, data: &[u8]) -> Vec<ControllerEvent> {
        let mut events = Vec::new();
        
        // Setup display if needed
        if !DISPLAY_INITIALIZED.load(Ordering::Relaxed) {
            self.setup_debug_display();
            DISPLAY_INITIALIZED.store(true, Ordering::Relaxed);
        }
        
        // Update live data display
        self.update_debug_data(data);
        
        // Only proceed if we have enough data
        if data.len() < 10 {
            return events;
        }
        
        // SPECIFIC DUALSHOCK 4 MAPPING
        // ===========================================
        
        // Process buttons - buttons are individual bits in bytes 5, 6, and 7
        
        // Square, Cross, Circle, Triangle (Byte 5)
        self.update_button_state(Button::Square, (data[5] & 0x10) != 0, &mut events);
        self.update_button_state(Button::Cross, (data[5] & 0x20) != 0, &mut events);
        self.update_button_state(Button::Circle, (data[5] & 0x40) != 0, &mut events);
        self.update_button_state(Button::Triangle, (data[5] & 0x80) != 0, &mut events);
        
        // L1, R1, L2, R2, Share, Options, L3, R3 (Byte 6)
        self.update_button_state(Button::L1, (data[6] & 0x01) != 0, &mut events);
        self.update_button_state(Button::R1, (data[6] & 0x02) != 0, &mut events);
        self.update_button_state(Button::L2, (data[6] & 0x04) != 0, &mut events);
        self.update_button_state(Button::R2, (data[6] & 0x08) != 0, &mut events);
        self.update_button_state(Button::Share, (data[6] & 0x10) != 0, &mut events);
        self.update_button_state(Button::Options, (data[6] & 0x20) != 0, &mut events);
        self.update_button_state(Button::L3, (data[6] & 0x40) != 0, &mut events);
        self.update_button_state(Button::R3, (data[6] & 0x80) != 0, &mut events);
        
        // PS button, Touchpad (Byte 7)
        self.update_button_state(Button::PS, (data[7] & 0x01) != 0, &mut events);
        self.update_button_state(Button::Touchpad, (data[7] & 0x02) != 0, &mut events);
        
        // Process D-pad which is in the lower 4 bits of byte 5
        let dpad = data[5] & 0x0F;
        
        // Map D-pad values to specific buttons
        match dpad {
            0 => { // Up
                self.update_button_state(Button::DpadUp, true, &mut events);
                self.update_button_state(Button::DpadDown, false, &mut events);
                self.update_button_state(Button::DpadLeft, false, &mut events);
                self.update_button_state(Button::DpadRight, false, &mut events);
            },
            1 => { // Up + Right
                self.update_button_state(Button::DpadUp, true, &mut events);
                self.update_button_state(Button::DpadDown, false, &mut events);
                self.update_button_state(Button::DpadLeft, false, &mut events);
                self.update_button_state(Button::DpadRight, true, &mut events);
            },
            2 => { // Right
                self.update_button_state(Button::DpadUp, false, &mut events);
                self.update_button_state(Button::DpadDown, false, &mut events);
                self.update_button_state(Button::DpadLeft, false, &mut events);
                self.update_button_state(Button::DpadRight, true, &mut events);
            },
            3 => { // Down + Right
                self.update_button_state(Button::DpadUp, false, &mut events);
                self.update_button_state(Button::DpadDown, true, &mut events);
                self.update_button_state(Button::DpadLeft, false, &mut events);
                self.update_button_state(Button::DpadRight, true, &mut events);
            },
            4 => { // Down
                self.update_button_state(Button::DpadUp, false, &mut events);
                self.update_button_state(Button::DpadDown, true, &mut events);
                self.update_button_state(Button::DpadLeft, false, &mut events);
                self.update_button_state(Button::DpadRight, false, &mut events);
            },
            5 => { // Down + Left
                self.update_button_state(Button::DpadUp, false, &mut events);
                self.update_button_state(Button::DpadDown, true, &mut events);
                self.update_button_state(Button::DpadLeft, true, &mut events);
                self.update_button_state(Button::DpadRight, false, &mut events);
            },
            6 => { // Left
                self.update_button_state(Button::DpadUp, false, &mut events);
                self.update_button_state(Button::DpadDown, false, &mut events);
                self.update_button_state(Button::DpadLeft, true, &mut events);
                self.update_button_state(Button::DpadRight, false, &mut events);
            },
            7 => { // Up + Left
                self.update_button_state(Button::DpadUp, true, &mut events);
                self.update_button_state(Button::DpadDown, false, &mut events);
                self.update_button_state(Button::DpadLeft, true, &mut events);
                self.update_button_state(Button::DpadRight, false, &mut events);
            },
            _ => { // Released or invalid
                self.update_button_state(Button::DpadUp, false, &mut events);
                self.update_button_state(Button::DpadDown, false, &mut events);
                self.update_button_state(Button::DpadLeft, false, &mut events);
                self.update_button_state(Button::DpadRight, false, &mut events);
            }
        }
        
        // Process analog sticks with reduced sensitivity to avoid flooding events
        
        // Left stick: bytes 1 and 2
        let left_x = self.normalize_stick_value(data[1], 0.10); // Increased deadzone
        let left_y = -self.normalize_stick_value(data[2], 0.10); // Invert Y
        
        self.check_axis_changed(Axis::LeftStickX, left_x, &mut events);
        self.check_axis_changed(Axis::LeftStickY, left_y, &mut events);
        
        // Right stick: bytes 3 and 4
        let right_x = self.normalize_stick_value(data[3], 0.10);
        let right_y = -self.normalize_stick_value(data[4], 0.10); // Invert Y
        
        self.check_axis_changed(Axis::RightStickX, right_x, &mut events);
        self.check_axis_changed(Axis::RightStickY, right_y, &mut events);
        
        // Process triggers (bytes 8 and 9) with high threshold to reduce events
        let l2 = self.normalize_trigger_value(data[8]);
        let r2 = self.normalize_trigger_value(data[9]);
        
        self.check_axis_changed(Axis::L2, l2, &mut events);
        self.check_axis_changed(Axis::R2, r2, &mut events);
        
        // Process touchpad data
        self.process_ds4_touchpad(data, &mut events);
        
        events
    }

    fn update_button_state(&mut self, button: Button, pressed: bool, events: &mut Vec<ControllerEvent>) {
        let prev_state = self.button_states.get(&button).copied().unwrap_or(false);
        
        if pressed != prev_state {
            events.push(ControllerEvent::ButtonPress {
                button,
                pressed,
            });
            
            // Update internal state
            self.button_states.insert(button, pressed);
        }
    }
    
    // Modified version with default sensitivity
    fn check_axis_changed(&mut self, axis: Axis, value: f32, events: &mut Vec<ControllerEvent>) {
        // Get previous value
        let previous = self.axis_values.get(&axis).copied().unwrap_or(0.0);
        
        // Use different sensitivity based on axis type
        let min_change = match axis {
            Axis::L2 | Axis::R2 => 0.10, // Higher threshold for triggers
            Axis::TouchpadX | Axis::TouchpadY => 0.02, // Lower threshold for touchpad
            _ => 0.05,                   // Standard threshold for sticks
        };
        
        // Only emit events if change is significant (reduced event frequency)
        if (value - previous).abs() > min_change {
            events.push(ControllerEvent::AxisMove {
                axis,
                value,
            });
            
            // Update internal state
            self.axis_values.insert(axis, value);
        }
    }

    fn normalize_trigger_value(&self, value: u8) -> f32 {
        // Triggers go from 0 (released) to 255 (fully pressed)
        let normalized = value as f32 / 255.0;
        
        // Apply small deadzone to avoid noise at rest position
        if normalized < 0.05 {
            return 0.0;
        }
        
        // Round to reduce number of events (only report 10 distinct values)
        (normalized * 10.0).round() / 10.0
    }
    
    fn normalize_stick_value(&self, value: u8, deadzone: f32) -> f32 {
        // Center is at 128 for DS4 sticks
        let centered = (value as f32) - 128.0;
        let normalized = centered / 128.0;
        
        // Apply deadzone
        if normalized.abs() < deadzone {
            return 0.0;
        }
        
        // Rescale values outside deadzone to use full range (-1.0 to 1.0)
        let sign = if normalized < 0.0 { -1.0 } else { 1.0 };
        let rescaled = sign * ((normalized.abs() - deadzone) / (1.0 - deadzone));
        
        // Clamp to valid range
        rescaled.max(-1.0).min(1.0)
    }


    // Improved version of process_ds4_touchpad
    fn process_ds4_touchpad(&mut self, data: &[u8], events: &mut Vec<ControllerEvent>) {
        // First dump the full report for debugging
        self.dump_full_hid_report(data);
        
        // Check data length and controller compatibility
        if data.len() < 30 {
            return; // Not enough data for even basic touchpad detection
        }
        
        // Check if this is a DualShock 4 controller
        let product_lower = self.device_info.product.to_lowercase();
        let is_dualshock = product_lower.contains("dualshock") || 
                           product_lower.contains("wireless controller");
                          
        if !is_dualshock {
            return;
        }
        
        // Try to determine if this is USB or Bluetooth mode
        // In Bluetooth mode, the report ID is usually different and data is offset
        let is_bluetooth = data[0] == 0x11; // Common BT report ID
        
        // For DualShock 4, try multiple potential touchpad data locations
        // These vary by controller version and connection type
        let potential_offsets = if is_bluetooth {
            // Bluetooth mode offsets: try several options
            [37, 39, 41, 35]
        } else {
            // USB mode offsets: v1 and v2 have slightly different layouts
            [35, 33, 40, 42]
        };
        
        // Try each potential offset
        for &touch_byte_offset in &potential_offsets {
            if touch_byte_offset + 4 > data.len() {
                continue; // Skip if we'd go out of bounds
            }
            
            // Check if touch is active (usually bit 7 of the first byte)
            let is_active = (data[touch_byte_offset] & 0x80) != 0;
            
            // Also check if there's any non-zero data in the touchpad area
            let has_data = data[touch_byte_offset] != 0 || 
                          data[touch_byte_offset + 1] != 0 || 
                          data[touch_byte_offset + 2] != 0;
            
            if is_active || has_data {
                // We might have found valid touchpad data!
                // Try to extract coordinates
                
                // First attempt: standard layout (12-bit coordinates)
                let x_low = data[touch_byte_offset + 1] as i32;
                let x_high = (data[touch_byte_offset + 2] & 0x0F) as i32;
                let x = ((x_high << 8) | x_low).min(DS4_TOUCHPAD_X_MAX);
                
                let y_high = ((data[touch_byte_offset + 2] & 0xF0) >> 4) as i32;
                let y_low = data[touch_byte_offset + 3] as i32;
                let y = ((y_high << 8) | y_low).min(DS4_TOUCHPAD_Y_MAX);
                
                // Alternative layout: directly in 2 bytes each
                let alt_x = ((data[touch_byte_offset + 1] as i32) | 
                            ((data[touch_byte_offset + 2] as i32) << 8)).min(DS4_TOUCHPAD_X_MAX);
                let alt_y = ((data[touch_byte_offset + 3] as i32) | 
                            ((data[touch_byte_offset + 4] as i32) << 8)).min(DS4_TOUCHPAD_Y_MAX);
                
                // Choose the interpretation that seems most valid
                let final_x = if alt_x > 0 && alt_x < DS4_TOUCHPAD_X_MAX { alt_x } else { x };
                let final_y = if alt_y > 0 && alt_y < DS4_TOUCHPAD_Y_MAX { alt_y } else { y };
                
                // Only process if we have seemingly valid coordinates
                if final_x > 0 && final_y > 0 && 
                   final_x < DS4_TOUCHPAD_X_MAX && final_y < DS4_TOUCHPAD_Y_MAX {
                    
                    // Only send events if position has changed significantly
                    let x_diff = (final_x - self.touchpad_last_x).abs();
                    let y_diff = (final_y - self.touchpad_last_y).abs();
                    
                    if !self.touchpad_tracking || 
                       x_diff > TOUCHPAD_UPDATE_THRESHOLD || 
                       y_diff > TOUCHPAD_UPDATE_THRESHOLD {
                        
                        // We have valid touchpad data! Push events
                        events.push(ControllerEvent::TouchpadMove {
                            x: Some(final_x),
                            y: Some(final_y),
                        });
                        
                        // Map to axis events for MIDI mapping
                        let x_norm = (final_x as f32 / DS4_TOUCHPAD_X_MAX as f32) * 2.0 - 1.0;
                        let y_norm = -((final_y as f32 / DS4_TOUCHPAD_Y_MAX as f32) * 2.0 - 1.0); // Invert Y
                        
                        self.check_axis_changed(Axis::TouchpadX, x_norm, events);
                        self.check_axis_changed(Axis::TouchpadY, y_norm, events);
                        
                        // Update last position
                        self.touchpad_last_x = final_x;
                        self.touchpad_last_y = final_y;
                        self.touchpad_tracking = true;
                        
                        // Print debug info including where we found the data
                        if self.debug_mode {
                            print!("\x1B[23;0HTouchpad: FOUND at offset {}! X={} Y={} | Bytes: {:02X} {:02X} {:02X} {:02X}", 
                                 touch_byte_offset, final_x, final_y,
                                 data[touch_byte_offset], data[touch_byte_offset + 1], 
                                 data[touch_byte_offset + 2], data[touch_byte_offset + 3]);
                            io::stdout().flush().unwrap();
                        }
                        
                        // We've found and processed valid touchpad data, so we're done
                        return;
                    }
                }
            }
        }
        
        // If we reach here, we didn't find valid touchpad data
        // Release the virtual touch if it was active
        if self.touchpad_tracking {
            self.touchpad_tracking = false;
            self.check_axis_changed(Axis::TouchpadX, 0.0, events);
            self.check_axis_changed(Axis::TouchpadY, 0.0, events);
        }
        
        // Update debug display
        if self.debug_mode {
            // Show some of the raw HID report that might contain touchpad data
            let start_offset = 33;
            let end_offset = std::cmp::min(data.len(), start_offset + 12);
            
            print!("\x1B[23;0HTouchpad: Not detected - Raw data: ");
            for i in start_offset..end_offset {
                print!("{:02X} ", data[i]);
            }
            print!("                  ");
            io::stdout().flush().unwrap();
        }
    }

    fn dump_full_hid_report(&self, data: &[u8]) {
        if !self.debug_mode {
            return;
        }
        
        // Clear a section of the screen for the full report dump
        for i in 25..35 {
            print!("\x1B[{};0H                                                                                ", i);
        }
        
        print!("\x1B[25;0HFULL HID REPORT DUMP:");
        
        // Dump data in rows of 16 bytes
        for i in 0..(data.len() + 15) / 16 {
            print!("\x1B[{};0H{:02X}:", i + 26, i * 16);
            
            for j in 0..16 {
                let idx = i * 16 + j;
                if idx < data.len() {
                    print!(" {:02X}", data[idx]);
                } else {
                    print!("   ");
                }
            }
            
            print!(" | ");
            
            for j in 0..16 {
                let idx = i * 16 + j;
                if idx < data.len() {
                    let c = data[idx];
                    if c >= 32 && c <= 126 {
                        print!("{}", c as char);
                    } else {
                        print!(".");
                    }
                }
            }
        }
        
        // Print a summary of potentially relevant touchpad bytes
        print!("\x1B[35;0HPotential touchpad data locations:");
        
        // Try different potential offsets where touchpad data might be
        let offsets = [33, 35, 40, 42, 44];
        
        for (i, &offset) in offsets.iter().enumerate() {
            if offset + 4 <= data.len() {
                print!("\x1B[{};0HOffset {}: {:02X} {:02X} {:02X} {:02X}", 
                      36 + i, offset, data[offset], 
                      data.get(offset + 1).copied().unwrap_or(0),
                      data.get(offset + 2).copied().unwrap_or(0), 
                      data.get(offset + 3).copied().unwrap_or(0));
            }
        }
        
        io::stdout().flush().unwrap();
    }
}





