//! 无线键鼠电量只读采集模块：
//! 1. 罗技 HID++ 2.0 统一电池协议（覆盖 Lightspeed / Bolt / Unifying 与有线充电设备，无需运行 G HUB）；
//! 2. VGN / Evision Telink 2.4G 键盘私有 HID 通道（如 VGN V98pro V2 `320F:5088`，无需运行 VHUB）；
//! 3. Windows 原生蓝牙/BLE 设备电池属性（`DEVPKEY_Device_Battery`，覆盖常见蓝牙键鼠）。
//!
//! 省电与安全约束：
//! - 正常每 60 秒轮询一次，避免高频通信反复唤醒休眠中的无线鼠标/键盘；
//! - 设备休眠未响应时在 30 分钟内保留最近一次有效电量并标记 `stale = true`，接收器拔出时自动移除；
//! - 仅在用户开启键鼠电量且灵动岛可见（非全屏避让细线）时工作，休眠时零轮询。

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const PERIPHERALS_EVENT: &str = "island-peripherals";

/// 单台键鼠外设的电量快照。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PeripheralBattery {
    pub id: String,
    /// "keyboard" | "mouse"
    pub kind: String,
    pub name: String,
    pub percent: u8,
    pub charging: bool,
    /// "logitech-hidpp" | "vgn-24g" | "bluetooth"
    pub transport: String,
    /// 无线设备短时休眠时复用上次有效电量。
    pub stale: bool,
    pub updated_at_ms: u64,
}

/// 广播给灵动岛与设置窗口的外设电量总快照。
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PeripheralBatterySnapshot {
    pub devices: Vec<PeripheralBattery>,
    pub checked_at_ms: u64,
}

/// 供 Tauri 状态管理的最新外设电量缓存。
#[derive(Default)]
pub struct PeripheralBatteryState(pub Arc<Mutex<PeripheralBatterySnapshot>>);

/// 单次低电量主动提醒事件（分「偏低 ≤20%」与「告急 ≤10%」两档）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LowBatteryAlert {
    pub id: String,
    pub critical: bool,
    pub label: String,
    pub tooltip: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
enum AlertLevel {
    #[default]
    None,
    Warn20,
    Crit10,
}

/// 无线键鼠低电量主动提醒状态机：
/// - 仅在未充电且非休眠缓存读数时触发；
/// - 同一设备在 20% 与 10% 两个档位各提醒一次，绝不重复刷屏；
/// - 设备进入充电状态或电量回升至 26% 以上时自动重置档位。
#[derive(Default)]
pub struct LowBatteryTracker {
    notified: HashMap<String, AlertLevel>,
}

impl LowBatteryTracker {
    pub const WARN_THRESHOLD: u8 = 20;
    pub const CRIT_THRESHOLD: u8 = 10;
    pub const RESET_THRESHOLD: u8 = 26;

    pub fn clear(&mut self) {
        self.notified.clear();
    }

    pub fn evaluate(&mut self, devices: &[PeripheralBattery]) -> Vec<LowBatteryAlert> {
        let active_ids: HashSet<&str> = devices.iter().map(|d| d.id.as_str()).collect();
        self.notified.retain(|id, _| active_ids.contains(id.as_str()));

        let mut alerts = Vec::new();
        for dev in devices {
            if dev.charging || dev.percent >= Self::RESET_THRESHOLD {
                self.notified.remove(&dev.id);
                continue;
            }
            if dev.stale {
                continue;
            }
            let target = if dev.percent <= Self::CRIT_THRESHOLD {
                AlertLevel::Crit10
            } else if dev.percent <= Self::WARN_THRESHOLD {
                AlertLevel::Warn20
            } else {
                AlertLevel::None
            };
            if target == AlertLevel::None {
                continue;
            }
            let prev = self.notified.get(&dev.id).copied().unwrap_or(AlertLevel::None);
            if target > prev {
                self.notified.insert(dev.id.clone(), target);
                let kind_label = if dev.kind == "keyboard" { "键盘" } else { "鼠标" };
                let critical = target == AlertLevel::Crit10;
                let label = if critical {
                    format!("{kind_label}电量告急 {}%", dev.percent)
                } else {
                    format!("{kind_label}电量偏低 {}%", dev.percent)
                };
                let tooltip = format!(
                    "{} 当前剩余 {}%，{}",
                    dev.name,
                    dev.percent,
                    if critical { "请尽快充电以防断连" } else { "建议及时充电" }
                );
                alerts.push(LowBatteryAlert {
                    id: dev.id.clone(),
                    critical,
                    label,
                    tooltip,
                });
            }
        }
        alerts
    }
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or(0)
}

/// 根据罗技 HID++ `0x0005`（Device Type）或产品名推断设备属于键盘还是鼠标。
pub fn classify_logitech_kind(device_type: Option<u8>, product_name: &str) -> &'static str {
    match device_type {
        Some(0) | Some(1) | Some(2) => "keyboard", // Keyboard / Remote / Numpad
        Some(3) | Some(4) | Some(5) | Some(6) => "mouse", // Mouse / Touchpad / Trackball / Presenter
        _ => {
            let lower = product_name.to_ascii_lowercase();
            if lower.contains("key") || lower.contains("kbd") || lower.contains("g915") || lower.contains("mx keys") {
                "keyboard"
            } else {
                "mouse"
            }
        }
    }
}

/// 解析罗技 HID++ 2.0 `0x1004`（Unified Battery `get_status`）响应。
/// 响应格式：`[0x11, dev_idx, feat_idx, fn|sw, soc(0..=100), level, charging_status, ...]`
pub fn parse_logitech_unified_battery(resp: &[u8]) -> Option<(u8, bool)> {
    if resp.len() < 7 || resp[0] != 0x11 || resp[2] == 0x8F || resp[2] == 0xFF {
        return None;
    }
    let soc = resp[4];
    let level = resp[5];
    let charging_code = resp[6];
    let percent = if (1..=100).contains(&soc) {
        soc
    } else if soc == 0 {
        // 极少数只上报离散档位的旧固件：bit0=临界(5%), bit1=低(20%), bit2=良好(50%), bit3=满(90%)
        if level & 0x08 != 0 {
            90
        } else if level & 0x04 != 0 {
            50
        } else if level & 0x02 != 0 {
            20
        } else if level & 0x01 != 0 {
            5
        } else {
            return None;
        }
    } else {
        return None;
    };
    // 1=充电中, 2=接近充满, 3=已充满（外接供电）, 4=慢充
    let charging = matches!(charging_code, 1 | 2 | 3 | 4);
    Some((percent, charging))
}

