use std::any::Any;
use std::collections::HashMap;
use std::error::Error;
use windows::Win32::Foundation::{
    HANDLE, INVALID_HANDLE_VALUE,
};
use crate::controller::{Controller, types::{ControllerEvent, Button, Axis, DeviceInfo}};
use crate::controller::profiles::{ControllerProfile, get_profile_for_device, create_profiles};

const DS4_TOUCHPAD_X_MAX: i32 = 1920;
const DS4_TOUCHPAD_Y_MAX: i32 = 942;
const TOUCHPAD_MIN_CHANGE: i32 = 5;

pub struct WindowsRawIOController {
    device_info: DeviceInfo,
    device_handle: HANDLE,
    button_states: HashMap<Button, bool>,
    axis_values: HashMap<Axis, f32>,
    
    // Touchpad specific state
    touchpad_active: bool,
    touchpad_last_x: i32,
    touchpad_last_y: i32,
    
    // Debug mode
    debug_mode: bool,
    
    // Last read report
    last_report: Vec<u8>,
    
    // HID device for actual implementation
    hid_device: hidapi::HidDevice,
    
    // Is this a DualShock controller?
    is_dualshock: bool,
    
    // Is this connected via Bluetooth?
    is_bluetooth: bool,
    
    // Profile
    profile: Option<&'static ControllerProfile>,
}

impl WindowsRawIOController {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        println!("Creating Windows Raw IO Controller...");
        
        // Use HID to find the controller
        let api = hidapi::HidApi::new()?;
        
        // Try to find a compatible controller
        let (device, device_info) = Self::find_controller(&api)?;
        
        println!("Found controller: {} (VID: 0x{:04X}, PID: 0x{:04X})", 
                 device_info.product, device_info.vid, device_info.pid);
        
        // Check if this is a DualShock controller
        let is_dualshock = device_info.product.to_lowercase().contains("dualshock") ||
                          device_info.product.to_lowercase().contains("wireless controller");
        
        // Create controller instance
        let mut controller = WindowsRawIOController {
            device_info,
            device_handle: INVALID_HANDLE_VALUE, // Not used with HidApi
            button_states: HashMap::new(),
            axis_values: HashMap::new(),
            touchpad_active: false,
            touchpad_last_x: 0,
            touchpad_last_y: 0,
            debug_mode: false,
            last_report: Vec::with_capacity(64),
            hid_device: device,
            is_dualshock,
            is_bluetooth: false, // Will be detected on first report
            profile: None,
        };
        
        // Cache the profile
        controller.profile = Some(controller.get_controller_profile());
        
