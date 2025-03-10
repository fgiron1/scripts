use hidapi::{HidApi, HidDevice};
use std::error::Error;
use std::collections::HashMap;
use std::time::Duration;
use std::thread;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use crate::controller::{Controller, types::{ControllerEvent, Button, Axis, DeviceInfo}};

// Constants for PS4 controller
const SONY_VID: u16 = 0x054C;
const DS4_V1_PID: u16 = 0x05C4;
const DS4_V2_PID: u16 = 0x09CC;

// Static to track if display has been initialized
static DISPLAY_INITIALIZED: AtomicBool = AtomicBool::new(false);

pub struct HidController {
    device: HidDevice,
    device_info: DeviceInfo,
    button_states: HashMap<Button, bool>,
    axis_values: HashMap<Axis, f32>,
    last_report: Vec<u8>,
    debug_mode: bool,
}

impl HidController {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        // Initialize HID API
        let api = HidApi::new()?;
        
        // Try to find a PlayStation 4 controller
        let (device, device_info) = Self::find_ps4_controller(&api)?;
        
        // Disable mapper's display by setting an environment variable
        std::env::set_var("PS4_DISABLE_MAPPER_DISPLAY", "1");
        
        println!("Found controller: {} (VID: 0x{:04X}, PID: 0x{:04X})", 
            device_info.product, device_info.vid, device_info.pid);
        
