use std::any::Any;
use std::collections::HashMap;
use std::error::Error;
use std::ptr::null;
use std::mem::size_of;

use windows::Win32::Foundation::{
    HANDLE, HWND, INVALID_HANDLE_VALUE, CloseHandle, GetLastError, BOOL, FALSE, TRUE
};

use windows::Win32::Devices::HumanInterfaceDevice::{
    HidD_GetHidGuid, HidD_GetProductString, HidD_GetManufacturerString,
    HidD_FreePreparsedData, HidD_GetPreparsedData, HidD_GetAttributes, HidD_SetNumInputBuffers,
    HidP_GetCaps, HidP_Input, HidP_GetValueCaps, HIDD_ATTRIBUTES, HIDP_CAPS, HIDP_VALUE_CAPS,
    HIDP_STATUS_SUCCESS, PHIDP_PREPARSED_DATA
};

use windows::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces,
    SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
    HDEVINFO, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA
};

use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, FILE_FLAG_OVERLAPPED,
    ReadFile
};

use windows::Win32::System::Threading::{
    CreateEventW, WaitForSingleObject,
};

use windows::Win32::System::IO::{
    GetOverlappedResult, OVERLAPPED,
};

use windows::core::{GUID, PCWSTR};

use crate::controller::{Controller, types::{ControllerEvent, Button, Axis, DeviceInfo}};
use crate::controller::profiles::{ConnectionType, ControllerProfile, get_profile_for_device, create_profiles};

// Constants for touchpad
const DS4_TOUCHPAD_X_MAX: i32 = 1920;
const DS4_TOUCHPAD_Y_MAX: i32 = 942;
const TOUCHPAD_MIN_CHANGE: i32 = 5;

// We'll define these constants directly since they're missing
const HIDP_LINK_COLLECTION_ROOT: u16 = 0;
const WAIT_TIMEOUT: u32 = 258;
const WAIT_OBJECT_0: u32 = 0;

// Structure to hold controller device information
struct ControllerDevice {
    handle: HANDLE,
    device_info: DeviceInfo,
    preparsed_data: PHIDP_PREPARSED_DATA,
    capabilities: HIDP_CAPS,
    value_caps: Vec<HIDP_VALUE_CAPS>,
    read_buffer: Vec<u8>,
    read_event: HANDLE,
    read_overlapped: OVERLAPPED,
    report_length: u32,
    is_dualshock: bool,
    is_bluetooth: bool,
}

impl Drop for ControllerDevice {
    fn drop(&mut self) {
        unsafe {
            if !self.preparsed_data.is_null() {
                HidD_FreePreparsedData(self.preparsed_data);
            }
            
            if self.handle != INVALID_HANDLE_VALUE {
                CloseHandle(self.handle);
            }
            
            if self.read_event != INVALID_HANDLE_VALUE {
                CloseHandle(self.read_event);
            }
        }
    }
}

// Allow cloning by duplicating handles (simplified for example)
impl Clone for ControllerDevice {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle,
            device_info: self.device_info.clone(),
            preparsed_data: self.preparsed_data,
            capabilities: self.capabilities,
            value_caps: self.value_caps.clone(),
            read_buffer: self.read_buffer.clone(),
            read_event: self.read_event,
            read_overlapped: self.read_overlapped,
            report_length: self.report_length,
            is_dualshock: self.is_dualshock,
            is_bluetooth: self.is_bluetooth,
        }
    }
}

pub struct WindowsRawIOController {
    device: ControllerDevice,
    button_states: HashMap<Button, bool>,
    axis_values: HashMap<Axis, f32>,
    
    // Touchpad specific state
    touchpad_active: bool,
    touchpad_last_x: i32,
    touchpad_last_y: i32,
    
    // Debug mode
    debug_mode: bool,
    
    // Profile
    profile: Option<&'static ControllerProfile>,
}

impl WindowsRawIOController {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        println!("Creating Windows Raw IO Controller...");
        
        // Find controller
        let device = Self::find_controller()?;
        
        println!("Found controller: {} (VID: 0x{:04X}, PID: 0x{:04X})", 
                 device.device_info.product, device.device_info.vid, device.device_info.pid);
        
