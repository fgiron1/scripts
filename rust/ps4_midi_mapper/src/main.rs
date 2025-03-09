use std::error::Error;
use std::thread;
use std::time::Duration;

mod device_registry;
mod controllers;
mod midi_output;
mod platform;

use crate::{
    device_registry::{Controller, ControllerEvent, Axis, Button, DeviceRegistry},
    midi_output::MidiSender,
    controllers::register_controllers
};

// Configuration
const MIDI_PORT_NAME: &str = "PS4 Port";
const MIDI_CHANNEL: u8 = 0;
const JOYSTICK_DEADZONE: f32 = 0.2;

// Control mappings
const BUTTON_MAPPINGS: [(Button, u8); 15] = [
    (Button::Cross, 36),
    (Button::Circle, 37),
    (Button::Triangle, 38),
    (Button::Square, 39),
    (Button::L1, 40),
    (Button::R1, 41),
    (Button::Share, 44),
    (Button::Options, 45),
    (Button::PS, 46),
    (Button::L3, 47),
    (Button::R3, 48),
    (Button::DpadUp, 49),
    (Button::DpadDown, 50),
    (Button::DpadLeft, 51),
    (Button::DpadRight, 52),
];

const AXIS_MAPPINGS: [(Axis, u8); 6] = [
    (Axis::LeftStickX, 23),
    (Axis::LeftStickY, 24),
    (Axis::RightStickX, 25),
    (Axis::RightStickY, 26),
    (Axis::L2, 27),
    (Axis::R2, 28),
];

struct MidiMapper {
    midi_sender: MidiSender,
    controllers: Vec<Box<dyn Controller>>,
    active_controls: Vec<(String, String, String)>,
}

impl MidiMapper {
    fn new(controllers: Vec<Box<dyn Controller>>) -> Result<Self, Box<dyn Error>> {
        let midi_sender = MidiSender::new(MIDI_PORT_NAME)?;
        
        // Print controller types
        for controller in &controllers {
            let metadata = controller.get_metadata();
            println!("Found controller: {} ({:04X}:{:04X})", 
                metadata.product, metadata.vid, metadata.pid);
        }

        Ok(Self {
            midi_sender,
            controllers,
            active_controls: Vec::new(),
        })
    }

    fn map_value(value: f32, out_min: u8, out_max: u8) -> u8 {
        let normalized = (value + 1.0) / 2.0;
        (normalized * (out_max as f32 - out_min as f32) + out_min as f32) as u8
    }

    fn process_axis(&mut self, axis: Axis, value: f32) -> Result<(), Box<dyn Error>> {
        if let Some(&(_, cc)) = AXIS_MAPPINGS.iter().find(|&&(a, _)| a == axis) {
            let midi_value = if matches!(axis, Axis::LeftStickX | Axis::LeftStickY | 
                Axis::RightStickX | Axis::RightStickY) && value.abs() < JOYSTICK_DEADZONE {
                return Ok(());
            } else {
                Self::map_value(value, 0, 127)
            };

            self.midi_sender.send_control_change(MIDI_CHANNEL, cc, midi_value)?;

            self.update_display(
                format!("{:?}", axis),
                format!("{:.4}", value),
                format!("CC {}: {}", cc, midi_value),
            );
        }
        Ok(())
    }

    fn process_button(&mut self, button: Button, pressed: bool) -> Result<(), Box<dyn Error>> {
        if let Some(&(_, note)) = BUTTON_MAPPINGS.iter().find(|&&(b, _)| b == button) {
            let velocity = if pressed { 127 } else { 0 };
            self.midi_sender.send_note_on(MIDI_CHANNEL, note, velocity)?;

            self.update_display(
                format!("{:?}", button),
                pressed.to_string(),
                format!("Note {}: {}", note, if pressed { "ON" } else { "OFF" }),
            );
        }
        Ok(())
    }

    fn update_display(&mut self, control: String, raw_value: String, midi_output: String) {
        self.active_controls.retain(|(c, _, _)| *c != control);
        self.active_controls.push((control, raw_value, midi_output));
        self.refresh_display();
    }

    fn refresh_display(&self) {
        print!("\x1B[2J\x1B[H");
        println!("{:<20} | {:<10} | {}", "Control", "Raw Value", "MIDI Output");
        println!("{}", "-".repeat(50));
        for (control, raw_value, midi_output) in &self.active_controls {
            println!("{:<20} | {:<10} | {}", control, raw_value, midi_output);
        }
    }

    fn run(&mut self) -> Result<(), Box<dyn Error>> {
        println!("Listening for inputs... (CTRL+C to exit)");
        loop {
            for controller in self.controllers {
                match controller.poll_events() {
                    Ok(events) => {
                        for event in events {
                            match event {
                                ControllerEvent::ButtonPress { button, pressed } => 
                                    self.process_button(button, pressed)?,
                                ControllerEvent::AxisMove { axis, value } => 
                                    self.process_axis(axis, value)?,
                                #[cfg(target_os = "linux")]
                                ControllerEvent::TouchpadMove { x, y } => {
                                    // Handle touchpad input if needed
                                }
                            }
                        }
                    }
                    Err(e) => eprintln!("Error polling controller: {}", e),
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn run(&mut self) -> Result<(), Box<dyn Error>> {
    println!("Listening for inputs... (CTRL+C to exit)");
    loop {
        let mut any_events = false;
        for controller in &mut self.controllers {
            match controller.poll_events() {
                Ok(events) => {
                    for event in events {
                        any_events = true;
                        match event {
                            // Handle events
                        }
                    }
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        if any_events {
            self.refresh_display();
        }
        thread::sleep(Duration::from_millis(10));
    }
}