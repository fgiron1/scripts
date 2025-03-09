use midir::{MidiOutput, MidiOutputConnection};
use std::error::Error;

pub struct MidiSender {
    connection: MidiOutputConnection,
}

impl MidiSender {
    pub fn new(port_name: &str) -> Result<Self, Box<dyn Error>> {
        let midi_out = MidiOutput::new("PS4 MIDI Mapper")?;
        
        // Find port by name or use first available
        let ports = midi_out.ports();
        let port = ports.iter()
            .find(|p| midi_out.port_name(p)
                .map(|name| name.contains(port_name))
                .unwrap_or(false))
            .or_else(|| ports.first())
            .ok_or("No MIDI output ports available")?;

        let connection = midi_out.connect(port, "ps4-midi")?;
        Ok(Self { connection })
    }

    pub fn send_control_change(&mut self, channel: u8, control: u8, value: u8) -> Result<(), Box<dyn Error>> {
        let msg = [0xB0 | (channel & 0x0F), control, value];
        self.connection.send(&msg)?;
        Ok(())
    }

    pub fn send_note_on(&mut self, channel: u8, note: u8, velocity: u8) -> Result<(), Box<dyn Error>> {
        let msg = [0x90 | (channel & 0x0F), note, velocity];
        self.connection.send(&msg)?;
        Ok(())
    }

    pub fn send_note_off(&mut self, channel: u8, note: u8) -> Result<(), Box<dyn Error>> {
        // Some devices prefer proper note-off messages instead of zero-velocity note-ons
        let msg = [0x80 | (channel & 0x0F), note, 0];
        self.connection.send(&msg)?;
        Ok(())
    }
}