/// 解析罗技 HID++ 2.0 `0x1000`（Battery Status）响应。
/// 响应格式：`[0x11, dev_idx, feat_idx, fn|sw, level(0..=100), next_level, status, ...]`
pub fn parse_logitech_battery_status(resp: &[u8]) -> Option<(u8, bool)> {
    if resp.len() < 7 || resp[0] != 0x11 || resp[2] == 0x8F || resp[2] == 0xFF {
        return None;
    }
    let percent = resp[4];
    if percent > 100 {
        return None;
    }
    let charging = matches!(resp[6], 1 | 2 | 3 | 4);
    Some((percent, charging))
}

/// 解析罗技 HID++ 2.0 `0x1001`（Battery Voltage）响应，将毫伏映射为近似电量百分比。
pub fn parse_logitech_battery_voltage(resp: &[u8]) -> Option<(u8, bool)> {
    if resp.len() < 7 || resp[0] != 0x11 || resp[2] == 0x8F || resp[2] == 0xFF {
        return None;
    }
    let mv = u16::from_be_bytes([resp[4], resp[5]]);
    if !(3000..=4500).contains(&mv) {
        return None;
    }
    let charging = (resp[6] & 0x80) != 0;
    // 常见单节锂电放电曲线近似（3500mV=0%, 4180mV=100%）
    let table: [(u16, u8); 11] = [
        (4180, 100),
        (4100, 90),
        (4020, 80),
        (3950, 70),
        (3890, 60),
        (3840, 50),
        (3790, 40),
        (3750, 30),
        (3700, 20),
        (3620, 10),
        (3500, 0),
    ];
    let percent = if mv >= table[0].0 {
        100
    } else if mv <= table[table.len() - 1].0 {
        0
    } else {
        let mut out = 0u8;
        for w in table.windows(2) {
            let (hi_v, hi_p) = w[0];
            let (lo_v, lo_p) = w[1];
            if mv <= hi_v && mv >= lo_v {
                let ratio = (mv - lo_v) as f64 / (hi_v - lo_v) as f64;
                out = (lo_p as f64 + ratio * (hi_p - lo_p) as f64).round() as u8;
                break;
            }
        }
        out
    };
    Some((percent, charging))
}

/// 解析 VGN / Evision Telink 2.4G 键盘（`320F:5088`, Report ID `0x04`, 命令 `0x1A`）响应。
/// 实测响应格式：`[0x04, sum_lo, sum_hi, 0x1A, ..., status(idx 7 == 0), percent(idx 8), charging(idx 9)]`
pub fn parse_vgn_telink_battery(resp: &[u8]) -> Option<(u8, bool)> {
    if resp.len() < 10 || resp[0] != 0x04 || resp[3] != 0x1A || resp[7] != 0x00 {
        return None;
    }
    let percent = resp[8];
    if percent > 100 {
        return None;
    }
    let charging = resp[9] != 0;
    Some((percent, charging))
}

/// 清理外设名称尾部的接收器冗余后缀，便于胶囊与悬停提示阅读。
pub fn clean_peripheral_name(raw: &str, fallback: &str) -> String {
    let trimmed = raw.trim().trim_matches('\0').trim();
    if trimmed.is_empty() {
        return fallback.to_string();
    }
    let stripped = trimmed
        .strip_suffix(" 2.4G Dongle")
        .or_else(|| trimmed.strip_suffix(" Dongle"))
        .or_else(|| trimmed.strip_suffix(" Wireless Receiver"))
        .or_else(|| trimmed.strip_suffix(" Receiver"))
        .unwrap_or(trimmed)
        .trim();
    if stripped.is_empty() {
        fallback.to_string()
    } else {
        stripped.to_string()
    }
}

/// 根据名称推断蓝牙外设是否为键盘或鼠标（过滤掉蓝牙耳机、手机等非键鼠设备）。
pub fn classify_bluetooth_peripheral(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    // 明确排除音频/手机类蓝牙设备
    for audio in [
        "freeclip", "airpods", "buds", "headset", "headphone", "earbud", "hands-free",
        "speaker", "audio", "耳机", "音箱", "手机", "watch", "band",
    ] {
        if lower.contains(audio) {
            return None;
        }
    }
    for kb in ["keyboard", "kbd", "keychron", "nuphy", "hhkb", "filco", "vgn", "mx keys", "k380", "k580", "键盘"] {
        if lower.contains(kb) {
            return Some("keyboard");
        }
    }
    for ms in ["mouse", "trackpad", "trackball", "mx master", "mx anywhere", "g304", "g pro", "viper", "deathadder", "鼠标", "触控板"] {
        if lower.contains(ms) {
            return Some("mouse");
        }
    }
    None
}

/// 将本次实时扫描结果与缓存合并：休眠设备在有效期（30 分钟）内保留上次有效读数并标 `stale`。
pub fn merge_peripheral_cache(
    cache: &mut HashMap<String, PeripheralBattery>,
    live: Vec<PeripheralBattery>,
    present_prefixes: &HashSet<String>,
    now_ms: u64,
) -> Vec<PeripheralBattery> {
    const MAX_STALE_MS: u64 = 30 * 60 * 1000;
    let mut seen_ids = HashSet::new();
    for item in live {
        seen_ids.insert(item.id.clone());
        cache.insert(item.id.clone(), item);
    }

    cache.retain(|id, item| {
        if seen_ids.contains(id) {
            return true;
        }
        // 仅保留对应接收器或蓝牙总线仍存在且未超时的缓存项
        let still_attached = present_prefixes
            .iter()
            .any(|prefix| id.starts_with(prefix));
        if !still_attached {
            return false;
        }
        if now_ms.saturating_sub(item.updated_at_ms) > MAX_STALE_MS {
            return false;
        }
        item.stale = true;
        true
    });

    // 统一排序：键盘在前、鼠标在后，同类型按名称稳定排序
    let mut list: Vec<PeripheralBattery> = cache.values().cloned().collect();
    list.sort_by(|a, b| {
        let rank = |k: &str| if k == "keyboard" { 0u8 } else { 1u8 };
        rank(&a.kind)
            .cmp(&rank(&b.kind))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });
    list
}

#[cfg(windows)]
mod platform {
    use super::*;
    use core::ffi::c_void;
    use std::mem::size_of;

    const DIGCF_PRESENT: u32 = 0x0000_0002;
    const DIGCF_DEVICEINTERFACE: u32 = 0x0000_0010;
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;
    const ERROR_IO_PENDING: u32 = 997;
    const WAIT_OBJECT_0: u32 = 0;

