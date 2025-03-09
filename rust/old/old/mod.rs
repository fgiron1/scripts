use std::error::Error;
use crate::driver_setup::DriverSetup;
use windows::core::GUID;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use crate::controller::types::ControllerEvent;


#[cfg(target_os = "windows")]
pub mod win_xinput;
#[cfg(target_os = "windows")]
pub mod directinput;

#[cfg(target_os = "linux")]
pub mod linux;

pub mod types {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Button {
        Cross, Circle, Triangle, Square,
        L1, R1, L3, R3,
        Share, Options, PS,
        DpadUp, DpadDown, DpadLeft, DpadRight,
        Unknown,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Axis {
        LeftStickX, LeftStickY,
        RightStickX, RightStickY,
        L2, R2,
    }

    #[derive(Debug)]
    pub enum ControllerEvent {
        ButtonPress { button: Button, pressed: bool },
        AxisMove { axis: Axis, value: f32 },
        TouchpadEvent { x: i32, y: i32 },
    }

    // Add missing types
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Input {
        TrackpadX, TrackpadY,
        LeftTrigger, RightTrigger,
        LeftStickX, LeftStickY,
        RightStickX, RightStickY,
        Button(u8),
    }

    #[derive(Debug)]
    pub struct DriverConfig {
        pub axes: std::collections::HashMap<Input, AxisConfig>,
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

    #[derive(Debug)]
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

pub trait Controller {
    fn poll_events(&mut self) -> std::result::Result<Vec<ControllerEvent>, Box<dyn Error>>;
}

#[derive(Debug)]
pub enum ControllerType {
    XInput,
    DirectInput,
    Linux,
}

#[cfg(target_os = "windows")]
pub use win_xinput::XInputController;
#[cfg(target_os = "windows")]
pub use directinput::DirectInputController;

#[cfg(target_os = "linux")]
pub use linux::LinuxController;

#[cfg(target_os = "windows")]
pub fn create_controller(user_candidate_guids: Option<Vec<GUID>>) -> Result<(Box<dyn Controller>, ControllerType), Box<dyn Error>> {
    let hinstance = unsafe { GetModuleHandleW(None)? }; // Directly use imported function
    let mut setup = DriverSetup::new(hinstance, user_candidate_guids)?; // Pasa la lista de GUIDs candidatos

    // Aquí enumeramos los dispositivos y buscamos uno que coincida con los GUIDs candidatos
    setup.enumerate_devices()?; // Esto llenará device_guids con los dispositivos encontrados

    // Ahora, intenta crear un dispositivo usando el primer GUID encontrado
    if let Some((_, guid)) = setup.device_guids.iter().next() { // Obtén el primer GUID encontrado
        let device = setup.create_device(guid)?; // Crea el dispositivo usando el GUID
        let dinput = DirectInputController::new(device)?; // Crea el controlador
        return Ok((Box::new(dinput), ControllerType::DirectInput));
    }

    Err("No se encontró un dispositivo compatible.".into()) // Manejo de error si no se encuentra un dispositivo
}