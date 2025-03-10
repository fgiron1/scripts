mod config;
mod controller;
mod midi;
mod mapper;

use std::error::Error;
use std::process;

fn main() -> Result<(), Box<dyn Error>> {
    // Display app header
    println!("PS4/Controller MIDI Mapper");
    println!("==========================");
    
    // List available MIDI ports
    println!("\nAvailable MIDI output ports:");
    match midi::MidiSender::list_ports() {
        Ok(ports) => {
            if ports.is_empty() {
                println!("  No MIDI ports found. Please connect a MIDI device.");
                return Err("No MIDI ports available".into());
            }
            
            for (i, port) in ports.iter().enumerate() {
                println!("  {}. {}", i + 1, port);
            }
        },
        Err(e) => {
            eprintln!("Error listing MIDI ports: {}", e);
            return Err(e);
        }
    }
    
    // Create a MIDI mapper instance
    let mut mapper = match mapper::MidiMapper::new() {
        Ok(mapper) => mapper,
        Err(e) => {
            eprintln!("Error creating MIDI mapper: {}", e);
            process::exit(1);
        }
    };
    
    // Run the mapper
    println!("\nMIDI mapping started. Press Ctrl+C to exit.");
    mapper.run()
}