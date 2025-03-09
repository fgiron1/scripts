use midir::{MidiOutput, MidiOutputConnection};
use crate::controller::types::{Input, InputEvent};
use std::error::Error;

pub struct MidiMapper {
    connection: MidiOutputConnection,
}

impl MidiMapper {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let midi_out = MidiOutput::new("PS4 MIDI Mapper")?;
        let ports = midi_out.ports();
        let port = ports.first().ok_or("No MIDI ports")?;
        let connection = midi_out.connect(port, "ps4-midi")?;
        Ok(Self { connection })
    }

    pub fn send_event(&mut self, event: InputEvent) -> Result<(), Box<dyn Error>> {
        // Implement your MIDI mapping logic here
        Ok(())
    }
}

fn input_to_midi(input: Input, value: f32) -> Option<MidiEvent> {
    match input {
        Input::TrackpadX => Some(MidiEvent::ControlChange(0, 12, scale_value(value))),
        Input::TrackpadY => Some(MidiEvent::ControlChange(0, 13, scale_value(value))),
        Input::LeftTrigger => Some(MidiEvent::ControlChange(0, 14, scale_value(value))),
        Input::RightTrigger => Some(MidiEvent::ControlChange(0, 15, scale_value(value))),
        Input::LeftStickX => Some(MidiEvent::ControlChange(0, 16, scale_value(value))),
        Input::LeftStickY => Some(MidiEvent::ControlChange(0, 17, scale_value(value))),
        Input::RightStickX => Some(MidiEvent::ControlChange(0, 18, scale_value(value))),
        Input::RightStickY => Some(MidiEvent::ControlChange(0, 19, scale_value(value))),
        Input::Button(btn) => Some(MidiEvent::NoteOn(0, btn, if value > 0.0 { 127 } else { 0 })),
        _ => None,
    }
}

fn scale_value(value: f32) -> u8 {
    ((value.clamp(-1.0, 1.0) + 1.0) * 63.0) as u8
}

#[derive(Debug)]
enum MidiEvent {
    ControlChange(u8, u8, u8),
    NoteOn(u8, u8, u8),
}

impl MidiEvent {
    fn to_midi_message(&self) -> Vec<u8> {
        match self {
            MidiEvent::ControlChange(channel, control, value) => {
                vec![0xB0 | channel, *control, *value]
            }
            MidiEvent::NoteOn(channel, note, velocity) => {
                vec![0x90 | channel, *note, *velocity]
            }
        }
    }
}