use std::collections::HashMap;
use crate::controller::types::{Button, Axis, DeviceInfo, AxisConfig};
use std::sync::OnceLock;

// Profiles cache - initialize profiles only once
static PROFILES_CACHE: OnceLock<Vec<ControllerProfile>> = OnceLock::new();

/// Represents a controller's connection method
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionType {
    USB,
    Bluetooth,
    Unknown
}

/// A controller profile contains mapping information for a specific controller model
#[derive(Clone)]
pub struct ControllerProfile {
    pub name: String,
    pub description: String,
    pub vid_pid_pairs: Vec<(u16, u16)>,
    pub button_map: HashMap<u32, Button>,
    pub axis_config: HashMap<Axis, AxisConfig>,
    pub dpad_type: DpadType,
    pub connection_type: ConnectionType,
}

/// Different controllers handle D-pads differently
#[derive(Clone)]
pub enum DpadType {
    /// D-pad represented as a hat/POV (common in DirectInput/HID)
    Hat { byte_index: usize, mask_values: HashMap<u8, Vec<Button>> },
    
    /// D-pad represented as individual buttons (common in XInput)
    Buttons,
    
    /// D-pad represented as two axes (rare)
    Axes { x_axis: Axis, y_axis: Axis },
}

impl ControllerProfile {
    /// Check if this profile matches the given device info and connection type
    pub fn matches(&self, device_info: &DeviceInfo, conn_type: ConnectionType) -> bool {
        // Only match if connection type matches or is unknown
        if self.connection_type != ConnectionType::Unknown && 
           conn_type != ConnectionType::Unknown && 
           self.connection_type != conn_type {
            return false;
        }
        
        // Match by VID/PID pair (exact match)
        if self.vid_pid_pairs.contains(&(device_info.vid, device_info.pid)) {
            return true;
        }
        
        // Match by name (case-insensitive substring match)
        let product_lower = device_info.product.to_lowercase();
        if product_lower.contains(&self.name.to_lowercase()) {
            return true;
        }
        
        false
    }
    
    /// Create a new profile for Bluetooth based on a USB profile
    pub fn create_bluetooth_variant(&self, bt_offset: usize) -> Self {
        let mut profile = self.clone();
        profile.connection_type = ConnectionType::Bluetooth;
        profile.name = format!("{} (Bluetooth)", profile.name);
        profile.description = format!("{} via Bluetooth", profile.description);
        
        // Adjust axis indices for Bluetooth offset
        let mut new_axis_config = HashMap::new();
        for (axis, config) in &profile.axis_config {
            let mut new_config = config.clone();
            new_config.byte_index += bt_offset;
            new_axis_config.insert(*axis, new_config);
        }
        profile.axis_config = new_axis_config;
        
        // Adjust button mapping for Bluetooth offset
        let mut new_button_map = HashMap::new();
        for (code, button) in &profile.button_map {
            let byte_index = (code >> 8) as usize;
            let bit_mask = *code & 0xFF;
            let new_code = ((byte_index + bt_offset) << 8) as u32 | bit_mask;
            new_button_map.insert(new_code, *button);
        }
        profile.button_map = new_button_map;
        
        // Adjust D-pad for Bluetooth offset
        if let DpadType::Hat { byte_index, mask_values } = &profile.dpad_type {
            profile.dpad_type = DpadType::Hat {
                byte_index: byte_index + bt_offset,
                mask_values: mask_values.clone(),
            };
        }
        
        profile
    }
}

/// Profile factory to create controller profiles
pub struct ProfileFactory;

