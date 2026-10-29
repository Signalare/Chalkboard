// SPDX-License-Identifier: MIT OR Apache-2.0

//! Raspberry Pi board detection and hardware capability record (SPEC §4, HW-001, HW-009).
//!
//! At boot, `chalkboard-agent` detects the board model from the kernel device tree
//! (`/proc/device-tree/model` and `/proc/device-tree/system/linux,revision`, with
//! fallback to `/proc/cpuinfo`) and inspects `/sys` and `/proc/mounts` to record
//! the node's hardware capabilities in the `Node` record.

use std::fmt;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Supported 64-bit Raspberry Pi board models (SPEC §4.1, HW-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BoardModel {
    /// Raspberry Pi Zero 2 W (Tier 1, BCM2837).
    PiZero2W,
    /// Raspberry Pi 4 Model B (Tier 1, BCM2711).
    Pi4B,
    /// Raspberry Pi 5 (Tier 1, BCM2712).
    Pi5,
    /// Raspberry Pi 3 Model B (Tier 2, BCM2837).
    Pi3B,
    /// Raspberry Pi 3 Model B+ (Tier 2, BCM2837).
    Pi3BPlus,
    /// Raspberry Pi 3 Model A+ (Tier 2, BCM2837).
    Pi3APlus,
    /// Raspberry Pi 400 (Tier 2, BCM2711).
    Pi400,
    /// Raspberry Pi 500 (Tier 2, BCM2712).
    Pi500,
    /// Raspberry Pi Compute Module 4 (Tier 2, BCM2711).
    Cm4,
    /// Raspberry Pi Compute Module 5 (Tier 2, BCM2712).
    Cm5,
    /// Raspberry Pi Compute Module 3 (Tier 3, BCM2837).
    Cm3,
    /// Raspberry Pi Compute Module 3+ (Tier 3, BCM2837).
    Cm3Plus,
    /// Raspberry Pi Compute Module 4S (Tier 3, BCM2711).
    Cm4S,
}

impl BoardModel {
    /// Human-readable canonical board name.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::PiZero2W => "Raspberry Pi Zero 2 W",
            Self::Pi4B => "Raspberry Pi 4 Model B",
            Self::Pi5 => "Raspberry Pi 5",
            Self::Pi3B => "Raspberry Pi 3 Model B",
            Self::Pi3BPlus => "Raspberry Pi 3 Model B+",
            Self::Pi3APlus => "Raspberry Pi 3 Model A+",
            Self::Pi400 => "Raspberry Pi 400",
            Self::Pi500 => "Raspberry Pi 500",
            Self::Cm4 => "Raspberry Pi Compute Module 4",
            Self::Cm5 => "Raspberry Pi Compute Module 5",
            Self::Cm3 => "Raspberry Pi Compute Module 3",
            Self::Cm3Plus => "Raspberry Pi Compute Module 3+",
            Self::Cm4S => "Raspberry Pi Compute Module 4S",
        }
    }

    /// Support tier defined in SPEC §4.1.
    #[must_use]
    pub const fn support_tier(self) -> SupportTier {
        match self {
            Self::PiZero2W | Self::Pi4B | Self::Pi5 => SupportTier::Tier1,
            Self::Pi3B
            | Self::Pi3BPlus
            | Self::Pi3APlus
            | Self::Pi400
            | Self::Pi500
            | Self::Cm4
            | Self::Cm5 => SupportTier::Tier2,
            Self::Cm3 | Self::Cm3Plus | Self::Cm4S => SupportTier::Tier3,
        }
    }

    /// Expected SoC family for this board model (SPEC §4.1).
    #[must_use]
    pub const fn soc_family(self) -> SocFamily {
        match self {
            Self::PiZero2W
            | Self::Pi3B
            | Self::Pi3BPlus
            | Self::Pi3APlus
            | Self::Cm3
            | Self::Cm3Plus => SocFamily::Bcm2837,
            Self::Pi4B | Self::Pi400 | Self::Cm4 | Self::Cm4S => SocFamily::Bcm2711,
            Self::Pi5 | Self::Pi500 | Self::Cm5 => SocFamily::Bcm2712,
        }
    }

    /// Number of independent HDMI outputs driven by the board (RND-016, HW-009).
    #[must_use]
    pub const fn hdmi_outputs(self) -> u8 {
        match self {
            Self::PiZero2W
            | Self::Pi3B
            | Self::Pi3BPlus
            | Self::Pi3APlus
            | Self::Cm3
            | Self::Cm3Plus
            | Self::Cm4S => 1,
            Self::Pi4B | Self::Pi400 | Self::Cm4 | Self::Pi5 | Self::Pi500 | Self::Cm5 => 2,
        }
    }

    /// Whether the board has built-in Ethernet on the board itself (HW-008).
    #[must_use]
    const fn has_builtin_ethernet(self) -> bool {
        matches!(
            self,
            Self::Pi3B | Self::Pi3BPlus | Self::Pi4B | Self::Pi400 | Self::Pi5 | Self::Pi500
        )
    }
}

impl fmt::Display for BoardModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

/// Hardware support tier (SPEC §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SupportTier {
    /// Tested on hardware before every release; all §21 budgets guaranteed.
    Tier1,
    /// Smoke-tested on hardware before every minor release (TEST-011).
    Tier2,
    /// Community-supported; fixes accepted, no release testing guarantees.
    Tier3,
}

/// Raspberry Pi 64-bit SoC family (SPEC §1.4, §4.1).
///
/// Ordered from lowest to highest capability (`Bcm2837 < Bcm2711 < Bcm2712`)
/// to match the node ranking rule in WIFI-011.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SocFamily {
    /// BCM2837 / RP3A0-AU (Pi 3 generation, Zero 2 W, CM3/CM3+).
    Bcm2837,
    /// BCM2711 (Pi 4B, Pi 400, CM4, CM4S).
    Bcm2711,
    /// BCM2712 (Pi 5, Pi 500, CM5).
    Bcm2712,
}