        Ok(controller)
    }
    
    // Find the best controller using HidApi
    fn find_controller(api: &hidapi::HidApi) -> Result<(hidapi::HidDevice, DeviceInfo), Box<dyn Error>> {
        // First, prefer DualShock 4 controllers
        for device_info in api.device_list() {
            if device_info.vendor_id() == 0x054C && // Sony
               (device_info.product_id() == 0x05C4 || device_info.product_id() == 0x09CC) { // DS4 v1/v2
                if let Some(product) = device_info.product_string() {
                    if let Ok(device) = api.open_path(device_info.path()) {
                        // Set non-blocking mode
                        let _ = device.set_blocking_mode(false);
                        
                        // Create device info
                        let dev_info = DeviceInfo {
                            vid: device_info.vendor_id(),
                            pid: device_info.product_id(),
                            manufacturer: device_info.manufacturer_string().unwrap_or("Sony").to_string(),
                            product: product.to_string(),
                        };
                        
                        return Ok((device, dev_info));
                    }
                }
            }
        }
        
        // Then try any game controller
        for device_info in api.device_list() {
            if let Some(product) = device_info.product_string() {
                // Look for controllers
                if (product.contains("Controller") || 
                    product.contains("Gamepad") || 
                    product.contains("DualShock") ||
                    product.contains("Xbox")) && 
                    !product.contains("Touchpad") {
                    
                    if let Ok(device) = api.open_path(device_info.path()) {
                        // Set non-blocking mode
                        let _ = device.set_blocking_mode(false);
                        
                        // Create device info
                        let dev_info = DeviceInfo {
                            vid: device_info.vendor_id(),
                            pid: device_info.product_id(),
                            manufacturer: device_info.manufacturer_string()
                                .unwrap_or("Unknown").to_string(),
                            product: product.to_string(),
                        };
                        
                        return Ok((device, dev_info));
                    }
                }
            }
        }
        
        Err("No compatible controller found".into())
    }
    
    // Get the controller profile
    fn get_controller_profile(&self) -> &'static ControllerProfile {
        // If we already have a cached profile, return it
        if let Some(profile) = self.profile {
            return profile;
        }
        
        // Get all available profiles
        let profiles = create_profiles();
        
        // Try to find a matching profile
        if let Some(profile) = get_profile_for_device(&self.device_info, profiles) {
            return profile;
        }
        
        // Fall back to generic profile
        profiles.last().expect("At least one profile should exist")
    }
    
    // Parse controller report
    fn parse_report(&mut self, data: &[u8], events: &mut Vec<ControllerEvent>) {
        // Check if this is the first report for a DualShock 4
        if self.is_dualshock && !self.is_bluetooth && !self.last_report.is_empty() {
            self.is_bluetooth = data[0] == 0x11; // Common BT report ID
        }
        
        // Get the current profile
        let profile = self.get_controller_profile();
        
        // Use profile-based mapping for buttons and axes
        self.parse_with_profile(data, profile, events);
        
        // For DualShock controllers, try to extract touchpad data
        if self.is_dualshock {
            self.extract_touchpad_data(data, events);
        }
    }
    
    // Parse using profile-based approach
    fn parse_with_profile(&mut self, data: &[u8], profile: &ControllerProfile, events: &mut Vec<ControllerEvent>) {
        // Process buttons based on the profile's button map
        for (code, button) in &profile.button_map {
            let byte_index = (code >> 8) as usize;
            let bit_mask = (*code & 0xFF) as u8;
            
            if byte_index < data.len() {
                let pressed = (data[byte_index] & bit_mask) != 0;
                self.update_button(*button, pressed, events);
            }
        }
        
        // Process axes based on the profile's axis config
        for (axis, config) in &profile.axis_config {
            if config.byte_index < data.len() {
                let raw_value = data[config.byte_index];
                let normalized = config.normalize(raw_value);
                self.update_axis(*axis, normalized, events);
            }
        }
        
        // Process D-pad based on the profile's D-pad type
        match &profile.dpad_type {
            crate::controller::profiles::DpadType::Hat { byte_index, mask_values } => {
                if *byte_index < data.len() {
                    let hat_value = data[*byte_index];
                    if let Some(buttons) = mask_values.get(&hat_value) {
                        // For each button in the current mask, set it to pressed
                        for button in buttons {
                            self.update_button(*button, true, events);
                        }
                        
                        // For each dpad button not in the current mask, set it to released
                        let dpad_buttons = [Button::DpadUp, Button::DpadRight, Button::DpadDown, Button::DpadLeft];
                        for button in &dpad_buttons {
                            if !buttons.contains(button) {
                                self.update_button(*button, false, events);
                            }
                        }
                    }
                }
            },
            crate::controller::profiles::DpadType::Buttons => {
                // Buttons are handled by the button map above
            },
            crate::controller::profiles::DpadType::Axes { x_axis, y_axis } => {
                // Get axis values and convert to d-pad button presses
                let x_value = self.axis_values.get(x_axis).copied().unwrap_or(0.0);
                let y_value = self.axis_values.get(y_axis).copied().unwrap_or(0.0);
                
                self.update_button(Button::DpadLeft, x_value < -0.5, events);
                self.update_button(Button::DpadRight, x_value > 0.5, events);
                self.update_button(Button::DpadUp, y_value < -0.5, events);
                self.update_button(Button::DpadDown, y_value > 0.5, events);
            }
        }
    }
    
    // Extract touchpad data (specifically for DualShock 4)
    fn extract_touchpad_data(&mut self, data: &[u8], events: &mut Vec<ControllerEvent>) {
        // Different data offsets for USB vs Bluetooth mode
        let touchpad_offset = if self.is_bluetooth { 35 } else { 33 };
        
        // Check if we have enough data
        if data.len() <= touchpad_offset + 4 {
            return;
        }
        
        // DS4 touchpad data format:
        // Byte 0: Touch state (bit 7: active when 0, bits 0-6: touch ID)
        // Bytes 1-2: X position (12 bits)
        // Bytes 2-3: Y position (12 bits)
        
        let touch_state = data[touchpad_offset];
        let is_touching = (touch_state & 0x80) == 0; // Active when bit 7 is 0
        
        if is_touching {
            // Extract 12-bit X and Y coordinates
            let x = ((data[touchpad_offset + 1] & 0x0F) as i32) << 8 | (data[touchpad_offset + 2] as i32);
            let y = ((data[touchpad_offset + 2] & 0xF0) as i32) << 4 | (data[touchpad_offset + 3] as i32);
            
            // Validate coordinates
            if x > 0 && x < DS4_TOUCHPAD_X_MAX && y > 0 && y < DS4_TOUCHPAD_Y_MAX {
                self.update_touchpad_position(x, y, events);
            }
        } else if self.touchpad_active {
            // Touch ended
            self.end_touch(events);
        }
    }
    
    // Update button state
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
    
    // Update axis value
    fn update_axis(&mut self, axis: Axis, value: f32, events: &mut Vec<ControllerEvent>) {
        let previous = self.axis_values.get(&axis).copied().unwrap_or(0.0);
        
        // Use appropriate sensitivity threshold
        let min_change = match axis {
            Axis::L2 | Axis::R2 => 0.05,               // Triggers
            Axis::TouchpadX | Axis::TouchpadY => 0.01, // Touchpad
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
    
    // Update touchpad position
    fn update_touchpad_position(&mut self, x: i32, y: i32, events: &mut Vec<ControllerEvent>) {
        // Check if position has changed significantly
        let x_diff = (x - self.touchpad_last_x).abs();
        let y_diff = (y - self.touchpad_last_y).abs();
        
        if !self.touchpad_active || x_diff > TOUCHPAD_MIN_CHANGE || y_diff > TOUCHPAD_MIN_CHANGE {
            // Send touchpad event
            events.push(ControllerEvent::TouchpadMove {
                x: Some(x),
                y: Some(y),
            });
            
            // Normalize coordinates for MIDI mapping
            let x_norm = (x as f32 / DS4_TOUCHPAD_X_MAX as f32) * 2.0 - 1.0;
            let y_norm = -((y as f32 / DS4_TOUCHPAD_Y_MAX as f32) * 2.0 - 1.0); // Invert Y
            
            // Also send as axis events
            events.push(ControllerEvent::AxisMove {
                axis: Axis::TouchpadX,
                value: x_norm,
            });
            
            events.push(ControllerEvent::AxisMove {
                axis: Axis::TouchpadY,
                value: y_norm,
            });
            
            // Update state
            self.touchpad_last_x = x;
            self.touchpad_last_y = y;
            self.touchpad_active = true;
            
            if self.debug_mode {
                println!("Touchpad: X={}, Y={} (normalized: {:.2}, {:.2})", 
                         x, y, x_norm, y_norm);
            }
        }
    }
    
    // Handle touch release
    fn end_touch(&mut self, events: &mut Vec<ControllerEvent>) {
        if self.touchpad_active {
            self.touchpad_active = false;
            
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
                println!("Touchpad: Touch released");
            }
        }
    }
    
    // Enable debug mode
    pub fn enable_debug(&mut self) {
        self.debug_mode = true;
        println!("Debug mode enabled for Raw IO controller");
    }
}

impl Controller for WindowsRawIOController {
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn Error>> {
        let mut events = Vec::new();
        
        // Read from the HID device
        let mut buf = [0u8; 64];
        match self.hid_device.read_timeout(&mut buf, 0) {
            Ok(size) if size > 0 => {
                // Save the report
                self.last_report = buf[..size].to_vec();
                
                // Parse the report
                self.parse_report(&buf[..size], &mut events);
            },
            Err(e) => {
                if !e.to_string().contains("timed out") && 
                   !e.to_string().contains("temporarily unavailable") {
                    return Err(format!("Error reading controller: {}", e).into());
                }
            },
            _ => { /* No data available */ }
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