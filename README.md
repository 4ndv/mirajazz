# Mirajazz

A Rust crate for interfacing with Mirabox and Ajazz "stream controller" devices

This is a hardfork of [elgato-streamdeck](https://github.com/streamduck-org/elgato-streamdeck) crate, with notable differences:

- No Elgato-related code. For that you should use an original library
- No device-specific code in the library, which devices to support is up to you
- No async interface (for now), due to hidapi-rs lack of proper async support

The idea is to have a common lowlevel library serving as a backbone for device-specific [OpenDeck](https://github.com/nekename/OpenDeck) plugins

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
