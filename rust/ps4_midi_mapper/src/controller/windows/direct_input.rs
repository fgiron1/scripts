use windows::{
    core::{PCWSTR, ComInterface},
    Win32::{
        Devices::{
            HumanInterfaceDevice::{
                DirectInput8Create, IDirectInput8W, IDirectInputDevice8W, 
                DISCL_BACKGROUND, DISCL_NONEXCLUSIVE, 
                DIJOYSTATE2, DIDEVICEINSTANCEW, 
                DIPROPRANGE, DIPROP_RANGE, DIPROPDWORD, DIPROP_BUFFERSIZE, DIPROP_DEADZONE,
                DIPH_DEVICE, DIPH_BYOFFSET,
            },
        },
        Foundation::{HWND, BOOL},
        System::{
            Com::CoInitialize,
            LibraryLoader::GetModuleHandleW,
        }
    },
};
use std::{
    ffi::c_void, 
    mem::{size_of, zeroed},
    ptr::null_mut,
    error::Error,
    collections::HashSet,
};
use crate::controller::{Controller, types::{ControllerEvent, Button, Axis, DeviceInfo}};

// DirectInput constants for DS4
const SONY_VID: u32 = 0x054C;
const DS4_V1_PID: u32 = 0x05C4;
const DS4_V2_PID: u32 = 0x09CC;
const DI_JOYSTICK_DEADZONE: i32 = 7840;  // ~25% of i32 range
const TRIGGER_DEADZONE: u8 = 30;      // ~12% of u8 range

// DirectInput button indices for DS4
const DI_BUTTON_SQUARE: usize = 0;
const DI_BUTTON_CROSS: usize = 1;
const DI_BUTTON_CIRCLE: usize = 2;
const DI_BUTTON_TRIANGLE: usize = 3;
const DI_BUTTON_L1: usize = 4;
const DI_BUTTON_R1: usize = 5;
const DI_BUTTON_L2: usize = 6;
const DI_BUTTON_R2: usize = 7;
const DI_BUTTON_SHARE: usize = 8;
const DI_BUTTON_OPTIONS: usize = 9;
const DI_BUTTON_L3: usize = 10;
const DI_BUTTON_R3: usize = 11;
const DI_BUTTON_PS: usize = 12;
const DI_BUTTON_TOUCHPAD: usize = 13;

// We need to save device instances globally since the enumeration callback can't easily access our struct
static mut FOUND_DEVICE_INSTANCE: Option<DIDEVICEINSTANCEW> = None;

// Configure device properties for a DirectInput device
fn configure_device_properties(device: &IDirectInputDevice8W) -> Result<(), Box<dyn Error>> {
    // Set buffer size (this helps with not losing events)
    let mut prop_word = DIPROPDWORD {
        diph: windows::Win32::Devices::HumanInterfaceDevice::DIPROPHEADER {
            dwSize: size_of::<DIPROPDWORD>() as u32,
            dwHeaderSize: size_of::<windows::Win32::Devices::HumanInterfaceDevice::DIPROPHEADER>() as u32,
            dwObj: 0,
            dwHow: DIPH_DEVICE,
        },
        dwData: 32, // Buffer size
    };
    
    unsafe {
        device.SetProperty(
            &DIPROP_BUFFERSIZE,
            &mut prop_word.diph as *mut _ as *mut _,
        )?;
    }
    
    // Set axis ranges for X, Y, Z, RX, RY, RZ
    let axes = [
        (0x00000001, -32768, 32767), // X-axis
        (0x00000002, -32768, 32767), // Y-axis
        (0x00000003, -32768, 32767), // Z-axis
        (0x00000004, -32768, 32767), // RX-axis
        (0x00000005, -32768, 32767), // RY-axis
        (0x00000006, 0, 255),        // RZ-axis (triggers use 0-255)
    ];
    
    for (axis, min, max) in axes {
        let mut prop_range = DIPROPRANGE {
            diph: windows::Win32::Devices::HumanInterfaceDevice::DIPROPHEADER {
                dwSize: size_of::<DIPROPRANGE>() as u32,
                dwHeaderSize: size_of::<windows::Win32::Devices::HumanInterfaceDevice::DIPROPHEADER>() as u32,
                dwObj: axis,
                dwHow: DIPH_BYOFFSET,
            },
            lMin: min,
            lMax: max,
        };
        
        unsafe {
            device.SetProperty(
                &DIPROP_RANGE,
                &mut prop_range.diph as *mut _ as *mut _,
            )?;
        }
    }
    
    // Set deadzone for joysticks
    let axes = [0x00000001, 0x00000002, 0x00000003, 0x00000004]; // X, Y, Z, RX
    
    for axis in axes {
        let mut prop_dz = DIPROPDWORD {
            diph: windows::Win32::Devices::HumanInterfaceDevice::DIPROPHEADER {
                dwSize: size_of::<DIPROPDWORD>() as u32,
                dwHeaderSize: size_of::<windows::Win32::Devices::HumanInterfaceDevice::DIPROPHEADER>() as u32,
                dwObj: axis,
                dwHow: DIPH_BYOFFSET,
            },
            dwData: DI_JOYSTICK_DEADZONE as u32,
        };
        
        unsafe {
            device.SetProperty(
                &DIPROP_DEADZONE,
                &mut prop_dz.diph as *mut _ as *mut _,
            )?;
        }
    }
    
    Ok(())
}