        // Create controller instance
        let mut controller = WindowsRawIOController {
            button_states: HashMap::new(),
            axis_values: HashMap::new(),
            touchpad_active: false,
            touchpad_last_x: 0,
            touchpad_last_y: 0,
            debug_mode: false,
            device,
            profile: None,
        };
        
        // Cache the profile
        controller.profile = Some(controller.get_controller_profile());
        
        Ok(controller)
    }
    
    fn find_controller() -> Result<ControllerDevice, Box<dyn Error>> {
        // Get HID GUID
        let mut hid_guid = GUID::default();
        unsafe { HidD_GetHidGuid(&mut hid_guid); }
        
        // Get list of all HID devices
        let device_info_set = unsafe {
            SetupDiGetClassDevsW(
                &hid_guid,
                PCWSTR::null(),
                HWND::default(),
                DIGCF_DEVICEINTERFACE | DIGCF_PRESENT,
            )
        };
        
        if device_info_set == HDEVINFO::default() {
            return Err("Failed to get device information set".into());
        }
        
        // Prepare to enumerate devices
        let mut device_interface_data = SP_DEVICE_INTERFACE_DATA {
            cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };
        
        let mut device_index = 0;
        let mut found_devices = Vec::new();
        
        // Loop through devices
        loop {
            let success = unsafe {
                SetupDiEnumDeviceInterfaces(
                    device_info_set,
                    None,
                    &hid_guid,
                    device_index,
                    &mut device_interface_data,
                )
            };
            
            if !success.as_bool() {
                let error = unsafe { GetLastError() };
                if error.0 == 259 { // ERROR_NO_MORE_ITEMS
                    break;
                } else {
                    unsafe { SetupDiDestroyDeviceInfoList(device_info_set) };
                    return Err(format!("Error enumerating device interfaces: {}", error.0).into());
                }
            }
            
            // Get the device path size
            let mut required_size = 0u32;
            unsafe {
                SetupDiGetDeviceInterfaceDetailW(
                    device_info_set,
                    &device_interface_data,
                    None,
                    0,
                    &mut required_size,
                    None,
                )
            };
            
            // Allocate buffer for device path
            let mut device_interface_detail_data = vec![0u8; required_size as usize];
            let p_device_interface_detail_data = device_interface_detail_data.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
            
            // Set the size field
            unsafe {
                (*p_device_interface_detail_data).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            }
            
            // Get device info data
            let mut device_info_data = SP_DEVINFO_DATA {
                cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            
            // Get device path
            let success = unsafe {
                SetupDiGetDeviceInterfaceDetailW(
                    device_info_set,
                    &device_interface_data,
                    Some(p_device_interface_detail_data),
                    required_size,
                    &mut required_size,
                    Some(&mut device_info_data),
                )
            };
            
            if !success.as_bool() {
                device_index += 1;
                continue;
            }
            
            // Try to open the device
            let device_handle = unsafe {
                CreateFileW(
                    PCWSTR((*p_device_interface_detail_data).DevicePath.as_ptr()),
                    windows::Win32::Storage::FileSystem::FILE_GENERIC_READ | windows::Win32::Storage::FileSystem::FILE_GENERIC_WRITE,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    null(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED,
                    HANDLE::default(),
                )?
            };
            
            if device_handle == INVALID_HANDLE_VALUE {
                device_index += 1;
                continue;
            }
            
            // Get device attributes
            let mut attrs = HIDD_ATTRIBUTES {
                Size: size_of::<HIDD_ATTRIBUTES>() as u32,
                ..Default::default()
            };
            
            let success = unsafe { HidD_GetAttributes(device_handle, &mut attrs) };
            
            if !success.as_bool() {
                unsafe { CloseHandle(device_handle) };
                device_index += 1;
                continue;
            }
            
            // Get device strings
            let mut product_buffer = [0u16; 128];
            let mut manufacturer_buffer = [0u16; 128];
            
            let has_product = unsafe {
                HidD_GetProductString(device_handle, product_buffer.as_mut_ptr() as _, 
                                     (product_buffer.len() * size_of::<u16>()) as u32)
            }.as_bool();
            
            let has_manufacturer = unsafe {
                HidD_GetManufacturerString(device_handle, manufacturer_buffer.as_mut_ptr() as _, 
                                          (manufacturer_buffer.len() * size_of::<u16>()) as u32)
            }.as_bool();
            
            // Convert strings from UTF-16
            let product = if has_product {
                let end = product_buffer.iter().position(|&c| c == 0).unwrap_or(product_buffer.len());
                String::from_utf16_lossy(&product_buffer[0..end])
            } else {
                "Unknown".to_string()
            };
            
            let manufacturer = if has_manufacturer {
                let end = manufacturer_buffer.iter().position(|&c| c == 0).unwrap_or(manufacturer_buffer.len());
                String::from_utf16_lossy(&manufacturer_buffer[0..end])
            } else {
                "Unknown".to_string()
            };
            
            // Check if it's a controller
            let is_controller = product.to_lowercase().contains("controller") || 
                               product.to_lowercase().contains("gamepad") ||
                               product.to_lowercase().contains("dualshock") ||
                               product.to_lowercase().contains("xbox");
            
            // Only process controllers
            if is_controller {
                // Get device capabilities
                let mut preparsed_data = PHIDP_PREPARSED_DATA::default();
                let preparsed_result = unsafe { HidD_GetPreparsedData(device_handle, &mut preparsed_data) };
                
                if !preparsed_result.as_bool() || preparsed_data.is_null() {
                    unsafe { CloseHandle(device_handle) };
                    device_index += 1;
                    continue;
                }
                
                let mut caps = HIDP_CAPS::default();
                let caps_result = unsafe { HidP_GetCaps(preparsed_data, &mut caps) };
                
                if caps_result != HIDP_STATUS_SUCCESS {
                    unsafe {
                        HidD_FreePreparsedData(preparsed_data);
                        CloseHandle(device_handle);
                    }
                    device_index += 1;
                    continue;
                }
                
                // Get value capabilities
                let mut value_caps_length = caps.NumberInputValueCaps;
                let mut value_caps = Vec::with_capacity(value_caps_length as usize);
                value_caps.resize(value_caps_length as usize, HIDP_VALUE_CAPS::default());
                
                let value_caps_result = unsafe {
                    HidP_GetValueCaps(
                        HidP_Input,
                        value_caps.as_mut_ptr(),
                        &mut value_caps_length,
                        preparsed_data,
                    )
                };
                
                // Create read event
                let read_event = unsafe { CreateEventW(null(), TRUE, FALSE, PCWSTR::null()) };
                
                if read_event == INVALID_HANDLE_VALUE {
                    unsafe {
                        HidD_FreePreparsedData(preparsed_data);
                        CloseHandle(device_handle);
                    }
                    device_index += 1;
                    continue;
                }
                
                // Create overlapped structure
                let mut read_overlapped = OVERLAPPED::default();
                read_overlapped.hEvent = read_event;
                
                // Set buffer size
                unsafe { HidD_SetNumInputBuffers(device_handle, 64) };
                
                // Check if it's a DualShock controller
                let is_dualshock = product.to_lowercase().contains("dualshock") ||
                                  product.to_lowercase().contains("wireless controller");
                                  
                // Check if it's Bluetooth connected (for DualShock)
                let is_bluetooth = is_dualshock && 
                                  (attrs.ProductID == 0x05C5 || attrs.ProductID == 0x09C2);
                
                // Create the device info
                let device_info = DeviceInfo {
                    vid: attrs.VendorID,
                    pid: attrs.ProductID,
                    manufacturer,
                    product,
                };
                
                // Create read buffer based on report length
                let report_length = caps.InputReportByteLength as u32;
                let read_buffer = vec![0u8; report_length as usize];
                
                // Add this device to found devices
                found_devices.push(ControllerDevice {
                    handle: device_handle,
                    device_info,
                    preparsed_data,
                    capabilities: caps,
                    value_caps: value_caps[0..value_caps_length as usize].to_vec(),
                    read_buffer,
                    read_event,
                    read_overlapped,
                    report_length,
                    is_dualshock,
                    is_bluetooth,
                });
            } else {
                unsafe { CloseHandle(device_handle) };
            }
            
            device_index += 1;
        }
        
        // Clean up
        unsafe { SetupDiDestroyDeviceInfoList(device_info_set) };
        
        // Find the best device
        
        // First priority: DualShock 4 controllers
        for device in &found_devices {
            if device.is_dualshock &&
               (device.device_info.pid == 0x05C4 || // DS4 v1
                device.device_info.pid == 0x09CC) { // DS4 v2
                return Ok(device.clone());
            }
        }
        
        // Second priority: Any Xbox controller
        for device in &found_devices {
            if device.device_info.product.to_lowercase().contains("xbox") {
                return Ok(device.clone());
            }
        }
        
        // Last resort: Any game controller
        if let Some(device) = found_devices.first() {
            return Ok(device.clone());
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
        if let Some(profile) = get_profile_for_device(&self.device.device_info, profiles) {
            return profile;
        }
        
        // Fall back to generic profile
        profiles.last().expect("At least one profile should exist")
    }
    
    // Read data from the device
    fn read_device_data(&mut self) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        // Prepare read buffer
        let mut bytes_read = 0u32;
        
        // Reset the overlapped structure
        self.device.read_overlapped.Internal = 0;
        self.device.read_overlapped.InternalHigh = 0;
        self.device.read_overlapped.Anonymous.Anonymous.Offset = 0;
        self.device.read_overlapped.Anonymous.Anonymous.OffsetHigh = 0;
        
        // Start the read operation
        let read_result = unsafe {
            ReadFile(
                self.device.handle,
                self.device.read_buffer.as_mut_ptr() as _,
                self.device.report_length,
                &mut bytes_read,
                &mut self.device.read_overlapped,
            )
        };
        
        if !read_result.as_bool() {
            let error = unsafe { GetLastError() };
            
            // If the operation is pending, wait for it
            if error.0 != 997 { // ERROR_IO_PENDING
                return Err(format!("Error reading from device: {}", error.0).into());
            }
            
            // Wait for the read to complete with a short timeout (5ms)
            let wait_result = unsafe { 
                WaitForSingleObject(self.device.read_event, 5)
            };
            
            if wait_result == WAIT_TIMEOUT {
                // No data available yet
                return Ok(None);
            }
            
            // Get the result
            let get_result = unsafe {
                GetOverlappedResult(
                    self.device.handle,
                    &mut self.device.read_overlapped,
                    &mut bytes_read,
                    FALSE,
                )
            };
            
            if !get_result.as_bool() {
                let error = unsafe { GetLastError() };
                return Err(format!("Error getting read result: {}", error.0).into());
            }
        }
        
        // If we got data, return it
        if bytes_read > 0 {
            return Ok(Some(self.device.read_buffer.clone()));
        }
        
        // No data
        Ok(None)
    }
    
    // Parse the raw data from the device
    fn parse_report(&mut self, data: &[u8], events: &mut Vec<ControllerEvent>) {
        // Get the current profile
        let profile = self.get_controller_profile();
        
        // First data received - detect Bluetooth mode for DS4
        if self.device.is_dualshock && !self.device.is_bluetooth && data[0] == 0x11 {
            self.device.is_bluetooth = true;
        }
        
        // Use profile-based mapping
        self.parse_with_profile(data, profile, events);
        
        // For DualShock controllers, try to extract touchpad data
        if self.device.is_dualshock {
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
    
    // Extract touchpad data from DualShock 4 controllers
    fn extract_touchpad_data(&mut self, data: &[u8], events: &mut Vec<ControllerEvent>) {
        if !self.device.is_dualshock || data.len() < 10 {
            return;
        }
        
        // Different offsets based on connection type
        let touchpad_offset = if self.device.is_bluetooth { 35 } else { 33 };
        
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
        
        // Try to read data from the device
        match self.read_device_data() {
            Ok(Some(data)) => {
                // We got data, parse it
                self.parse_report(&data, &mut events);
            },
            Ok(None) => {
                // No data available, that's fine
            },
            Err(e) => {
                return Err(format!("Error reading from controller: {}", e).into());
            }
        }
        
        Ok(events)
    }
    
    fn get_device_info(&self) -> DeviceInfo {
        self.device.device_info.clone()
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }
    
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}