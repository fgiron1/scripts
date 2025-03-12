/// Controller event types that can be mapped to MIDI
#[derive(Debug, Clone, PartialEq)]
pub enum ControllerEvent {
    ButtonPress {
        button: Button,
        pressed: bool,
    },
    AxisMove {
        axis: Axis,
        value: f32, // Normalized to range -1.0 to 1.0 (or 0.0 to 1.0 for triggers)
    },
    TouchpadMove {  // Available on all platforms
        x: Option<i32>,
        y: Option<i32>,
    },
}

/// Standard button types across different controllers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
    Cross,      // A on Xbox
    Circle,     // B on Xbox
    Square,     // X on Xbox
    Triangle,   // Y on Xbox
    L1,         // LB on Xbox
    R1,         // RB on Xbox
    L2,         // LT on Xbox (when used as button)
    R2,         // RT on Xbox (when used as button)
    Share,      // Back/View on Xbox
    Options,    // Start/Menu on Xbox
    PS,         // Guide on Xbox
    L3,         // Left stick click
    R3,         // Right stick click
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    Touchpad,   // PS4 touchpad click
    Unknown,
}

/// Standard axis types across different controllers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    LeftStickX,
    LeftStickY,
    RightStickX,
    RightStickY,
    L2,         // Left trigger analog
    R2,         // Right trigger analog
    TouchpadX,  // Touchpad X coordinate (available on all platforms)
    TouchpadY,  // Touchpad Y coordinate (available on all platforms)
    Unknown,
}

/// Configuration for controller axes
#[derive(Debug, Clone)]
pub struct AxisConfig {
    pub byte_index: usize,       // Index in the HID report
    pub center_value: u8,        // Center/rest value (typically 128 for sticks, 0 for triggers)
    pub range: u8,               // Full range of the axis
    pub invert: bool,            // Whether to invert the axis values
    pub deadzone: f32,           // Deadzone as a percentage (0.0-1.0)
    pub is_trigger: bool,        // True for triggers (0.0-1.0 range), false for sticks (-1.0-1.0 range)
}

impl AxisConfig {
    /// Normalize a raw axis value based on this configuration
    pub fn normalize(&self, raw_value: u8) -> f32 {
        if self.is_trigger {
            // Triggers map from 0-range to 0.0-1.0
            let value = raw_value as f32 / self.range as f32;
            
            // Apply deadzone
            if value < self.deadzone {
                return 0.0;
            }
            
            // Rescale to use full range
            let scaled = (value - self.deadzone) / (1.0 - self.deadzone);
            return if self.invert { 1.0 - scaled } else { scaled };
        } else {
            // Sticks map from 0-255 to -1.0-1.0 with center at center_value
            let centered = (raw_value as i16) - (self.center_value as i16);
            let mut normalized = centered as f32 / self.range as f32;
            
            // Invert if necessary
            if self.invert {
                normalized = -normalized;
            }
            
            // Apply deadzone
            if normalized.abs() < self.deadzone {
                return 0.0;
            }
            
            // Rescale values outside deadzone to use full range
            let rescaled = (normalized.abs() - self.deadzone) / (1.0 - self.deadzone);
            
            // Apply original sign
            if normalized < 0.0 {
                -rescaled.min(1.0)
            } else {
                rescaled.min(1.0)
            }
        }
    }
}

/// Basic device information
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub vid: u16,          // Vendor ID
    pub pid: u16,          // Product ID
    pub manufacturer: String,
    pub product: String,
}