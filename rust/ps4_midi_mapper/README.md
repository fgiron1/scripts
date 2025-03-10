# PS4 MIDI Mapper

A cross-platform utility that maps PlayStation 4 DualShock controllers (and other game controllers) to MIDI messages, allowing you to use your game controller as a MIDI controller for music software.

## Features

- **Controller Support**:
  - PlayStation 4 DualShock controllers (both v1 and v2)
  - Xbox controllers
  - Any DirectInput-compatible controller on Windows
  - Most controllers supported by the `gilrs` crate on Linux

- **Input Mapping**:
  - Maps buttons to MIDI notes
  - Maps analog sticks and triggers to MIDI CC messages
  - Maps touchpad (on Linux) to MIDI CC messages

- **Cross-Platform**:
  - **Windows**: XInput and DirectInput support
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
   ```
4. The executable will be in `target/release/ps4_midi_mapper`

### Windows-Specific Notes

On Windows, the application requires administrator privileges to properly access the controller. The executable will automatically request these privileges when launched.

## Usage

1. Connect your controller to your computer.
2. Run the PS4 MIDI Mapper application.
3. The program will automatically detect your controller and list available MIDI ports.
4. Select a MIDI port to use (or the first one will be selected by default).
5. Start playing! Press buttons and move sticks to generate MIDI messages.

If no controller is connected when you start the application, it will:
1. Display a message indicating no controller was found
2. Begin polling for a connected controller
3. Show a progress indicator while waiting
4. Connect automatically once a controller is detected

### Configuration

You can customize the MIDI mappings by editing the `config.rs` file and recompiling:

- `MIDI_PORT_NAME`: The name of the MIDI port to connect to (leave empty to use the first available port)
- `MIDI_CHANNEL`: The MIDI channel to use (0-15)
- `JOYSTICK_DEADZONE`: Deadzone for analog sticks (0.0-1.0)
- `BUTTON_MAPPINGS`: Button to MIDI note mappings
- `AXIS_MAPPINGS`: Axis to MIDI CC mappings

## Technical Details

### Windows Implementation

The Windows implementation uses a tiered approach:

1. **XInput** - Tried first for Xbox controllers and some DS4 controllers with drivers that support XInput.
2. **DirectInput** - Used as a fallback for DS4 controllers and other controllers not supported by XInput.

The DirectInput implementation specifically looks for Sony DualShock 4 controllers using both VID/PID matching and name-based detection. It includes robust error handling and reconnection support.

### Linux Implementation

The Linux implementation uses two libraries:

1. **gilrs** - For general controller input (buttons, sticks, triggers)
2. **evdev** - For touchpad support specific to the DS4 controller

## Troubleshooting

### Controller Not Detected

#### Windows
- Make sure you're running the application as administrator
- Try reconnecting the controller
- If using Bluetooth, try re-pairing the controller
- Install the latest drivers for your controller

#### Linux
- Check that your user has proper permissions for /dev/input/* devices
- Try adding your user to the 'input' group: `sudo usermod -a -G input yourusername`
- For Bluetooth connections, ensure Bluetooth is properly configured

### No MIDI Output

- Make sure you have a MIDI device connected or a virtual MIDI port set up
- Check that your DAW or other MIDI software is properly configured to receive MIDI input
- Try using a MIDI monitor utility to verify MIDI messages are being sent

## License

This project is licensed under the MIT License.

## Acknowledgments

- Built with Rust and several open-source libraries
- Special thanks to the developers of gilrs, evdev, and midir