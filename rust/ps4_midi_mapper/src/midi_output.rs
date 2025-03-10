use midir::{MidiOutput, MidiOutputConnection};
use std::error::Error;

/// Handles MIDI output with automatic port discovery and error handling
pub struct MidiSender {
    connection: MidiOutputConnection,
}

impl MidiSender {
    pub fn new(port_name: &str) -> Result<Self, Box<dyn Error>> {
        let midi_out = MidiOutput::new("PS4 MIDI Mapper")?;
        let ports = midi_out.ports();
        
        let port = ports.iter()
            .find(|p| {
                midi_out.port_name(p)
                    .map(|name| name.contains(port_name))
                    .unwrap_or(false)
            })
            .ok_or("No matching MIDI ports found")?;
            
        Ok(Self {
            connection: midi_out.connect(port, "ps4-midi")?
        })
    }
    /// Send Note On message (channel 0-15, note 0-127, velocity 0-127)
    pub fn send_note_on(&mut self, channel: u8, note: u8, velocity: u8) -> Result<(), Box<dyn Error>> {
        let status = 0x90 | (channel & 0x0F);  // Note On status byte
        self.connection.send(&[status, note, velocity])?;
        Ok(())
    }

    /// Send Note Off message (channel 0-15, note 0-127)
    pub fn send_note_off(&mut self, channel: u8, note: u8) -> Result<(), Box<dyn Error>> {
        let status = 0x80 | (channel & 0x0F);  // Note Off status byte
        self.connection.send(&[status, note, 0])?;  // Velocity 0 for Note Off
        Ok(())
    }

    /// Send Control Change message (channel 0-15, control 0-127, value 0-127)
    pub fn send_control_change(&mut self, channel: u8, control: u8, value: u8) -> Result<(), Box<dyn Error>> {
        let status = 0xB0 | (channel & 0x0F);  // Control Change status byte
        self.connection.send(&[status, control, value])?;
        Ok(())
    }

    /// Send Program Change message (channel 0-15, program 0-127)
    pub fn send_program_change(&mut self, channel: u8, program: u8) -> Result<(), Box<dyn Error>> {
        let status = 0xC0 | (channel & 0x0F);  // Program Change status byte
        self.connection.send(&[status, program])?;
        Ok(())
    }
}