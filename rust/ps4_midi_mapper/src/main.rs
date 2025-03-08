use gilrs::{Gilrs, Button, Axis, Event};
use midir::{MidiOutput, MidiOutputConnection};
use evdev::{Device, InputEvent, AbsoluteAxisCode};
use std::error::Error;
use std::thread;
use std::time::Duration;
use std::fs;

// ===== CONFIGURATION =====
const MIDI_PORT_NAME: &str = "PS4 Controller MIDI";
const MIDI_CHANNEL: u8 = 0;
const JOYSTICK_DEADZONE: f32 = 0.2; // Normalized deadzone for joysticks (0.0-1.0)
const CONTROLLER_BASE_NAME: &str = "Wireless Controller";

// ===== CONTROL MAPPINGS =====
const BUTTON_MAPPINGS: [(Button, u8); 14] = [
    (Button::South, 36),    // Cross
    (Button::East, 37),     // Circle
    (Button::North, 38),    // Triangle
    (Button::West, 39),     // Square
    (Button::LeftTrigger, 40),  // L1
    (Button::RightTrigger, 41), // R1
    (Button::LeftTrigger2, 42), // L2
    (Button::RightTrigger2, 43),// R2
    (Button::Select, 44),   // Share
    (Button::Start, 45),    // Options
    (Button::Mode, 46),     // PS Button
    (Button::LeftThumb, 47),// L3
    (Button::RightThumb, 48),// R3
    (Button::Unknown, 49),  // Touchpad Click (handled by evdev)
];

const AXIS_MAPPINGS: [(Axis, u8); 6] = [
    (Axis::LeftStickX, 23), // Left X
    (Axis::LeftStickY, 24), // Left Y
    (Axis::RightStickX, 25),// Right X
    (Axis::RightStickY, 26),// Right Y
    (Axis::LeftZ, 27),      // L2 Analog
    (Axis::RightZ, 28),     // R2 Analog
];

const TOUCHPAD_MAPPINGS: [(AbsoluteAxisCode, u8); 2] = [
    (AbsoluteAxisCode::ABS_MT_POSITION_X, 29), // Touchpad X
    (AbsoluteAxisCode::ABS_MT_POSITION_Y, 30), // Touchpad Y
];

// ===== MIDI MAPPER STRUCT =====
struct MidiMapper {
    midi_conn: MidiOutputConnection,
    gilrs: Gilrs,
    touchpad_device: Option<Device>,
    active_controls: Vec<(String, String, String)>, // (Control, Raw Value, MIDI Output)
}

impl MidiMapper {
    /// Create a new `MidiMapper` instance.
    fn new() -> Result<Self, Box<dyn Error>> {
        let midi_out = MidiOutput::new(MIDI_PORT_NAME)?;
        let ports = midi_out.ports();
        let midi_port = ports.get(0).ok_or("No MIDI output ports available")?;
        let midi_conn = midi_out.connect(midi_port, MIDI_PORT_NAME)?;

        let gilrs = Gilrs::new()?;
        if gilrs.gamepads().count() == 0 {
            return Err("No controllers found!".into());
        }

        // Find touchpad device
        let touchpad_device = find_touchpad_device()?;

        Ok(Self {
            midi_conn,
            gilrs,
            touchpad_device,
            active_controls: Vec::new(),
        })
    }

    /// Map a value from one range to another.
    fn map_value(value: f32, out_min: u8, out_max: u8) -> u8 {
        let normalized = (value + 1.0) / 2.0; // Map from [-1.0, 1.0] to [0.0, 1.0]
        (normalized * (out_max as f32 - out_min as f32) + out_min as f32) as u8
    }

    /// Process axis input and send MIDI CC messages.
    fn process_axis(&mut self, axis: Axis, value: f32) -> Result<(), Box<dyn Error>> {
        if let Some(&(_, cc)) = AXIS_MAPPINGS.iter().find(|&&(a, _)| a == axis) {
            let midi_value = if axis == Axis::LeftStickX || axis == Axis::LeftStickY || 
                               axis == Axis::RightStickX || axis == Axis::RightStickY {
                // Apply deadzone to joysticks
                if value.abs() < JOYSTICK_DEADZONE {
                    return Ok(());
                }
                Self::map_value(value, 0, 127)
            } else {
                // No deadzone for triggers (LeftZ and RightZ)
                Self::map_value(value, 0, 127)
            };

            self.midi_conn.send(&[0xB0 | MIDI_CHANNEL, cc, midi_value])?;
            self.update_display(
                format!("{:?}", axis),
                format!("{:.4}", value),
                format!("CC {}: {}", cc, midi_value),
            );
        }
        Ok(())
    }

