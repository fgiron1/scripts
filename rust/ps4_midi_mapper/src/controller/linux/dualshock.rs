// src/controller/linux/dualshock.rs

use gilrs::{Gilrs, Event, Button as GilrsButton, Axis as GilrsAxis, EventType, GamepadId};
use evdev::{Device, EventType as EvdevEventType, AbsoluteAxisType};
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::thread;
use crate::controller::{Controller, types::{ControllerEvent, Button, Axis, DeviceInfo}};
use std::error::Error;
use std::collections::{HashMap, VecDeque};
use std::any::Any;

// Constants for the touchpad
const TOUCHPAD_X_MAX: i32 = 1920;
const TOUCHPAD_Y_MAX: i32 = 942;
const CONTROLLER_BASE_NAME: &str = "Wireless Controller"; // Also matches "Sony PlayStation DualShock"

// Structure to share data between threads
struct SharedState {
    touchpad_events: VecDeque<ControllerEvent>,
    display_updates: VecDeque<(String, String, String)>,
}

pub struct DualShockController {
    // Storing Gilrs itself is not thread-safe, but we'll only use it on the main thread
    gilrs: Gilrs,
    gamepad_id: GamepadId, // Store the actual GamepadId for direct comparison
    
    // Thread communication
    shared_state: Arc<Mutex<SharedState>>,
    touchpad_thread: Option<thread::JoinHandle<()>>,
    touchpad_running: Arc<Mutex<bool>>,
    
    // Local state tracking
    button_states: HashMap<Button, bool>,
    axis_values: HashMap<Axis, f32>,
    active_controls: Vec<(String, String, String)>, // For display
    
    // Display update management
    last_display_update: Instant,
}

// We need to explicitly mark it as Send to satisfy the trait bounds
unsafe impl Send for DualShockController {}

impl DualShockController {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        // Initialize Gilrs
        let gilrs = Gilrs::new()?;
        
        // Find a compatible game controller
        let mut gamepad_id = None;
        
        // Print all available controllers
        println!("Available controllers:");
        for (id, gamepad) in gilrs.gamepads() {
            let name = gamepad.name();
            println!("- Controller: {} (ID: {:?})", name, id);
            
            if name.contains("DualShock") || 
               name.contains("Wireless Controller") || 
               name.contains("Controller") {
                // Store the actual GamepadId for direct comparison
                gamepad_id = Some(id);
                println!("Selected controller: {}", name);
            }
        }
        
        let gamepad_id = match gamepad_id {
            Some(id) => id,
            None => return Err("No compatible controller found".into())
        };
        
        // Create shared state for thread communication
        let shared_state = Arc::new(Mutex::new(SharedState {
            touchpad_events: VecDeque::new(),
            display_updates: VecDeque::new(),
        }));
        
        // Flag to signal thread to stop
        let touchpad_running = Arc::new(Mutex::new(true));
        
        let mut controller = Self {
            gilrs,
            gamepad_id,
            shared_state,
            touchpad_thread: None,
            touchpad_running,
            button_states: HashMap::new(),
            axis_values: HashMap::new(),
            active_controls: Vec::new(),
            last_display_update: Instant::now(),
        };
        
        // Try to start the touchpad thread
        if let Err(e) = controller.start_touchpad_thread() {
            println!("Warning: Unable to start touchpad thread: {}", e);
            // Continue without touchpad thread
        }
        
