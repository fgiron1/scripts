use std::io::{self, Write};
use std::error::Error;
use std::mem::size_of;
use std::time::Duration;
use std::thread;

use windows::core::PCWSTR;
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces,
    SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
    HDEVINFO, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W
};
use windows::Win32::Devices::HumanInterfaceDevice::{
    HidD_GetHidGuid, HidD_GetProductString, HidD_GetManufacturerString,
    HidD_GetAttributes, HIDD_ATTRIBUTES, 
    HidD_GetPreparsedData, HidD_FreePreparsedData,
    HidP_GetCaps, HIDP_CAPS, PHIDP_PREPARSED_DATA
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, 
    ReadFile
};
use windows::Win32::Foundation::{
    HANDLE, GENERIC_READ, GENERIC_WRITE, 
    FALSE, GetLastError, NTSTATUS
};

// Constants for display
const RESET: &str = "\x1B[0m";
const YELLOW: &str = "\x1B[33m";
const GREEN: &str = "\x1B[32m";
const CYAN: &str = "\x1B[36m";

// Constants for monitoring
const MONITOR_TIMEOUT_MS: u32 = 10;

/// Main function
fn main() -> Result<(), Box<dyn Error>> {
    println!("\n{}===== Windows HID Device Monitoring Tool ====={}",
             GREEN, RESET);
    println!("This tool helps inspect low-level HID device data");
    
    // Get available devices
    let devices = enumerate_hid_devices()?;
    
    // Display devices
    println!("\nAvailable HID Devices:");
    for (i, device) in devices.iter().enumerate() {
        println!("{}: {} (VID: 0x{:04X}, PID: 0x{:04X})", 
                 i, device.product, device.vid, device.pid);
    }
    
    // Select device
    print!("\nEnter device index to monitor: ");
    io::stdout().flush()?;
    
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    let device_index = input.trim().parse::<usize>()?;
    let selected_device = &devices[device_index];
    
    // Monitor selected device
    monitor_device(selected_device)?;
    
    Ok(())
}

/// Device information structure
#[derive(Debug)]
struct DeviceInfo {
    vid: u16,
    pid: u16,
    product: String,
    manufacturer: String,
    device_path: String,
}

/// Enumerate HID devices
fn enumerate_hid_devices() -> Result<Vec<DeviceInfo>, Box<dyn Error>> {
    let hid_guid = unsafe { HidD_GetHidGuid() };
    
    let device_info_set = unsafe {
        SetupDiGetClassDevsW(
            Some(&hid_guid),
            PCWSTR::null(),
            None,
            DIGCF_DEVICEINTERFACE | DIGCF_PRESENT
        )?
    };
    
    let mut devices = Vec::new();
    let mut device_interface_data = SP_DEVICE_INTERFACE_DATA {
        cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
        ..Default::default()
    };
    
    let mut device_index = 0;
    
    loop {
        match unsafe { 
            SetupDiEnumDeviceInterfaces(
                device_info_set,
                None,
                &hid_guid,
                device_index,
                &mut device_interface_data,
            ) 
        } {
            Ok(_) => {
                let device_path = get_device_path(device_info_set, &device_interface_data)?;
                
                if let Some(device_info) = get_device_details(&device_path) {
                    devices.push(device_info);
                }
            },
            Err(_) => break,
        }
        
        device_index += 1;
    }
    
    unsafe { SetupDiDestroyDeviceInfoList(device_info_set); }
    
    Ok(devices)
}

/// Get device path
fn get_device_path(
    device_info_set: HDEVINFO, 
    device_interface_data: &SP_DEVICE_INTERFACE_DATA
) -> Result<String, Box<dyn Error>> {
    let mut required_size = 0u32;
    
    unsafe {
        SetupDiGetDeviceInterfaceDetailW(
            device_info_set,
            device_interface_data,
            None,
            0,
            Some(&mut required_size),
            None,
        )
    };
    
    let mut device_interface_detail_data = vec![0u8; required_size as usize];
    let p_detail_data = device_interface_detail_data.as_mut_ptr() 
        as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
    
    unsafe {
        (*p_detail_data).cbSize = if cfg!(target_pointer_width = "64") { 8 } else { 6 };
    }
    
    unsafe {
        SetupDiGetDeviceInterfaceDetailW(
            device_info_set,
            device_interface_data,
            Some(p_detail_data),
            required_size,
            Some(&mut required_size),
            None,
        )?
    };
    
    let device_path = unsafe {
        PCWSTR((*p_detail_data).DevicePath.as_ptr())
            .to_string()
            .unwrap_or_else(|_| "Unknown".to_string())
    };
    
    Ok(device_path)
}

/// Get device details
fn get_device_details(device_path: &str) -> Option<DeviceInfo> {
    let path_wide: Vec<u16> = device_path.encode_utf16().chain(Some(0)).collect();
    
    let device_handle = match unsafe {
        CreateFileW(
            PCWSTR(path_wide.as_ptr()),
            GENERIC_READ.0 | GENERIC_WRITE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            Default::default(),
            None
        )
    } {
        Ok(handle) => handle,
        Err(_) => return None,
    };
    
    let mut attrs = HIDD_ATTRIBUTES {
        Size: size_of::<HIDD_ATTRIBUTES>() as u32,
        ..Default::default()
    };
    
    let get_attrs_result = unsafe { 
        HidD_GetAttributes(device_handle, &mut attrs) 
    };
    
    if i32::from(get_attrs_result.0) == FALSE.0 {
        unsafe { let _ = windows::Win32::Foundation::CloseHandle(device_handle); }
        return None;
    }
    
    let product = get_hid_string(&device_handle, HidStringType::Product)
        .unwrap_or_else(|_| "Unknown".to_string());
    
    let manufacturer = get_hid_string(&device_handle, HidStringType::Manufacturer)
        .unwrap_or_else(|_| "Unknown".to_string());
    
    unsafe { 
        let _ = windows::Win32::Foundation::CloseHandle(device_handle); 
    }
    
    Some(DeviceInfo {
        vid: attrs.VendorID,
        pid: attrs.ProductID,
        product,
        manufacturer,
        device_path: device_path.to_string(),
    })
}