/// Wi-Fi radio band capabilities of a node (HW-007, WIFI-011, WIFI-012).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WifiBands {
    /// No Wi-Fi radio present (e.g. non-wireless Compute Modules).
    None,
    /// 2.4 GHz only (Pi Zero 2 W, Pi 3B).
    Band2_4Ghz,
    /// Dual-band 2.4 GHz and 5 GHz (Pi 3B+, 3A+, 4B, 400, 5, 500, wireless CM4/CM5).
    DualBand,
}

impl WifiBands {
    /// Returns `true` if the radio supports 5 GHz (WIFI-011, WIFI-012).
    #[must_use]
    pub const fn supports_5ghz(self) -> bool {
        matches!(self, Self::DualBand)
    }

    /// Returns `true` if the node has any Wi-Fi radio (WIFI-003, WIFI-013).
    #[must_use]
    pub const fn has_wifi(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// Boot and `data` storage medium type (HW-005, HW-009).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BootDeviceType {
    /// Industrial / high-endurance microSD card.
    MicroSd,
    /// Compute Module on-board eMMC flash.
    Emmc,
    /// USB-attached SSD (supported on BCM2711 and BCM2712).
    UsbSsd,
    /// PCIe NVMe SSD (supported on BCM2712).
    Nvme,
}

/// Detected board identity and hardware capabilities for the `Node` record (HW-009).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardCapabilities {
    /// Canonical board model (SPEC §4.1).
    pub model: BoardModel,
    /// Raw model string reported by the device tree or `/proc/cpuinfo`.
    pub model_name: String,
    /// Raw 32-bit Raspberry Pi OTP revision code.
    pub revision_code: u32,
    /// Hardware support tier (Tier 1, 2, or 3).
    pub support_tier: SupportTier,
    /// Broadcom SoC generation.
    pub soc_family: SocFamily,
    /// Total RAM in megabytes decoded from the revision code.
    pub ram_mb: u32,
    /// Number of HDMI display outputs (`1` or `2`).
    pub hdmi_outputs: u8,
    /// Supported Wi-Fi frequency bands.
    pub wifi_bands: WifiBands,
    /// Whether built-in or USB Ethernet is present.
    pub has_ethernet: bool,
    /// Whether a hardware RTC (`/sys/class/rtc/rtc*`) is present.
    pub has_rtc: bool,
    /// Storage device type from which the node booted.
    pub boot_device: BootDeviceType,
}

/// Errors returned when board or capability detection fails (HW-001, HW-005).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardError {
    /// Device tree and `/proc/cpuinfo` could not be found under the given root.
    MissingDeviceIdentity {
        /// Path that was inspected.
        root: String,
    },
    /// The revision code could not be parsed.
    InvalidRevisionCode {
        /// Raw details describing why parsing failed.
        reason: String,
    },
    /// The hardware is not a supported 64-bit Raspberry Pi board (HW-001).
    UnsupportedBoard {
        /// Raw revision code if available.
        revision_code: Option<u32>,
        /// Reported model string if available.
        model_name: Option<String>,
        /// Human-readable explanation of why the board is rejected.
        reason: String,
    },
    /// The boot device type is not supported on this SoC family (HW-005).
    UnsupportedBootDevice {
        /// Detected board model.
        model: BoardModel,
        /// Detected boot device type.
        boot_device: BootDeviceType,
        /// Explanation referencing HW-005.
        reason: String,
    },
}

impl fmt::Display for BoardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingDeviceIdentity { root } => write!(
                f,
                "cannot detect board under '{root}': neither /proc/device-tree/system/linux,revision nor /proc/cpuinfo is readable"
            ),
            Self::InvalidRevisionCode { reason } => {
                write!(f, "invalid Raspberry Pi revision code: {reason}")
            }
            Self::UnsupportedBoard {
                revision_code,
                model_name,
                reason,
            } => {
                let model = model_name.as_deref().unwrap_or("unknown model");
                match revision_code {
                    Some(rev) => write!(
                        f,
                        "unsupported board '{model}' (revision {rev:#08x}): {reason} (SPEC §4.1, HW-001)"
                    ),
                    None => write!(
                        f,
                        "unsupported board '{model}': {reason} (SPEC §4.1, HW-001)"
                    ),
                }
            }
            Self::UnsupportedBootDevice {
                model,
                boot_device,
                reason,
            } => write!(
                f,
                "unsupported boot device {boot_device:?} on {model}: {reason} (SPEC §4.2, HW-005)"
            ),
        }
    }
}

impl std::error::Error for BoardError {}

/// Decoded fields from a Raspberry Pi OTP revision code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedRevision {
    /// Supported board model.
    pub model: BoardModel,
    /// SoC family encoded in bits `[12..=15]`.
    pub soc_family: SocFamily,
    /// RAM size in megabytes encoded in bits `[20..=22]`.
    pub ram_mb: u32,
}

