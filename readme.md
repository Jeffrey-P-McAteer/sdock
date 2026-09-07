
# Skeuomorphic Dock

`sdock` is a dock reminiscient of Apple's MacOS Dock from the 2009-2014 era.

Architcture:

 - `./sdock`
    - Main application logic written as a library, contains traits for any OS-specific requirements
 - `./sdock-<OS>-<ARCH>`
    - Actual binary implementation per os & architecture, imports `./sdock/` and implements all OS-specific traits and behaviors.
    - OS note: Windows and MacOS only have a single display platform, but Linux has both X11 and Wayland. We have decided to implement Wayland support only. X11 still has Compiz + Cairo-dock, which blows this out of the water anyhow.
 - `./sdock-tests/`
   - Either OS-agnostic OR linux-x64 only tests suitable for rendering previews and confirming the physical rendering attributes of the system.
   - Imports `./sdock` and simulates events into it instead of connecting to real OS events and hardware.



# Building

// TODO: document build system

# Packaging

// TODO: document packaging for all targets, likely via some zig cc magic


# License

The code in this repository is under the GPLv2 license, see `LICENSE.txt` for details.
The auto-upgrade clause has been removed because your legal rights shouldn't have that sort of volatility.