        Ok(controller)
    }
    
    // Helper function to map value range (similar to your working code)
    fn map_value(value: f32, out_min: u8, out_max: u8) -> u8 {
        let normalized = (value + 1.0) / 2.0; // Map from [-1.0, 1.0] to [0.0, 1.0]
        (normalized * (out_max as f32 - out_min as f32) + out_min as f32) as u8
    }
    
    // Start a separate thread for touchpad processing
    fn start_touchpad_thread(&mut self) -> Result<(), Box<dyn Error>> {
        // Find the touchpad device
        match Self::find_touchpad_device() {
            Ok(Some(touchpad_device)) => {
                println!("Found touchpad device for PS4 controller, starting touchpad thread");
                
                // Clone values needed for the thread
                let shared_state = Arc::clone(&self.shared_state);
                let running = Arc::clone(&self.touchpad_running);
                
                // Start the thread
                let handle = thread::spawn(move || {
                    let mut touchpad_device = touchpad_device; // Take ownership in the thread
                    let mut touchpad_x = 0;
                    let mut touchpad_y = 0;
                    let mut touchpad_active = false;
                    
                    // Loop until signaled to stop
                    while *running.lock().unwrap() {
                        // Process touchpad events
                        if let Err(e) = Self::process_touchpad_in_thread(
                            &mut touchpad_device, 
                            &shared_state,
                            &mut touchpad_x,
                            &mut touchpad_y,
                            &mut touchpad_active
                        ) {
                            // Only log non-would-block errors
                            if e.to_string().contains("would block") == false {
                                eprintln!("Touchpad thread error: {}", e);
                            }
                        }
                        
                        // Short sleep to prevent hammering CPU
                        thread::sleep(Duration::from_millis(1));
                    }
                    
                    println!("Touchpad thread stopping");
                });
                
                self.touchpad_thread = Some(handle);
                Ok(())
            },
            Ok(None) => {
                println!("No touchpad device found");
                Ok(()) // Not an error, just no touchpad
            },
            Err(e) => {
                println!("Warning: Touchpad detection error: {}", e);
                Err(e)
            }
        }
    }
    
    // Static method to find the touchpad device
    fn find_touchpad_device() -> Result<Option<Device>, Box<dyn Error>> {
        println!("Searching for touchpad device...");
        
        for entry in fs::read_dir("/dev/input")? {
            let path = entry?.path();
            
            if let Ok(mut device) = Device::open(&path) {
                // Check if this is the touchpad for a DualShock controller
                if let Some(name) = device.name() {
                    // Check for common touchpad identifiers
                    if name.contains("Touchpad") || 
                       name.contains("Touch") || 
                       name.contains("SONY") || 
                       name.contains("Sony") ||
                       name.contains(CONTROLLER_BASE_NAME) {
                        
                        if let Some(abs_info) = device.supported_absolute_axes() {
                            // Check for typical touchpad axes using AbsoluteAxisType
                            if abs_info.contains(AbsoluteAxisType::ABS_MT_POSITION_X) ||
                               abs_info.contains(AbsoluteAxisType::ABS_MT_TRACKING_ID) ||
                               abs_info.contains(AbsoluteAxisType::ABS_X) {
                                
                                println!("Found touchpad device: {}", name);
                                
                                // Try to grab the device - if it fails, just continue
                                if let Err(e) = device.grab() {
                                    println!("Warning: couldn't grab touchpad: {}", e);
                                }
                                
                                return Ok(Some(device));
                            }
                        }
                    }
                }
            }
        }
        
        println!("No touchpad device found");
        Ok(None)
    }
    
    // Static method to process touchpad events in a separate thread
    fn process_touchpad_in_thread(
        touchpad: &mut Device,
        shared_state: &Arc<Mutex<SharedState>>,
        touchpad_x: &mut i32,
        touchpad_y: &mut i32,
        touchpad_active: &mut bool
    ) -> Result<(), Box<dyn Error>> {
        // Track if we've seen X or Y changes in this batch
        let mut x_updated = false;
        let mut y_updated = false;
        let mut touch_started = false;
        let mut touch_ended = false;
        let mut events = Vec::new();
        let mut display_updates = Vec::new();
        
        // Process pending events
        let fetched_events = touchpad.fetch_events()?;
        
        for ev in fetched_events {
            match ev.event_type() {
                EvdevEventType::ABSOLUTE => {
                    // Use AbsoluteAxisType instead of AbsoluteAxisCode
                    let abs_code = AbsoluteAxisType(ev.code());
                    match abs_code {
                        AbsoluteAxisType::ABS_MT_POSITION_X => {
                            *touchpad_x = ev.value();
                            x_updated = true;
                        },
                        AbsoluteAxisType::ABS_MT_POSITION_Y => {
                            *touchpad_y = ev.value();
                            y_updated = true;
                        },
                        AbsoluteAxisType::ABS_MT_TRACKING_ID => {
                            if ev.value() == -1 {
                                touch_ended = true;
                            } else {
                                touch_started = true;
                                *touchpad_active = true;
                            }
                        },
                        AbsoluteAxisType::ABS_X => {
                            *touchpad_x = ev.value();
                            x_updated = true;
                        },
                        AbsoluteAxisType::ABS_Y => {
                            *touchpad_y = ev.value();
                            y_updated = true;
                        },
                        _ => {}
                    }
                },
                EvdevEventType::KEY => {
                    let raw_code = ev.code();
                    
                    // BTN_TOUCH is typically 330
                    if raw_code == 330 {
                        if ev.value() == 1 {
                            touch_started = true;
                            *touchpad_active = true;
                        } else if ev.value() == 0 {
                            touch_ended = true;
                        }
                    }
                    
                    // BTN_LEFT (touchpad click) is typically 272
                    if raw_code == 272 {
                        let pressed = ev.value() == 1;
                        let button = Button::Touchpad;
                        
                        events.push(ControllerEvent::ButtonPress {
                            button,
                            pressed,
                        });
                        
                        // Add display update
                        display_updates.push((
                            "Touchpad Click".to_string(),
                            pressed.to_string(),
                            format!("Note {}: {}", 
                                    button_to_midi_note(button), 
                                    if pressed { "ON" } else { "OFF" }),
                        ));
                    }
                },
                EvdevEventType::SYNCHRONIZATION => {
                    // If we have new coordinates and touch is active, send them
                    if *touchpad_active && (x_updated || y_updated) {
                        events.push(ControllerEvent::TouchpadMove {
                            x: if x_updated { Some(*touchpad_x) } else { None },
                            y: if y_updated { Some(*touchpad_y) } else { None },
                        });
                        
                        // Also map to axes for MIDI mapping
                        if x_updated {
                            let x_norm = (*touchpad_x as f32 / TOUCHPAD_X_MAX as f32) * 2.0 - 1.0;
                            events.push(ControllerEvent::AxisMove {
                                axis: Axis::TouchpadX,
                                value: x_norm,
                            });
                            
                            // Add display update
                            display_updates.push((
                                "Touchpad X".to_string(),
                                format!("{}", *touchpad_x),
                                format!("CC {}: {}", 
                                        axis_to_midi_cc(Axis::TouchpadX),
                                        ((x_norm + 1.0) * 63.5) as u8),
                            ));
                        }
                        
                        if y_updated {
                            // Invert Y since touchpad coordinates are top-to-bottom
                            let y_norm = -((*touchpad_y as f32 / TOUCHPAD_Y_MAX as f32) * 2.0 - 1.0);
                            events.push(ControllerEvent::AxisMove {
                                axis: Axis::TouchpadY,
                                value: y_norm,
                            });
                            
                            // Add display update
                            display_updates.push((
                                "Touchpad Y".to_string(),
                                format!("{}", *touchpad_y),
                                format!("CC {}: {}", 
                                        axis_to_midi_cc(Axis::TouchpadY),
                                        ((y_norm + 1.0) * 63.5) as u8),
                            ));
                        }
                    }
                    
                    // Reset trackers
                    x_updated = false;
                    y_updated = false;
                },
                _ => {}
            }
        }
        
        // Update touchpad active state
        if touch_ended && !touch_started {
            *touchpad_active = false;
            
            // Send axis values of 0 to indicate touch ended
            events.push(ControllerEvent::AxisMove {
                axis: Axis::TouchpadX,
                value: 0.0,
            });
            
            events.push(ControllerEvent::AxisMove {
                axis: Axis::TouchpadY,
                value: 0.0,
            });
            
            // Add display updates for when touch ends
            display_updates.push((
                "Touchpad X".to_string(),
                "0".to_string(),
                format!("CC {}: 0", axis_to_midi_cc(Axis::TouchpadX)),
            ));
            
            display_updates.push((
                "Touchpad Y".to_string(),
                "0".to_string(),
                format!("CC {}: 0", axis_to_midi_cc(Axis::TouchpadY)),
            ));
        }
        
        // If we have events to report, add them to the shared state
        if !events.is_empty() || !display_updates.is_empty() {
            if let Ok(mut state) = shared_state.lock() {
                // Add events
                for event in events {
                    state.touchpad_events.push_back(event);
                }
                
                // Add display updates
                for update in display_updates {
                    state.display_updates.push_back(update);
                }
            }
        }
        
        Ok(())
    }
    
    /// Refresh the display with the current state of active controls
    fn refresh_display(&mut self) {
        // Apply any pending display updates from the touchpad thread
        if let Ok(mut state) = self.shared_state.lock() {
            while let Some((control, raw_value, midi_output)) = state.display_updates.pop_front() {
                // Debug when adding touchpad events
                println!("Adding touchpad event to display: {}", control);
                self.active_controls.retain(|(c, _, _)| *c != control);
                self.active_controls.push((control, raw_value, midi_output));
            }
        }
        
        // Only refresh display at most every 50ms to avoid excessive screen updates
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_display_update);
        
        if elapsed > Duration::from_millis(50) {
            print!("\x1B[2J\x1B[H"); // Clear screen and move cursor to top-left
            println!("{:<20} | {:<10} | {}", "Control", "Raw Value", "MIDI Output");
            println!("{}", "-".repeat(50));
            
            // Sort controls for more consistent display
            let mut sorted_controls = self.active_controls.clone();
            sorted_controls.sort_by(|(a, _, _), (b, _, _)| a.cmp(b));
            
            for (control, raw_value, midi_output) in &sorted_controls {
                println!("{:<20} | {:<10} | {}", control, raw_value, midi_output);
            }
            
            self.last_display_update = now;
        }
    }
    
    // Process controller inputs with limited timeout to prevent blocking
    fn process_controller_inputs(&mut self) -> Vec<ControllerEvent> {
        let mut events = Vec::new();
        
        // Debugging output
        // println!("Polling controller events...");
        
        // Process a limited number of events to avoid blocking
        let mut event_count = 0;
        let max_events = 20; // Increased to catch more events
        
        while event_count < max_events {
            if let Some(Event { id, event, .. }) = self.gilrs.next_event() {
                // Only process events for our controller
                if id != self.gamepad_id {
                    continue;
                }
                
                match event {
                    EventType::ButtonPressed(button, _) => {
                        if let Some(mapped_button) = map_button(button) {
                            // Skip L2/R2 buttons as they're handled as axes
                            if mapped_button != Button::L2 && mapped_button != Button::R2 {
                                self.button_states.insert(mapped_button, true);
                                events.push(ControllerEvent::ButtonPress {
                                    button: mapped_button,
                                    pressed: true,
                                });
                                
                                // Update display
                                self.active_controls.retain(|(c, _, _)| *c != format!("{:?}", mapped_button));
                                self.active_controls.push((
                                    format!("{:?}", mapped_button),
                                    "true".to_string(),
                                    format!("Note {}: ON", button_to_midi_note(mapped_button)),
                                ));
                            }
                        }
                    }
                    
                    EventType::ButtonReleased(button, _) => {
                        if let Some(mapped_button) = map_button(button) {
                            // Skip L2/R2 buttons as they're handled as axes
                            if mapped_button != Button::L2 && mapped_button != Button::R2 {
                                self.button_states.insert(mapped_button, false);
                                events.push(ControllerEvent::ButtonPress {
                                    button: mapped_button,
                                    pressed: false,
                                });
                                
                                // Update display
                                self.active_controls.retain(|(c, _, _)| *c != format!("{:?}", mapped_button));
                                self.active_controls.push((
                                    format!("{:?}", mapped_button),
                                    "false".to_string(),
                                    format!("Note {}: OFF", button_to_midi_note(mapped_button)),
                                ));
                            }
                        }
                    }
                    
                    EventType::AxisChanged(axis, value, _) => {
                        // Debug print to identify the axis
                        println!("Axis: {:?} = {}", axis, value);
                        
                        // Try to map the axis
                        let mapped_axis = match axis {
                            GilrsAxis::LeftZ => {
                                println!("L2 TRIGGER DETECTED: {}", value);
                                Some(Axis::L2)
                            },
                            GilrsAxis::RightZ => {
                                println!("R2 TRIGGER DETECTED: {}", value);
                                Some(Axis::R2)
                            },
                            GilrsAxis::LeftStickX => Some(Axis::LeftStickX),
                            GilrsAxis::LeftStickY => Some(Axis::LeftStickY),
                            GilrsAxis::RightStickX => Some(Axis::RightStickX),
                            GilrsAxis::RightStickY => Some(Axis::RightStickY),
                            _ => {
                                // Print unknown axes for debugging - these might be our triggers!
                                println!("Other axis detected: {:?} = {}", axis, value);
                                None
                            }
                        };
                        
                        if let Some(mapped_axis) = mapped_axis {
                            // Only register substantial changes to reduce event spam
                            let old_value = self.axis_values.get(&mapped_axis).copied().unwrap_or(0.0);
                            
                            // For triggers, we want to detect even small changes at the beginning of the press
                            let threshold = match mapped_axis {
                                Axis::L2 | Axis::R2 => 0.005, // Lower threshold for triggers
                                _ => 0.01,            // Regular threshold for other axes
                            };
                            
                            if (value - old_value).abs() > threshold {
                                self.axis_values.insert(mapped_axis, value);
                                
                                // For trigger axes (L2/R2), convert the -1.0 to 1.0 range to 0.0 to 1.0
                                // Many libraries use -1.0 for unpressed and 1.0 for fully pressed
                                let normalized_value = match mapped_axis {
                                    Axis::L2 | Axis::R2 => (value + 1.0) / 2.0, // Convert -1.0,1.0 to 0.0,1.0
                                    _ => value,
                                };
                                
                                events.push(ControllerEvent::AxisMove {
                                    axis: mapped_axis,
                                    value: normalized_value,
                                });
                                
                                // Map to MIDI CC value (0-127)
                                let midi_value = match mapped_axis {
                                    Axis::L2 | Axis::R2 => (normalized_value * 127.0) as u8,
                                    _ => ((value + 1.0) / 2.0 * 127.0) as u8,
                                };
                                
                                // Update display
                                self.active_controls.retain(|(c, _, _)| *c != format!("{:?}", mapped_axis));
                                
                                // For triggers, show the 0.0-1.0 range instead of -1.0 to 1.0
                                let display_value = match mapped_axis {
                                    Axis::L2 | Axis::R2 => format!("{:.4}", normalized_value),
                                    _ => format!("{:.4}", value),
                                };
                                
                                self.active_controls.push((
                                    format!("{:?}", mapped_axis),
                                    display_value,
                                    format!("CC {}: {}", axis_to_midi_cc(mapped_axis), midi_value),
                                ));
                            }
                        }
                    }
                    
                    _ => {}
                }
                
                event_count += 1;
            } else {
                // No more events
                break;
            }
        }
        
        events
    }
}

