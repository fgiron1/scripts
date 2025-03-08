import pywinusb.hid as hid

# List all HID devices
devices = hid.HidDeviceFilter().get_devices()
if not devices:
    print("No HID devices detected. Please connect your controller.")
    exit()

print("Available HID devices:")
for i, device in enumerate(devices):
    print(f"{i}: {device.product_name} (Vendor ID: {device.vendor_id}, Product ID: {device.product_id})")

# Function to monitor raw input data
def monitor_device(device):
    print(f"\nMonitoring {device.product_name}...")
    print("Press any button or move any axis on the controller to see raw input data.")
    
    def raw_data_handler(data):
        print(f"Raw data: {data}")

    try:
        device.open()
        device.set_raw_data_handler(raw_data_handler)
        while True:
            pass  # Keep the script running to capture input
    except Exception as e:
        print(f"Error: {e}")
    finally:
        device.close()

# Select a device by index
device_index = int(input("\nEnter the index of the device you want to monitor: "))
if device_index < 0 or device_index >= len(devices):
    print("Invalid device index.")
    exit()

# Start monitoring the selected device
monitor_device(devices[device_index])