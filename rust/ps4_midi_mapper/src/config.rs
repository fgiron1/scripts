use crate::controller::types::{Button, Axis};

// MIDI Configuration
pub const MIDI_PORT_NAME: &str = ""; // Empty string means use first available port
pub const MIDI_CHANNEL: u8 = 0; // MIDI channel (0-15)

// Controller Configuration
pub const JOYSTICK_DEADZONE: f32 = 0.2; // Normalized deadzone (0.0-1.0)

// Mapping Configuration
pub struct MidiMapping {
    pub button: Button,
    pub note: u8,
}

pub struct AxisMapping {
    pub axis: Axis,
    pub cc: u8,
}

// Button to MIDI note mappings
pub const BUTTON_MAPPINGS: &[MidiMapping] = &[
    MidiMapping { button: Button::Cross, note: 36 },
    MidiMapping { button: Button::Circle, note: 37 },
    MidiMapping { button: Button::Triangle, note: 38 },
    MidiMapping { button: Button::Square, note: 39 },
    MidiMapping { button: Button::L1, note: 40 },
    MidiMapping { button: Button::R1, note: 41 },
    MidiMapping { button: Button::L2, note: 42 },
    MidiMapping { button: Button::R2, note: 43 },
    MidiMapping { button: Button::Share, note: 44 },
    MidiMapping { button: Button::Options, note: 45 },
    MidiMapping { button: Button::PS, note: 46 },
    MidiMapping { button: Button::L3, note: 47 },
    MidiMapping { button: Button::R3, note: 48 },
    MidiMapping { button: Button::DpadUp, note: 49 },
    MidiMapping { button: Button::DpadDown, note: 50 },
    MidiMapping { button: Button::DpadLeft, note: 51 },
    MidiMapping { button: Button::DpadRight, note: 52 },
    MidiMapping { button: Button::Touchpad, note: 53 },
];

// Axis to MIDI CC mappings
pub const AXIS_MAPPINGS: &[AxisMapping] = &[
    AxisMapping { axis: Axis::LeftStickX, cc: 23 },
    AxisMapping { axis: Axis::LeftStickY, cc: 24 },
    AxisMapping { axis: Axis::RightStickX, cc: 25 },
    AxisMapping { axis: Axis::RightStickY, cc: 26 },
    AxisMapping { axis: Axis::L2, cc: 27 },
    AxisMapping { axis: Axis::R2, cc: 28 },
    AxisMapping { axis: Axis::TouchpadX, cc: 29 },
    AxisMapping { axis: Axis::TouchpadY, cc: 30 },
];