        // Create with default states
        Ok(Self {
            device,
            device_info,
            button_states: HashMap::new(),
            axis_values: HashMap::new(),
            last_report: vec![0; 64],  // Default buffer size
            debug_mode: true,          // Enable debug mode by default
        })
    }
    
    fn find_ps4_controller(api: &HidApi) -> Result<(HidDevice, DeviceInfo), Box<dyn Error>> {
        println!("Searching for controllers...");
        
        // First try looking for PS4 controllers by vendor/product ID
        for device_info in api.device_list() {
            if device_info.vendor_id() == SONY_VID && 
               (device_info.product_id() == DS4_V1_PID || 
                device_info.product_id() == DS4_V2_PID) {
                
                // For DS4, we want the interface that provides full controller data
                let is_input_interface = 
                    (device_info.usage_page() == 0x01 && device_info.usage() == 0x05) ||
                    (device_info.interface_number() == 0);
                
                if is_input_interface {
                    // Try to open the device
                    if let Ok(device) = api.open_path(device_info.path()) {
                        // Get string info
                        let manufacturer = match device.get_manufacturer_string() {
                            Ok(Some(s)) => s,
                            _ => "Sony".to_string(),
                        };
                            
                        let product = match device.get_product_string() {
                            Ok(Some(s)) => s,
                            _ => "DualShock 4".to_string(),
                        };
                        
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
        
        // If exact PS4 controller not found, try looking for any controller-like device
        for device_info in api.device_list() {
            // Get product name from the device info
            let product_name = match device_info.product_string() {
                Some(name) => name,
                None => continue,
            };
            
            if product_name.contains("Controller") || 
               product_name.contains("Gamepad") || 
               product_name.contains("DualShock") {
                
                if let Ok(device) = api.open_path(device_info.path()) {
                    // Get string info
                    let manufacturer = match device.get_manufacturer_string() {
                        Ok(Some(s)) => s,
                        _ => "Unknown".to_string(),
                    };
                    
                    // Create device info
                    let dev_info = DeviceInfo {
                        vid: device_info.vendor_id(),
                        pid: device_info.product_id(),
                        manufacturer,
                        product: product_name.to_string(),
                    };
                    
                    return Ok((device, dev_info));
                }
            }
        }
        
        Err("No compatible controller found via HID. Make sure your controller is connected and powered on.".into())
    }
    
    // Debug information display - first time setup
    fn setup_debug_display(&self) {
        // Clear screen
        print!("\x1B[2J\x1B[H");
        
        // Print static headers
        println!("DualShock Controller Debug - {}", self.device_info.product);
        println!("========================================");
        println!("Press buttons on your controller to see input events");
        println!("Press Ctrl+C to exit\n");
        
        // Static labels for input values
        println!("Left Stick X:  ");
        println!("Left Stick Y:  ");
        println!("Right Stick X: ");
        println!("Right Stick Y: ");
        println!("Left Trigger:  ");
        println!("Right Trigger: ");
        println!("D-Pad:         ");
        println!("");
        println!("Face Buttons:  ");
        println!("Shoulder:      ");
        println!("Stick Press:   ");
        println!("Special:       ");
        println!("");
        println!("Raw Button Bits:");
        println!("Raw HID Report:");
        
        io::stdout().flush().unwrap();
    }
    
    // Just update the data fields without changing structure
    fn update_debug_data(&self, data: &[u8]) {
        if !self.debug_mode || data.len() < 10 {
            return;
        }
        
        // Calculate all values
        let left_x_raw = data[1];
        let left_y_raw = data[2];
        let right_x_raw = data[3];
        let right_y_raw = data[4];
        let l2_raw = data[8];
        let r2_raw = data[9];
        
        let left_x_norm = self.normalize_axis_value(left_x_raw);
        let left_y_norm = -self.normalize_axis_value(left_y_raw);
        let right_x_norm = self.normalize_axis_value(right_x_raw);
        let right_y_norm = -self.normalize_axis_value(right_y_raw);
        let l2_norm = l2_raw as f32 / 255.0;
        let r2_norm = r2_raw as f32 / 255.0;
        
        // Extract D-pad value
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
        
        // Position cursor and update each value - use fixed-width fields
        
        // Analog inputs
        print!("\x1B[5;15H{:3} ({:+.2})                ", left_x_raw, left_x_norm);
        print!("\x1B[6;15H{:3} ({:+.2})                ", left_y_raw, left_y_norm);
        print!("\x1B[7;15H{:3} ({:+.2})                ", right_x_raw, right_x_norm);
        print!("\x1B[8;15H{:3} ({:+.2})                ", right_y_raw, right_y_norm);
        print!("\x1B[9;15H{:3} ({:.2})                 ", l2_raw, l2_norm);
        print!("\x1B[10;15H{:3} ({:.2})                ", r2_raw, r2_norm);
        print!("\x1B[11;15H{:<18}          ", dpad_dir);
        
        // Button states - use consistent width fields
        print!("\x1B[13;15H□:{:<5} ×:{:<5} ○:{:<5} △:{:<5}     ", 
            if square   { "ON" } else { "off" },
            if cross    { "ON" } else { "off" },
            if circle   { "ON" } else { "off" },
            if triangle { "ON" } else { "off" });
            
        print!("\x1B[14;15HL1:{:<5} R1:{:<5} L2:{:<5} R2:{:<5}  ", 
            if l1      { "ON" } else { "off" },
            if r1      { "ON" } else { "off" },
            if l2_btn  { "ON" } else { "off" },
            if r2_btn  { "ON" } else { "off" });
            
        print!("\x1B[15;15HL3:{:<5} R3:{:<5}                    ", 
            if l3 { "ON" } else { "off" },
            if r3 { "ON" } else { "off" });
            
        print!("\x1B[16;15HSHARE:{:<5} OPTIONS:{:<5} PS:{:<5} TOUCHPAD:{:<5}", 
            if share    { "ON" } else { "off" },
            if options  { "ON" } else { "off" },
            if ps_btn   { "ON" } else { "off" },
            if touchpad { "ON" } else { "off" });
            
        // Raw data - use fixed fields
        print!("\x1B[18;15H{:08b} {:08b} {:08b}            ", data[5], data[6], data[7]);
        
        // Raw HID report
        print!("\x1B[19;15H");
        for i in 0..std::cmp::min(data.len(), 10) {
            print!("{:02X} ", data[i]);
        }
        print!("                          ");
        
        io::stdout().flush().unwrap();
    }
    
    fn parse_ds4_report(&mut self, data: &[u8]) -> Vec<ControllerEvent> {
        let mut events = Vec::new();
        
        // Setup display if needed
        if !DISPLAY_INITIALIZED.load(Ordering::Relaxed) {
            self.setup_debug_display();
            DISPLAY_INITIALIZED.store(true, Ordering::Relaxed);
        }
        
        // Update live data
        self.update_debug_data(data);
        
        // Only proceed if we have enough data
        if data.len() < 10 {
            return events;
        }
        
        // Process D-pad which is in the lower 4 bits of byte 5
        let dpad = data[5] & 0x0F;
        
        self.check_dpad_button(Button::DpadUp, dpad, &[0, 1, 7], &mut events);
        self.check_dpad_button(Button::DpadRight, dpad, &[1, 2, 3], &mut events);
        self.check_dpad_button(Button::DpadDown, dpad, &[3, 4, 5], &mut events);
        self.check_dpad_button(Button::DpadLeft, dpad, &[5, 6, 7], &mut events);
        
        // Extract button data from bytes 5, 6, and 7
        // Standard DualShock 4 v1 button mapping documentation
        
        // Byte 5 (upper 4 bits)
        self.update_button_state(Button::Square, (data[5] & 0x10) != 0, &mut events);
        self.update_button_state(Button::Cross, (data[5] & 0x20) != 0, &mut events);
        self.update_button_state(Button::Circle, (data[5] & 0x40) != 0, &mut events);
        self.update_button_state(Button::Triangle, (data[5] & 0x80) != 0, &mut events);
        
        // Byte 6
        self.update_button_state(Button::L1, (data[6] & 0x01) != 0, &mut events);
        self.update_button_state(Button::R1, (data[6] & 0x02) != 0, &mut events);
        self.update_button_state(Button::L2, (data[6] & 0x04) != 0, &mut events);
        self.update_button_state(Button::R2, (data[6] & 0x08) != 0, &mut events);
        self.update_button_state(Button::Share, (data[6] & 0x10) != 0, &mut events);
        self.update_button_state(Button::Options, (data[6] & 0x20) != 0, &mut events);
        self.update_button_state(Button::L3, (data[6] & 0x40) != 0, &mut events);
        self.update_button_state(Button::R3, (data[6] & 0x80) != 0, &mut events);
        
        // Byte 7
        self.update_button_state(Button::PS, (data[7] & 0x01) != 0, &mut events);
        self.update_button_state(Button::Touchpad, (data[7] & 0x02) != 0, &mut events);
        
        // Process analog sticks
        // Left stick: bytes 1 and 2
        let left_x = self.normalize_axis_value(data[1]);
        let left_y = -self.normalize_axis_value(data[2]);  // Invert Y for proper up/down
        
        self.check_axis_changed(Axis::LeftStickX, left_x, &mut events);
        self.check_axis_changed(Axis::LeftStickY, left_y, &mut events);
        
        // Right stick: bytes 3 and 4
        let right_x = self.normalize_axis_value(data[3]);
        let right_y = -self.normalize_axis_value(data[4]);  // Invert Y
        
        self.check_axis_changed(Axis::RightStickX, right_x, &mut events);
        self.check_axis_changed(Axis::RightStickY, right_y, &mut events);
        
        // Triggers: bytes 8 (L2) and 9 (R2)
        let l2 = data[8] as f32 / 255.0;
        let r2 = data[9] as f32 / 255.0;
        
        self.check_axis_changed(Axis::L2, l2, &mut events);
        self.check_axis_changed(Axis::R2, r2, &mut events);
        
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
    
    fn check_dpad_button(&mut self, button: Button, dpad_value: u8, active_values: &[u8], events: &mut Vec<ControllerEvent>) {
        // Determine if the button should be pressed based on the D-pad value
        let pressed = active_values.contains(&dpad_value);
        
        // Check if state changed
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
    
    fn check_axis_changed(&mut self, axis: Axis, value: f32, events: &mut Vec<ControllerEvent>) {
        // Check if value has changed enough to generate an event
        let prev_value = self.axis_values.get(&axis).copied().unwrap_or(0.0);
        
        // Use a small threshold to avoid sending events for tiny changes
        if (value - prev_value).abs() > 0.01 {
            events.push(ControllerEvent::AxisMove {
                axis,
                value,
            });
            
            // Update internal state
            self.axis_values.insert(axis, value);
        }
    }
    
    fn normalize_axis_value(&self, value: u8) -> f32 {
        // Convert from 0-255 to -1.0 to 1.0
        // First scale to -128 to 127, then divide by 128 to get -1.0 to 0.992...
        ((value as i16) - 128) as f32 / 128.0
    }
}

impl Controller for HidController {
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn Error>> {
        // Buffer to read HID report
        let mut buf = [0u8; 64];
        
        // Read with timeout (1ms for low latency)
        match self.device.read_timeout(&mut buf, 1) {
            Ok(size) if size > 0 => {
                // Only process if data has changed
                if buf[..size] != self.last_report[..size.min(self.last_report.len())] {
                    // Save report for comparison next time
                    self.last_report = buf[..size].to_vec();
                    
                    // Parse and return events
                    return Ok(self.parse_ds4_report(&buf[..size]));
                }
            },
            Ok(_) => {
                // No data or no change, just return empty vector
            },
            Err(e) => {
                // Handle "Resource temporarily unavailable" error by waiting
                // This can happen on some systems when the device is busy
                if e.to_string().contains("temporarily unavailable") {
                    thread::sleep(Duration::from_millis(1)); // Minimal sleep for latency
                    return Ok(Vec::new());
                }
                
                return Err(format!("Error reading controller: {}", e).into());
            }
        }
        
        // Return empty list if no data or no changes
        Ok(Vec::new())
    }
    
    fn get_device_info(&self) -> DeviceInfo {
        self.device_info.clone()
    }
}