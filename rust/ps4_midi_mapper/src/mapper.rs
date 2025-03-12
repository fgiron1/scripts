use std::error::Error;
use std::thread;
use std::time::Duration;
use std::collections::HashMap;
use std::env;

use crate::config::{MIDI_CHANNEL, JOYSTICK_DEADZONE, BUTTON_MAPPINGS, AXIS_MAPPINGS};
use crate::controller::{Controller, types::{ControllerEvent, Button, Axis}};
use crate::midi::MidiSender;

pub struct MidiMapper {
    midi_sender: MidiSender,
    pub controller: Box<dyn Controller>,
    button_states: HashMap<Button, bool>,
    axis_values: HashMap<Axis, f32>,
    display_info: Vec<(String, String, String)>,
    disable_display: bool,
    last_midi_values: HashMap<u8, u8>,  // Added: Track last MIDI CC values sent
}

impl MidiMapper {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        // Create MIDI sender
        let midi_sender = MidiSender::new(crate::config::MIDI_PORT_NAME)?;
        
        // Create controller
        let controller = crate::controller::create_controller()?;
        
        // Get controller info
        let device_info = controller.get_device_info();
        println!("\nConnected controller: {} ({:04X}:{:04X})", 
            device_info.product, device_info.vid, device_info.pid);
        
        // Check if mapper display is disabled
        let disable_display = env::var("PS4_DISABLE_MAPPER_DISPLAY").is_ok();
        
