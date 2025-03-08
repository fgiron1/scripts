// src/controller/mod.rs
use std::error::Error;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
mod directinput;

#[cfg(target_os = "linux")]
mod linux;

pub mod types {
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
        Cross,       // South
        Circle,      // East
        Triangle,    // North
        Square,      // West
        L1,          // LeftTrigger2
        R1,          // RightTrigger2
        L3,          // LeftThumb
        R3,          // RightThumb
        Share,       // Select
        Options,     // Start
        PS,          // Mode
        DpadUp,      // DpadUp
        DpadDown,    // DpadDown
        DpadLeft,    // DpadLeft
        DpadRight,   // DpadRight
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
        LeftStickX,
        LeftStickY,
        RightStickX,
        RightStickY,
        L2,          // LeftZ
        R2,          // RightZ
}

    #[derive(Debug)]
pub enum ControllerEvent {
    ButtonPress { button: Button, pressed: bool },
    AxisMove { axis: Axis, value: f32 },
    TouchpadEvent { x: i32, y: i32 },
}
}

pub trait Controller {
    fn poll_events(&mut self) -> Result<Vec<types::ControllerEvent>, Box<dyn Error>>;
}

#[derive(Debug)]
pub enum ControllerType {
    XInput,
    DirectInput,
    Linux,
}

// Platform-specific exports
#[cfg(target_os = "windows")]
pub use windows::XInputController;
#[cfg(target_os = "windows")]
pub use directinput::DirectInputController;

#[cfg(target_os = "linux")]
pub use linux::LinuxController;

#[cfg(target_os = "windows")]
pub fn create_controller() -> Result<(Box<dyn Controller>, ControllerType), Box<dyn Error>> {
    // Try XInput first
    if let Ok(xinput) = XInputController::new() {
        return Ok((Box::new(xinput), ControllerType::XInput));
    }
    
    // Fall back to DirectInput
    let dinput = DirectInputController::new()?;
    Ok((Box::new(dinput), ControllerType::DirectInput))
}

#[cfg(target_os = "linux")]
pub fn create_controller() -> Result<(Box<dyn Controller>, ControllerType), Box<dyn Error>> {
    let controller = LinuxController::new()?;
    Ok((Box::new(controller), ControllerType::Linux))
}