/// Enum to specify which string to retrieve from HID device
enum HidStringType {
    Product,
    Manufacturer,
}

/// Get string from HID device
fn get_hid_string(
    device_handle: &HANDLE, 
    string_type: HidStringType
) -> Result<String, Box<dyn Error>> {
    let mut buffer = [0u16; 256];
    
    let result = match string_type {
        HidStringType::Product => unsafe {
            HidD_GetProductString(
                *device_handle, 
                buffer.as_mut_ptr() as _, 
                (buffer.len() * size_of::<u16>()) as u32
            )
        },
        HidStringType::Manufacturer => unsafe {
            HidD_GetManufacturerString(
                *device_handle, 
                buffer.as_mut_ptr() as _, 
                (buffer.len() * size_of::<u16>()) as u32
            )
        },
    };
    
    if i32::from(result.0) == FALSE.0 {
        return Err("Could not retrieve device string".into());
    }
    
    let len = buffer.iter().position(|&x| x == 0).unwrap_or(buffer.len());
    Ok(String::from_utf16_lossy(&buffer[..len]))
}

/// Monitor a specific device
fn monitor_device(device: &DeviceInfo) -> Result<(), Box<dyn Error>> {
    println!("\n{}===== Monitoring Device: {} ====={}",
             GREEN, device.product, RESET);
    println!("VID: 0x{:04X}, PID: 0x{:04X}", device.vid, device.pid);
    println!("Path: {}", device.device_path);
    
    // Convert path to wide string
    let path_wide: Vec<u16> = device.device_path.encode_utf16().chain(Some(0)).collect();
    
    // Open device
    let device_handle = unsafe {
        CreateFileW(
            PCWSTR(path_wide.as_ptr()),
            GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            Default::default(),
            None
        )?
    };
    
    // Get preparsed data to determine report size
    let mut preparsed_data = PHIDP_PREPARSED_DATA::default();
    let preparsed_result = unsafe { 
        HidD_GetPreparsedData(device_handle, &mut preparsed_data) 
    };
    if i32::from(preparsed_result.0) == FALSE.0 {
        unsafe { let _ = windows::Win32::Foundation::CloseHandle(device_handle); }
        return Err("Could not get preparsed data".into());
    }
    
    // Get device capabilities
    let mut caps = HIDP_CAPS::default();
    let caps_result = unsafe { 
        HidP_GetCaps(preparsed_data, &mut caps) 
    };
    
    if caps_result != NTSTATUS(0) {  // Non-zero indicates an error
        unsafe { 
            let _ = HidD_FreePreparsedData(preparsed_data);
            let _ = windows::Win32::Foundation::CloseHandle(device_handle); 
        }
        return Err("Could not get device capabilities".into());
    }
    
    println!("\nDevice Capabilities:");
    println!("  Input Report Size: {} bytes", caps.InputReportByteLength);
    println!("  Output Report Size: {} bytes", caps.OutputReportByteLength);
    println!("  Feature Report Size: {} bytes", caps.FeatureReportByteLength);
    
    // Monitoring setup
    println!("\n{}Press Ctrl+C to stop monitoring.{}", YELLOW, RESET);
    
    let mut report_buffer = vec![0u8; caps.InputReportByteLength as usize];
    let mut report_counter = 0u32;
    let mut change_counter = 0u32;
    
    // Monitoring loop
    loop {
        // Read report
        let mut bytes_read = 0u32;
        let read_result = unsafe {
            ReadFile(
                device_handle,
                Some(&mut report_buffer),
                Some(&mut bytes_read),
                None
            )
        };
        
        // Check if read was successful and we read something
        if read_result.is_ok() && bytes_read > 0 {
            report_counter += 1;
            
            // Print raw data
            println!("\n{}Report #{} (Size: {}):{}", 
                     CYAN, report_counter, bytes_read, RESET);
            
            // Hex representation
            print!("HEX : ");
            for &byte in &report_buffer[..bytes_read as usize] {
                print!("{:02X} ", byte);
            }
            println!();
            
            // ASCII representation
            print!("ASCII: ");
            for &byte in &report_buffer[..bytes_read as usize] {
                let c = byte as char;
                print!("{} ", if c.is_ascii_graphic() { c } else { '.' });
            }
            println!();
            
            // Change detection
            if bytes_read as usize > 1 {
                change_counter += 1;
            }
            
            // Optional delay to prevent overwhelming output
            thread::sleep(Duration::from_millis(MONITOR_TIMEOUT_MS as u64));
        } else {
            // Check for errors
            let error = unsafe { GetLastError() };
            if error.0 != 0 {
                println!("Error reading device: {}", error.0);
                break;
            }
        }
    }
    
    // Cleanup
    unsafe { 
        let _ = HidD_FreePreparsedData(preparsed_data);
        let _ = windows::Win32::Foundation::CloseHandle(device_handle); 
    }
    
    Ok(())
}