#!/usr/bin/env python3

from evdev import InputDevice, list_devices, ecodes
import rtmidi
import os
import sys
from select import select
import time

# ===== CONFIGURATION =====
CONTROLLER_BASE_NAME = "Wireless Controller"
MIDI_PORT_NAME = "PS4 Controller MIDI"
MIDI_CHANNEL = 0
JOYSTICK_DEADZONE = 1000  # Adjust based on your controller's drift

# ===== CONTROL MAPPINGS =====
BUTTON_MAPPINGS = {
    ecodes.BTN_SOUTH: ('Cross', 36),
    ecodes.BTN_EAST: ('Circle', 37),
    ecodes.BTN_NORTH: ('Triangle', 38),
    ecodes.BTN_WEST: ('Square', 39),
    ecodes.BTN_TL: ('L1', 40),
    ecodes.BTN_TR: ('R1', 41),
    ecodes.BTN_TL2: ('L2', 42),
    ecodes.BTN_TR2: ('R2', 43),
    ecodes.BTN_SELECT: ('Share', 44),
    ecodes.BTN_START: ('Options', 45),
    ecodes.BTN_MODE: ('PS Button', 46),
    ecodes.BTN_THUMBL: ('L3', 47),
    ecodes.BTN_THUMBR: ('R3', 48),
    ecodes.BTN_GEAR_DOWN: ('Touchpad Click', 49)
}

ANALOG_MAPPINGS = {
    ecodes.ABS_X: ('Left X', 23, (-32768, 32767)),
    ecodes.ABS_Y: ('Left Y', 24, (-32768, 32767)),
    ecodes.ABS_RX: ('Right X', 25, (-32768, 32767)),
    ecodes.ABS_RY: ('Right Y', 26, (-32768, 32767)),
    ecodes.ABS_Z: ('L2 Analog', 27, (0, 255)),
    ecodes.ABS_RZ: ('R2 Analog', 28, (0, 255))
}

TOUCHPAD_MAPPINGS = {
    ecodes.ABS_MT_POSITION_X: ('Touchpad X', 29, (0, 1920)),
    ecodes.ABS_MT_POSITION_Y: ('Touchpad Y', 30, (0, 942))
}