/// Decodes and validates a 32-bit Raspberry Pi OTP revision code against SPEC §4.1 (HW-001).
///
/// # Errors
///
/// Returns [`BoardError::UnsupportedBoard`] if the revision code uses the old-style
/// 32-bit encoding, targets a 32-bit SoC, targets the excluded Raspberry Pi 2B v1.2,
/// or encodes an unknown board/processor/RAM combination.
pub fn decode_revision(
    revision_code: u32,
    model_name: Option<&str>,
) -> Result<DecodedRevision, BoardError> {
    let model_opt = model_name.map(str::to_owned);

    // Bit 23 must be 1 for new-style revision codes; all old-style revisions are 32-bit Pi 1/CM1.
    if (revision_code & (1 << 23)) == 0 {
        return Err(BoardError::UnsupportedBoard {
            revision_code: Some(revision_code),
            model_name: model_opt,
            reason: "old-style 32-bit Raspberry Pi revision code is not supported".to_owned(),
        });
    }

    let processor_id = (revision_code >> 12) & 0x0f;
    let board_type = (revision_code >> 4) & 0xff;
    let memory_id = (revision_code >> 20) & 0x07;

    // Specifically call out Pi 2B (type 0x04), including v1.2 with BCM2837 (SPEC §2.2, §4.1).
    if board_type == 0x04 {
        return Err(BoardError::UnsupportedBoard {
            revision_code: Some(revision_code),
            model_name: model_opt,
            reason: "Raspberry Pi 2 Model B (including v1.2) is explicitly excluded".to_owned(),
        });
    }

    let soc_family = match processor_id {
        0 => {
            return Err(BoardError::UnsupportedBoard {
                revision_code: Some(revision_code),
                model_name: model_opt,
                reason: "32-bit BCM2835 SoC is not supported".to_owned(),
            });
        }
        1 => {
            return Err(BoardError::UnsupportedBoard {
                revision_code: Some(revision_code),
                model_name: model_opt,
                reason: "32-bit BCM2836 SoC is not supported".to_owned(),
            });
        }
        2 => SocFamily::Bcm2837,
        3 => SocFamily::Bcm2711,
        4 => SocFamily::Bcm2712,
        other => {
            return Err(BoardError::UnsupportedBoard {
                revision_code: Some(revision_code),
                model_name: model_opt,
                reason: format!("unrecognised processor ID {other:#x} in revision code"),
            });
        }
    };

    let model = match board_type {
        0x08 => BoardModel::Pi3B,
        0x0a => BoardModel::Cm3,
        0x0d => BoardModel::Pi3BPlus,
        0x0e => BoardModel::Pi3APlus,
        0x10 => BoardModel::Cm3Plus,
        0x11 => BoardModel::Pi4B,
        0x12 => BoardModel::PiZero2W,
        0x13 => BoardModel::Pi400,
        0x14 => BoardModel::Cm4,
        0x15 => BoardModel::Cm4S,
        0x17 => BoardModel::Pi5,
        0x18 => BoardModel::Pi500,
        0x19 => BoardModel::Cm5,
        0x00..=0x03 | 0x06 | 0x09 | 0x0c => {
            return Err(BoardError::UnsupportedBoard {
                revision_code: Some(revision_code),
                model_name: model_opt,
                reason: format!(
                    "32-bit Raspberry Pi board type {board_type:#04x} is not supported"
                ),
            });
        }
        other => {
            return Err(BoardError::UnsupportedBoard {
                revision_code: Some(revision_code),
                model_name: model_opt,
                reason: format!("unrecognised Raspberry Pi board type {other:#04x}"),
            });
        }
    };

    if model.soc_family() != soc_family {
        return Err(BoardError::UnsupportedBoard {
            revision_code: Some(revision_code),
            model_name: model_opt,
            reason: format!(
                "board model {model} expects {:?}, but revision code reports {soc_family:?}",
                model.soc_family()
            ),
        });
    }

    let ram_mb = match memory_id {
        0 => 256,
        1 => 512,
        2 => 1024,
        3 => 2048,
        4 => 4096,
        5 => 8192,
        6 => 16384,
        other => {
            return Err(BoardError::UnsupportedBoard {
                revision_code: Some(revision_code),
                model_name: model_opt,
                reason: format!("unrecognised RAM size code {other} in revision code"),
            });
        }
    };

    Ok(DecodedRevision {
        model,
        soc_family,
        ram_mb,
    })
}

/// Detects the current board and its hardware capabilities from `/` (HW-001, HW-009).
///
/// # Errors
///
/// Returns a [`BoardError`] if the system is not a supported 64-bit Raspberry Pi
/// or its boot device violates HW-005.
pub fn detect() -> Result<BoardCapabilities, BoardError> {
    detect_from_root(Path::new("/"))
}

/// Detects the board model and capabilities from a configurable filesystem root.
///
/// Passing `/` inspects the running machine; tests pass a fixture directory containing
/// captured `/proc` and `/sys` files.
///
/// # Errors
///
/// Returns a [`BoardError`] if identity files are missing, the board is unsupported
/// (HW-001), or the boot device is unsupported on the detected SoC family (HW-005).
pub fn detect_from_root(root: &Path) -> Result<BoardCapabilities, BoardError> {
    let dt_model = read_trimmed_file(&root.join("proc/device-tree/model"));
    let cpuinfo = read_trimmed_file(&root.join("proc/cpuinfo"));

    let model_name = dt_model
        .clone()
        .or_else(|| cpuinfo.as_deref().and_then(extract_cpuinfo_model))
        .unwrap_or_default();

    // If a device-tree model string is present and clearly not a Raspberry Pi, reject early.
    if !model_name.is_empty() && !model_name.contains("Raspberry Pi") {
        return Err(BoardError::UnsupportedBoard {
            revision_code: None,
            model_name: Some(model_name),
            reason: "device-tree model is not a Raspberry Pi".to_owned(),
        });
    }

    let revision_code = read_revision_code(root, cpuinfo.as_deref(), &model_name)?;
    let decoded = decode_revision(
        revision_code,
        if model_name.is_empty() {
            None
        } else {
            Some(model_name.as_str())
        },
    )?;

    let effective_model_name = if model_name.is_empty() {
        decoded.model.display_name().to_owned()
    } else {
        model_name
    };

    let wifi_bands = detect_wifi_bands(root, decoded.model);
    let has_ethernet = detect_ethernet(root, decoded.model);
    let has_rtc = detect_rtc(root);
    let boot_device = detect_boot_device(root, decoded.model)?;

    Ok(BoardCapabilities {
        model: decoded.model,
        model_name: effective_model_name,
        revision_code,
        support_tier: decoded.model.support_tier(),
        soc_family: decoded.soc_family,
        ram_mb: decoded.ram_mb,
        hdmi_outputs: decoded.model.hdmi_outputs(),
        wifi_bands,
        has_ethernet,
        has_rtc,
        boot_device,
    })
}

