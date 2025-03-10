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
    #[cfg(target_os = "linux")]
    TouchpadMove {
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
    #[cfg(target_os = "linux")]
    TouchpadX,
    #[cfg(target_os = "linux")]
    TouchpadY,
    Unknown,
}

/// Basic device information
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub vid: u16,          // Vendor ID
    pub pid: u16,          // Product ID
    pub manufacturer: String,
    pub product: String,
}