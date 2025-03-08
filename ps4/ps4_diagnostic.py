#!/usr/bin/env python3

from evdev import InputDevice, list_devices, ecodes
import os
import sys
from select import select

# ===== CONFIGURATION =====
CONTROLLER_BASE_NAME = "Wireless Controller"
COLORS = {
    'main': '\033[93m',    # Yellow
    'touchpad': '\033[94m',# Blue
    'motion': '\033[95m',  # Purple
    'reset': '\033[0m'
}

# ===== BUTTON MAPPINGS =====
BUTTON_NAMES = {
    ecodes.BTN_SOUTH: 'Cross (X)',
    ecodes.BTN_EAST: 'Circle (O)',
    ecodes.BTN_NORTH: 'Triangle',
    ecodes.BTN_WEST: 'Square',
    ecodes.BTN_TL: 'L1',
    ecodes.BTN_TR: 'R1',
    ecodes.BTN_TL2: 'L2',
    ecodes.BTN_TR2: 'R2',
    ecodes.BTN_SELECT: 'Share',
    ecodes.BTN_START: 'Options',
    ecodes.BTN_MODE: 'PS Button',
    ecodes.BTN_THUMBL: 'L3',
    ecodes.BTN_THUMBR: 'R3',
    ecodes.BTN_DPAD_UP: 'D-pad Up',
    ecodes.BTN_DPAD_DOWN: 'D-pad Down',
    ecodes.BTN_DPAD_LEFT: 'D-pad Left',
    ecodes.BTN_DPAD_RIGHT: 'D-pad Right'
}

AXIS_NAMES = {
    ecodes.ABS_X: 'Left X',
    ecodes.ABS_Y: 'Left Y',
    ecodes.ABS_RX: 'Right X',
    ecodes.ABS_RY: 'Right Y',
    ecodes.ABS_Z: 'L2 Analog',
    ecodes.ABS_RZ: 'R2 Analog',
    ecodes.ABS_HAT0X: 'D-pad X',
    ecodes.ABS_HAT0Y: 'D-pad Y'
}

class ControllerMonitor:
    def __init__(self):
        self.devices = []
        self.active_controls = {}  # Track active controls and their values
        
        self.detect_devices()
        self.select_devices()
        self.print_header()
        
    def detect_devices(self):
        self.devices = [
            InputDevice(path) for path in list_devices()
            if CONTROLLER_BASE_NAME in (dev := InputDevice(path)).name
        ]
        
        if not self.devices:
            print("No controller found!")
            sys.exit(1)

    def select_devices(self):
        print("Found components:")
        for idx, dev in enumerate(self.devices):
            print(f"[{idx}] {dev.name}")
            
        choices = input("Select components (comma-separated, 'a' for all): ").strip()
        self.selected = self.devices if choices.lower() == 'a' else [
            self.devices[int(idx)] for idx in choices.split(',')
        ]

    def print_header(self):
        os.system('clear')
        print(f"{'Control':<20} | {'Value':<10}")
        print("-" * 30)

    def monitor(self):
        print("\nStarting monitoring (CTRL+C to exit)\n")
        try:
            for dev in self.selected:
                dev.grab()
                
            while True:
                r, _, _ = select(self.selected, [], [], 0.1)
                for dev in r:
                    for event in dev.read():
                        self.handle_event(event)
                        self.print_controls()
        except KeyboardInterrupt:
            print("\nExiting...")
        finally:
            for dev in self.selected:
                dev.ungrab()

    def handle_event(self, event):
        if event.type == ecodes.EV_SYN:
            return
            
        # Get control name
        if event.type == ecodes.EV_KEY:
            name = BUTTON_NAMES.get(event.code, f"Button {event.code}")
            value = "Pressed" if event.value else "Released"
        elif event.type == ecodes.EV_ABS:
            name = AXIS_NAMES.get(event.code, f"Axis {event.code}")
            value = event.value
        else:
            return
            
        # Update active controls
        self.active_controls[name] = value

    def print_controls(self):
        # Move cursor to top of display area
        print("\033[2;0H")  # Row 2, column 0
        
        # Print all active controls
        for name, value in self.active_controls.items():
            print(f"{name:<20} | {value:<10}")
            
        # Clear remaining lines
        remaining_lines = len(self.active_controls) + 2
        print("\033[J", end="")  # Clear from cursor to end of screen

if __name__ == "__main__":
    if os.geteuid() != 0:
        print("Warning: Run with sudo for best results")
        
    ControllerMonitor().monitor()