fn read_trimmed_file(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let trimmed = text.trim_matches(|c: char| c == '\0' || c.is_ascii_whitespace());
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn extract_cpuinfo_field<'a>(cpuinfo: &'a str, key: &str) -> Option<&'a str> {
    for line in cpuinfo.lines() {
        if let Some((k, v)) = line.split_once(':')
            && k.trim().eq_ignore_ascii_case(key)
        {
            let val = v.trim();
            if !val.is_empty() {
                return Some(val);
            }
        }
    }
    None
}

fn extract_cpuinfo_model(cpuinfo: &str) -> Option<String> {
    extract_cpuinfo_field(cpuinfo, "Model").map(str::to_owned)
}

fn read_revision_code(
    root: &Path,
    cpuinfo: Option<&str>,
    model_name: &str,
) -> Result<u32, BoardError> {
    let dt_rev_path = root.join("proc/device-tree/system/linux,revision");
    if let Ok(bytes) = fs::read(&dt_rev_path) {
        if bytes.len() == 4 {
            return Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        }
        let text = String::from_utf8_lossy(&bytes);
        let trimmed = text
            .trim_matches(|c: char| c == '\0' || c.is_ascii_whitespace())
            .trim_start_matches("0x")
            .trim_start_matches("0X");
        if !trimmed.is_empty() {
            return u32::from_str_radix(trimmed, 16).map_err(|e| BoardError::InvalidRevisionCode {
                reason: format!("cannot parse '{trimmed}' in {}: {e}", dt_rev_path.display()),
            });
        }
    }

    if let Some(info) = cpuinfo
        && let Some(rev_str) = extract_cpuinfo_field(info, "Revision")
    {
        let clean = rev_str.trim_start_matches("0x").trim_start_matches("0X");
        return u32::from_str_radix(clean, 16).map_err(|e| BoardError::InvalidRevisionCode {
            reason: format!("cannot parse Revision '{rev_str}' in /proc/cpuinfo: {e}"),
        });
    }

    if !model_name.is_empty() {
        return Err(BoardError::UnsupportedBoard {
            revision_code: None,
            model_name: Some(model_name.to_owned()),
            reason: "missing Raspberry Pi revision code in device tree and /proc/cpuinfo"
                .to_owned(),
        });
    }

    Err(BoardError::MissingDeviceIdentity {
        root: root.display().to_string(),
    })
}

fn detect_wifi_bands(root: &Path, model: BoardModel) -> WifiBands {
    let ieee80211_dir = root.join("sys/class/ieee80211");
    let phy_present = fs::read_dir(&ieee80211_dir).ok().map(|entries| {
        entries
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().starts_with("phy"))
    });

    match model {
        BoardModel::PiZero2W | BoardModel::Pi3B => {
            if phy_present == Some(false) {
                WifiBands::None
            } else {
                WifiBands::Band2_4Ghz
            }
        }
        BoardModel::Pi3BPlus
        | BoardModel::Pi3APlus
        | BoardModel::Pi4B
        | BoardModel::Pi400
        | BoardModel::Pi5
        | BoardModel::Pi500 => {
            if phy_present == Some(false) {
                WifiBands::None
            } else {
                WifiBands::DualBand
            }
        }
        BoardModel::Cm4 | BoardModel::Cm5 => {
            if phy_present == Some(true) {
                WifiBands::DualBand
            } else {
                WifiBands::None
            }
        }
        BoardModel::Cm3 | BoardModel::Cm3Plus | BoardModel::Cm4S => {
            if phy_present == Some(true) {
                WifiBands::Band2_4Ghz
            } else {
                WifiBands::None
            }
        }
    }
}

fn detect_ethernet(root: &Path, model: BoardModel) -> bool {
    let net_dir = root.join("sys/class/net");
    let Ok(entries) = fs::read_dir(&net_dir) else {
        return model.has_builtin_ethernet();
    };

    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str == "lo"
            || name_str.starts_with("wlan")
            || name_str.starts_with("ap")
            || name_str.starts_with("p2p")
            || name_str.starts_with("mon")
        {
            continue;
        }
        let iface_path = entry.path();
        if iface_path.join("wireless").exists() || iface_path.join("phy80211").exists() {
            continue;
        }
        return true;
    }

    false
}

fn detect_rtc(root: &Path) -> bool {
    let rtc_dir = root.join("sys/class/rtc");
    let Ok(entries) = fs::read_dir(&rtc_dir) else {
        return false;
    };
    entries
        .filter_map(Result::ok)
        .any(|e| e.file_name().to_string_lossy().starts_with("rtc"))
}

fn detect_boot_device(root: &Path, model: BoardModel) -> Result<BootDeviceType, BoardError> {
    let block_dev = find_mounted_boot_block_device(root).or_else(|| find_sys_block_device(root));

    let boot_device = match block_dev.as_deref() {
        Some(dev) if dev.starts_with("nvme") => BootDeviceType::Nvme,
        Some(dev) if dev.starts_with("sd") => BootDeviceType::UsbSsd,
        Some(dev) if dev.starts_with("mmcblk") => classify_mmc_device(root, dev, model),
        _ => match model {
            BoardModel::Cm3 | BoardModel::Cm3Plus | BoardModel::Cm4S => BootDeviceType::Emmc,
            _ => BootDeviceType::MicroSd,
        },
    };

    validate_boot_device(model, boot_device)?;
    Ok(boot_device)
}

