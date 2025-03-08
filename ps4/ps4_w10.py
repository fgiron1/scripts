#!/usr/bin/env python3
import os
import rtmidi
import pygame
import sys
import time
import platform

# ===== CONFIGURATION =====
MIDI_PORT_NAME = "PS4 Controller MIDI"
MIDI_CHANNEL = 0
JOYSTICK_DEADZONE = 0.2  # Normalized deadzone for joysticks (0-1.0)

# ===== CONTROL MAPPINGS =====
BUTTON_MAPPINGS = {
    0: ('Cross', 36),        # Typically Square/X button
    1: ('Circle', 37),       # Typically Circle button
    2: ('Triangle', 38),     # Typically Triangle button
    3: ('Square', 39),       # Typically Square button
    4: ('L1', 40),
    5: ('R1', 41),
    6: ('L2', 42),
    7: ('R2', 43),
    8: ('Share', 44),
    9: ('Options', 45),
    10: ('PS Button', 46),
    11: ('L3', 47),
    12: ('R3', 48),
    13: ('Touchpad Click', 49)
}

AXIS_MAPPINGS = {
    0: ('Left X', 23),
    1: ('Left Y', 24),
    2: ('Right X', 25),
    3: ('Right Y', 26),
    4: ('L2 Analog', 27),
    5: ('R2 Analog', 28)
}

class CrossPlatformMidiMapper:
    def __init__(self):
        self.midi = rtmidi.MidiOut()
        self.joystick = None
        self.active_controls = {}
        self.last_sent_values = {}
        self.last_button_states = {}
        
        self.init_midi()
        self.init_joystick()
        self.init_display()

    def init_midi(self):
        if MIDI_PORT_NAME in self.midi.get_ports():
            self.midi.open_port(self.midi.get_ports().index(MIDI_PORT_NAME))
        else:
            self.midi.open_virtual_port(MIDI_PORT_NAME)
        print(f"MIDI output: {MIDI_PORT_NAME}")

    def init_joystick(self):
        pygame.init()
        pygame.joystick.init()
        
        if pygame.joystick.get_count() == 0:
            print("No controllers found!")
            sys.exit(1)
            
        self.joystick = pygame.joystick.Joystick(0)
        self.joystick.init()
        print(f"Controller found: {self.joystick.get_name()}")

    def init_display(self):
        print(f"{'Control':<20} | {'Raw Value':<10} | {'MIDI Output'}")
        print("-" * 50)

    def map_value(self, value, out_min=0, out_max=127):
        return int((value + 1) * (out_max - out_min) / 2 + out_min)

    def process_axis(self, axis, value):
        if axis in [0, 1, 2, 3]:  # Sticks
            if abs(value) < JOYSTICK_DEADZONE:
                return None
            return self.map_value(value)
        else:  # Triggers
            return self.map_value(value, out_min=0, out_max=127)

    def refresh_display(self):
        for name, (value, midi) in self.active_controls.items():
            print(f"{name:<20} | {value:<10.4f} | {midi}")

    def monitor(self):
        print("\nListening for inputs... (CTRL+C to exit)\n")
        try:
            while True:
                pygame.event.pump()
                
                # Process Axes
                for axis in AXIS_MAPPINGS:
                    value = self.joystick.get_axis(axis)
                    name, cc = AXIS_MAPPINGS[axis]
                    midi_value = self.process_axis(axis, value)
                    
                    if midi_value is not None:
                        control_key = f"axis_{axis}"
                        if control_key not in self.last_sent_values or self.last_sent_values[control_key] != midi_value:
                            self.last_sent_values[control_key] = midi_value
                            msg = [0xB0 | MIDI_CHANNEL, cc, midi_value]
                            self.midi.send_message(msg)
                            self.active_controls[name] = (value, f"CC {cc}: {midi_value}")
                
                # Process Buttons
                for btn in BUTTON_MAPPINGS:
                    pressed = self.joystick.get_button(btn)
                    if btn in self.last_button_states and self.last_button_states[btn] == pressed:
                        continue
                        
                    self.last_button_states[btn] = pressed
                    name, note = BUTTON_MAPPINGS[btn]
                    velocity = 127 if pressed else 0
                    msg = [0x90 | MIDI_CHANNEL, note, velocity]
                    self.midi.send_message(msg)
                    self.active_controls[name] = (pressed, f"Note {note} {'ON' if velocity else 'OFF'}")

                self.refresh_display()
                time.sleep(0.01)
                if platform.system() == "Windows":
                    print("\033[2J\033[H")  # Clear screen for Windows
                else:
                    print("\033[2;0H")  # Move cursor to line 2 for Linux

        except KeyboardInterrupt:
            print("\nExiting...")
        finally:
            self.midi.close_port()
            pygame.quit()

if __name__ == "__main__":
    if platform.system() == "Linux" and os.geteuid() != 0:
        print("Warning: Run with sudo for best results on Linux!")
        
    CrossPlatformMidiMapper().monitor()