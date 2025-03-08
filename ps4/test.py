#!/usr/bin/env python3

from evdev import InputDevice, list_devices
import time

devices = [InputDevice(path) for path in list_devices()]
for dev in devices:
    if "Wireless Controller" in dev.name:
        print(f"Monitoring: {dev.name}")
        try:
            for event in dev.read_loop():
                print(f"Event: {event}")
        except PermissionError:
            print("Permission denied! Check /dev/input/ permissions.")
        except Exception as e:
            print(f"Error: {str(e)}")

