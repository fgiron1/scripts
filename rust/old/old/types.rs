use std::collections::HashMap;

pub mod types {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Input {
        TrackpadX,
        TrackpadY,
        LeftTrigger,
        RightTrigger,
        LeftStickX,
        LeftStickY,
        RightStickX,
        RightStickY,
        Button(u8),
    }
    
    #[derive(Debug)]
    pub struct DriverConfig {
        pub axes: HashMap<Input, AxisConfig>,
    }
    
    #[derive(Debug)]
    pub struct AxisConfig {
        pub range: std::ops::Range<f32>,
    }
    
    #[derive(Debug, Clone, Copy)]
    pub enum InputEvent {
        Axis(Input, f32),
        Button(Input, bool),
    }
    
    #[derive(Debug, Clone, Copy)]
    pub struct Joystick;
    
    impl Joystick {
        pub fn with_deadzone(value: i16, deadzone: i16) -> f32 {
            let deadzone = deadzone.clamp(0, i16::MAX);
            let abs_value = value.abs();
            
            if abs_value <= deadzone {
                0.0
            } else {
                let normalized = (abs_value - deadzone) as f32 / (i16::MAX - deadzone) as f32;
                normalized.copysign(value as f32)
            }
        }
    }
}