    const CM_GETIDLIST_FILTER_PRESENT: u32 = 0x0000_0100;
    const CM_LOCATE_DEVNODE_NORMAL: u32 = 0x0000_0000;
    const CR_SUCCESS: u32 = 0;
    const CR_BUFFER_SMALL: u32 = 0x0000_001A;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct DevPropKey {
        fmtid: Guid,
        pid: u32,
    }

    /// Windows `DEVPKEY_Device_Battery` = `{104EA319-6EE2-4701-BD47-8DDBF425BBE5}, 2`
    const DEVPKEY_DEVICE_BATTERY: DevPropKey = DevPropKey {
        fmtid: Guid {
            data1: 0x104EA319,
            data2: 0x6EE2,
            data3: 0x4701,
            data4: [0xBD, 0x47, 0x8D, 0xDB, 0xF4, 0x25, 0xBB, 0xE5],
        },
        pid: 2,
    };

    /// Windows `DEVPKEY_Device_FriendlyName` = `{A45C254E-DF1C-4EFD-8020-67D146A850E0}, 14`
    const DEVPKEY_DEVICE_FRIENDLY_NAME: DevPropKey = DevPropKey {
        fmtid: Guid {
            data1: 0xA45C254E,
            data2: 0xDF1C,
            data3: 0x4EFD,
            data4: [0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0],
        },
        pid: 14,
    };

    /// Windows `DEVPKEY_NAME` = `{B725F130-47EF-101A-A5F1-02608C9EEBAC}, 10`
    const DEVPKEY_NAME: DevPropKey = DevPropKey {
        fmtid: Guid {
            data1: 0xB725F130,
            data2: 0x47EF,
            data3: 0x101A,
            data4: [0xA5, 0xF1, 0x02, 0x60, 0x8C, 0x9E, 0xEB, 0xAC],
        },
        pid: 10,
    };

    #[repr(C)]
    struct SpDeviceInterfaceData {
        cb_size: u32,
        interface_class_guid: Guid,
        flags: u32,
        reserved: usize,
    }

    #[repr(C)]
    struct HiddAttributes {
        size: u32,
        vendor_id: u16,
        product_id: u16,
        version_number: u16,
    }

    #[repr(C)]
    #[derive(Default)]
    struct HidpCaps {
        usage: u16,
        usage_page: u16,
        input_report_byte_length: u16,
        output_report_byte_length: u16,
        feature_report_byte_length: u16,
        reserved: [u16; 17],
        number_link_collection_nodes: u16,
        number_input_button_caps: u16,
        number_input_value_caps: u16,
        number_input_data_indices: u16,
        number_output_button_caps: u16,
        number_output_value_caps: u16,
        number_output_data_indices: u16,
        number_feature_button_caps: u16,
        number_feature_value_caps: u16,
        number_feature_data_indices: u16,
    }