// Mapping of DirectInput button indices to our Button enum
fn map_button_indices() -> Vec<Button> {
    let mut mapping = vec![Button::Unknown; 32]; // Pre-allocate with Unknown
    
    mapping[DI_BUTTON_CROSS] = Button::Cross;
    mapping[DI_BUTTON_CIRCLE] = Button::Circle;
    mapping[DI_BUTTON_SQUARE] = Button::Square;
    mapping[DI_BUTTON_TRIANGLE] = Button::Triangle;
    mapping[DI_BUTTON_L1] = Button::L1;
    mapping[DI_BUTTON_R1] = Button::R1;
    mapping[DI_BUTTON_L2] = Button::L2;
    mapping[DI_BUTTON_R2] = Button::R2;
    mapping[DI_BUTTON_SHARE] = Button::Share;
    mapping[DI_BUTTON_OPTIONS] = Button::Options;
    mapping[DI_BUTTON_L3] = Button::L3;
    mapping[DI_BUTTON_R3] = Button::R3;
    mapping[DI_BUTTON_PS] = Button::PS;
    mapping[DI_BUTTON_TOUCHPAD] = Button::Touchpad;
    
    mapping
}

// Process DPad from the POV hat
fn process_dpad(
    state: &DIJOYSTATE2, 
    last_state: &DIJOYSTATE2, 
    events: &mut Vec<ControllerEvent>,
    button_states: &mut HashSet<Button>
) {
    let pov = state.rgdwPOV[0];
    let last_pov = last_state.rgdwPOV[0];
    
    // Only process if the value changed
    if pov == last_pov {
        return;
    }
    
    // Check which dpad buttons were previously active
    let was_up = button_states.contains(&Button::DpadUp);
    let was_right = button_states.contains(&Button::DpadRight);
    let was_down = button_states.contains(&Button::DpadDown);
    let was_left = button_states.contains(&Button::DpadLeft);
    
    // Determine which dpad buttons are active now
    let is_up = pov != 0xFFFFFFFF && ((pov >= 31500) || (pov <= 4500));
    let is_right = pov != 0xFFFFFFFF && (pov >= 4500 && pov <= 13500);
    let is_down = pov != 0xFFFFFFFF && (pov >= 13500 && pov <= 22500);
    let is_left = pov != 0xFFFFFFFF && (pov >= 22500 && pov <= 31500);
    
    // Generate events for any changed states
    if was_up != is_up {
        events.push(ControllerEvent::ButtonPress { button: Button::DpadUp, pressed: is_up });
        if is_up { button_states.insert(Button::DpadUp); } else { button_states.remove(&Button::DpadUp); }
    }
    
    if was_right != is_right {
        events.push(ControllerEvent::ButtonPress { button: Button::DpadRight, pressed: is_right });
        if is_right { button_states.insert(Button::DpadRight); } else { button_states.remove(&Button::DpadRight); }
    }
    
    if was_down != is_down {
        events.push(ControllerEvent::ButtonPress { button: Button::DpadDown, pressed: is_down });
        if is_down { button_states.insert(Button::DpadDown); } else { button_states.remove(&Button::DpadDown); }
    }
    
    if was_left != is_left {
        events.push(ControllerEvent::ButtonPress { button: Button::DpadLeft, pressed: is_left });
        if is_left { button_states.insert(Button::DpadLeft); } else { button_states.remove(&Button::DpadLeft); }
    }
}

