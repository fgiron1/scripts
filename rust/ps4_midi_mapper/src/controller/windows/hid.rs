use hidapi::{HidApi, HidDevice};
use std::error::Error;
use std::collections::HashMap;
use std::time::Duration;
use std::thread;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use crate::config::{AXIS_MAPPINGS, BUTTON_MAPPINGS};
use crate::controller::{Controller, types::{ControllerEvent, Button, Axis, DeviceInfo}};
use crate::controller::profiles::{self, ControllerProfile};

// Static to track if display has been initialized
static DISPLAY_INITIALIZED: AtomicBool = AtomicBool::new(false);

pub struct HidController {
    device: HidDevice,
    device_info: DeviceInfo,
    button_states: HashMap<Button, bool>,
    axis_values: HashMap<Axis, f32>,
    last_report: Vec<u8>,
    debug_mode: bool,
    profile: ControllerProfile,
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
        
        // Create with default states
        Ok(Self {
            device,
            device_info,
            button_states: HashMap::new(),
            axis_values: HashMap::new(),
            last_report: vec![0; 64],  // Default buffer size
            debug_mode: true,          // Enable debug mode by default
            profile,
        })
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
    fn setup_debug_display(&self) {
        // Clear screen
        print!("\x1B[2J\x1B[H");
        
        // Print static headers
        println!("DualShock Controller Debug - Wireless Controller");
        println!("Press buttons on your controller to see input events");
        println!("Press Ctrl+C to exit\n");
        
        // Static labels for input values - exactly matching screenshot
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
        println!("------------------------------|----------------------------");
        println!("                 □:off  ×:off  ○:off  △:off  | □:0  ×:0  ○:0  △:0");
        println!("                 L1:off  R1:off  L2:off  R2:off  | L1:0  R1:0  L2:0  R2:0");
        println!("                 L3:off  R3:off                | L3:0  R3:0");
        println!("                 SHARE:off  OPTIONS:off  PS:off  | SH:0  OPT:0  PS:0  TP:0");
        println!("Face Buttons:");
        println!("Shoulder:");
        println!("Stick Press:");
        println!("Special:");
        println!("");
        println!("Raw Button Bits:");
        println!("Raw HID Report:");
        
        io::stdout().flush().unwrap();
    }
    
    fn update_debug_data(&self, data: &[u8]) {
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
        print!("\x1B[12;28HReleased    | No notes ON");
        
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
        print!("\x1B[21;0HRaw Button Bits: {:08b} {:08b} {:08b}", data[5], data[6], data[7]);
        
        print!("\x1B[22;0HRaw HID Report:  ");
        for i in 0..std::cmp::min(data.len(), 10) {
            print!("{:02X} ", data[i]);
        }
        print!("                         ");
        
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
    
    // Normalize stick values with deadzone
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
    
    // Normalize trigger values (0-255 to 0-1 range)
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
                    return Ok(self.parse_hid_report(&buf[..size]));
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