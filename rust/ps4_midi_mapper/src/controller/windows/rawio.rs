use std::any::Any;
use std::collections::HashMap;
use std::error::Error;
use std::mem::size_of;
use std::time::Duration;

use windows::Win32::Foundation::{HANDLE, HWND, INVALID_HANDLE_VALUE, CloseHandle, GetLastError, FALSE, TRUE, ERROR_DEVICE_NOT_CONNECTED, ERROR_IO_PENDING, WAIT_EVENT};
use windows::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, FILE_FLAG_OVERLAPPED,ReadFile};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject, ResetEvent};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::core::PCWSTR;

use windows::Win32::Devices::HumanInterfaceDevice::{
    HidD_GetHidGuid, HidD_GetProductString, HidD_GetManufacturerString,
    HidD_FreePreparsedData, HidD_GetPreparsedData, HidD_GetAttributes, HidD_SetNumInputBuffers,
    HidP_GetCaps, HidP_GetValueCaps, HIDD_ATTRIBUTES, HIDP_CAPS, HIDP_VALUE_CAPS,
    HIDP_STATUS_SUCCESS, PHIDP_PREPARSED_DATA, HIDP_REPORT_TYPE
};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces,
    SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
    HDEVINFO, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W
};

use crate::controller::{Controller, types::{ControllerEvent, Button, Axis, DeviceInfo}};
use crate::controller::profiles::{ControllerProfile, get_profile_for_device, create_profiles};
use std::sync::atomic::{AtomicUsize, Ordering};
use crate::controller::{self, create_controller};

// Constants
const DS4_TOUCHPAD_X_MAX: i32 = 1920;
const DS4_TOUCHPAD_Y_MAX: i32 = 942;
const TOUCHPAD_MIN_CHANGE: i32 = 5;
const WAIT_TIMEOUT: u32 = 258;
const WAIT_OBJECT_0: u32 = 0;
const READ_TIMEOUT_MS: u32 = 2;
const INPUT_BUFFER_SIZE: u32 = 64;

struct ControllerDevice {
    handle: HANDLE,
    device_info: DeviceInfo,
    preparsed_data: PHIDP_PREPARSED_DATA,
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
            if self.preparsed_data != PHIDP_PREPARSED_DATA::default() {
                HidD_FreePreparsedData(self.preparsed_data);
                self.preparsed_data = PHIDP_PREPARSED_DATA::default();
            }
            
            if self.read_event != INVALID_HANDLE_VALUE {
                CloseHandle(self.read_event);
                self.read_event = INVALID_HANDLE_VALUE;
            }
            
            if self.handle != INVALID_HANDLE_VALUE {
                CloseHandle(self.handle);
                self.handle = INVALID_HANDLE_VALUE;
            }
        }
    }
}

impl Clone for ControllerDevice {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle,
            device_info: self.device_info.clone(),
            preparsed_data: self.preparsed_data,
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

// Make ControllerDevice thread-safe
unsafe impl Send for ControllerDevice {}

pub struct WindowsRawIOController {
    device: ControllerDevice,
    button_states: HashMap<Button, bool>,
    axis_values: HashMap<Axis, f32>,
    touchpad_active: bool,
    touchpad_last_x: i32,
    touchpad_last_y: i32,
    debug_mode: bool,
    profile: Option<&'static ControllerProfile>,
}

impl WindowsRawIOController {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        // Find controller
        let device = Self::find_controller()?;
        
        // Pre-allocate hashmaps with capacity for lower latency
        let button_states = HashMap::with_capacity(20);
        let axis_values = HashMap::with_capacity(10);
        
        // Create controller instance
        let mut controller = WindowsRawIOController {
            button_states,
            axis_values,
            touchpad_active: false,
            touchpad_last_x: 0,
            touchpad_last_y: 0,
            debug_mode: false,
            device,
            profile: None,
        };
        
        // Cache the profile for faster lookups
        controller.profile = Some(controller.get_controller_profile());
        
        println!("Connected to controller: {} (VID: 0x{:04X}, PID: 0x{:04X})", 
                 controller.device.device_info.product, 
                 controller.device.device_info.vid, 
                 controller.device.device_info.pid);
        
        Ok(controller)
    }
    