// DirectInput device enumeration callback
unsafe extern "system" fn enum_devices_callback(
    device_instance: *mut DIDEVICEINSTANCEW,
    _context: *mut c_void,
) -> BOOL {
    // Safety check - make sure the pointer is valid
    if device_instance.is_null() {
        return BOOL(0); // Continue enumeration
    }
    
    let instance = *device_instance;
    
    // Extract vendor and product IDs from the product GUID
    let vendor_id = ((instance.guidProduct.data1 >> 16) & 0xFFFF) as u32;
    let product_id = (instance.guidProduct.data1 & 0xFFFF) as u32;
    
    // Check if this is a Sony DualShock 4
    if vendor_id == SONY_VID && (product_id == DS4_V1_PID || product_id == DS4_V2_PID) {
        // Found a DS4, store it and stop enumeration
        FOUND_DEVICE_INSTANCE = Some(instance);
        return BOOL(1); // Stop enumeration
    }
    
    // Check if the device name contains "DualShock" or "DS4" as a fallback
    let product_name = instance.tszProductName;
    
    // Safely extract the product name
    let mut name_str = String::new();
    let mut i = 0;
    // Make sure we don't read past the end of the array or hit a null terminator
    while i < product_name.len() && product_name[i] != 0 {
        // Safely convert from u16 to char
        if let Some(c) = char::from_u32(product_name[i] as u32) {
            name_str.push(c);
        }
        i += 1;
    }
    
    if name_str.contains("DualShock") || name_str.contains("DS4") || name_str.contains("Wireless Controller") {
        // Found a DS4 by name, store it and stop enumeration
        FOUND_DEVICE_INSTANCE = Some(instance);
        return BOOL(1); // Stop enumeration
    }
    
    // Continue enumeration
    BOOL(0)
}

pub struct DirectInputDeviceContext {
    di: IDirectInput8W,
    device: Option<IDirectInputDevice8W>, 
}

// This wrapper ensures the DirectInput device can be safely sent between threads
pub struct ThreadSafeDirectInputDevice {
    // Using Option to allow us to take ownership in Drop impl
    context: Option<DirectInputDeviceContext>,
}

// Manually implement Send for our wrapper
unsafe impl Send for ThreadSafeDirectInputDevice {}

impl Drop for ThreadSafeDirectInputDevice {
    fn drop(&mut self) {
        // Clean up DirectInput device on drop
        if let Some(mut context) = self.context.take() {
            if let Some(device) = context.device.take() {
                unsafe {
                    let _ = device.Unacquire();
                    // Device will be dropped automatically
                }
            }
            // DirectInput interface will be dropped automatically
        }
    }
}

/// DirectInput controller implementation
pub struct DirectInputController {
    device: ThreadSafeDirectInputDevice,
    device_info: DeviceInfo,
    last_state: DIJOYSTATE2,
    button_states: HashSet<Button>,
}

impl DirectInputController {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        // Initialize COM library (required for DirectInput)
        unsafe {
            // Ignore any error from CoInitialize since it might already be initialized
            let _ = CoInitialize(None);
        }
        
        // Create DirectInput8 interface
        let di = unsafe {
            let mut di: Option<*mut c_void> = None;
            // Handle any errors from DirectInput8Create gracefully
            let result = DirectInput8Create(
                GetModuleHandleW(PCWSTR::null())?,
                0x0800, // DIRECTINPUT_VERSION
                &IDirectInput8W::IID,
                &mut di as *mut _ as *mut *mut c_void,
                None,
            );
            
            if result.is_err() {
                return Err("Failed to create DirectInput interface".into());
            }
            
            if di.is_none() {
                return Err("DirectInput creation returned null pointer".into());
            }
            
            let direct_input_ptr = di.unwrap();
            std::mem::transmute::<*mut c_void, IDirectInput8W>(direct_input_ptr)
        };
        
