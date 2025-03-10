# PS4 MIDI Mapper

A cross-platform utility that maps PlayStation 4 DualShock controllers (and other game controllers) to MIDI messages, allowing you to use your game controller as a MIDI controller for music software.

## Key Features

- **Improved Controller Compatibility**:
  - Works with PlayStation 4 DualShock controllers **without requiring special drivers**
  - Supports Xbox controllers
  - Compatible with most generic game controllers
  - Low-level HID access for maximum compatibility on Windows

- **Input Mapping**:
  - Maps buttons to MIDI notes
  - Maps analog sticks and triggers to MIDI CC messages
  - Maps touchpad (on Linux) to MIDI CC messages

- **Cross-Platform**:
  - **Windows**: XInput and HID support
  - **Linux**: Support via gilrs and evdev

- **User Experience**:
  - Real-time visualization of controller inputs and MIDI outputs
  - Controller hotplugging support with automatic reconnection
  - Customizable MIDI mappings
  - Clean console interface with status updates

## Requirements

### Windows
- Windows 7 or later
- A DualShock 4 controller (wired or via Bluetooth)
- MIDI output device or virtual MIDI port

### Linux
- A DualShock 4 controller (wired or via Bluetooth)
- MIDI output device or virtual MIDI port
- `libudev` and `libasound2` development packages

## Installation

### From Source

1. Make sure you have Rust installed. If not, install it from [rustup.rs](https://rustup.rs/).
2. Clone this repository:
   ```
   git clone https://github.com/yourusername/ps4_midi_mapper.git
   cd ps4_midi_mapper
   ```
3. Build the project:
   ```
   cargo build --release