impl ProfileFactory {
    /// Creates a DualShock 4 profile (v1 or v2) for the specified connection type
    pub fn create_dualshock4_profile(version: u8, connection: ConnectionType) -> ControllerProfile {
        let mut base = Self::create_dualshock4_base();
        
        // Set version-specific properties
        if version == 1 {
            base.name = "DualShock 4 v1".to_string();
            base.description = "Sony PlayStation 4 DualShock Controller v1 (CUH-ZCT1)".to_string();
            base.vid_pid_pairs = vec![(0x054C, 0x05C4)]; // Sony DS4 v1
        } else {
            base.name = "DualShock 4 v2".to_string();
            base.description = "Sony PlayStation 4 DualShock Controller v2 (CUH-ZCT2)".to_string();
            base.vid_pid_pairs = vec![(0x054C, 0x09CC)]; // Sony DS4 v2
        }
        
        // Handle connection type
        if connection == ConnectionType::Bluetooth {
            return base.create_bluetooth_variant(2); // DS4 has 2-byte offset in BT mode
        }
        
        base
    }
    
    /// Create base DualShock 4 profile (shared between v1/v2 and USB/BT)
    fn create_dualshock4_base() -> ControllerProfile {
        let mut button_map = HashMap::new();
        
        // Button mapping for DualShock 4 (USB mode)
        button_map.insert(0x0510, Button::Square);
        button_map.insert(0x0520, Button::Cross);
        button_map.insert(0x0540, Button::Circle);
        button_map.insert(0x0580, Button::Triangle);
        
        button_map.insert(0x0601, Button::L1);
        button_map.insert(0x0602, Button::R1);
        button_map.insert(0x0604, Button::L2);       // Digital press
        button_map.insert(0x0608, Button::R2);       // Digital press
        button_map.insert(0x0610, Button::Share);
        button_map.insert(0x0620, Button::Options);
        button_map.insert(0x0640, Button::L3);       // Left stick press
        button_map.insert(0x0680, Button::R3);       // Right stick press
        
        button_map.insert(0x0701, Button::PS);       // PS button
        button_map.insert(0x0702, Button::Touchpad); // Touchpad click
        
        // Axis configuration
        let mut axis_config = HashMap::new();
        
        // Left stick X: byte 1
        axis_config.insert(Axis::LeftStickX, AxisConfig {
            byte_index: 1,
            center_value: 128,
            range: 128,
            invert: false,
            deadzone: 0.05,
            is_trigger: false,
        });
        
        // Left stick Y: byte 2 (inverted)
        axis_config.insert(Axis::LeftStickY, AxisConfig {
            byte_index: 2,
            center_value: 128,
            range: 128,
            invert: true,   // Invert for proper up/down
            deadzone: 0.05,
            is_trigger: false,
        });
        
        // Right stick X: byte 3
        axis_config.insert(Axis::RightStickX, AxisConfig {
            byte_index: 3,
            center_value: 128,
            range: 128,
            invert: false,
            deadzone: 0.05,
            is_trigger: false,
        });
        
        // Right stick Y: byte 4 (inverted)
        axis_config.insert(Axis::RightStickY, AxisConfig {
            byte_index: 4,
            center_value: 128,
            range: 128,
            invert: true,   // Invert for proper up/down
            deadzone: 0.05,
            is_trigger: false,
        });
        
        // L2 trigger: byte 8
        axis_config.insert(Axis::L2, AxisConfig {
            byte_index: 8,
            center_value: 0,
            range: 255,
            invert: false,
            deadzone: 0.01,  // Lower deadzone for triggers
            is_trigger: true,
        });
        
        // R2 trigger: byte 9
        axis_config.insert(Axis::R2, AxisConfig {
            byte_index: 9,
            center_value: 0,
            range: 255,
            invert: false,
            deadzone: 0.01,  // Lower deadzone for triggers
            is_trigger: true,
        });
        
        // D-pad mapping
        let mut mask_values = HashMap::new();
        mask_values.insert(0, vec![Button::DpadUp]);
        mask_values.insert(1, vec![Button::DpadUp, Button::DpadRight]);
        mask_values.insert(2, vec![Button::DpadRight]);
        mask_values.insert(3, vec![Button::DpadDown, Button::DpadRight]);
        mask_values.insert(4, vec![Button::DpadDown]);
        mask_values.insert(5, vec![Button::DpadDown, Button::DpadLeft]);
        mask_values.insert(6, vec![Button::DpadLeft]);
        mask_values.insert(7, vec![Button::DpadUp, Button::DpadLeft]);
        mask_values.insert(8, vec![]);  // No D-pad buttons pressed
        
        let dpad_type = DpadType::Hat {
            byte_index: 5,
            mask_values,
        };
        
        ControllerProfile {
            name: "DualShock 4 Base".to_string(),
            description: "Sony PlayStation 4 DualShock Controller Base Profile".to_string(),
            vid_pid_pairs: vec![],  // No VID/PID pairs for base profile
            button_map,
            axis_config,
            dpad_type,
            connection_type: ConnectionType::USB,
        }
    }
    