        // Enumerate devices to find a DS4 controller with extra safety
        let found_device = unsafe {
            // Reset the static variable
            FOUND_DEVICE_INSTANCE = None;
            
            // Use a try block to catch any panics or errors during enumeration
            let enum_result = std::panic::catch_unwind(|| {
                di.EnumDevices(
                    windows::Win32::Devices::HumanInterfaceDevice::DI8DEVCLASS_GAMECTRL, 
                    Some(enum_devices_callback),
                    null_mut(),
                    windows::Win32::Devices::HumanInterfaceDevice::DIEDFL_ATTACHEDONLY,
                )
            });
            
            // If enumeration failed or panicked, return None
            if enum_result.is_err() || (enum_result.is_ok() && enum_result.unwrap().is_err()) {
                None
            } else {
                // Otherwise, take the found device (if any)
                FOUND_DEVICE_INSTANCE.take()
            }
        };
        
        // If no device was found, return early
        if found_device.is_none() {
            return Err("No DualShock 4 controller found".into());
        }
        
        // Initialize with default values
        let mut controller = Self {
            device: ThreadSafeDirectInputDevice {
                context: Some(DirectInputDeviceContext {
                    di,
                    device: None,
                }),
            },
            device_info: DeviceInfo {
                vid: 0,
                pid: 0,
                manufacturer: String::new(),
                product: String::new(),
            },
            last_state: unsafe { zeroed() },
            button_states: HashSet::new(),
        };
        
        // Try to initialize the device with proper error handling
        let device_instance = found_device.unwrap();
        if let Err(e) = controller.initialize_device(device_instance) {
            return Err(format!("Failed to initialize DirectInput device: {}", e).into());
        }
        
        Ok(controller)
    }
    
    fn initialize_device(&mut self, device_instance: DIDEVICEINSTANCEW) -> Result<(), Box<dyn Error>> {
        // Extract device context
        let context = self.device.context.as_mut()
            .ok_or("DirectInput context is not available")?;
            
        // Create the device
        let device = unsafe {
            let mut device = None;
            context.di.CreateDevice(
                &device_instance.guidInstance,
                &mut device,
                None,
            )?;
            device.unwrap()
        };
        
        // Set data format to a joystick format that works with DS4
        // The c_dfDIJoystick2 constant isn't directly available in this Windows crate version
        // Instead, we'll use a safer workaround using a FFI call to the DirectInput DLL
        unsafe {
            // Link to DirectInput8 through dynamic loading
            use windows::Win32::System::LibraryLoader::{LoadLibraryA, GetProcAddress};
            
            // Load dinput8.dll
            let dinput_lib = LoadLibraryA(windows::core::s!("dinput8.dll")).unwrap();
            
            // Get the global joystick data format constant
            // The DLL exports the c_dfDIJoystick2 symbol
            let dfJoy2_addr = GetProcAddress(dinput_lib, windows::core::s!("c_dfDIJoystick2")).unwrap();
            
            // Use this address as the data format
            device.SetDataFormat(dfJoy2_addr as *mut _)?;
        }
        
        // Set cooperative level - use desktop window for background operation
        unsafe {
            device.SetCooperativeLevel(
                HWND(0), // Desktop window
                DISCL_BACKGROUND | DISCL_NONEXCLUSIVE,
            )?;
        }
        
        // Configure device properties
        configure_device_properties(&device)?;
        
        // Acquire the device
        unsafe {
            device.Acquire()?;
        }
        
        // Extract device information
        let product_name = {
            let product_name = device_instance.tszProductName;
            let mut len = 0;
            while len < product_name.len() && product_name[len] != 0 {
                len += 1;
            }
            String::from_utf16_lossy(&product_name[0..len])
        };
        
        let vendor_id = ((device_instance.guidProduct.data1 >> 16) & 0xFFFF) as u16;
        let product_id = (device_instance.guidProduct.data1 & 0xFFFF) as u16;
        
        // Update device info
        self.device_info = DeviceInfo {
            vid: vendor_id,
            pid: product_id,
            manufacturer: "Sony".to_string(),
            product: product_name,
        };
        
        // Store the device
        context.device = Some(device);
        
        Ok(())
    }
    
    fn normalize_axis(&self, value: i32, is_trigger: bool) -> f32 {
        if is_trigger {
            // Triggers (L2/R2) are 0-255
            (value as f32 / 255.0).clamp(0.0, 1.0)
        } else {
            // Sticks are -32768 to 32767
            (value as f32 / 32767.0).clamp(-1.0, 1.0)
        }
    }
}

