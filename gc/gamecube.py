import pywinusb.hid as hid
import mido
import time
import math

# Initialize MIDI
midi_out = mido.open_output("GameCube port 1")

# -----------------------------------------------------------
# PRECISION-CALIBRATED CONFIGURATION
# -----------------------------------------------------------
BUTTON_MIDI_MAP = {
    "A": 60, "B": 62, "X": 64, "Y": 65, "Z": 67
}

AXIS_MIDI_MAP = {
    "left_stick_x": 12,
    "right_stick_x": 13,
    "left_stick_y": 14,
    "right_stick_y": 15,
    "L_trigger": 20,
    "R_trigger": 21,
}

# Precision-tuned parameters
SMOOTHING_BUFFER_SIZE = 4
DEADZONE_PERCENT = 0.12
AXIS_DEBOUNCE_THRESHOLD = 2
AXIS_DEBOUNCE_TIME = 0.025
BUTTON_DEBOUNCE_TIME = 0.1
# -----------------------------------------------------------

# System State
axis_smoothing_buffer = {axis: [] for axis in AXIS_MIDI_MAP.keys()}
axis_states = {axis: {"raw": 128, "last_change": time.time()} 
              for axis in AXIS_MIDI_MAP.keys()}
button_states = {button: {"state": False, "last_change": time.time()} 
                for button in BUTTON_MIDI_MAP.keys()}

def apply_deadzone(value):
    center = 128
    deadzone_range = DEADZONE_PERCENT * 255
    return center if abs(value - center) < deadzone_range else value

def smooth_axis(axis, value):
    axis_smoothing_buffer[axis].append(value)
    if len(axis_smoothing_buffer[axis]) > SMOOTHING_BUFFER_SIZE:
        axis_smoothing_buffer[axis].pop(0)
    return int(sum(axis_smoothing_buffer[axis])/len(axis_smoothing_buffer[axis]))

def precision_debounce(axis, new_value):
    current_time = time.time()
    state = axis_states[axis]
    
    if abs(new_value - state["raw"]) > 15:
        state["raw"] = new_value
        state["last_change"] = current_time
    elif abs(new_value - state["raw"]) > AXIS_DEBOUNCE_THRESHOLD:
        if current_time - state["last_change"] >= AXIS_DEBOUNCE_TIME:
            state["raw"] = new_value
            state["last_change"] = current_time
    return state["raw"]

def debounce_button(button, new_state):
    current_time = time.time()
    if new_state != button_states[button]["state"]:
        if current_time - button_states[button]["last_change"] >= BUTTON_DEBOUNCE_TIME:
            button_states[button]["state"] = new_state
            button_states[button]["last_change"] = current_time
    return button_states[button]["state"]


def soft_scale(norm, d=0.05):
    """
    For a normalized value norm in [-1,1], apply a soft knee near 0.
    For |norm| < d, use a cubic interpolation that has zero derivative at 0
    and matches identity at |norm| = d.
    For |norm| >= d, return norm unchanged.
    """
    if abs(norm) < d:
        # For x in [0, d]: f(x) = A*x^3 + B*x^2,
        # with f(0)=0, f'(0)=0, f(d)=d, and f'(d)=1.
        # Solving yields:
        #   A = -1/d^2, and B = 2/d.
        if norm >= 0:
            return (-1/(d**2)) * norm**3 + (2/d) * norm**2
        else:
            # Use symmetry for negative values
            pos = -norm
            return -((-1/(d**2)) * pos**3 + (2/d) * pos**2)
    else:
        return norm

def scale_to_full_range(value, d=0.05, min_val=15, max_val=240):
    # Calculate the center from the calibrated min and max values
    center = (max_val + min_val) / 2.0

    # Normalize the value:
    # - For values above the center, map center to 0 and max_val to 1.
    # - For values below the center, map center to 0 and min_val to -1.
    if value >= center:
        norm = (value - center) / (max_val - center)
    else:
        norm = (value - center) / (center - min_val)

    # Apply soft knee smoothing near 0 using parameter d.
    soft = soft_scale(norm, d)

    # Map from [-1, 1] to [0, 127]
    midi_val = int(round((soft + 1) / 2 * 127))
    return max(0, min(127, midi_val))



def decode_raw_data(data):
    if len(data) < 10: return None

    axes = {
        "left_stick_x": precision_debounce("left_stick_x", 
                      smooth_axis("left_stick_x", apply_deadzone(data[3]))),
        "left_stick_y": precision_debounce("left_stick_y", 
                      smooth_axis("left_stick_y", apply_deadzone(data[4]))),
        "right_stick_x": precision_debounce("right_stick_x", 
                      smooth_axis("right_stick_x", apply_deadzone(data[5]))),
        "right_stick_y": precision_debounce("right_stick_y", 
                      smooth_axis("right_stick_y", apply_deadzone(data[6]))),
        "L_trigger": precision_debounce("L_trigger", 
                      smooth_axis("L_trigger", apply_deadzone(data[7]))),
        "R_trigger": precision_debounce("R_trigger", 
                      smooth_axis("R_trigger", apply_deadzone(data[8]))),
    }

    buttons = {
        "A": debounce_button("A", bool(data[1] & 0b00000010)),
        "B": debounce_button("B", bool(data[1] & 0b00000100)),
        "X": debounce_button("X", bool(data[1] & 0b00000001)),
        "Y": debounce_button("Y", bool(data[1] & 0b00001000)),
        "Z": debounce_button("Z", bool(data[1] & 0b10000000)),
    }

    return {"buttons": buttons, "axes": axes}

def send_midi(decoded_data):
    # Buttons
    for button, note in BUTTON_MIDI_MAP.items():
        state = decoded_data["buttons"][button]
        midi_out.send(mido.Message("note_on" if state else "note_off", 
                      note=note, velocity=100))

    # Axes with triple validation
    for axis, cc in AXIS_MIDI_MAP.items():
        raw = decoded_data["axes"][axis]
        midi_val = scale_to_full_range(raw)
        midi_val = max(0, min(127, midi_val))  # Final safety check
        midi_out.send(mido.Message("control_change", control=cc, value=midi_val))

# -----------------------------------------------------------
# DEVICE SETUP
# -----------------------------------------------------------
devices = hid.HidDeviceFilter().get_devices()
device = devices[8] if len(devices) > 8 else None

if device:
    def raw_data_handler(data):
        decoded = decode_raw_data(data)
        if decoded: send_midi(decoded)

    try:
        device.open()
        device.set_raw_data_handler(raw_data_handler)
        print(f"Active: {device.product_name}")
        while True: time.sleep(0.1)
    finally: device.close()
else: print("Device unavailable")