    #[repr(C)]
    #[derive(Default)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        h_event: *mut c_void,
    }

    #[link(name = "setupapi")]
    extern "system" {
        fn SetupDiGetClassDevsW(
            class_guid: *const Guid,
            enumerator: *const u16,
            hwnd_parent: *mut c_void,
            flags: u32,
        ) -> *mut c_void;
        fn SetupDiEnumDeviceInterfaces(
            device_info_set: *mut c_void,
            device_info_data: *mut c_void,
            interface_class_guid: *const Guid,
            member_index: u32,
            device_interface_data: *mut SpDeviceInterfaceData,
        ) -> i32;
        fn SetupDiGetDeviceInterfaceDetailW(
            device_info_set: *mut c_void,
            device_interface_data: *mut SpDeviceInterfaceData,
            device_interface_detail_data: *mut c_void,
            device_interface_detail_data_size: u32,
            required_size: *mut u32,
            device_info_data: *mut c_void,
        ) -> i32;
        fn SetupDiDestroyDeviceInfoList(device_info_set: *mut c_void) -> i32;
    }

    #[link(name = "hid")]
    extern "system" {
        fn HidD_GetHidGuid(hid_guid: *mut Guid);
        fn HidD_GetAttributes(hid_device_object: *mut c_void, attributes: *mut HiddAttributes) -> u8;
        fn HidD_GetPreparsedData(hid_device_object: *mut c_void, preparsed_data: *mut *mut c_void) -> u8;
        fn HidD_FreePreparsedData(preparsed_data: *mut c_void) -> u8;
        fn HidP_GetCaps(preparsed_data: *mut c_void, capabilities: *mut HidpCaps) -> i32;
        fn HidD_GetProductString(
            hid_device_object: *mut c_void,
            buffer: *mut c_void,
            buffer_length: u32,
        ) -> u8;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateFileW(
            file_name: *const u16,
            desired_access: u32,
            share_mode: u32,
            security_attributes: *mut c_void,
            creation_disposition: u32,
            flags_and_attributes: u32,
            template_file: *mut c_void,
        ) -> *mut c_void;
        fn CreateEventW(
            event_attributes: *mut c_void,
            manual_reset: i32,
            initial_state: i32,
            name: *const u16,
        ) -> *mut c_void;
        fn WriteFile(
            file: *mut c_void,
            buffer: *const u8,
            bytes_to_write: u32,
            bytes_written: *mut u32,
            overlapped: *mut Overlapped,
        ) -> i32;
        fn ReadFile(
            file: *mut c_void,
            buffer: *mut u8,
            bytes_to_read: u32,
            bytes_read: *mut u32,
            overlapped: *mut Overlapped,
        ) -> i32;
        fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
        fn GetOverlappedResult(
            file: *mut c_void,
            overlapped: *mut Overlapped,
            bytes_transferred: *mut u32,
            wait: i32,
        ) -> i32;
        fn CancelIoEx(file: *mut c_void, overlapped: *mut Overlapped) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn GetLastError() -> u32;
    }

    #[link(name = "cfgmgr32")]
    extern "system" {
        fn CM_Get_Device_ID_List_SizeW(
            len: *mut u32,
            filter: *const u16,
            flags: u32,
        ) -> u32;
        fn CM_Get_Device_ID_ListW(
            filter: *const u16,
            buffer: *mut u16,
            buffer_len: u32,
            flags: u32,
        ) -> u32;
        fn CM_Locate_DevNodeW(
            dev_inst: *mut u32,
            device_id: *const u16,
            flags: u32,
        ) -> u32;
        fn CM_Get_DevNode_PropertyW(
            dev_inst: u32,
            property_key: *const DevPropKey,
            property_type: *mut u32,
            property_buffer: *mut u8,
            property_buffer_size: *mut u32,
            flags: u32,
        ) -> u32;
    }

    fn is_invalid_handle(h: *mut c_void) -> bool {
        h.is_null() || h == (-1isize as *mut c_void)
    }

    struct ScopedHandle(*mut c_void);
    impl Drop for ScopedHandle {
        fn drop(&mut self) {
            if !is_invalid_handle(self.0) {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }

    struct HidEndpoint {
        vid: u16,
        pid: u16,
        usage_page: u16,
        usage: u16,
        input_len: usize,
        output_len: usize,
        product: String,
        path_wide: Vec<u16>,
    }

    /// 枚举当前已连接且我们关心的自定义 HID 通道（只打开元数据句柄，无副作用）。
    fn enumerate_target_hid_endpoints() -> Vec<HidEndpoint> {
        let mut hid_guid = Guid::default();
        unsafe {
            HidD_GetHidGuid(&mut hid_guid);
        }
        let hdev = unsafe {
            SetupDiGetClassDevsW(
                &hid_guid,
                core::ptr::null(),
                core::ptr::null_mut(),
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            )
        };
        if is_invalid_handle(hdev) {
            return Vec::new();
        }
        let mut endpoints = Vec::new();
        let mut index = 0u32;
        loop {
            let mut ifdata = SpDeviceInterfaceData {
                cb_size: size_of::<SpDeviceInterfaceData>() as u32,
                interface_class_guid: Guid::default(),
                flags: 0,
                reserved: 0,
            };
            let ok = unsafe {
                SetupDiEnumDeviceInterfaces(
                    hdev,
                    core::ptr::null_mut(),
                    &hid_guid,
                    index,
                    &mut ifdata,
                )
            };
            if ok == 0 {
                break;
            }
            index += 1;

            let mut required = 0u32;
            unsafe {
                SetupDiGetDeviceInterfaceDetailW(
                    hdev,
                    &mut ifdata,
                    core::ptr::null_mut(),
                    0,
                    &mut required,
                    core::ptr::null_mut(),
                );
            }
            if required < 8 || required > 8192 {
                continue;
            }
            let words = (required as usize).div_ceil(4);
            let mut detail_buf = vec![0u32; words];
            // 64 位 Windows 下 SP_DEVICE_INTERFACE_DETAIL_DATA_W 的 cbSize 固定为 8（32 位为 6）。
            detail_buf[0] = if size_of::<usize>() == 8 { 8 } else { 6 };
            let got = unsafe {
                SetupDiGetDeviceInterfaceDetailW(
                    hdev,
                    &mut ifdata,
                    detail_buf.as_mut_ptr() as *mut c_void,
                    (detail_buf.len() * 4) as u32,
                    core::ptr::null_mut(),
                    core::ptr::null_mut(),
                )
            };
            if got == 0 {
                continue;
            }
            let path_ptr = unsafe { (detail_buf.as_ptr() as *const u8).add(4) as *const u16 };
            let max_u16 = (required as usize).saturating_sub(4) / 2;
            let mut path_wide = Vec::new();
            for i in 0..max_u16 {
                let unit = unsafe { *path_ptr.add(i) };
                path_wide.push(unit);
                if unit == 0 {
                    break;
                }
            }
            if path_wide.last() != Some(&0) {
                path_wide.push(0);
            }
            let path_text = String::from_utf16_lossy(&path_wide[..path_wide.len().saturating_sub(1)])
                .to_ascii_lowercase();
            // 快速初筛：只对罗技 (046D) 与 VGN/Telink (320F) 打开查询句柄
            if !path_text.contains("vid_046d") && !path_text.contains("vid_320f") {
                continue;
            }

            let h0 = unsafe {
                CreateFileW(
                    path_wide.as_ptr(),
                    0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    core::ptr::null_mut(),
                    OPEN_EXISTING,
                    0,
                    core::ptr::null_mut(),
                )
            };
            if is_invalid_handle(h0) {
                continue;
            }
            let _guard = ScopedHandle(h0);
            let mut attrs = HiddAttributes {
                size: size_of::<HiddAttributes>() as u32,
                vendor_id: 0,
                product_id: 0,
                version_number: 0,
            };
            if unsafe { HidD_GetAttributes(h0, &mut attrs) } == 0 {
                continue;
            }
            let mut ppd: *mut c_void = core::ptr::null_mut();
            let mut caps = HidpCaps::default();
            if unsafe { HidD_GetPreparsedData(h0, &mut ppd) } == 0 || ppd.is_null() {
                continue;
            }
            let caps_ok = unsafe { HidP_GetCaps(ppd, &mut caps) } >= 0;
            unsafe {
                HidD_FreePreparsedData(ppd);
            }
            if !caps_ok {
                continue;
            }

            let is_logitech_hidpp = attrs.vendor_id == 0x046D
                && caps.usage_page == 0xFF00
                && caps.usage == 0x0002
                && caps.input_report_byte_length >= 20
                && caps.output_report_byte_length >= 20;
            let is_vgn_telink = attrs.vendor_id == 0x320F
                && attrs.product_id == 0x5088
                && caps.usage_page == 0xFF1C
                && caps.usage == 0x0092
                && caps.input_report_byte_length >= 64
                && caps.output_report_byte_length >= 64;
            if !is_logitech_hidpp && !is_vgn_telink {
                continue;
            }

            let mut pbuf = [0u16; 128];
            let product = if unsafe {
                HidD_GetProductString(h0, pbuf.as_mut_ptr() as *mut c_void, (pbuf.len() * 2) as u32)
            } != 0
            {
                let len = pbuf.iter().position(|&c| c == 0).unwrap_or(pbuf.len());
                String::from_utf16_lossy(&pbuf[..len])
            } else {
                String::new()
            };

            endpoints.push(HidEndpoint {
                vid: attrs.vendor_id,
                pid: attrs.product_id,
                usage_page: caps.usage_page,
                usage: caps.usage,
                input_len: caps.input_report_byte_length as usize,
                output_len: caps.output_report_byte_length as usize,
                product,
                path_wide,
            });
        }
        unsafe {
            SetupDiDestroyDeviceInfoList(hdev);
        }
        endpoints
    }

    /// 异步 Overlapped 方式收发一帧 HID 报文，严格带超时取消，永不卡死后台线程。
    fn hid_exchange<F>(
        handle: *mut c_void,
        payload: &[u8],
        read_len: usize,
        timeout_ms: u32,
        mut matcher: F,
    ) -> Option<Vec<u8>>
    where
        F: FnMut(&[u8]) -> bool,
    {
        let ev_w = unsafe { CreateEventW(core::ptr::null_mut(), 1, 0, core::ptr::null()) };
        if is_invalid_handle(ev_w) {
            return None;
        }
        let _ev_w_guard = ScopedHandle(ev_w);
        let mut ov_w = Overlapped {
            h_event: ev_w,
            ..Default::default()
        };
        let mut written = 0u32;
        let ok_w = unsafe {
            WriteFile(
                handle,
                payload.as_ptr(),
                payload.len() as u32,
                &mut written,
                &mut ov_w,
            )
        };
        if ok_w == 0 {
            let err = unsafe { GetLastError() };
            if err != ERROR_IO_PENDING {
                return None;
            }
            if unsafe { WaitForSingleObject(ev_w, timeout_ms) } != WAIT_OBJECT_0 {
                unsafe {
                    // 取消异步写入后必须等待驱动归还 OVERLAPPED，避免局部变量提前销毁引发异步越界写入。
                    if CancelIoEx(handle, &mut ov_w) != 0 {
                        let mut cancelled = 0u32;
                        let _ = GetOverlappedResult(handle, &mut ov_w, &mut cancelled, 1);
                    }
                }
                return None;
            }
            if unsafe { GetOverlappedResult(handle, &mut ov_w, &mut written, 0) } == 0 {
                return None;
            }
        }

        // 读取最多 4 帧以跳过无关的异步通知包
        let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms as u64);
        for _ in 0..4 {
            let remaining = deadline
                .checked_duration_since(std::time::Instant::now())
                .map(|d| d.as_millis().max(10) as u32);
            let Some(rem_ms) = remaining else {
                break;
            };
            let ev_r = unsafe { CreateEventW(core::ptr::null_mut(), 1, 0, core::ptr::null()) };
            if is_invalid_handle(ev_r) {
                break;
            }
            let _ev_r_guard = ScopedHandle(ev_r);
            let mut ov_r = Overlapped {
                h_event: ev_r,
                ..Default::default()
            };
            let mut rbuf = vec![0u8; read_len];
            let mut nread = 0u32;
            let ok_r = unsafe {
                ReadFile(
                    handle,
                    rbuf.as_mut_ptr(),
                    rbuf.len() as u32,
                    &mut nread,
                    &mut ov_r,
                )
            };
            if ok_r == 0 {
                let err = unsafe { GetLastError() };
                if err != ERROR_IO_PENDING {
                    break;
                }
                if unsafe { WaitForSingleObject(ev_r, rem_ms) } != WAIT_OBJECT_0 {
                    unsafe {
                        // 取消异步读取后必须等待内核完成取消，防止驱动向已释放缓冲区写回数据。
                        if CancelIoEx(handle, &mut ov_r) != 0 {
                            let mut cancelled = 0u32;
                            let _ = GetOverlappedResult(handle, &mut ov_r, &mut cancelled, 1);
                        }
                    }
                    break;
                }
                if unsafe { GetOverlappedResult(handle, &mut ov_r, &mut nread, 0) } == 0 {
                    break;
                }
            }
            rbuf.truncate(nread as usize);
            if matcher(&rbuf) {
                return Some(rbuf);
            }
        }
        None
    }

    fn hidpp_query(
        handle: *mut c_void,
        dev_idx: u8,
        feature_idx: u8,
        fn_nibble: u8,
        params: &[u8],
        out_len: usize,
        in_len: usize,
    ) -> Option<Vec<u8>> {
        const SW_ID: u8 = 0x0A;
        let mut req = vec![0u8; out_len.max(20)];
        req[0] = 0x11;
        req[1] = dev_idx;
        req[2] = feature_idx;
        req[3] = (fn_nibble << 4) | SW_ID;
        for (i, &b) in params.iter().take(req.len().saturating_sub(4)).enumerate() {
            req[4 + i] = b;
        }
        hid_exchange(handle, &req, in_len.max(20), 180, |r| {
            r.len() >= 7
                && r[0] == 0x11
                && r[1] == dev_idx
                && (r[2] == feature_idx || r[2] == 0x8F || r[2] == 0xFF)
        })
    }

    fn hidpp_find_feature(
        handle: *mut c_void,
        dev_idx: u8,
        feature_id: u16,
        out_len: usize,
        in_len: usize,
    ) -> Option<u8> {
        let resp = hidpp_query(
            handle,
            dev_idx,
            0x00,
            0x00,
            &[(feature_id >> 8) as u8, (feature_id & 0xFF) as u8],
            out_len,
            in_len,
        )?;
        if resp[2] == 0x8F || resp[2] == 0xFF || resp[4] == 0 {
            None
        } else {
            Some(resp[4])
        }
    }

    fn probe_logitech_slot(
        ep: &HidEndpoint,
        handle: *mut c_void,
        dev_idx: u8,
        now_ms: u64,
    ) -> Option<PeripheralBattery> {
        // 先 Ping 确认设备在线且支持 HID++ 2.0
        let ping = hidpp_query(handle, dev_idx, 0x00, 0x01, &[0x00, 0x00, 0x5A], ep.output_len, ep.input_len)?;
        if ping[2] == 0x8F || ping[2] == 0xFF || ping[4] < 2 {
            return None;
        }

        // 依次尝试 Unified Battery (0x1004)、Battery Status (0x1000)、Battery Voltage (0x1001)
        let battery = if let Some(idx) = hidpp_find_feature(handle, dev_idx, 0x1004, ep.output_len, ep.input_len) {
            hidpp_query(handle, dev_idx, idx, 0x01, &[], ep.output_len, ep.input_len)
                .and_then(|r| parse_logitech_unified_battery(&r))
        } else if let Some(idx) = hidpp_find_feature(handle, dev_idx, 0x1000, ep.output_len, ep.input_len) {
            hidpp_query(handle, dev_idx, idx, 0x00, &[], ep.output_len, ep.input_len)
                .and_then(|r| parse_logitech_battery_status(&r))
        } else if let Some(idx) = hidpp_find_feature(handle, dev_idx, 0x1001, ep.output_len, ep.input_len) {
            hidpp_query(handle, dev_idx, idx, 0x00, &[], ep.output_len, ep.input_len)
                .and_then(|r| parse_logitech_battery_voltage(&r))
        } else {
            None
        }?;

        // 查询真实设备名与设备类型 (0x0005)
        let mut dev_type: Option<u8> = None;
        let mut name = String::new();
        if let Some(name_idx) = hidpp_find_feature(handle, dev_idx, 0x0005, ep.output_len, ep.input_len) {
            if let Some(tr) = hidpp_query(handle, dev_idx, name_idx, 0x02, &[], ep.output_len, ep.input_len) {
                if tr[2] != 0x8F && tr[2] != 0xFF {
                    dev_type = Some(tr[4]);
                }
            }
            if let Some(lr) = hidpp_query(handle, dev_idx, name_idx, 0x00, &[], ep.output_len, ep.input_len) {
                if lr[2] != 0x8F && lr[2] != 0xFF {
                    let target_len = (lr[4] as usize).min(48);
                    let mut raw_bytes = Vec::with_capacity(target_len);
                    let mut offset = 0usize;
                    while offset < target_len {
                        let Some(chunk) = hidpp_query(
                            handle,
                            dev_idx,
                            name_idx,
                            0x01,
                            &[offset as u8],
                            ep.output_len,
                            ep.input_len,
                        ) else {
                            break;
                        };
                        if chunk[2] == 0x8F || chunk[2] == 0xFF {
                            break;
                        }
                        let take = (target_len - offset).min(chunk.len().saturating_sub(4));
                        if take == 0 {
                            break;
                        }
                        raw_bytes.extend_from_slice(&chunk[4..4 + take]);
                        offset += take;
                    }
                    let decoded = String::from_utf8_lossy(&raw_bytes)
                        .trim_matches('\0')
                        .trim()
                        .to_string();
                    if !decoded.is_empty() {
                        name = decoded;
                    }
                }
            }
        }
        let clean_name = clean_peripheral_name(
            if name.is_empty() { &ep.product } else { &name },
            "Logitech 设备",
        );
        let kind = classify_logitech_kind(dev_type, &clean_name);
        Some(PeripheralBattery {
            id: format!("logi:{:04x}:{:02x}", ep.pid, dev_idx),
            kind: kind.to_string(),
            name: clean_name,
            percent: battery.0,
            charging: battery.1,
            transport: "logitech-hidpp".to_string(),
            stale: false,
            updated_at_ms: now_ms,
        })
    }

    fn probe_logitech_endpoint(ep: &HidEndpoint, now_ms: u64) -> Vec<PeripheralBattery> {
        let h = unsafe {
            CreateFileW(
                ep.path_wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                core::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                core::ptr::null_mut(),
            )
        };
        if is_invalid_handle(h) {
            return Vec::new();
        }
        let _guard = ScopedHandle(h);
        // 优先测试直连索引 0xFF：有线/单设备模式直接返回结果，避免在槽位 1..6 重复枚举。
        if let Some(direct) = probe_logitech_slot(ep, h, 0xFF, now_ms) {
            return vec![direct];
        }
        let mut found = Vec::new();
        for slot in 1u8..=6 {
            if let Some(item) = probe_logitech_slot(ep, h, slot, now_ms) {
                found.push(item);
            }
        }
        found
    }

    fn probe_vgn_endpoint(ep: &HidEndpoint, now_ms: u64) -> Option<PeripheralBattery> {
        let h = unsafe {
            CreateFileW(
                ep.path_wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                core::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                core::ptr::null_mut(),
            )
        };
        if is_invalid_handle(h) {
            return None;
        }
        let _guard = ScopedHandle(h);
        // Report ID 0x04, 16-bit 小端校验和 0x001A, 命令 0x1A（只读电量查询）
        let mut req = vec![0u8; ep.output_len.max(64)];
        req[0] = 0x04;
        req[1] = 0x1A;
        req[2] = 0x00;
        req[3] = 0x1A;
        let resp = hid_exchange(h, &req, ep.input_len.max(64), 240, |r| {
            r.len() >= 10 && r[0] == 0x04 && r[3] == 0x1A
        })?;
        let (percent, charging) = parse_vgn_telink_battery(&resp)?;
        let name = clean_peripheral_name(&ep.product, "VGN 2.4G 键盘");
        Some(PeripheralBattery {
            id: format!("vgn:{:04x}:{:04x}", ep.vid, ep.pid),
            kind: "keyboard".to_string(),
            name,
            percent,
            charging,
            transport: "vgn-24g".to_string(),
            stale: false,
            updated_at_ms: now_ms,
        })
    }

    fn read_devnode_string_prop(dev_inst: u32, key: &DevPropKey) -> Option<String> {
        let mut prop_type = 0u32;
        let mut bytes = 0u32;
        let rc = unsafe {
            CM_Get_DevNode_PropertyW(
                dev_inst,
                key,
                &mut prop_type,
                core::ptr::null_mut(),
                &mut bytes,
                0,
            )
        };
        if rc != CR_BUFFER_SMALL || bytes == 0 || bytes > 1024 {
            return None;
        }
        let mut buf = vec![0u8; bytes as usize];
        let rc2 = unsafe {
            CM_Get_DevNode_PropertyW(
                dev_inst,
                key,
                &mut prop_type,
                buf.as_mut_ptr(),
                &mut bytes,
                0,
            )
        };
        if rc2 != CR_SUCCESS {
            return None;
        }
        let u16s: Vec<u16> = buf
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&v| v != 0)
            .collect();
        let text = String::from_utf16_lossy(&u16s).trim().to_string();
        if text.is_empty() { None } else { Some(text) }
    }

    fn read_bluetooth_batteries(now_ms: u64) -> Vec<PeripheralBattery> {
        let mut list_len = 0u32;
        if unsafe { CM_Get_Device_ID_List_SizeW(&mut list_len, core::ptr::null(), CM_GETIDLIST_FILTER_PRESENT) } != CR_SUCCESS
            || list_len == 0
            || list_len > 2_000_000
        {
            return Vec::new();
        }
        let mut buf = vec![0u16; list_len as usize];
        if unsafe { CM_Get_Device_ID_ListW(core::ptr::null(), buf.as_mut_ptr(), list_len, CM_GETIDLIST_FILTER_PRESENT) } != CR_SUCCESS {
            return Vec::new();
        }

        let mut results = Vec::new();
        let mut start = 0usize;
        while start < buf.len() {
            let end = buf[start..].iter().position(|&c| c == 0).unwrap_or(0);
            if end == 0 {
                break;
            }
            let slice = &buf[start..start + end];
            start += end + 1;

            let id_str = String::from_utf16_lossy(slice);
            let upper = id_str.to_ascii_uppercase();
            if !upper.starts_with("BTHENUM\\") && !upper.starts_with("BTHLE\\") && !upper.starts_with("BTHLEDEVICE\\") {
                continue;
            }
            let mut wide_id = slice.to_vec();
            wide_id.push(0);
            let mut dev_inst = 0u32;
            if unsafe { CM_Locate_DevNodeW(&mut dev_inst, wide_id.as_ptr(), CM_LOCATE_DEVNODE_NORMAL) } != CR_SUCCESS {
                continue;
            }
            let mut prop_type = 0u32;
            let mut bat_buf = [0u8; 4];
            let mut bat_size = bat_buf.len() as u32;
            if unsafe {
                CM_Get_DevNode_PropertyW(
                    dev_inst,
                    &DEVPKEY_DEVICE_BATTERY,
                    &mut prop_type,
                    bat_buf.as_mut_ptr(),
                    &mut bat_size,
                    0,
                )
            } != CR_SUCCESS
                || bat_size == 0
            {
                continue;
            }
            let percent = bat_buf[0];
            if percent > 100 {
                continue;
            }
            let raw_name = read_devnode_string_prop(dev_inst, &DEVPKEY_DEVICE_FRIENDLY_NAME)
                .or_else(|| read_devnode_string_prop(dev_inst, &DEVPKEY_NAME))
                .unwrap_or_default();
            let Some(kind) = classify_bluetooth_peripheral(&raw_name) else {
                continue;
            };
            let clean = clean_peripheral_name(&raw_name, if kind == "keyboard" { "蓝牙键盘" } else { "蓝牙鼠标" });
            results.push(PeripheralBattery {
                id: format!("bt:{}", upper),
                kind: kind.to_string(),
                name: clean,
                percent,
                charging: false,
                transport: "bluetooth".to_string(),
                stale: false,
                updated_at_ms: now_ms,
            });
        }
        results
    }

    pub fn scan_live_peripherals(now_ms: u64) -> (Vec<PeripheralBattery>, HashSet<String>) {
        let mut live = Vec::new();
        let mut present_prefixes = HashSet::new();
        let mut seen_names = HashSet::new();

        for ep in enumerate_target_hid_endpoints() {
            if ep.vid == 0x046D && ep.usage_page == 0xFF00 && ep.usage == 0x0002 {
                present_prefixes.insert(format!("logi:{:04x}:", ep.pid));
                for item in probe_logitech_endpoint(&ep, now_ms) {
                    seen_names.insert(item.name.to_ascii_lowercase());
                    live.push(item);
                }
            } else if ep.vid == 0x320F && ep.pid == 0x5088 {
                present_prefixes.insert(format!("vgn:{:04x}:{:04x}", ep.vid, ep.pid));
                if let Some(item) = probe_vgn_endpoint(&ep, now_ms) {
                    seen_names.insert(item.name.to_ascii_lowercase());
                    live.push(item);
                }
            }
        }

        present_prefixes.insert("bt:".to_string());
        for item in read_bluetooth_batteries(now_ms) {
            if !seen_names.contains(&item.name.to_ascii_lowercase()) {
                live.push(item);
            }
        }
        (live, present_prefixes)
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;
    pub fn scan_live_peripherals(_now_ms: u64) -> (Vec<PeripheralBattery>, HashSet<String>) {
        (Vec::new(), HashSet::new())
    }
}