impl Controller for DirectInputController {
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn Error>> {
        let mut events = Vec::new();
        
        // Extract device context
        let context = match &self.device.context {
            Some(ctx) => ctx,
            None => return Ok(events), // No context, return empty events
        };
        
        // Get device
        let device = match &context.device {
            Some(dev) => dev,
            None => return Ok(events), // No device, return empty events
        };
        
        // Try to poll the device
        let poll_result = unsafe { device.Poll() };
        
        // If poll failed due to input being lost, try to reacquire
        if poll_result.is_err() {
            unsafe { 
                let _ = device.Unacquire();
                let acquire_result = device.Acquire();
                if acquire_result.is_err() {
                    // Still couldn't acquire, return empty events
                    return Ok(events);
                }
                // Retry polling after reacquiring
                if device.Poll().is_err() {
                    return Ok(events);
                }
            }
        }
        
        // Get the current device state
        let mut state: DIJOYSTATE2 = unsafe { zeroed() };
        let get_result = unsafe { device.GetDeviceState(size_of::<DIJOYSTATE2>() as u32, &mut state as *mut _ as *mut c_void) };
        
        if get_result.is_err() {
            // Failed to get state, return empty events
            return Ok(events);
        }
        
        // Process axes
        // Left stick
        let lx = state.lX;
        let ly = state.lY;
        if lx != self.last_state.lX {
            events.push(ControllerEvent::AxisMove { 
                axis: Axis::LeftStickX, 
                value: self.normalize_axis(lx, false) 
            });
        }
        
        if ly != self.last_state.lY {
            events.push(ControllerEvent::AxisMove { 
                axis: Axis::LeftStickY, 
                // DirectInput has Y increasing downward, so we negate it
                value: -self.normalize_axis(ly, false) 
            });
        }
        
        // Right stick (mapped to Z and RZ in DirectInput for DS4)
        let rx = state.lZ;
        let ry = state.lRz;
        if rx != self.last_state.lZ {
            events.push(ControllerEvent::AxisMove { 
                axis: Axis::RightStickX, 
                value: self.normalize_axis(rx, false) 
            });
        }
        
        if ry != self.last_state.lRz {
            events.push(ControllerEvent::AxisMove { 
                axis: Axis::RightStickY, 
                // DirectInput has Y increasing downward, so we negate it
                value: -self.normalize_axis(ry, false) 
            });
        }
        
        // Triggers (L2 and R2)
        let l2 = state.lRx;
        let r2 = state.lRy;
        if l2 != self.last_state.lRx {
            events.push(ControllerEvent::AxisMove { 
                axis: Axis::L2, 
                value: self.normalize_axis(l2, true) 
            });
        }
        
        if r2 != self.last_state.lRy {
            events.push(ControllerEvent::AxisMove { 
                axis: Axis::R2, 
                value: self.normalize_axis(r2, true) 
            });
        }
        
        // Process buttons
        for (i, button) in map_button_indices().iter().enumerate() {
            if i < 32 { // Safety check
                let button_state = (state.rgbButtons[i] & 0x80) != 0;
                if (self.last_state.rgbButtons[i] & 0x80) != (state.rgbButtons[i] & 0x80) {
                    events.push(ControllerEvent::ButtonPress { 
                        button: *button, 
                        pressed: button_state 
                    });
                    
                    if button_state {
                        self.button_states.insert(*button);
                    } else {
                        self.button_states.remove(button);
                    }
                }
            }
        }
        
        // Process D-Pad (POV)
        process_dpad(&state, &self.last_state, &mut events, &mut self.button_states);
        
        // Save the current state for next comparison
        self.last_state = state;
        
        Ok(events)
    }
    
    fn get_device_info(&self) -> DeviceInfo {
        self.device_info.clone()
    }
}