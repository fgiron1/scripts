use std::error::Error;
use std::thread;
use std::time::Duration;
use std::collections::HashMap;
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::config::{MIDI_CHANNEL, JOYSTICK_DEADZONE, BUTTON_MAPPINGS, AXIS_MAPPINGS};
use crate::controller::{Controller, types::{ControllerEvent, Button, Axis}};
use crate::midi::MidiSender;

// Static flag to reduce repeated environment checks
static DISPLAY_DISABLED: AtomicBool = AtomicBool::new(false);
static DISPLAY_INIT_DONE: AtomicBool = AtomicBool::new(false);

pub struct MidiMapper {
    midi_sender: MidiSender,
    pub controller: Box<dyn Controller>,
    button_states: HashMap<Button, bool>,
    axis_values: HashMap<Axis, f32>,
    last_midi_cc_values: HashMap<u8, u8>,  // Track last MIDI CC values to prevent duplicates
    last_button_time: HashMap<Button, std::time::Instant>, // For debouncing buttons
}

impl MidiMapper {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        // Check if display is disabled - do this only once
        if !DISPLAY_INIT_DONE.load(Ordering::Relaxed) {
            let disable_display = env::var("PS4_DISABLE_MAPPER_DISPLAY").is_ok();
            DISPLAY_DISABLED.store(disable_display, Ordering::Relaxed);
            DISPLAY_INIT_DONE.store(true, Ordering::Relaxed);
        }

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
            last_midi_cc_values: HashMap::new(),
            last_button_time: HashMap::new(),
        })
    }
    
    // Process button presses - map to MIDI notes
    fn process_button(&mut self, button: Button, pressed: bool) -> Result<(), Box<dyn Error>> {
        // Get previous state
        let previous = self.button_states.get(&button).copied().unwrap_or(false);
        
        // Skip if state hasn't changed
        if previous == pressed {
            return Ok(());
        }
        
        // Simple debouncing - avoid rapid triggering
        let now = std::time::Instant::now();
        let last_time = self.last_button_time.get(&button).copied().unwrap_or_else(|| now - Duration::from_secs(1));
        
        // Require minimum time between button state changes (except first press)
        if previous && now.duration_since(last_time) < Duration::from_millis(20) {
            return Ok(());
        }
        
        // Update state
        self.button_states.insert(button, pressed);
        self.last_button_time.insert(button, now);
        
        // Find mapping for this button
        if let Some(mapping) = BUTTON_MAPPINGS.iter().find(|m| m.button == button) {
            // Send MIDI note message
            let velocity = if pressed { 127 } else { 0 };
            self.midi_sender.send_note(MIDI_CHANNEL, mapping.note, velocity)?;
            
            // Print status if display is enabled
            if !DISPLAY_DISABLED.load(Ordering::Relaxed) {
                println!("Button {:?}: {} -> Note {} {}", 
                    button, 
                    if pressed { "Pressed" } else { "Released" },
                    mapping.note,
                    if pressed { "on (127)" } else { "off (0)" }
                );
            }
        }
        
        Ok(())
    }

    // Process axis movements - map to MIDI CC
    fn process_axis(&mut self, axis: Axis, value: f32) -> Result<(), Box<dyn Error>> {
        // Skip processing if value hasn't changed significantly
        let previous = self.axis_values.get(&axis).copied().unwrap_or(0.0);
        if (value - previous).abs() < 0.01 {
            return Ok(());
        }
        
        // Apply deadzone based on axis type
        let processed_value = match axis {
            // For analog sticks
            Axis::LeftStickX | Axis::LeftStickY | Axis::RightStickX | Axis::RightStickY => {
                if value.abs() < JOYSTICK_DEADZONE {
                    0.0
                } else {
                    // Rescale to use full range outside deadzone
                    let sign = if value < 0.0 { -1.0 } else { 1.0 };
                    sign * ((value.abs() - JOYSTICK_DEADZONE) / (1.0 - JOYSTICK_DEADZONE)).min(1.0)
                }
            },
            // For touchpad, ensure we have full range of motion
            Axis::TouchpadX | Axis::TouchpadY => value,
            // For triggers and others, use as-is
            _ => value
        };
        
        // Update state
        self.axis_values.insert(axis, processed_value);
        
        // Find mapping for this axis
        if let Some(mapping) = AXIS_MAPPINGS.iter().find(|m| m.axis == axis) {
            // Map to MIDI value based on axis type
            let midi_value = match axis {
                // For triggers, map from 0.0-1.0 to 0-127
                Axis::L2 | Axis::R2 => (processed_value * 127.0).round() as u8,
                
                // For all other axes, map from -1.0-1.0 to 0-127
                _ => ((processed_value + 1.0) * 63.5).round() as u8
            };
            
            // Only send if value has changed
            let last_value = self.last_midi_cc_values.get(&mapping.cc).copied().unwrap_or(255);
            if midi_value != last_value {
                self.midi_sender.send_control_change(MIDI_CHANNEL, mapping.cc, midi_value)?;
                self.last_midi_cc_values.insert(mapping.cc, midi_value);
                
                // Print status if display is enabled
                if !DISPLAY_DISABLED.load(Ordering::Relaxed) {
                    println!("Axis {:?}: {:.2} -> CC {} = {}", axis, processed_value, mapping.cc, midi_value);
                }
            }
        }
        
        Ok(())
    }

    // Process touchpad movements - this converts to axis movements
    fn process_touchpad(&mut self, x: Option<i32>, y: Option<i32>) -> Result<(), Box<dyn Error>> {
        // Process X coordinate if available
        if let Some(x_value) = x {
            // Normalize from raw coordinates to -1..1
            let normalized = (x_value as f32 / 960.0) - 1.0;
            self.process_axis(Axis::TouchpadX, normalized)?;
        }
        
        // Process Y coordinate if available
        if let Some(y_value) = y {
            // Normalize from raw coordinates to -1..1 (inverted Y)
            let normalized = -((y_value as f32 / 471.0) - 1.0);
            self.process_axis(Axis::TouchpadY, normalized)?;
        }
        
        Ok(())
    }
    
    /// Main processing loop
    pub fn run(&mut self) -> Result<(), Box<dyn Error>> {
        // Initial setup message
        println!("\nMapping controller to MIDI...");
        println!("Press Ctrl+C to exit.");
        
        // Main loop
        let mut last_poll_time = std::time::Instant::now();
        
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
                            ControllerEvent::TouchpadMove { x, y } => {
                                self.process_touchpad(x, y)?;
                            }
                        }
                    }
                },
                Err(e) => {
                    // Only report errors after a timeout to prevent error spam
                    let now = std::time::Instant::now();
                    if now.duration_since(last_poll_time) > Duration::from_millis(250) {
                        println!("Controller error: {}", e);
                        last_poll_time = now;
                    }

                    // If this was a disconnect, let's wait a bit longer
                    if e.to_string().contains("disconnect") {
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            }            
            // Sleep to prevent excessive CPU usage, but keep latency low
            thread::sleep(Duration::from_millis(1));
        }
    }
}