fn run_scan_and_publish(
    app: &tauri::AppHandle,
    cache: &mut HashMap<String, PeripheralBattery>,
) -> PeripheralBatterySnapshot {
    use crate::windows;
    use tauri::{Emitter, Manager};

    let now_ms = now_epoch_ms();
    let (live, prefixes) = platform::scan_live_peripherals(now_ms);
    let devices = merge_peripheral_cache(cache, live, &prefixes, now_ms);
    let snapshot = PeripheralBatterySnapshot {
        devices,
        checked_at_ms: now_ms,
    };
    if let Some(state) = app.try_state::<PeripheralBatteryState>() {
        *state.0.lock().unwrap_or_else(|e| e.into_inner()) = snapshot.clone();
    }
    let _ = app.emit_to(windows::BAR, PERIPHERALS_EVENT, &snapshot);
    let _ = app.emit_to(windows::SETTINGS, PERIPHERALS_EVENT, &snapshot);
    snapshot
}

#[tauri::command]
pub fn peripheral_batteries(state: tauri::State<'_, PeripheralBatteryState>) -> PeripheralBatterySnapshot {
    state.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
pub async fn refresh_peripheral_batteries(app: tauri::AppHandle) -> Result<PeripheralBatterySnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let mut cache = HashMap::new();
        if let Some(state) = app.try_state::<PeripheralBatteryState>() {
            let prev = state.0.lock().unwrap_or_else(|e| e.into_inner()).clone();
            for dev in prev.devices {
                cache.insert(dev.id.clone(), dev);
            }
        }
        run_scan_and_publish(&app, &mut cache)
    })
    .await
    .map_err(|_| "刷新外设电量中断".to_string())
}

