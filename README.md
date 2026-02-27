# Mirajazz

![Crates.io Version](https://img.shields.io/crates/v/mirajazz)
![Crates.io License](https://img.shields.io/crates/l/mirajazz)

A Rust crate for interfacing with Mirabox and Ajazz "stream controller" devices

This is a hardfork of [elgato-streamdeck](https://github.com/streamduck-org/elgato-streamdeck) crate, with notable differences:

- No Elgato-related code. For that you should use an original library
- No device-specific code in the library, which devices to support is up to you
- Uses [async-hid](https://github.com/sidit77/async-hid) instead of [hidapi-rs](https://github.com/ruabmbua/hidapi-rs). For old synchronous implementation use version `v0.3.0`
- Async only

The idea is to have a common lowlevel library serving as a backbone for device-specific [OpenDeck](https://github.com/nekename/OpenDeck) plugins

## Protocol versions

These versions are our own internal concept, and mostly used for telling the library which exact set of quirks to apply for specific protocol variations.

Mirabox (the manufacturer behind these devices) has no plans to provide any specifics on protocol implementation, as stated by their staff member on official Discord, so we have to improvise and reverse engineer.

### protocol_version = 0

Do not use this directly. This is a fallback for devices with *very* old firmware and will be set internally if needed, use version 1

- 512-bytes packets
- Device is missing serial number (that's how we know when to fallback to pv 0)
- Reported firmware version is `1.0.0.0`
- No `ACK` and `OK` in input report
- No support for both keypress states (aka long press, PTT)
- Only seen on one rebranded variation of 293S

Example devices: 293S rebranded as Soomfon

### protocol_version = 1

- 512-bytes packets
- No support for both keypress states (aka long press, PTT)
- Hardcoded serial of `355499441494`
- Broken serial report on Windows, so we also hardcode it ourself, do not rely on serial reported in `HidDeviceInfo`, always use `serial_number` function of connected `Device`

Example devices: Mirabox 293S (not 293SV3) with latest firmware, Ajazz AKP153 and regional variations

### protocol_version = 2

- 1024-bytes packets
- No support for both keypress states (aka long press, PTT)
- Unique serial numbers
- Requires extra command to clear the screen (handled by the library)

Example devices: Mirabox N3, Ajazz AKP03 (and variations, with PID starting with 1, not 3)

### protocol_version = 3

- 1024-bytes packets
- Does support both keypress states (aka long press, PTT)
- Unique serial numbers
- Requires extra command to clear the screen (handled by the library)
- Supports gifs

Example devices: Mirabox N3 rev. 2 (PID 1003), Mirabox N4, Akp03 rev. 2 (PIDs starting with 3)

## Flags

In some cases specific devices are not covered just by matching by protocol version

### `with_supports_both_encoder_states(supports: bool)`

Allows to disable handling both "Up" and "Down" states for encoder presses

Default: true if protocol_version > 2

## Current limitations

- Depends on tokio for wrapping synchronous image manipulation tasks
- No way to read firmware version due to async-hid not supporting feature reports for now
- "Old" devices have several issues with their serial number:
  - The serial number is same for all the devices: `355499441494`
  - On Windows the serial number changes each time device is reconnected
  - If you have the device that always returns this serial number, just hardcode it...

## Reference implementations

There is couple OpenDeck plugins made by me, which can be used as a starting point for making your own:

- [opendeck-akp03](https://github.com/4ndv/opendeck-akp03) for Ajazz AKP 03 / Mirabox N3 and derivatives
- [opendeck-akp153](https://github.com/4ndv/opendeck-akp153) for Ajazz AKP153 / Mirabox HSV293S and derivatives

If you plan to fork any of them, [here's the checklist](https://github.com/4ndv/mirajazz/wiki/Checklist-for-forking-my-existing-plugins) of things you'll need to do

## udev rules

For using on Linux, you are required to bring your own udev rules for all the VID/PID pairs you want to support. Without the udev rules, you wouldn't be able to connect to the devices from the userspace.

Here's an example for Ajazz AKP03R (VID 0x0300, PID 0x1003):

```
SUBSYSTEM=="usb", ATTR{idVendor}=="0300", ATTR{idProduct}=="1003", MODE="0660", TAG+="uaccess", GROUP="plugdev"
SUBSYSTEM=="usb", ATTRS{idVendor}=="0300", ATTRS{idProduct}=="1003", MODE="0660", TAG+="uaccess", GROUP="plugdev"
KERNEL=="hidraw*", SUBSYSTEM=="hidraw", ATTR{idVendor}=="0300", ATTR{idProduct}=="1003", MODE="0660", TAG+="uaccess", GROUP="plugdev"
KERNEL=="hidraw*", SUBSYSTEM=="hidraw", ATTRS{idVendor}=="0300", ATTRS{idProduct}=="1003", MODE="0660", TAG+="uaccess", GROUP="plugdev"
```

## Acknowledgments

- [@TheJebForge](https://github.com/TheJebForge) for the elgato-streamdeck library
- [@ZCube](https://github.com/ZCube) for initial ajazz devices support in the original library
- [@teras](https://github.com/teras) for more devices and fixes in the original library
- [@nekename](https://github.com/nekename) for reviewing my code for "v2" devices and maintaining original library

Commit history of the original library can be found [here](https://github.com/streamduck-org/elgato-streamdeck/commits/main/)
