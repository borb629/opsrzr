# devices/

This directory will hold per-device TOML definition files mapping Razer USB
product IDs to their capabilities.

Each `*.toml` file describes one device: name, USB PID, device type
(mouse/keyboard), transaction ID, response wait time, supported features
(lighting, DPI, polling rate, battery), LED regions, matrix dimensions, and
DPI range. The schema is defined in Phase 2 of the project plan.

Capability data is ported from the OpenRazer daemon's per-device classes
(`daemon/openrazer_daemon/hardware/mouse.py` and `keyboards.py`), with
attribution. See CONTRIBUTING.md at the repository root for how to add
support for a new device.
