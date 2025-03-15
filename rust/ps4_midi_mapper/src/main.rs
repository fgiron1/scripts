mod config;
mod controller;
mod midi;
mod mapper;

use std::error::Error;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    // Display app header
    println!("PS4/Controller MIDI Mapper");
    println!("==========================");
    println!("Version 0.1.0");
    
    #[cfg(target_os = "windows")]
    println!("Platform: Windows");
    #[cfg(target_os = "linux")]
    println!("Platform: Linux");
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    println!("Platform: Unsupported");
    
    // List available MIDI ports
    println!("\nAvailable MIDI ports:");
    match midi::MidiSender::list_ports() {
        Ok(ports) => {
            if ports.is_empty() {
                println!("  No MIDI ports found. Please connect a MIDI device.");
                println!("\nPress Enter to exit...");
                let _ = io::stdin().read_line(&mut String::new());
                return Err("No MIDI ports available".into());
            }
            
            for (i, port) in ports.iter().enumerate() {
                println!("  {}. {}", i + 1, port);
            }
        },
        Err(e) => {
            eprintln!("Error listing MIDI ports: {}", e);
            println!("\nPress Enter to exit...");
            let _ = io::stdin().read_line(&mut String::new());
            return Err(e);
        }
    }
    
    // Attempt to create a MIDI mapper instance
    println!("\nSearching for controllers...");
    io::stdout().flush()?;
    
    // Create a MIDI mapper instance
    let mut mapper = match mapper::MidiMapper::new() {
        Ok(mapper) => {
            println!("Controller connected successfully!");
            mapper
        },
        Err(e) => {
            eprintln!("\nError: Could not find or connect to a compatible controller.");
            eprintln!("Details: {}", e);
            eprintln!("\nPlease make sure your controller is connected and try again.");
            println!("\nThe program will now wait for a controller to be connected.");
            println!("You can press Ctrl+C at any time to exit.");
            
            // Wait for a controller to be connected
            let mut attempts = 0;
            let max_attempts = 30; // Try for about 30 seconds
            
            println!("Polling for controller: ");
            print!("  "); // Initial indent for the progress bar
            io::stdout().flush()?;
            
            let mut connected_mapper = None;
            
            while attempts < max_attempts {
                print!(".");
                io::stdout().flush()?;
                thread::sleep(Duration::from_secs(1));
                attempts += 1;
                
                // Suppress error output during polling
                if let Ok(m) = mapper::MidiMapper::new() {
                    println!("\nController connected successfully!");
                    connected_mapper = Some(m);
                    break;
                }
                
                // Add a new line every 10 dots to keep the output clean
                if attempts % 10 == 0 && attempts < max_attempts {
                    println!("");
                    print!("  "); // Indent for the new line
                    io::stdout().flush()?;
                }
            }
            
            // Check if we connected a mapper during polling
            if let Some(m) = connected_mapper {
                m
            } else {
                println!("\nTimed out waiting for controller connection.");
                println!("\nPress Enter to exit...");
                let _ = io::stdin().read_line(&mut String::new());
                return Err("No controller connected after timeout".into());
            }
        }
    };
    
    // Display controller information
    let device_info = mapper.controller.get_device_info();
    println!("\nConnected controller: {} ({:04X}:{:04X})", 
        device_info.product, device_info.vid, device_info.pid);
    println!("Manufacturer: {}", device_info.manufacturer);
    
    // Display mapping information
    println!("\nButton mappings:");
    for mapping in config::BUTTON_MAPPINGS {
        println!("  {:10} -> MIDI Note {}", 
            format!("{:?}", mapping.button), mapping.note);
    }
    
    println!("\nAxis mappings:");
    for mapping in config::AXIS_MAPPINGS {
        println!("  {:12} -> MIDI CC {}", 
            format!("{:?}", mapping.axis), mapping.cc);
    }
    
    println!("\nControls are now being mapped to MIDI. Any controller input will be sent to your MIDI device.");
    println!("Press Ctrl+C to exit.");
    
    // Run the mapper
    match mapper.run() {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("\nError during mapping: {}", e);
            println!("\nPress Enter to exit...");
            let _ = io::stdin().read_line(&mut String::new());
            Err(e)
        }
    }
}