impl Controller for DualShockController {
    fn poll_events(&mut self) -> Result<Vec<ControllerEvent>, Box<dyn Error>> {
        let mut events = Vec::new();
        
        // Process controller events first - these should be quick
        let controller_events = self.process_controller_inputs();
        events.extend(controller_events);
        
        // Collect any events from the touchpad thread
        if let Ok(mut state) = self.shared_state.lock() {
            while let Some(event) = state.touchpad_events.pop_front() {
                events.push(event);
            }
        }
        
        // Update the display
        self.refresh_display();
        
        // Add a small sleep to prevent excessive CPU usage
        thread::sleep(Duration::from_micros(500)); // 0.5ms sleep
        
        Ok(events)
    }
    
    fn get_device_info(&self) -> DeviceInfo {
        DeviceInfo {
            vid: 0x054C, // Sony
            pid: 0x05C4, // DualShock 4 v1
            manufacturer: "Sony".to_string(),
            product: "DualShock 4".to_string(),
        }
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }
    
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Drop for DualShockController {
    fn drop(&mut self) {
        // Signal the touchpad thread to stop
        if let Ok(mut running) = self.touchpad_running.lock() {
            *running = false;
        }
        
        // Wait for the thread to finish (optional, could also just detach)
        if let Some(handle) = self.touchpad_thread.take() {
            if let Err(e) = handle.join() {
                eprintln!("Error joining touchpad thread: {:?}", e);
            }
        }
    }
}

// Mapping functions
fn map_button(button: GilrsButton) -> Option<Button> {
    match button {
        GilrsButton::South => Some(Button::Cross),
        GilrsButton::East => Some(Button::Circle),
        GilrsButton::West => Some(Button::Square),
        GilrsButton::North => Some(Button::Triangle),
        GilrsButton::LeftTrigger => Some(Button::L1),
        GilrsButton::RightTrigger => Some(Button::R1),
        GilrsButton::LeftTrigger2 => Some(Button::L2),
        GilrsButton::RightTrigger2 => Some(Button::R2),
        GilrsButton::Select => Some(Button::Share),
        GilrsButton::Start => Some(Button::Options),
        GilrsButton::Mode => Some(Button::PS),
        GilrsButton::LeftThumb => Some(Button::L3),
        GilrsButton::RightThumb => Some(Button::R3),
        GilrsButton::DPadUp => Some(Button::DpadUp),
        GilrsButton::DPadDown => Some(Button::DpadDown),
        GilrsButton::DPadLeft => Some(Button::DpadLeft),
        GilrsButton::DPadRight => Some(Button::DpadRight),
        _ => None,
    }
}

fn map_axis(axis: GilrsAxis) -> Option<Axis> {
    match axis {
        GilrsAxis::LeftStickX => Some(Axis::LeftStickX),
        GilrsAxis::LeftStickY => Some(Axis::LeftStickY),
        GilrsAxis::RightStickX => Some(Axis::RightStickX),
        GilrsAxis::RightStickY => Some(Axis::RightStickY),
        GilrsAxis::LeftZ => Some(Axis::L2),
        GilrsAxis::RightZ => Some(Axis::R2),
        // Handle unmapped axes
        axis => {
            // Print unknown axis for debugging
            println!("Unmapped axis: {:?}", axis);
            None
        },
    }
}

// Map buttons to MIDI notes (from your original config)
fn button_to_midi_note(button: Button) -> u8 {
    match button {
        Button::Cross => 36,
        Button::Circle => 37,
        Button::Triangle => 38,
        Button::Square => 39,
        Button::L1 => 40,
        Button::R1 => 41,
        Button::L2 => 42,
        Button::R2 => 43,
        Button::Share => 44,
        Button::Options => 45,
        Button::PS => 46,
        Button::L3 => 47,
        Button::R3 => 48,
        Button::Touchpad => 53,
        Button::DpadUp => 49,
        Button::DpadDown => 50,
        Button::DpadLeft => 51,
        Button::DpadRight => 52,
        _ => 0,
    }
}

// Map axes to MIDI CC values (from your original config)
fn axis_to_midi_cc(axis: Axis) -> u8 {
    match axis {
        Axis::LeftStickX => 23,
        Axis::LeftStickY => 24,
        Axis::RightStickX => 25,
        Axis::RightStickY => 26,
        Axis::L2 => 27,
        Axis::R2 => 28,
        Axis::TouchpadX => 29,
        Axis::TouchpadY => 30,
        _ => 0,
    }
}