    fn find_controller() -> Result<ControllerDevice, Box<dyn Error>> {
        println!("Searching for game controllers...");
        
        // Get HID GUID
        let hid_guid = unsafe { HidD_GetHidGuid() };
        
        // Get device interface list
        let device_info_set = match unsafe {
            SetupDiGetClassDevsW(
                Some(&hid_guid),
                PCWSTR::null(),
                HWND::default(),
                DIGCF_DEVICEINTERFACE | DIGCF_PRESENT,
            )
        } {
            Ok(info_set) => info_set,
            Err(e) => return Err(format!("Failed to get device information set: {}", e).into())
        };
    
        // Ensure cleanup of the device info set when done
        struct DeviceInfoSetCleanup(HDEVINFO);
        impl Drop for DeviceInfoSetCleanup {
            fn drop(&mut self) {
                unsafe { SetupDiDestroyDeviceInfoList(self.0); }
            }
        }
        let _cleanup = DeviceInfoSetCleanup(device_info_set);
        
        // Prepare to enumerate devices
        let mut device_interface_data = SP_DEVICE_INTERFACE_DATA {
            cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };
        
        let mut found_devices = Vec::new();
        let mut device_index = 0;
        
        // Loop through devices with error handling
        loop {
            // More robust error handling for device enumeration
            let enum_result = unsafe { 
                SetupDiEnumDeviceInterfaces(
                    device_info_set,
                    None,
                    &hid_guid,
                    device_index,
                    &mut device_interface_data,
                ) 
            };
    
            match enum_result {
                Ok(_) => {
                    // Continue with this device
                    if let Some(device) = Self::process_device_interface(device_info_set, &device_interface_data) {
                        found_devices.push(device);
                    }
                },
                Err(e) => {
                    // More detailed error checking
                    let error_code = e.code().0;
                    match error_code as i64 {
                        0x80070103 => {
                            // No more data is available - this is expected when we've enumerated all devices
                            println!("Finished device enumeration.");
                            break;
                        },
                        0x80070057 => {
                            // Invalid parameter - sometimes occurs during enumeration
                            println!("Invalid parameter during device enumeration. Continuing...");
                            break;
                        },
                        _ => {
                            println!("Unexpected error during device enumeration: 0x{:08X}", error_code);
                            break;
                        }
                    }
                }
            }
            
            device_index += 1;
    
            // Prevent infinite loop with a reasonable device limit
            if device_index > 1000 {
                println!("Exceeded maximum device enumeration limit.");
                break;
            }
        }
        
        // Added more logging for device detection
        println!("Total compatible devices found: {}", found_devices.len());
        
        // Find the best device
        Self::select_best_controller(found_devices)
    }