    /// Creates an Xbox controller profile
    pub fn create_xbox_profile() -> ControllerProfile {
        let mut button_map = HashMap::new();
        
        // Map buttons for XInput report format
        button_map.insert(0x0001, Button::DpadUp);     // XINPUT_GAMEPAD_DPAD_UP
        button_map.insert(0x0002, Button::DpadDown);   // XINPUT_GAMEPAD_DPAD_DOWN
        button_map.insert(0x0004, Button::DpadLeft);   // XINPUT_GAMEPAD_DPAD_LEFT
        button_map.insert(0x0008, Button::DpadRight);  // XINPUT_GAMEPAD_DPAD_RIGHT
        button_map.insert(0x0010, Button::Options);    // XINPUT_GAMEPAD_START
        button_map.insert(0x0020, Button::Share);      // XINPUT_GAMEPAD_BACK
        button_map.insert(0x0040, Button::L3);         // XINPUT_GAMEPAD_LEFT_THUMB
        button_map.insert(0x0080, Button::R3);         // XINPUT_GAMEPAD_RIGHT_THUMB
        button_map.insert(0x0100, Button::L1);         // XINPUT_GAMEPAD_LEFT_SHOULDER
        button_map.insert(0x0200, Button::R1);         // XINPUT_GAMEPAD_RIGHT_SHOULDER
        button_map.insert(0x0400, Button::PS);         // XINPUT_GAMEPAD_GUIDE
        button_map.insert(0x1000, Button::Cross);      // XINPUT_GAMEPAD_A
        button_map.insert(0x2000, Button::Circle);     // XINPUT_GAMEPAD_B
        button_map.insert(0x4000, Button::Square);     // XINPUT_GAMEPAD_X
        button_map.insert(0x8000, Button::Triangle);   // XINPUT_GAMEPAD_Y
        
        // XInput format doesn't use Hat for D-pad, it uses individual buttons
        let dpad_type = DpadType::Buttons;
        
        ControllerProfile {
            name: "Xbox Controller".to_string(),
            description: "Microsoft Xbox Controller (XInput compatible)".to_string(),
            vid_pid_pairs: vec![
                (0x045E, 0x028E), // Xbox 360 Controller
                (0x045E, 0x02FF), // Xbox One Controller
            ],
            button_map,
            axis_config: HashMap::new(), // XInput axes are handled separately
            dpad_type,
            connection_type: ConnectionType::Unknown, // Works for both USB/BT
        }
    }
    