fn find_mounted_boot_block_device(root: &Path) -> Option<String> {
    let mounts = read_trimmed_file(&root.join("proc/mounts"))?;
    let mut root_source = None;

    for line in mounts.lines() {
        let mut parts = line.split_whitespace();
        let Some(source) = parts.next() else {
            continue;
        };
        let Some(target) = parts.next() else {
            continue;
        };
        if !source.starts_with("/dev/") {
            continue;
        }
        if target == "/data" {
            if let Some(base) = base_block_device_name(source) {
                return Some(base);
            }
        } else if target == "/" && root_source.is_none() {
            root_source = base_block_device_name(source);
        }
    }

    root_source
}

fn base_block_device_name(dev_path: &str) -> Option<String> {
    let dev = dev_path.strip_prefix("/dev/")?;
    if dev.starts_with("mmcblk") || dev.starts_with("nvme") {
        // Strip partition suffix like `p3` from `mmcblk0p3` or `nvme0n1p3`.
        if let Some((base, part)) = dev.rsplit_once('p')
            && !base.is_empty()
            && base.chars().last().is_some_and(|c| c.is_ascii_digit())
            && !part.is_empty()
            && part.chars().all(|c| c.is_ascii_digit())
        {
            return Some(base.to_owned());
        }
        return Some(dev.to_owned());
    }
    if dev.starts_with("sd") {
        let base = dev.trim_end_matches(|c: char| c.is_ascii_digit());
        if !base.is_empty() {
            return Some(base.to_owned());
        }
    }
    None
}

fn find_sys_block_device(root: &Path) -> Option<String> {
    let block_dir = root.join("sys/block");
    let entries = fs::read_dir(&block_dir).ok()?;
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            (name.starts_with("nvme") || name.starts_with("sd") || name.starts_with("mmcblk"))
                && !name.ends_with("boot0")
                && !name.ends_with("boot1")
                && !name.ends_with("rpmb")
        })
        .collect();
    names.sort();
    names.into_iter().next()
}

fn classify_mmc_device(root: &Path, dev: &str, model: BoardModel) -> BootDeviceType {
    let type_path = root.join("sys/block").join(dev).join("device/type");
    if let Some(mmc_type) = read_trimmed_file(&type_path) {
        if mmc_type.eq_ignore_ascii_case("MMC") {
            return BootDeviceType::Emmc;
        }
        if mmc_type.eq_ignore_ascii_case("SD") {
            return BootDeviceType::MicroSd;
        }
    }

    if root.join("sys/block").join(format!("{dev}boot0")).exists() {
        return BootDeviceType::Emmc;
    }

    match model {
        BoardModel::Cm3 | BoardModel::Cm3Plus | BoardModel::Cm4S => BootDeviceType::Emmc,
        _ => BootDeviceType::MicroSd,
    }
}