    fn process_device_interface(
        device_info_set: HDEVINFO, 
        device_interface_data: &SP_DEVICE_INTERFACE_DATA
    ) -> Option<ControllerDevice> {
        // Allocate larger buffer to prevent truncation
        let mut required_size = 0u32;
        let mut device_interface_detail_data = vec![0u8; 1024]; // Increased buffer size
        let p_device_interface_detail_data = device_interface_detail_data.as_mut_ptr() 
            as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
        
        // Set the cbSize field correctly
        unsafe {
            (*p_device_interface_detail_data).cbSize = if cfg!(target_pointer_width = "64") {
                8 // 64-bit size
            } else {
                6 // 32-bit size
            };
        }
        
        // Get device interface detail with larger buffer
        let detail_result = unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                device_info_set,
                device_interface_data,
                Some(p_device_interface_detail_data),
                device_interface_detail_data.len() as u32,
                Some(&mut required_size),
                None,
            )
        };
        
        if detail_result.is_err() {
            let error_code = unsafe { GetLastError() };
            println!("Device interface detail error: 0x{:08X}", error_code.0);
            return None;
        }
        
        // Get the device path
        let device_path = unsafe {
            PCWSTR((*p_device_interface_detail_data).DevicePath.as_ptr())
                .to_string()
                .unwrap_or_else(|_| "Unknown".to_string())
        };
        
        // Try to open the device
        let device_handle = match unsafe {
            CreateFileW(
                PCWSTR(device_path.encode_utf16().chain(Some(0)).collect::<Vec<_>>().as_ptr()),
                0x80000000,  // GENERIC_READ
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                None
            )
        } {
            Ok(handle) => {
                if handle.is_invalid() {
                    println!("Invalid device handle");
                    return None;
                }
                handle
            },
            Err(e) => {
                let error_code = e.code().0 as u32;
                match error_code {
                    0x80070005 => {
                        println!("Access denied for device. Requires administrator privileges or special handling.");
                        println!("Device path: {}", device_path);
                    },
                    0x80070020 => {
                        println!("Device is being used by another process. Path: {}", device_path);
                    },
                    _ => {
                        println!("Could not open device. Error: 0x{:08X}, Path: {}", error_code, device_path);
                    }
                }
                return None;
            }
        };
    
        // Rest of the initialization remains the same as in the original implementation
        Self::initialize_hid_device(device_handle)
    }

    fn initialize_hid_device(device_handle: HANDLE) -> Option<ControllerDevice> {
        // Get device attributes
        let mut attrs = HIDD_ATTRIBUTES {
            Size: size_of::<HIDD_ATTRIBUTES>() as u32,
            ..Default::default()
        };
        
        if !unsafe { HidD_GetAttributes(device_handle, &mut attrs) }.as_bool() {
            unsafe { CloseHandle(device_handle) };
            return None;
        }
        
        // Get device strings
        let (product, manufacturer) = Self::get_device_strings(device_handle);
        
        // Check if it's a controller
        let is_controller = product.to_lowercase().contains("controller") || 
                           product.to_lowercase().contains("gamepad") ||
                           product.to_lowercase().contains("dualshock") ||
                           product.to_lowercase().contains("xbox");
        
        if !is_controller {
            unsafe { CloseHandle(device_handle) };
            return None;
        }
        
        // Get device capabilities
        let mut preparsed_data = PHIDP_PREPARSED_DATA::default();
        if !unsafe { HidD_GetPreparsedData(device_handle, &mut preparsed_data) }.as_bool() || 
           preparsed_data == PHIDP_PREPARSED_DATA::default() {
            unsafe { CloseHandle(device_handle) };
            return None;
        }
        
        let mut caps = HIDP_CAPS::default();
        if unsafe { HidP_GetCaps(preparsed_data, &mut caps) } != HIDP_STATUS_SUCCESS {
            unsafe {
                HidD_FreePreparsedData(preparsed_data);
                CloseHandle(device_handle);
            }
            return None;
        }
        
        // Get value capabilities
        let value_caps = Self::get_value_caps(preparsed_data, caps.NumberInputValueCaps);
        
        // Create read event
        let read_event_result = unsafe { 
            CreateEventW(
                None,
                TRUE,  
                FALSE, 
                PCWSTR::null()
            ) 
        };
        
        let read_event = match read_event_result {
            Ok(event) => event,
            Err(_) => {
                unsafe {
                    HidD_FreePreparsedData(preparsed_data);
                    CloseHandle(device_handle);
                }
                return None;
            }
        };
        
        if read_event.is_invalid() {
            unsafe {
                HidD_FreePreparsedData(preparsed_data);
                CloseHandle(device_handle);
            }
            return None;
        }
        
        // Create overlapped structure
        let mut read_overlapped = OVERLAPPED::default();
        read_overlapped.hEvent = read_event;
        
        // Set buffer size for better throughput
        unsafe { HidD_SetNumInputBuffers(device_handle, INPUT_BUFFER_SIZE) };
        
        // Check if it's a DualShock controller
        let is_dualshock = product.to_lowercase().contains("dualshock") ||
                          (product.to_lowercase().contains("wireless controller") && 
                           manufacturer.to_lowercase().contains("sony"));
                          
        // Check if it's Bluetooth connected
        let is_bluetooth = is_dualshock && 
                         (attrs.ProductID == 0x05C5 || // DS4 v1 Bluetooth
                          attrs.ProductID == 0x09C2);  // DS4 v2 Bluetooth
        
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
        
        Some(ControllerDevice {
            handle: device_handle,
            device_info,
            preparsed_data,
            value_caps,
            read_buffer,
            read_event,
            read_overlapped,
            report_length,
            is_dualshock,
            is_bluetooth,
        })
    }
    
    fn get_device_strings(device_handle: HANDLE) -> (String, String) {
        let mut product_buffer = [0u16; 128];
        let mut manufacturer_buffer = [0u16; 128];
        
        let has_product = unsafe {
            HidD_GetProductString(
                device_handle, 
                product_buffer.as_mut_ptr() as _, 
                (product_buffer.len() * size_of::<u16>()) as u32
            )
        }.as_bool();
        
        let has_manufacturer = unsafe {
            HidD_GetManufacturerString(
                device_handle, 
                manufacturer_buffer.as_mut_ptr() as _, 
                (manufacturer_buffer.len() * size_of::<u16>()) as u32
            )
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
        
        (product, manufacturer)
    }
    
    fn get_value_caps(preparsed_data: PHIDP_PREPARSED_DATA, count: u16) -> Vec<HIDP_VALUE_CAPS> {
        let mut value_caps = Vec::new();
        
        if count > 0 {
            let mut value_caps_buffer = vec![HIDP_VALUE_CAPS::default(); count as usize];
            let mut value_caps_length = count;
            
            let value_caps_result = unsafe {
                HidP_GetValueCaps(
                    HIDP_REPORT_TYPE(0),  // HidP_Input
                    value_caps_buffer.as_mut_ptr(),
                    &mut value_caps_length,
                    preparsed_data,
                )
            };
            
            if value_caps_result == HIDP_STATUS_SUCCESS {
                value_caps = value_caps_buffer[0..value_caps_length as usize].to_vec();
            }
        }
        
        value_caps
    }
    
    fn select_best_controller(found_devices: Vec<ControllerDevice>) -> Result<ControllerDevice, Box<dyn Error>> {
        if found_devices.is_empty() {
            return Err("No compatible controller found".into());
        }
        
        // First priority: DualShock 4 controllers
        for device in &found_devices {
            if device.is_dualshock &&
               (device.device_info.pid == 0x05C4 || // DS4 v1
                device.device_info.pid == 0x09CC || // DS4 v2
                device.device_info.pid == 0x05C5 || // DS4 v1 Bluetooth
                device.device_info.pid == 0x09C2) { // DS4 v2 Bluetooth
                
                return Ok(device.clone());
            }
        }
        
        // Second priority: Any Xbox controller
        for device in &found_devices {
            if device.device_info.product.to_lowercase().contains("xbox") {
                return Ok(device.clone());
            }
        }
        
        // Last resort: First controller in the list
        Ok(found_devices[0].clone())
    }

    // Get the controller profile
    fn get_controller_profile(&self) -> &'static ControllerProfile {
        // Use cached profile if available
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
    
    // Read data from the device - optimized for low latency
    fn read_device_data(&mut self) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        unsafe {
            // Reset the event
            ResetEvent(self.device.read_event);
            
            // Reset the overlapped structure - critical for reliability
            self.device.read_overlapped.Internal = 0;
            self.device.read_overlapped.InternalHigh = 0;
            self.device.read_overlapped.Anonymous.Anonymous.Offset = 0;
            self.device.read_overlapped.Anonymous.Anonymous.OffsetHigh = 0;
            self.device.read_overlapped.hEvent = self.device.read_event;
            
            let mut bytes_read = 0u32;
            
            // Start the read operation
            match ReadFile(
                self.device.handle,
                Some(&mut self.device.read_buffer),
                Some(&mut bytes_read),
                Some(&mut self.device.read_overlapped),
            ) {
                Ok(_) => {
                    // Immediate success
                    if bytes_read > 0 {
                        return Ok(Some(self.device.read_buffer[0..bytes_read as usize].to_vec()));
                    }
                    return Ok(None);
                },
                Err(e) => {
                    // Check if pending
                    let error = GetLastError();
                    if error != ERROR_IO_PENDING {
                        return Err(format!("Error reading from device: {}", e).into());
                    }
                    
                    // Wait with short timeout for lowest latency
                    let wait_result = WaitForSingleObject(self.device.read_event, READ_TIMEOUT_MS);
                    
                    if wait_result == WAIT_EVENT(WAIT_TIMEOUT) {
                        // No data ready yet, cancel and return
                        CancelIoEx(self.device.handle, Some(&self.device.read_overlapped));
                        return Ok(None);
                    }
                    
                    if wait_result != WAIT_EVENT(WAIT_OBJECT_0) {
                        return Err(format!("Wait error: {}", GetLastError().0).into());
                    }
                    
                    // Get the result
                    match GetOverlappedResult(
                        self.device.handle,
                        &mut self.device.read_overlapped,
                        &mut bytes_read,
                        FALSE,
                    ) {
                        Ok(_) => {
                            if bytes_read > 0 {
                                return Ok(Some(self.device.read_buffer[0..bytes_read as usize].to_vec()));
                            }
                        },
                        Err(e) => {
                            let error = GetLastError();
                            if error == ERROR_DEVICE_NOT_CONNECTED {
                                return Err("Controller disconnected".into());
                            }
                            return Err(format!("Error getting read result: {}", e).into());
                        }
                    }
                }
            }
            
            Ok(None)
        }
    }

    // Parse the raw data from the device - optimized for speed
    fn parse_report(&mut self, data: &[u8], events: &mut Vec<ControllerEvent>) {
        // First data received - detect Bluetooth mode for DS4
        if self.device.is_dualshock && !self.device.is_bluetooth && data[0] == 0x11 {
            self.device.is_bluetooth = true;
        }
        
        // Get the cached profile
        let profile = self.get_controller_profile();
        
        // Process buttons and axes
        self.parse_with_profile(data, profile, events);
        
        // For DualShock controllers, try to extract touchpad data
        if self.device.is_dualshock {
            self.extract_touchpad_data(data, events);
        }
    }
    
    // Profile-based input parsing
    fn parse_with_profile(&mut self, data: &[u8], profile: &ControllerProfile, events: &mut Vec<ControllerEvent>) {
        // Process buttons based on the profile's button map
        for (code, button) in &profile.button_map {
            let byte_index = (code >> 8) as usize;
            if byte_index < data.len() {
                let bit_mask = (*code & 0xFF) as u8;
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
        
        // Process D-pad
        match &profile.dpad_type {
            crate::controller::profiles::DpadType::Hat { byte_index, mask_values } => {
                if *byte_index < data.len() {
                    let hat_value = data[*byte_index];
                    if let Some(buttons) = mask_values.get(&hat_value) {
                        // Process active buttons
                        for button in buttons {
                            self.update_button(*button, true, events);
                        }
                        
                        // Release inactive buttons
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
                // Handled by button map
            },
            crate::controller::profiles::DpadType::Axes { x_axis, y_axis } => {
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
        
        // Check if enough data
        if data.len() <= touchpad_offset + 4 {
            return;
        }
        
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
    
    // Update button state - only generate events on changes
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
    
    // Update axis value - apply thresholds to reduce MIDI traffic
    fn update_axis(&mut self, axis: Axis, value: f32, events: &mut Vec<ControllerEvent>) {
        let previous = self.axis_values.get(&axis).copied().unwrap_or(0.0);
        
        // Use appropriate sensitivity threshold based on axis type
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
                println!("Touchpad: X={}, Y={}", x, y);
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
                println!("Touchpad: released");
            }
        }
    }
    
    // Enable debug mode
    pub fn enable_debug(&mut self) {
        self.debug_mode = true;
        println!("Debug mode enabled");
    }
}

impl Controller for WindowsRawIOController {
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn Error>> {
        static CONSECUTIVE_ERRORS: AtomicUsize = AtomicUsize::new(0);
        
        let mut events = Vec::with_capacity(10);
        
        match self.read_device_data() {
            Ok(Some(data)) => {
                CONSECUTIVE_ERRORS.store(0, Ordering::Relaxed);
                self.parse_report(&data, &mut events);
            },
            Ok(None) => {
                return Ok(events);
            },
            Err(e) => {
                let error_count = CONSECUTIVE_ERRORS.fetch_add(1, Ordering::Relaxed) + 1;
                
                println!("Controller read error (attempt {}): {}", error_count, e);
                
                if error_count > 5 {
                    println!("Attempting to reconnect controller...");
                    
                    // Instead of storing the new controller, just return an error
                    return Err("Controller disconnected. Please reconnect.".into());
                }
                
                std::thread::sleep(Duration::from_millis(50));
                return Ok(events);
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