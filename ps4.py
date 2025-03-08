import pywinusb.hid as hid

prev_raw_data = None

def find_ps4_controller():
    all_devices = hid.HidDeviceFilter().get_devices()
    valid_pids = {
        0x05C4: 'v1',  # DS4 v1 USB
        0x09CC: 'v1',  # DS4 v1 BT
        0x0BA0: 'v2',  # DS4 v2 USB
        0x0CDE: 'v2'   # DS4 v2 BT
    }
    for device in all_devices:
        if device.vendor_id == 0x054C and device.product_id in valid_pids:
            return device, valid_pids[device.product_id]
    return None, None

def decode_ps4_data(data, version):
    if not data:
        return None

    is_bluetooth = (version == 'v1' and data[0] == 0x11) or (version == 'v2' and data[0] == 0x31)
    
    # Skip Bluetooth header if present
    processed_data = data[1:] if is_bluetooth else data
    
    # Validate length (USB: 64, BT: 78)
    if (is_bluetooth and len(processed_data) < 63) or (not is_bluetooth and len(processed_data) < 64):
        return None

    # --- Buttons (Reverse-engineered for clone) ---
    buttons = {
        'square':   bool(processed_data[5] & 0x01),  # Byte 5, Bit 0
        'cross':    bool(processed_data[5] & 0x02),  # Byte 5, Bit 1
        'circle':   bool(processed_data[5] & 0x04),  # Byte 5, Bit 2
        'triangle': bool(processed_data[5] & 0x08),  # Byte 5, Bit 3
        'L1':       bool(processed_data[6] & 0x01),  # Byte 6, Bit 0
        'R1':       bool(processed_data[6] & 0x02),  # Byte 6, Bit 1
        'L2_btn':   bool(processed_data[6] & 0x04),  # Byte 6, Bit 2
        'R2_btn':   bool(processed_data[6] & 0x08),  # Byte 6, Bit 3
        'share':    bool(processed_data[6] & 0x10),  # Byte 6, Bit 4
        'options':  bool(processed_data[6] & 0x20),  # Byte 6, Bit 5
        'L3':       bool(processed_data[6] & 0x40),  # Byte 6, Bit 6
        'R3':       bool(processed_data[6] & 0x80),  # Byte 6, Bit 7
        'PS':       bool(processed_data[7] & 0x01),  # Byte 7, Bit 0
        'touchpad_btn': bool(processed_data[7] & 0x02),  # Byte 7, Bit 1
    }

    # --- Sticks/Triggers ---
    lx = processed_data[0]  # Byte 0
    ly = processed_data[1]  # Byte 1
    rx = processed_data[2]  # Byte 2
    ry = processed_data[3]  # Byte 3
    l2 = processed_data[7]  # Byte 7
    r2 = processed_data[8]  # Byte 8

    # --- Touchpad ---
    touch_active = bool(processed_data[34] & 0x80)  # Byte 34, Bit 7
    touch_x = processed_data[35] | ((processed_data[36] & 0x0F) << 8)  # Bytes 35-36
    touch_y = processed_data[37] | ((processed_data[38] & 0x0F) << 8)  # Bytes 37-38

    return {
        'buttons': buttons,
        'lx': lx, 'ly': ly, 'rx': rx, 'ry': ry,
        'l2': l2, 'r2': r2,
        'touchpad': (touch_x, touch_y, touch_active)
    }

def on_data(data):
    global prev_raw_data
    state = decode_ps4_data(data, controller_version)
    
    if not state:
        return
    
    print("="*40)
    print(f"Buttons: {state['buttons']}")
    print(f"Sticks: LX={state['lx']}, LY={state['ly']}, RX={state['rx']}, RY={state['ry']}")
    print(f"Triggers: L2={state['l2']}, R2={state['r2']}")
    print(f"Touchpad: Active={state['touchpad'][2]}, X={state['touchpad'][0]}, Y={state['touchpad'][1]}")
    
    # Raw HID changes (optional)
    if prev_raw_data:
        print_changed_bytes(data, prev_raw_data)
    prev_raw_data = data.copy()

def print_changed_bytes(current, previous):
    changes = []
    for i in range(min(len(current), len(previous))):
        if current[i] != previous[i]:
            changes.append(f"Byte {i}: 0x{current[i]:02X} (was 0x{previous[i]:02X})")
    if changes:
        print("Raw HID changes:")
        for change in changes:
            print(f"  {change}")

# Main script
ps4_controller, controller_version = find_ps4_controller()
if ps4_controller:
    print(f"PS4 Controller ({controller_version}) found!")
    ps4_controller.open()
    ps4_controller.set_raw_data_handler(lambda data: on_data(data))
    print("Press Ctrl+C to exit.")
    try:
        while True:
            pass
    except KeyboardInterrupt:
        ps4_controller.close()
else:
    print("Controller not found.")