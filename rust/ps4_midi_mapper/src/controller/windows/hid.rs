use hidapi::{HidApi, HidDevice};
use std::error::Error;
use std::collections::HashMap;
use std::time::Duration;
use std::thread;
use crate::controller::{Controller, types::{ControllerEvent, Button, Axis, DeviceInfo}};

// Constants for PS4 controller
const SONY_VID: u16 = 0x054C;
const DS4_V1_PID: u16 = 0x05C4;
const DS4_V2_PID: u16 = 0x09CC;

pub struct HidController {
    device: HidDevice,
    device_info: DeviceInfo,
    button_states: HashMap<Button, bool>,
    axis_values: HashMap<Axis, f32>,
    last_report: Vec<u8>,
}

impl HidController {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        // Initialize HID API
        let api = HidApi::new()?;
        
        // Try to find a PlayStation 4 controller
        let (device, device_info) = Self::find_ps4_controller(&api)?;
        
        // Create with default states
        Ok(Self {
            device,
            device_info,
            button_states: HashMap::new(),
            axis_values: HashMap::new(),
            last_report: vec![0; 64],  // Default buffer size
        })
    }
    
    fn find_ps4_controller(api: &HidApi) -> Result<(HidDevice, DeviceInfo), Box<dyn Error>> {
        // First try looking for PS4 controllers by vendor/product ID
        for device_info in api.device_list() {
            if device_info.vendor_id() == SONY_VID && 
               (device_info.product_id() == DS4_V1_PID || 
                device_info.product_id() == DS4_V2_PID) {
                
                // Special case for DS4: look for usage page 0x01, usage 0x05
                // That's the one that gives us the full state including buttons and axes
                if (device_info.usage_page() == 0x01 && device_info.usage() == 0x05) ||
                   // Fallback if usage page/usage isn't available
                   (device_info.interface_number() == 0) {
                    
                    // Try to open the device
                    if let Ok(device) = api.open_path(device_info.path()) {
                        // Get string info - properly unwrap the Result<Option<String>, Error>
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
                        
                        return Ok((device, dev_info));
                    }
                }
            }
        }
        
        // If exact PS4 controller not found, try looking for any controller-like device
        for device_info in api.device_list() {
            // Get product name from the device info - hidapi gives us Option<&str>
            let product_name = match device_info.product_string() {
                Some(name) => name,
                None => continue,
            };
            
            if product_name.contains("Controller") || 
               product_name.contains("Gamepad") || 
               product_name.contains("DualShock") {
                
                if let Ok(device) = api.open_path(device_info.path()) {
                    // Get string info - properly unwrap the Result<Option<String>, Error>
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
    
    
    fn parse_ds4_report(&mut self, data: &[u8]) -> Vec<ControllerEvent> {
        let mut events = Vec::new();
        
        // Only proceed if we have enough data (DS4 reports are typically 64 bytes)
        if data.len() < 10 {
            return events;
        }
        
        // Extract button data (byte 5 and 6 in the DS4 HID report)
        let buttons = ((data[5] as u16) << 8) | (data[6] as u16);
        
        // Check for button presses
        // These button mappings are specific to the DS4 HID report format
        let button_mappings = [
            (0x0001, Button::Square),    // Square
            (0x0002, Button::Cross),     // Cross
            (0x0004, Button::Circle),    // Circle
            (0x0008, Button::Triangle),  // Triangle
            (0x0010, Button::L1),        // L1
            (0x0020, Button::R1),        // R1
            (0x0040, Button::L2),        // L2 (button mode)
            (0x0080, Button::R2),        // R2 (button mode)
            (0x0100, Button::Share),     // Share
            (0x0200, Button::Options),   // Options
            (0x0400, Button::L3),        // L3
            (0x0800, Button::R3),        // R3
            (0x1000, Button::PS),        // PS button
            (0x2000, Button::Touchpad),  // Touchpad click
        ];
        
        for (mask, button) in &button_mappings {
            let pressed = (buttons & mask) != 0;
            let prev_state = self.button_states.get(button).copied().unwrap_or(false);
            
            if pressed != prev_state {
                events.push(ControllerEvent::ButtonPress {
                    button: *button,
                    pressed,
                });
                
                // Update internal state
                self.button_states.insert(*button, pressed);
            }
        }
        
        // D-pad is encoded in the first 4 bits of byte 5
        let dpad = data[5] & 0x0F;
        
        // Check D-pad states
        self.check_dpad_button(Button::DpadUp, dpad, &[0, 1, 7], &mut events);
        self.check_dpad_button(Button::DpadRight, dpad, &[1, 2, 3], &mut events);
        self.check_dpad_button(Button::DpadDown, dpad, &[3, 4, 5], &mut events);
        self.check_dpad_button(Button::DpadLeft, dpad, &[5, 6, 7], &mut events);
        
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
        
        // Read with timeout (10ms)
        match self.device.read_timeout(&mut buf, 10) {
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
                    thread::sleep(Duration::from_millis(5));
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