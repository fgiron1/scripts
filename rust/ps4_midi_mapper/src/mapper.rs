use std::error::Error;
use std::thread;
use std::time::Duration;
use std::collections::HashMap;

use crate::config::{MIDI_CHANNEL, JOYSTICK_DEADZONE, BUTTON_MAPPINGS, AXIS_MAPPINGS};
use crate::controller::{Controller, types::{ControllerEvent, Button, Axis}};
use crate::midi::MidiSender;

pub struct MidiMapper {
    midi_sender: MidiSender,
    pub controller: Box<dyn Controller>,
    button_states: HashMap<Button, bool>,
    axis_values: HashMap<Axis, f32>,
    display_info: Vec<(String, String, String)>,
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
        
        Ok(Self {
            midi_sender,
            controller,
            button_states: HashMap::new(),
            axis_values: HashMap::new(),
            display_info: Vec::new(),
        })
    }
    
    /// Map value from -1.0 to 1.0 range to MIDI CC value (0-127)
    fn map_value(value: f32) -> u8 {
        // Map from -1.0..1.0 to 0..127
        ((value * 0.5 + 0.5) * 127.0).clamp(0.0, 127.0) as u8
    }
    
    /// Process a controller button event
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
                
                // Update display
                self.update_display(
                    format!("{:?}", button),
                    format!("{}", if pressed { "Pressed" } else { "Released" }),
                    format!("Note {} {}", mapping.note, if pressed { "on (127)" } else { "off (0)" }),
                );
            }
        }
        
        Ok(())
    }
    
    /// Process a controller axis event
    fn process_axis(&mut self, axis: Axis, value: f32) -> Result<(), Box<dyn Error>> {
        // Apply deadzone for analog sticks (not triggers)
        let mut processed_value = value;
        
        if matches!(axis, 
            Axis::LeftStickX | Axis::LeftStickY | Axis::RightStickX | Axis::RightStickY
        ) && value.abs() < JOYSTICK_DEADZONE {
            processed_value = 0.0;
        }
        
        // Check if value has changed significantly
        let previous = self.axis_values.get(&axis).copied().unwrap_or(0.0);
        let change = (processed_value - previous).abs();
        
        // Only process if change is significant (reduces event spam)
        if change > 0.01 {
            // Update state
            self.axis_values.insert(axis, processed_value);
            
            // Find mapping for this axis
            if let Some(mapping) = AXIS_MAPPINGS.iter().find(|m| m.axis == axis) {
                // Send MIDI CC message
                let midi_value = Self::map_value(processed_value);
                self.midi_sender.send_control_change(MIDI_CHANNEL, mapping.cc, midi_value)?;
                
                // Update display
                self.update_display(
                    format!("{:?}", axis),
                    format!("{:.2}", processed_value),
                    format!("CC {} = {}", mapping.cc, midi_value),
                );
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
        println!("\nWaiting for controller inputs...");
        println!("Press Ctrl+C to exit.");
        
        // Initialize the display
        self.refresh_display();
        
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