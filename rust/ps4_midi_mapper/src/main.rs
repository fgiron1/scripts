// src/main.rs
use std::error::Error;
use std::thread;
use std::time::Duration;

mod controller;
mod driver_setup;
mod midi_output;

use controller::{Controller, ControllerType, types::{Button, Axis, ControllerEvent}};
use driver_setup::DriverSetup;
use midi_output::MidiOutput;

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
    midi_out: MidiOutput,
    controller: Box<dyn Controller>,
    active_controls: Vec<(String, String, String)>,
}

impl MidiMapper {
    fn new() -> Result<Self, Box<dyn Error>> {
        let midi_out = MidiOutput::new(MIDI_PORT_NAME)?;

        // Create controller and get its type
        let (controller, ctype) = controller::create_controller()?;

        // Print controller type
        match ctype {
            ControllerType::XInput => println!("Using XInput (DS4Windows)"),
            ControllerType::DirectInput => println!("Using DirectInput (Native Bluetooth)"),
            ControllerType::Linux => println!("Using Linux Native Input"),
        }

        Ok(Self {
            midi_out,
            controller,
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

            self.midi_out.send(&[0xB0 | MIDI_CHANNEL, cc, midi_value])?;
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
            self.midi_out.send(&[0x90 | MIDI_CHANNEL, note, velocity])?;
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
            let events = self.controller.poll_events()?;
            for event in events {
                match event {
                    ControllerEvent::ButtonPress { button, pressed } => 
                        self.process_button(button, pressed)?,
                    ControllerEvent::AxisMove { axis, value } => 
                        self.process_axis(axis, value)?,
                    ControllerEvent::TouchpadEvent { x: _, y: _ } => {
                        #[cfg(target_os = "linux")]
                        self.process_touchpad(x, y)?;
                    }
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    // Check drivers
    let setup = DriverSetup::new();
    setup.check_requirements()?;

    // Create mapper
    let mut mapper = MidiMapper::new()?;
    mapper.run()
}