fn validate_boot_device(model: BoardModel, boot_device: BootDeviceType) -> Result<(), BoardError> {
    let soc = model.soc_family();
    match (soc, boot_device) {
        (SocFamily::Bcm2837, BootDeviceType::UsbSsd) => Err(BoardError::UnsupportedBootDevice {
            model,
            boot_device,
            reason: "USB boot on the BCM2837 family is not supported".to_owned(),
        }),
        (SocFamily::Bcm2837 | SocFamily::Bcm2711, BootDeviceType::Nvme) => {
            Err(BoardError::UnsupportedBootDevice {
                model,
                boot_device,
                reason: "NVMe boot is only supported on the BCM2712 family".to_owned(),
            })
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{
        BoardCapabilities, BoardError, BoardModel, BootDeviceType, SocFamily, SupportTier,
        WifiBands, detect_from_root,
    };

    struct TempFixture {
        path: PathBuf,
    }

    impl TempFixture {
        fn new(label: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "chalkboard-board-test-{}-{label}-{id}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create temp fixture root");
            Self { path }
        }

        fn root(&self) -> &Path {
            &self.path
        }

        fn write_device_tree(&self, model: &str, revision: u32) {
            let dt_sys = self.path.join("proc/device-tree/system");
            fs::create_dir_all(&dt_sys).expect("create proc/device-tree/system");
            let mut model_bytes = model.as_bytes().to_vec();
            model_bytes.push(0);
            fs::write(self.path.join("proc/device-tree/model"), model_bytes)
                .expect("write proc/device-tree/model");
            fs::write(dt_sys.join("linux,revision"), revision.to_be_bytes())
                .expect("write linux,revision");
        }

        fn add_wifi_phy(&self, phy_name: &str) {
            fs::create_dir_all(self.path.join("sys/class/ieee80211").join(phy_name))
                .expect("create ieee80211 phy");
        }

        fn add_empty_ieee80211(&self) {
            fs::create_dir_all(self.path.join("sys/class/ieee80211"))
                .expect("create empty ieee80211");
        }

        fn add_net_iface(&self, iface: &str, is_wireless: bool) {
            let iface_dir = self.path.join("sys/class/net").join(iface);
            fs::create_dir_all(&iface_dir).expect("create net iface");
            if is_wireless {
                fs::create_dir_all(iface_dir.join("wireless")).expect("create wireless marker");
            }
        }

        fn add_rtc(&self, rtc_name: &str) {
            fs::create_dir_all(self.path.join("sys/class/rtc").join(rtc_name)).expect("create rtc");
        }

        fn add_mounts(&self, mounts: &str) {
            let proc_dir = self.path.join("proc");
            fs::create_dir_all(&proc_dir).expect("create proc");
            fs::write(proc_dir.join("mounts"), mounts).expect("write proc/mounts");
        }

        fn add_mmc_block(&self, dev: &str, mmc_type: &str) {
            let dev_dir = self.path.join("sys/block").join(dev).join("device");
            fs::create_dir_all(&dev_dir).expect("create sys/block/<dev>/device");
            fs::write(dev_dir.join("type"), format!("{mmc_type}\n")).expect("write mmc type");
        }
    }

    impl Drop for TempFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    struct BoardCase {
        label: &'static str,
        dt_model: &'static str,
        revision: u32,
        with_wifi: bool,
        with_eth: bool,
        with_rtc: bool,
        mmc_type: &'static str,
        expected: BoardCapabilities,
    }

    fn run_board_case(case: &BoardCase) {
        let fx = TempFixture::new(case.label);
        fx.write_device_tree(case.dt_model, case.revision);
        if case.with_wifi {
            fx.add_wifi_phy("phy0");
            fx.add_net_iface("wlan0", true);
        } else {
            fx.add_empty_ieee80211();
        }
        fx.add_net_iface("lo", false);
        if case.with_eth {
            fx.add_net_iface("eth0", false);
        }
        if case.with_rtc {
            fx.add_rtc("rtc0");
        }
        fx.add_mounts("/dev/mmcblk0p4 /data f2fs rw,noatime 0 0\n");
        fx.add_mmc_block("mmcblk0", case.mmc_type);

        let actual = detect_from_root(fx.root()).expect("board detection must succeed");
        assert_eq!(actual, case.expected, "mismatch for case {}", case.label);
    }

    #[test]
    fn hw_001_hw_009_tier1_boards_detected() {
        let cases = [
            BoardCase {
                label: "pi-zero-2-w",
                dt_model: "Raspberry Pi Zero 2 W Rev 1.0",
                revision: 0x0090_2120,
                with_wifi: true,
                with_eth: false,
                with_rtc: false,
                mmc_type: "SD",
                expected: BoardCapabilities {
                    model: BoardModel::PiZero2W,
                    model_name: "Raspberry Pi Zero 2 W Rev 1.0".to_owned(),
                    revision_code: 0x0090_2120,
                    support_tier: SupportTier::Tier1,
                    soc_family: SocFamily::Bcm2837,
                    ram_mb: 512,
                    hdmi_outputs: 1,
                    wifi_bands: WifiBands::Band2_4Ghz,
                    has_ethernet: false,
                    has_rtc: false,
                    boot_device: BootDeviceType::MicroSd,
                },
            },
            BoardCase {
                label: "pi-4b-4gb",
                dt_model: "Raspberry Pi 4 Model B Rev 1.4",
                revision: 0x00c0_3114,
                with_wifi: true,
                with_eth: true,
                with_rtc: false,
                mmc_type: "SD",
                expected: BoardCapabilities {
                    model: BoardModel::Pi4B,
                    model_name: "Raspberry Pi 4 Model B Rev 1.4".to_owned(),
                    revision_code: 0x00c0_3114,
                    support_tier: SupportTier::Tier1,
                    soc_family: SocFamily::Bcm2711,
                    ram_mb: 4096,
                    hdmi_outputs: 2,
                    wifi_bands: WifiBands::DualBand,
                    has_ethernet: true,
                    has_rtc: false,
                    boot_device: BootDeviceType::MicroSd,
                },
            },
            BoardCase {
                label: "pi-5-8gb",
                dt_model: "Raspberry Pi 5 Model B Rev 1.0",
                revision: 0x00d0_4170,
                with_wifi: true,
                with_eth: true,
                with_rtc: true,
                mmc_type: "SD",
                expected: BoardCapabilities {
                    model: BoardModel::Pi5,
                    model_name: "Raspberry Pi 5 Model B Rev 1.0".to_owned(),
                    revision_code: 0x00d0_4170,
                    support_tier: SupportTier::Tier1,
                    soc_family: SocFamily::Bcm2712,
                    ram_mb: 8192,
                    hdmi_outputs: 2,
                    wifi_bands: WifiBands::DualBand,
                    has_ethernet: true,
                    has_rtc: true,
                    boot_device: BootDeviceType::MicroSd,
                },
            },
        ];

        for case in &cases {
            run_board_case(case);
        }
    }

    #[test]
    fn hw_001_hw_009_tier2_boards_detected() {
        let cases = [
            BoardCase {
                label: "pi-3b",
                dt_model: "Raspberry Pi 3 Model B Rev 1.2",
                revision: 0x00a0_2082,
                with_wifi: true,
                with_eth: true,
                with_rtc: false,
                mmc_type: "SD",
                expected: BoardCapabilities {
                    model: BoardModel::Pi3B,
                    model_name: "Raspberry Pi 3 Model B Rev 1.2".to_owned(),
                    revision_code: 0x00a0_2082,
                    support_tier: SupportTier::Tier2,
                    soc_family: SocFamily::Bcm2837,
                    ram_mb: 1024,
                    hdmi_outputs: 1,
                    wifi_bands: WifiBands::Band2_4Ghz,
                    has_ethernet: true,
                    has_rtc: false,
                    boot_device: BootDeviceType::MicroSd,
                },
            },
            BoardCase {
                label: "pi-3b-plus",
                dt_model: "Raspberry Pi 3 Model B Plus Rev 1.3",
                revision: 0x00a0_20d3,
                with_wifi: true,
                with_eth: true,
                with_rtc: false,
                mmc_type: "SD",
                expected: BoardCapabilities {
                    model: BoardModel::Pi3BPlus,
                    model_name: "Raspberry Pi 3 Model B Plus Rev 1.3".to_owned(),
                    revision_code: 0x00a0_20d3,
                    support_tier: SupportTier::Tier2,
                    soc_family: SocFamily::Bcm2837,
                    ram_mb: 1024,
                    hdmi_outputs: 1,
                    wifi_bands: WifiBands::DualBand,
                    has_ethernet: true,
                    has_rtc: false,
                    boot_device: BootDeviceType::MicroSd,
                },
            },
            BoardCase {
                label: "pi-3a-plus",
                dt_model: "Raspberry Pi 3 Model A Plus Rev 1.0",
                revision: 0x0090_20e0,
                with_wifi: true,
                with_eth: false,
                with_rtc: false,
                mmc_type: "SD",
                expected: BoardCapabilities {
                    model: BoardModel::Pi3APlus,
                    model_name: "Raspberry Pi 3 Model A Plus Rev 1.0".to_owned(),
                    revision_code: 0x0090_20e0,
                    support_tier: SupportTier::Tier2,
                    soc_family: SocFamily::Bcm2837,
                    ram_mb: 512,
                    hdmi_outputs: 1,
                    wifi_bands: WifiBands::DualBand,
                    has_ethernet: false,
                    has_rtc: false,
                    boot_device: BootDeviceType::MicroSd,
                },
            },
            BoardCase {
                label: "pi-400",
                dt_model: "Raspberry Pi 400 Rev 1.0",
                revision: 0x00c0_3130,
                with_wifi: true,
                with_eth: true,
                with_rtc: false,
                mmc_type: "SD",
                expected: BoardCapabilities {
                    model: BoardModel::Pi400,
                    model_name: "Raspberry Pi 400 Rev 1.0".to_owned(),
                    revision_code: 0x00c0_3130,
                    support_tier: SupportTier::Tier2,
                    soc_family: SocFamily::Bcm2711,
                    ram_mb: 4096,
                    hdmi_outputs: 2,
                    wifi_bands: WifiBands::DualBand,
                    has_ethernet: true,
                    has_rtc: false,
                    boot_device: BootDeviceType::MicroSd,
                },
            },
            BoardCase {
                label: "pi-500",
                dt_model: "Raspberry Pi 500 Rev 1.0",
                revision: 0x00d0_4180,
                with_wifi: true,
                with_eth: true,
                with_rtc: true,
                mmc_type: "SD",
                expected: BoardCapabilities {
                    model: BoardModel::Pi500,
                    model_name: "Raspberry Pi 500 Rev 1.0".to_owned(),
                    revision_code: 0x00d0_4180,
                    support_tier: SupportTier::Tier2,
                    soc_family: SocFamily::Bcm2712,
                    ram_mb: 8192,
                    hdmi_outputs: 2,
                    wifi_bands: WifiBands::DualBand,
                    has_ethernet: true,
                    has_rtc: true,
                    boot_device: BootDeviceType::MicroSd,
                },
            },
            BoardCase {
                label: "cm4-emmc-wifi",
                dt_model: "Raspberry Pi Compute Module 4 Rev 1.1",
                revision: 0x00b0_3141,
                with_wifi: true,
                with_eth: true,
                with_rtc: true,
                mmc_type: "MMC",
                expected: BoardCapabilities {
                    model: BoardModel::Cm4,
                    model_name: "Raspberry Pi Compute Module 4 Rev 1.1".to_owned(),
                    revision_code: 0x00b0_3141,
                    support_tier: SupportTier::Tier2,
                    soc_family: SocFamily::Bcm2711,
                    ram_mb: 2048,
                    hdmi_outputs: 2,
                    wifi_bands: WifiBands::DualBand,
                    has_ethernet: true,
                    has_rtc: true,
                    boot_device: BootDeviceType::Emmc,
                },
            },
            BoardCase {
                label: "cm5-emmc-no-wifi",
                dt_model: "Raspberry Pi Compute Module 5 Rev 1.0",
                revision: 0x00c0_4190,
                with_wifi: false,
                with_eth: true,
                with_rtc: true,
                mmc_type: "MMC",
                expected: BoardCapabilities {
                    model: BoardModel::Cm5,
                    model_name: "Raspberry Pi Compute Module 5 Rev 1.0".to_owned(),
                    revision_code: 0x00c0_4190,
                    support_tier: SupportTier::Tier2,
                    soc_family: SocFamily::Bcm2712,
                    ram_mb: 4096,
                    hdmi_outputs: 2,
                    wifi_bands: WifiBands::None,
                    has_ethernet: true,
                    has_rtc: true,
                    boot_device: BootDeviceType::Emmc,
                },
            },
        ];

        for case in &cases {
            run_board_case(case);
        }
    }

    #[test]
    fn hw_001_hw_009_tier3_boards_detected() {
        let cases = [
            BoardCase {
                label: "cm3",
                dt_model: "Raspberry Pi Compute Module 3 Rev 1.0",
                revision: 0x00a0_20a0,
                with_wifi: false,
                with_eth: true,
                with_rtc: false,
                mmc_type: "MMC",
                expected: BoardCapabilities {
                    model: BoardModel::Cm3,
                    model_name: "Raspberry Pi Compute Module 3 Rev 1.0".to_owned(),
                    revision_code: 0x00a0_20a0,
                    support_tier: SupportTier::Tier3,
                    soc_family: SocFamily::Bcm2837,
                    ram_mb: 1024,
                    hdmi_outputs: 1,
                    wifi_bands: WifiBands::None,
                    has_ethernet: true,
                    has_rtc: false,
                    boot_device: BootDeviceType::Emmc,
                },
            },
            BoardCase {
                label: "cm3-plus",
                dt_model: "Raspberry Pi Compute Module 3 Plus Rev 1.0",
                revision: 0x00a0_2100,
                with_wifi: false,
                with_eth: true,
                with_rtc: false,
                mmc_type: "MMC",
                expected: BoardCapabilities {
                    model: BoardModel::Cm3Plus,
                    model_name: "Raspberry Pi Compute Module 3 Plus Rev 1.0".to_owned(),
                    revision_code: 0x00a0_2100,
                    support_tier: SupportTier::Tier3,
                    soc_family: SocFamily::Bcm2837,
                    ram_mb: 1024,
                    hdmi_outputs: 1,
                    wifi_bands: WifiBands::None,
                    has_ethernet: true,
                    has_rtc: false,
                    boot_device: BootDeviceType::Emmc,
                },
            },
            BoardCase {
                label: "cm4s",
                dt_model: "Raspberry Pi Compute Module 4S Rev 1.0",
                revision: 0x00a0_3150,
                with_wifi: false,
                with_eth: true,
                with_rtc: false,
                mmc_type: "MMC",
                expected: BoardCapabilities {
                    model: BoardModel::Cm4S,
                    model_name: "Raspberry Pi Compute Module 4S Rev 1.0".to_owned(),
                    revision_code: 0x00a0_3150,
                    support_tier: SupportTier::Tier3,
                    soc_family: SocFamily::Bcm2711,
                    ram_mb: 1024,
                    hdmi_outputs: 1,
                    wifi_bands: WifiBands::None,
                    has_ethernet: true,
                    has_rtc: false,
                    boot_device: BootDeviceType::Emmc,
                },
            },
        ];

        for case in &cases {
            run_board_case(case);
        }
    }

    #[test]
    fn hw_001_rejects_unsupported_boards_clearly() {
        // 1. Raspberry Pi 2 Model B v1.2 (uses BCM2837, but explicitly excluded in SPEC §2.2, §4.1).
        let pi2_v1_2 = TempFixture::new("pi2b-v1-2");
        pi2_v1_2.write_device_tree("Raspberry Pi 2 Model B Rev 1.2", 0x00a2_2042);
        let err = detect_from_root(pi2_v1_2.root()).expect_err("Pi 2B v1.2 must be rejected");
        assert!(
            matches!(err, BoardError::UnsupportedBoard { .. }),
            "expected UnsupportedBoard, got {err:?}"
        );
        assert!(err.to_string().contains("Raspberry Pi 2 Model B"));

        // 2. 32-bit Raspberry Pi Zero W (BCM2835, type 0x0c).
        let pi_zero_w = TempFixture::new("pi-zero-w-32bit");
        pi_zero_w.write_device_tree("Raspberry Pi Zero W Rev 1.1", 0x0090_00c1);
        let err = detect_from_root(pi_zero_w.root()).expect_err("Pi Zero W must be rejected");
        assert!(err.to_string().contains("BCM2835"));

        // 3. Old-style 32-bit revision code (bit 23 == 0).
        let pi1 = TempFixture::new("pi1-old-style");
        pi1.write_device_tree("Raspberry Pi Model B Rev 2", 0x0000_000e);
        let err = detect_from_root(pi1.root()).expect_err("Old-style Pi 1 must be rejected");
        assert!(err.to_string().contains("old-style 32-bit"));

        // 4. Non-Raspberry Pi board.
        let other = TempFixture::new("generic-arm64");
        other.write_device_tree("Generic ARM64 SBC", 0x00c0_3114);
        let err = detect_from_root(other.root()).expect_err("Non-Pi board must be rejected");
        assert!(err.to_string().contains("not a Raspberry Pi"));
    }

    #[test]
    fn hw_005_boot_devices_and_bcm2837_usb_rejection() {
        // USB SSD boot on Pi 4B (BCM2711) -> allowed.
        let pi4_usb = TempFixture::new("pi4-usb-ssd");
        pi4_usb.write_device_tree("Raspberry Pi 4 Model B Rev 1.5", 0x00c0_3115);
        pi4_usb.add_mounts("/dev/sda4 /data f2fs rw,noatime 0 0\n");
        let caps = detect_from_root(pi4_usb.root()).expect("Pi 4B USB SSD boot is supported");
        assert_eq!(caps.boot_device, BootDeviceType::UsbSsd);

        // NVMe boot on Pi 5 (BCM2712) -> allowed.
        let pi5_nvme = TempFixture::new("pi5-nvme");
        pi5_nvme.write_device_tree("Raspberry Pi 5 Model B Rev 1.0", 0x00d0_4170);
        pi5_nvme.add_mounts("/dev/nvme0n1p4 /data f2fs rw,noatime 0 0\n");
        let caps = detect_from_root(pi5_nvme.root()).expect("Pi 5 NVMe boot is supported");
        assert_eq!(caps.boot_device, BootDeviceType::Nvme);

        // USB SSD boot on Pi 3B+ (BCM2837) -> rejected per HW-005.
        let pi3_usb = TempFixture::new("pi3-usb-rejected");
        pi3_usb.write_device_tree("Raspberry Pi 3 Model B Plus Rev 1.3", 0x00a0_20d3);
        pi3_usb.add_mounts("/dev/sda4 /data f2fs rw,noatime 0 0\n");
        let err = detect_from_root(pi3_usb.root()).expect_err("BCM2837 USB boot must be rejected");
        assert!(matches!(
            err,
            BoardError::UnsupportedBootDevice {
                model: BoardModel::Pi3BPlus,
                boot_device: BootDeviceType::UsbSsd,
                ..
            }
        ));
    }

    #[test]
    fn hw_004_hw_008_usb_ethernet_and_ds3231_rtc_on_zero_2_w() {
        // Zero 2 W with a USB Ethernet dongle (`enx...`) and DS3231 I2C RTC (`rtc0`),
        // identified via `/proc/cpuinfo` fallback.
        let fx = TempFixture::new("zero2w-usb-eth-rtc");
        let proc_dir = fx.root().join("proc");
        fs::create_dir_all(&proc_dir).expect("create proc");
        fs::write(
            proc_dir.join("cpuinfo"),
            "processor\t: 0\nRevision\t: 902120\nModel\t\t: Raspberry Pi Zero 2 W Rev 1.0\n",
        )
        .expect("write cpuinfo");
        fx.add_wifi_phy("phy0");
        fx.add_net_iface("wlan0", true);
        fx.add_net_iface("enx00e04c680001", false);
        fx.add_rtc("rtc0");

        let caps = detect_from_root(fx.root()).expect("Zero 2 W cpuinfo fallback must succeed");
        assert_eq!(caps.model, BoardModel::PiZero2W);
        assert!(caps.has_ethernet);
        assert!(caps.has_rtc);
        assert_eq!(caps.wifi_bands, WifiBands::Band2_4Ghz);
    }
}