    /// Process button input and send MIDI note messages.
    fn process_button(&mut self, button: Button, pressed: bool) -> Result<(), Box<dyn Error>> {
        if let Some(&(_, note)) = BUTTON_MAPPINGS.iter().find(|&&(b, _)| b == button) {
            let velocity = if pressed { 127 } else { 0 };
            self.midi_conn.send(&[0x90 | MIDI_CHANNEL, note, velocity])?;
            self.update_display(
                format!("{:?}", button),
                pressed.to_string(),
                format!("Note {}: {}", note, if pressed { "ON" } else { "OFF" }),
            );
        }
        Ok(())
    }

    /// Process touchpad input and send MIDI CC messages.
    fn process_touchpad(&mut self, event: InputEvent) -> Result<(), Box<dyn Error>> {
        if let Some(&(axis, cc)) = TOUCHPAD_MAPPINGS.iter().find(|&&(a, _)| a == AbsoluteAxisCode(event.code())) {
            // Scale touchpad values to MIDI range (0–127)
            let midi_value = match axis {
                AbsoluteAxisCode::ABS_MT_POSITION_X => ((event.value() as f32 / 1920.0) * 127.0) as u8,
                AbsoluteAxisCode::ABS_MT_POSITION_Y => ((event.value() as f32 / 1080.0) * 127.0) as u8,
                _ => 0, // Default to 0 for unknown axes
            };

            self.midi_conn.send(&[0xB0 | MIDI_CHANNEL, cc, midi_value])?;
            self.update_display(
                format!("Touchpad {:?}", axis),
                format!("{}", event.value()),
                format!("CC {}: {}", cc, midi_value),
            );
        }
        Ok(())
    }

    /// Update the display with the latest control values.
    fn update_display(&mut self, control: String, raw_value: String, midi_output: String) {
        self.active_controls.retain(|(c, _, _)| *c != control);
        self.active_controls.push((control, raw_value, midi_output));
        self.refresh_display();
    }

    /// Refresh the display with the current state of active controls.
    fn refresh_display(&self) {
        print!("\x1B[2J\x1B[H"); // Clear screen and move cursor to top-left
        println!("{:<20} | {:<10} | {}", "Control", "Raw Value", "MIDI Output");
        println!("{}", "-".repeat(50));
        for (control, raw_value, midi_output) in &self.active_controls {
            println!("{:<20} | {:<10} | {}", control, raw_value, midi_output);
        }
    }

    /// Main loop to process inputs and send MIDI messages.
    fn run(&mut self) -> Result<(), Box<dyn Error>> {
        println!("Listening for inputs... (CTRL+C to exit)");

        loop {
            // Process standard controller inputs
            while let Some(Event { id: _, event, .. }) = self.gilrs.next_event() {
                match event {
                    gilrs::EventType::ButtonPressed(button, _) => self.process_button(button, true)?,
                    gilrs::EventType::ButtonReleased(button, _) => self.process_button(button, false)?,
                    gilrs::EventType::AxisChanged(axis, value, _) => self.process_axis(axis, value)?,
                    _ => {}
                }
            }

            // Process touchpad inputs (if available)
            if let Some(device) = &mut self.touchpad_device {
                let events: Vec<InputEvent> = match device.fetch_events() {
                    Ok(events) => events.collect(),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Vec::new(),
                    Err(e) => return Err(e.into()),
                };

                for event in events {
                    self.process_touchpad(event)?;
                }
            }

            thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Find the touchpad device associated with the PS4 controller.
fn find_touchpad_device() -> Result<Option<Device>, Box<dyn Error>> {
    for entry in fs::read_dir("/dev/input")? {
        let entry = entry?;
        let path = entry.path();
        if let Ok(device) = Device::open(&path) {
            if device.name().unwrap_or_default().contains(CONTROLLER_BASE_NAME)
                && device.supported_events().contains(evdev::EventType::ABSOLUTE)
            {
                if let Some(abs_axes) = device.supported_absolute_axes() {
                    if abs_axes.contains(AbsoluteAxisCode::ABS_MT_POSITION_X) {
                        // Set non-blocking mode to prevent input waits
                        if let Err(e) = device.set_nonblocking(true) {
                            eprintln!("Couldn't set non-blocking mode: {}", e);
                            continue;
                        }
                        return Ok(Some(device));
                    }
                }
            }
        }
    }
    Ok(None)
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut mapper = MidiMapper::new()?;
    mapper.run()
}