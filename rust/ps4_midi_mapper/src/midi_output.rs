use midir::{MidiOutput, MidiOutputConnection};
use std::error::Error;

/// Handles MIDI output with automatic port discovery and error handling
pub struct MidiSender {
    connection: midir::MidiOutputConnection,
}

impl MidiSender {
    pub fn new(port_name: &str) -> Result<Self, Box<dyn Error>> {
        let midi_out = midir::MidiOutput::new("PS4 MIDI Mapper")?;
        let ports = midi_out.ports();
        let port = ports.iter()
            .find(|p| midi_out.port_name(p)
                .map(|name| name.contains(port_name))
                .unwrap_or(false))
            .ok_or("No MIDI ports found")?;
            
        Ok(Self {
            connection: midi_out.connect(port, "ps4-midi")?
        })
    }
}