/// 启动后台低频键鼠电量监视线程（默认 60 秒轮询一次，全屏避让期间亦持续低频监测以确保低电量及时浮出预警）。
pub fn start(app: tauri::AppHandle) {
    use crate::{windows, AppState};
    use std::sync::atomic::Ordering;
    use tauri::{Emitter, Manager};

    std::thread::Builder::new()
        .name("island-peripherals".into())
        .spawn(move || {
            let mut cache: HashMap<String, PeripheralBattery> = HashMap::new();
            let mut low_battery = LowBatteryTracker::default();
            let mut last_poll: Option<std::time::Instant> = None;
            let mut was_enabled = false;

            while app.state::<AppState>().running.load(Ordering::Relaxed) {
                let ui = app.state::<AppState>().ui.snapshot();
                let enabled = ui.island_appearance.telemetry_peripherals;
                let bar_visible = app
                    .get_webview_window(windows::BAR)
                    .and_then(|w| w.is_visible().ok())
                    .unwrap_or(false);
                // 只要开启了键鼠电量且灵动岛处于运行可见状态，每 60 秒低频读取一次（含全屏/最大化避让期，避免错过低电预警）。
                let active = enabled && bar_visible;

                if active {
                    let due = !was_enabled
                        || last_poll.is_none_or(|t| t.elapsed() >= Duration::from_secs(60));
                    if due {
                        was_enabled = true;
                        last_poll = Some(std::time::Instant::now());
                        let snapshot = run_scan_and_publish(&app, &mut cache);
                        for alert in low_battery.evaluate(&snapshot.devices) {
                            let _ = app.emit_to(
                                windows::BAR,
                                "island-feedback",
                                serde_json::json!({
                                    "kind": if alert.critical { "battery-critical" } else { "battery-low" },
                                    "label": alert.label,
                                    "tooltip": alert.tooltip,
                                    "urgent": true,
                                }),
                            );
                        }
                    }
                } else if was_enabled && !enabled {
                    was_enabled = false;
                    last_poll = None;
                    cache.clear();
                    low_battery.clear();
                    let empty = PeripheralBatterySnapshot::default();
                    if let Some(state) = app.try_state::<PeripheralBatteryState>() {
                        *state.0.lock().unwrap_or_else(|e| e.into_inner()) = empty.clone();
                    }
                    let _ = app.emit_to(windows::BAR, PERIPHERALS_EVENT, &empty);
                }

                std::thread::sleep(Duration::from_secs(2));
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_logitech_unified_and_legacy_battery_packets() {
        // 真机 G304 X LIGHTSPEED 报文：100% 充满
        let g304x = [0x11, 0xFF, 0x06, 0x1A, 0x64, 0x08, 0x03, 0x01, 0, 0, 0, 0];
        assert_eq!(parse_logitech_unified_battery(&g304x), Some((100, true)));

        // 无线放电中 47%
        let discharging = [0x11, 0x01, 0x06, 0x1A, 47, 0x04, 0x00, 0];
        assert_eq!(parse_logitech_unified_battery(&discharging), Some((47, false)));

        // 仅上报离散档位（soc=0, level=低电 bit1）
        let discrete = [0x11, 0x01, 0x06, 0x1A, 0, 0x02, 0x00, 0];
        assert_eq!(parse_logitech_unified_battery(&discrete), Some((20, false)));

        // HID++ 错误包直接忽略
        let err = [0x11, 0x01, 0x8F, 0x00, 0x01, 0x00, 0x00];
        assert_eq!(parse_logitech_unified_battery(&err), None);

        // 旧版 0x1000 Battery Status 与 0x1001 Voltage
        assert_eq!(
            parse_logitech_battery_status(&[0x11, 0x01, 0x04, 0x0A, 75, 50, 0]),
            Some((75, false))
        );
        // 3840mV => 50%, bit7=1 充电中
        let mv = 3840u16.to_be_bytes();
        assert_eq!(
            parse_logitech_battery_voltage(&[0x11, 0x01, 0x04, 0x0A, mv[0], mv[1], 0x80]),
            Some((50, true))
        );
    }

    #[test]
    fn parses_vgn_v98pro_v2_telink_battery_packet() {
        // 真机 VGN V98pro V2 2.4G Dongle 报文：50% 未充电
        let ok = [4, 26, 0, 26, 0, 0, 0, 0, 50, 0, 0, 0];
        assert_eq!(parse_vgn_telink_battery(&ok), Some((50, false)));

        let charging = [4, 26, 0, 26, 0, 0, 0, 0, 88, 1, 0, 0];
        assert_eq!(parse_vgn_telink_battery(&charging), Some((88, true)));

        // status != 0 必须忽略
        let failed = [4, 26, 0, 26, 0, 0, 0, 0xFF, 50, 0, 0, 0];
        assert_eq!(parse_vgn_telink_battery(&failed), None);
    }

    #[test]
    fn classifies_bluetooth_peripherals_and_cleans_names() {
        assert_eq!(classify_bluetooth_peripheral("HUAWEI FreeClip Hands-Free AG"), None);
        assert_eq!(classify_bluetooth_peripheral("Keychron K3 Pro Keyboard"), Some("keyboard"));
        assert_eq!(classify_bluetooth_peripheral("MX Master 3S"), Some("mouse"));
        assert_eq!(clean_peripheral_name("VGN V98pro V2 2.4G Dongle", "键盘"), "VGN V98pro V2");
    }

    #[test]
    fn cache_keeps_sleeping_device_until_detached_or_expired() {
        let mut cache = HashMap::new();
        let dev = PeripheralBattery {
            id: "logi:c547:01".into(),
            kind: "mouse".into(),
            name: "G304 X".into(),
            percent: 66,
            charging: false,
            transport: "logitech-hidpp".into(),
            stale: false,
            updated_at_ms: 1000,
        };
        let attached: HashSet<String> = ["logi:c547:".into()].into_iter().collect();
        let first = merge_peripheral_cache(&mut cache, vec![dev], &attached, 1000);
        assert_eq!(first.len(), 1);
        assert!(!first[0].stale);

        // 鼠标休眠未回应但接收器仍在：保留上次读数并标记 stale
        let sleeping = merge_peripheral_cache(&mut cache, vec![], &attached, 61_000);
        assert_eq!(sleeping.len(), 1);
        assert!(sleeping[0].stale);
        assert_eq!(sleeping[0].percent, 66);

        // 接收器拔出：立即清除
        let detached = merge_peripheral_cache(&mut cache, vec![], &HashSet::new(), 62_000);
        assert!(detached.is_empty());
    }

    #[test]
    fn low_battery_tracker_triggers_once_per_tier_and_resets_on_charge() {
        let mut tracker = LowBatteryTracker::default();
        let make = |percent: u8, charging: bool, stale: bool| PeripheralBattery {
            id: "vgn:320f:5088".into(),
            kind: "keyboard".into(),
            name: "VGN V98pro V2".into(),
            percent,
            charging,
            transport: "vgn-24g".into(),
            stale,
            updated_at_ms: 1000,
        };
        assert!(tracker.evaluate(&[make(45, false, false)]).is_empty());
        assert!(tracker.evaluate(&[make(15, false, true)]).is_empty());
        let warn = tracker.evaluate(&[make(18, false, false)]);
        assert_eq!(warn.len(), 1);
        assert!(!warn[0].critical);
        assert!(warn[0].label.contains("键盘电量偏低 18%"));
        assert!(tracker.evaluate(&[make(16, false, false)]).is_empty());
        let crit = tracker.evaluate(&[make(9, false, false)]);
        assert_eq!(crit.len(), 1);
        assert!(crit[0].critical);
        assert!(crit[0].label.contains("键盘电量告急 9%"));
        assert!(tracker.evaluate(&[make(8, false, false)]).is_empty());
        assert!(tracker.evaluate(&[make(12, true, false)]).is_empty());
        let again = tracker.evaluate(&[make(19, false, false)]);
        assert_eq!(again.len(), 1);
        assert!(!again[0].critical);
    }

    #[test]
    #[ignore]
    fn read_only_windows_peripherals_smoke() {
        let (live, _) = platform::scan_live_peripherals(now_epoch_ms());
        println!("detected peripherals: {live:?}");
        assert!(!live.is_empty(), "应至少检测到本机已连接的无线键鼠");
    }
}
