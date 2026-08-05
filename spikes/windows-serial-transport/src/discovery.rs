//! Path-C prototype: independent USB discovery via `nusb` — separate from serial I/O.
//!
//! ## What this answers
//!
//! Architecture C proposes `serial2` (or any minimal I/O crate) for bytes, plus an
//! independent Windows/USB layer for enumeration and identity. This module measures what
//! `nusb` 0.2.5 can actually contribute to that layer.
//!
//! ## What `nusb` provides — verified by reading its source at the pinned version
//!
//! Cross-platform: VID, PID, manufacturer string, product string, serial number, class
//! codes, speed, interfaces. Windows-specific: `instance_id`, `parent_instance_id`,
//! `location_paths`, `port_chain`, `driver`. Hotplug: `nusb::watch_devices()`.
//!
//! ## What `nusb` does NOT provide — the finding that matters
//!
//! **No API returns a COM port name, and the strings `PortName` /
//! `GUID_DEVINTERFACE_COMPORT` do not appear anywhere in its source.** `nusb` alone
//! cannot answer "which COM port is this USB device?".
//!
//! The documented Windows mechanism for that join is: take the USB device's instance ID
//! (which `nusb` *does* expose), walk to the child device node of the USB serial
//! function, and read its `PortName` from the device registry via
//! SetupAPI/CfgMgr32. That step is not implemented here because it requires either
//! direct FFI (`windows-sys` — `unsafe` in our code, which this spike forbids) or a
//! wrapper crate audited separately (`windows`, or WMI queries via the `wmi` crate).
//! Whether the join works end-to-end on a real adapter is
//! `REQUIRES_WINDOWS_HARDWARE_TEST`.

use nusb::MaybeFuture;

/// USB-level identity, as much of it as `nusb` exposes on this platform.
#[derive(Debug, Clone, Default)]
pub struct UsbDeviceIdentity {
    pub vid: u16,
    pub pid: u16,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial_number: Option<String>,
    /// Windows-only join key toward a COM name (via SetupAPI, not via nusb itself).
    pub instance_id: Option<String>,
    pub parent_instance_id: Option<String>,
}

/// Enumerate USB devices. Returns identities, never panics on missing metadata.
pub fn probe_usb_devices() -> Result<Vec<UsbDeviceIdentity>, String> {
    let devices = nusb::list_devices().wait().map_err(|e| e.to_string())?;
    Ok(devices
        .map(|info| {
            #[allow(unused_mut)]
            let mut identity = UsbDeviceIdentity {
                vid: info.vendor_id(),
                pid: info.product_id(),
                manufacturer: info.manufacturer_string().map(str::to_string),
                product: info.product_string().map(str::to_string),
                serial_number: info.serial_number().map(str::to_string),
                ..Default::default()
            };
            #[cfg(target_os = "windows")]
            {
                identity.instance_id = Some(info.instance_id().to_string_lossy().into_owned());
                identity.parent_instance_id =
                    Some(info.parent_instance_id().to_string_lossy().into_owned());
            }
            identity
        })
        .collect())
}

/// Can this platform's build of the prototype produce a COM name for a USB device?
///
/// Deliberately a constant `false`: nusb exposes the join key, not the join. Kept as a
/// function so the report can cite an executable statement rather than prose.
pub fn can_map_usb_identity_to_com_name() -> bool {
    false
}