        Ok(Self {
            midi_sender,
            controller,
            button_states: HashMap::new(),
            axis_values: HashMap::new(),
            display_info: Vec::new(),
            disable_display,
            last_midi_values: HashMap::new()
        })
    }
    
    /// Map value from -1.0 to 1.0 range to MIDI CC value (0-127)
    fn map_value(value: f32) -> u8 {
        // Map from -1.0..1.0 to 0..127
        ((value * 0.5 + 0.5) * 127.0).clamp(0.0, 127.0) as u8
    }
    
    fn process_button(&mut self, button: Button, pressed: bool) -> Result<(), Box<dyn Error>> {
        // Check if state has changed
        let previous = self.button_states.get(&button).copied().unwrap_or(false);
        
        if previous != pressed {
            // Update state
            self.button_states.insert(button, pressed);
            
            // Find mapping for this button
            if let Some(mapping) = BUTTON_MAPPINGS.iter().find(|m| m.button == button) {
                // Send MIDI note message
                let velocity = if pressed { 127 } else { 0 };
                self.midi_sender.send_note(MIDI_CHANNEL, mapping.note, velocity)?;
                
                // Update display if enabled
                if !self.disable_display {
                    self.update_display(
                        format!("{:?}", button),
                        format!("{}", if pressed { "Pressed" } else { "Released" }),
                        format!("Note {} {}", mapping.note, if pressed { "on (127)" } else { "off (0)" }),
                    );
                }
            }
        }
        
        Ok(())
    }

    fn process_axis(&mut self, axis: Axis, value: f32) -> Result<(), Box<dyn Error>> {
        // Skip processing if value hasn't changed significantly
        let previous = self.axis_values.get(&axis).copied().unwrap_or(0.0);
        if (value - previous).abs() < 0.05 {
            return Ok(());
        }
        
        // Apply deadzone differently per axis type
        let processed_value = match axis {
            // For analog sticks
            Axis::LeftStickX | Axis::LeftStickY | Axis::RightStickX | Axis::RightStickY => {
                if value.abs() < JOYSTICK_DEADZONE {
                    0.0
                } else {
                    value
                }
            },
            // For triggers - use as-is with no additional filtering
            _ => value
        };
        
        // Update state even if we won't send a MIDI message
        self.axis_values.insert(axis, processed_value);
        
        // Find mapping for this axis
        if let Some(mapping) = AXIS_MAPPINGS.iter().find(|m| m.axis == axis) {
            // Map to MIDI value
            let midi_value = match axis {
                // For triggers, map from 0.0-1.0 to 0-127
                Axis::L2 | Axis::R2 => (processed_value * 127.0).round() as u8,
                
                // For sticks, map from -1.0-1.0 to 0-127 with center at 64
                _ => {
                    if processed_value <= -1.0 {
                        0
                    } else if processed_value >= 1.0 {
                        127
                    } else {
                        ((processed_value + 1.0) * 63.5).round() as u8
                    }
                }
            };
            
            // Send MIDI only if value has actually changed
            let prev_midi = self.last_midi_values.get(&mapping.cc).copied().unwrap_or(0);
            if midi_value != prev_midi {
                self.midi_sender.send_control_change(MIDI_CHANNEL, mapping.cc, midi_value)?;
                self.last_midi_values.insert(mapping.cc, midi_value);
                
                // Update display if enabled
                if !self.disable_display {
                    self.update_display(
                        format!("{:?}", axis),
                        format!("{:.2}", processed_value),
                        format!("CC {} = {}", mapping.cc, midi_value),
                    );
                }
            }
        }
        
        Ok(())
    }
    #[cfg(target_os = "linux")]
    fn process_touchpad(&mut self, x: Option<i32>, y: Option<i32>) -> Result<(), Box<dyn Error>> {
        // Process touchpad X coordinate
        if let Some(x_value) = x {
            // Normalize from 0..1920 to -1..1
            let normalized = (x_value as f32 / 960.0) - 1.0;
            self.process_axis(Axis::TouchpadX, normalized)?;
        }
        
        // Process touchpad Y coordinate
        if let Some(y_value) = y {
            // Normalize from 0..942 to -1..1 (inverted, because Y is from top to bottom)
            let normalized = -((y_value as f32 / 471.0) - 1.0);
            self.process_axis(Axis::TouchpadY, normalized)?;
        }
        
        Ok(())
    }
    
    /// Update the display with current control information
    fn update_display(&mut self, control: String, value: String, midi: String) {
        // Skip if display is disabled
        if self.disable_display {
            return;
        }
        
        // Find existing entry for this control or add a new one
        let pos = self.display_info.iter().position(|(c, _, _)| *c == control);
        
        if let Some(index) = pos {
            self.display_info[index] = (control, value, midi);
        } else {
            self.display_info.push((control, value, midi));
        }
        
        // Sort by control name for consistent display
        self.display_info.sort_by(|a, b| a.0.cmp(&b.0));
        
        // Refresh the display
        self.refresh_display();
    }
    
    /// Display current controller state in a table format
    fn refresh_display(&self) {
        // Skip if display is disabled
        if self.disable_display {
            return;
        }
        
        // Clear screen (ANSI escape code)
        print!("\x1B[2J\x1B[H");
        
        println!("PS4/Controller MIDI Mapper");
        println!("==========================");
        println!();
        
        // Display active controls in a table format
        println!("{:<15} | {:<10} | {}", "Control", "Value", "MIDI");
        println!("{}", "-".repeat(45));
        
        for (control, value, midi) in &self.display_info {
            println!("{:<15} | {:<10} | {}", control, value, midi);
        }
    }
    
    /// Main processing loop
    pub fn run(&mut self) -> Result<(), Box<dyn Error>> {
        // Initial display setup
        if !self.disable_display {
            println!("\nWaiting for controller inputs...");
            println!("Press Ctrl+C to exit.");
            
            // Initialize the display
            self.refresh_display();
        }
        
        loop {
            // Poll controller for events
            match self.controller.poll_events() {
                Ok(events) => {
                    for event in events {
                        match event {
                            ControllerEvent::ButtonPress { button, pressed } => {
                                self.process_button(button, pressed)?;
                            },
                            ControllerEvent::AxisMove { axis, value } => {
                                self.process_axis(axis, value)?;
                            },
                            #[cfg(target_os = "linux")]
                            ControllerEvent::TouchpadMove { x, y } => {
                                self.process_touchpad(x, y)?;
                            }
                        }
                    }
                },
                Err(e) => {
                    println!("Controller error: {}", e);
                    return Err(e);
                }
            }
            
            // Sleep to prevent excessive CPU usage
            thread::sleep(Duration::from_millis(10));
        }
    }
}