class MidiMapper:
    def __init__(self):
        self.midi = rtmidi.MidiOut()
        self.main_device = None
        self.touchpad_device = None
        self.active_controls = {}
        self.last_sent_values = {}  # Track last sent MIDI values to avoid duplicates
        self.last_button_states = {}  # Track last button states to avoid repeats
        
        self.init_midi()
        self.find_controllers()
        self.init_display()

    def init_midi(self):
        """Initialize MIDI output"""
        if MIDI_PORT_NAME in self.midi.get_ports():
            self.midi.open_port(self.midi.get_ports().index(MIDI_PORT_NAME))
        else:
            self.midi.open_virtual_port(MIDI_PORT_NAME)
        print(f"MIDI output: {MIDI_PORT_NAME}")

    def find_controllers(self):
        """Find the controller devices (main and touchpad)"""
        for path in list_devices():
            try:
                dev = InputDevice(path)
                if CONTROLLER_BASE_NAME in dev.name:
                    if "Touchpad" in dev.name:
                        self.touchpad_device = dev
                        print(f"Found touchpad controller: {dev.name} at {dev.path}")
                    elif "Motion" not in dev.name:
                        self.main_device = dev
                        print(f"Found main controller: {dev.name} at {dev.path}")
            except (IOError, OSError) as e:
                print(f"Error accessing device at {path}: {e}")
        
        if not self.main_device:
            print("Main controller not found!")
            print("Available devices:")
            for path in list_devices():
                try:
                    dev = InputDevice(path)
                    print(f"  {dev.path}: {dev.name}")
                except:
                    pass
            sys.exit(1)

    def init_display(self):
        """Initialize display output"""
        os.system('clear')
        print(f"{'Control':<20} | {'Raw Value':<10} | {'MIDI Output'}")
        print("-" * 50)

    def map_value(self, value, in_min, in_max, out_min, out_max):
        """Map a value from one range to another"""
        return int((value - in_min) * (out_max - out_min) / (in_max - in_min) + out_min)

    def process_joystick(self, code, value):
        """Process joystick input and return MIDI value"""
        name, cc, (min_val, max_val) = ANALOG_MAPPINGS[code]
        
        # Apply deadzone (centered at 0 for joysticks)
        center = (min_val + max_val) // 2
        if abs(value - center) < JOYSTICK_DEADZONE:
            return 64  # Center position for MIDI
        
        # Map to MIDI range (0-127)
        return self.map_value(value, min_val, max_val, 0, 127)

    def process_trigger(self, code, value):
        """Process trigger input and return MIDI value"""
        name, cc, (min_val, max_val) = ANALOG_MAPPINGS[code]
        return self.map_value(value, min_val, max_val, 0, 127)

    def process_touchpad(self, code, value):
        """Process touchpad input and return MIDI value"""
        name, cc, (min_val, max_val) = TOUCHPAD_MAPPINGS[code]
        return self.map_value(value, min_val, max_val, 0, 127)

    def handle_event(self, event, device_type):
        """Process controller events"""
        if event.type == ecodes.EV_KEY:
            if event.code in BUTTON_MAPPINGS:
                # Only process when button state changes to avoid repeats
                if event.code not in self.last_button_states or self.last_button_states[event.code] != event.value:
                    self.last_button_states[event.code] = event.value
                    
                    name, note = BUTTON_MAPPINGS[event.code]
                    velocity = 127 if event.value else 0
                    msg = [0x90 | MIDI_CHANNEL, note, velocity]
                    self.midi.send_message(msg)
                    self.active_controls[name] = (event.value, f"Note {note} {'ON' if velocity else 'OFF'}")
                    self.refresh_display()
        
        elif event.type == ecodes.EV_ABS:
            if device_type == "main" and event.code in ANALOG_MAPPINGS:
                name, cc, input_range = ANALOG_MAPPINGS[event.code]
                
                # Process the value based on the control type
                if event.code in [ecodes.ABS_X, ecodes.ABS_Y, ecodes.ABS_RX, ecodes.ABS_RY]:
                    midi_value = self.process_joystick(event.code, event.value)
                else:  # Triggers
                    midi_value = self.process_trigger(event.code, event.value)
                
                # Only send MIDI if the value has changed
                control_key = f"{name}_{cc}"
                if control_key not in self.last_sent_values or self.last_sent_values[control_key] != midi_value:
                    self.last_sent_values[control_key] = midi_value
                    msg = [0xB0 | MIDI_CHANNEL, cc, midi_value]
                    self.midi.send_message(msg)
                    self.active_controls[name] = (event.value, f"CC {cc}: {midi_value}")
                    self.refresh_display()
            
            elif device_type == "touchpad" and event.code in TOUCHPAD_MAPPINGS:
                name, cc, input_range = TOUCHPAD_MAPPINGS[event.code]
                midi_value = self.process_touchpad(event.code, event.value)
                
                # Only send MIDI if the value has changed significantly (to reduce traffic)
                control_key = f"{name}_{cc}"
                if (control_key not in self.last_sent_values or 
                    abs(self.last_sent_values[control_key] - midi_value) > 1):
                    self.last_sent_values[control_key] = midi_value
                    msg = [0xB0 | MIDI_CHANNEL, cc, midi_value]
                    self.midi.send_message(msg)
                    self.active_controls[name] = (event.value, f"CC {cc}: {midi_value}")
                    self.refresh_display()

    def refresh_display(self):
        """Update display with current state"""
        print("\033[2;0H")  # Move cursor to line 2
        for name, (value, midi) in sorted(self.active_controls.items()):
            print(f"{name:<20} | {value:<10} | {midi}")
        print("\033[J", end="")

    def monitor(self):
        """Main event loop"""
        print("\nListening for inputs... (CTRL+C to exit)\n")
        try:
            if self.main_device:
                self.main_device.grab()
            if self.touchpad_device:
                self.touchpad_device.grab()
            
            while True:
                devices = [dev for dev in [self.main_device, self.touchpad_device] if dev]
                r, _, _ = select(devices, [], [], 0.1)
                
                for dev in r:
                    try:
                        for event in dev.read():
                            device_type = "main" if dev == self.main_device else "touchpad"
                            self.handle_event(event, device_type)
                    except Exception as e:
                        print(f"Error reading events from {dev.name}: {e}")
                
                time.sleep(0.001)  # Small sleep to prevent CPU hogging
        
        except KeyboardInterrupt:
            print("\nExiting...")
        finally:
            if self.main_device:
                try:
                    self.main_device.ungrab()
                except:
                    pass
            if self.touchpad_device:
                try:
                    self.touchpad_device.ungrab()
                except:
                    pass
            print("Cleanup complete")

if __name__ == "__main__":
    if os.geteuid() != 0:
        print("Warning: Run with sudo for best results!")
        
    MidiMapper().monitor()