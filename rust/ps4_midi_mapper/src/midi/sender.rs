use midir::{MidiOutput, MidiOutputConnection};
use std::error::Error;

/// Provides a simple interface for sending MIDI messages
pub struct MidiSender {
    connection: MidiOutputConnection,
}

impl MidiSender {
    /// Create a new MIDI sender
    /// 
    /// Attempts to connect to a MIDI output port containing the given port name substring.
    /// If multiple ports match, connects to the first one found.
    pub fn new(port_name_contains: &str) -> Result<Self, Box<dyn Error>> {
        let midi_out = MidiOutput::new("PS4 MIDI Mapper")?;
        let ports = midi_out.ports();
        
        // Try to find a port with the given name
        let port = if port_name_contains.is_empty() {
            // If no name is specified, use the first available port
            ports.get(0).ok_or("No MIDI output ports available")?
        } else {
            // Otherwise find a port containing the given name
            ports.iter()
                .find(|p| midi_out.port_name(p).map(|n| n.contains(port_name_contains)).unwrap_or(false))
                .ok_or(format!("No MIDI output port containing '{}' found", port_name_contains))?
        };
        
        let connection = midi_out.connect(port, "ps4-midi-mapper")?;
        
        Ok(Self { connection })
    }
    
    /// Send a note on message (0 velocity means note off)
    pub fn send_note(&mut self, channel: u8, note: u8, velocity: u8) -> Result<(), Box<dyn Error>> {
        let channel = channel & 0x0F; // Ensure channel is 0-15
        
        // Velocity 0 is functionally equivalent to note off
        let status_byte = if velocity == 0 { 0x80 } else { 0x90 };
        
        self.connection.send(&[status_byte | channel, note, velocity])?;
        Ok(())
    }
    
    /// Send a control change message
    pub fn send_control_change(&mut self, channel: u8, controller: u8, value: u8) -> Result<(), Box<dyn Error>> {
        let channel = channel & 0x0F; // Ensure channel is 0-15
        self.connection.send(&[0xB0 | channel, controller, value])?;
        Ok(())
    }
    
    /// List available MIDI output ports
    pub fn list_ports() -> Result<Vec<String>, Box<dyn Error>> {
        let midi_out = MidiOutput::new("PS4 MIDI Mapper")?;
        let mut port_names = Vec::new();
        
        for port in midi_out.ports() {
            if let Ok(name) = midi_out.port_name(&port) {
                port_names.push(name);
            }
        }
        
        Ok(port_names)
    }
}