    /// Creates a generic controller profile as fallback
    pub fn create_generic_profile() -> ControllerProfile {
        // Basic generic profile with common mappings
        let mut button_map = HashMap::new();
        let mut axis_config = HashMap::new();
        
        // Assume common HID gamepad layout
        button_map.insert(0x0301, Button::Cross);      // A/X/Cross
        button_map.insert(0x0302, Button::Circle);     // B/O/Circle
        button_map.insert(0x0304, Button::Square);     // X/Square
        button_map.insert(0x0308, Button::Triangle);   // Y/Triangle
        
        button_map.insert(0x0310, Button::L1);         // Left shoulder
        button_map.insert(0x0320, Button::R1);         // Right shoulder
        
        // Common stick axes
        axis_config.insert(Axis::LeftStickX, AxisConfig {
            byte_index: 1,
            center_value: 128,
            range: 128,
            invert: false,
            deadzone: 0.1,  // Larger deadzone for unknown controllers
            is_trigger: false,
        });
        
        axis_config.insert(Axis::LeftStickY, AxisConfig {
            byte_index: 2,
            center_value: 128,
            range: 128,
            invert: true,
            deadzone: 0.1,
            is_trigger: false,
        });
        
        ControllerProfile {
            name: "Generic Controller".to_string(),
            description: "Generic HID Gamepad - Basic Mapping".to_string(),
            vid_pid_pairs: vec![],  // Empty means it's a fallback profile
            button_map,
            axis_config,
            dpad_type: DpadType::Hat {
                byte_index: 0,
                mask_values: HashMap::new(),
            },
            connection_type: ConnectionType::Unknown,  // Works with any connection
        }
    }
}

/// Detect the connection type for a device based on name and properties
pub fn detect_connection_type(device_info: &DeviceInfo) -> ConnectionType {
    // Check for Bluetooth indicators in product name
    let product_lower = device_info.product.to_lowercase();
    if product_lower.contains("bluetooth") || 
       product_lower.contains("wireless") {
        return ConnectionType::Bluetooth;
    }
    
    // Sony DualShock 4 v1 (CUH-ZCT1)
    if device_info.vid == 0x054C && device_info.pid == 0x05C4 {
        return ConnectionType::USB;
    }
    
    // Sony DualShock 4 v2 (CUH-ZCT2)
    if device_info.vid == 0x054C && device_info.pid == 0x09CC {
        return ConnectionType::USB;
    }
    
    // Sony DualShock 4 in Bluetooth mode
    if device_info.vid == 0x054C && 
       (device_info.pid == 0x05C5 || device_info.pid == 0x09C2) {
        return ConnectionType::Bluetooth;
    }
    
    ConnectionType::Unknown
}

/// Create profiles for all known controllers (lazy initialization pattern)
pub fn create_profiles() -> &'static Vec<ControllerProfile> {
    PROFILES_CACHE.get_or_init(|| {
        vec![
            ProfileFactory::create_dualshock4_profile(1, ConnectionType::USB),
            ProfileFactory::create_dualshock4_profile(1, ConnectionType::Bluetooth),
            ProfileFactory::create_dualshock4_profile(2, ConnectionType::USB),
            ProfileFactory::create_dualshock4_profile(2, ConnectionType::Bluetooth),
            ProfileFactory::create_xbox_profile(),
            ProfileFactory::create_generic_profile(),
        ]
    })
}

/// Get the best profile for a specific device
pub fn get_profile_for_device<'a>(device_info: &DeviceInfo, profiles: &'a [ControllerProfile]) -> Option<&'a ControllerProfile> {
    let connection_type = detect_connection_type(device_info);
    
    // Detailed connection type detection for PlayStation controllers
    let detailed_connection_type = if device_info.vid == 0x054C {
        // Sony-specific detection logic
        match device_info.pid {
            // DS4 v1 Bluetooth
            0x05C5 => ConnectionType::Bluetooth,
            // DS4 v1 USB
            0x05C4 => ConnectionType::USB,
            // DS4 v2 Bluetooth
            0x09C2 => ConnectionType::Bluetooth,
            // DS4 v2 USB
            0x09CC => ConnectionType::USB,
            // Unknown
            _ => connection_type
        }
    } else {
        connection_type
    };
    
    // Try to match with exact connection type
    for profile in profiles {
        if profile.matches(device_info, detailed_connection_type) {
            return Some(profile);
        }
    }
    
    // Try with generic connection type
    for profile in profiles {
        if profile.matches(device_info, ConnectionType::Unknown) {
            return Some(profile);
        }
    }
    
    // Fallback to the generic profile
    profiles.last()
}

/// Convenience function to create generic profile
pub fn create_generic_profile() -> ControllerProfile {
    ProfileFactory::create_generic_profile()
}