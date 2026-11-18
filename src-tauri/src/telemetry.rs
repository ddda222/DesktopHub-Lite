//! 胶囊轻量系统遥测：仅在本机用户态只读采集活动物理网卡吞吐、处理器总占用与物理内存占用。
//!
//! 约束与边界：
//! - 不建立网络连接、不嗅探数据包、不将网卡名称或接口描述写入日志；
//! - 仅累加处于连接状态的物理以太网（6）与 Wi-Fi（71）硬件接口，排除隧道与虚拟接口；
//! - 关闭微读数、未勾选指标、灵动岛隐藏或收为全屏避让细线时自动休眠并清空差分基线。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Instant;

pub const TELEMETRY_EVENT: &str = "island-telemetry";

const MIN_SAMPLE_GAP_US: u64 = 100_000;
const MAX_SAMPLE_GAP_US: u64 = 10_000_000;

/// 单块活动物理网卡的累计字节快照。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdapterCounter {
    pub id: u64,
    pub in_octets: u64,
    pub out_octets: u64,
}

/// 网速差分状态。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NetRateSample {
    Ready { down_bps: f64, up_bps: f64 },
    WarmingUp,
    Unavailable,
}

/// 基于单调时钟的网卡吞吐率差分器，不受系统墙上时钟跳变影响。
#[derive(Default)]
pub struct NetRateCalculator {
    previous: Option<BTreeMap<u64, (u64, u64)>>,
    previous_us: u64,
}

impl NetRateCalculator {
    pub fn reset(&mut self) {
        self.previous = None;
        self.previous_us = 0;
    }

    pub fn sample(&mut self, counters: &[AdapterCounter], timestamp_us: u64) -> NetRateSample {
        if counters.is_empty() {
            self.reset();
            return NetRateSample::Unavailable;
        }
        let mut current = BTreeMap::new();
        for item in counters {
            if item.id == 0 || current.insert(item.id, (item.in_octets, item.out_octets)).is_some() {
                self.reset();
                return NetRateSample::Unavailable;
            }
        }
        let Some(before) = &self.previous else {
            return self.rebase(current, timestamp_us);
        };
        if current.len() != before.len() || current.keys().any(|id| !before.contains_key(id)) {
            return self.rebase(current, timestamp_us);
        }
        if timestamp_us <= self.previous_us {
            return self.rebase(current, timestamp_us);
        }
        let elapsed_us = timestamp_us - self.previous_us;
        if elapsed_us > MAX_SAMPLE_GAP_US {
            return self.rebase(current, timestamp_us);
        }
        if elapsed_us < MIN_SAMPLE_GAP_US {
            return NetRateSample::WarmingUp;
        }

        let mut down_bytes: u128 = 0;
        let mut up_bytes: u128 = 0;
        for (id, &(in_now, out_now)) in &current {
            let &(in_prev, out_prev) = &before[id];
            if in_now < in_prev || out_now < out_prev {
                return self.rebase(current, timestamp_us);
            }
            down_bytes += (in_now - in_prev) as u128;
            up_bytes += (out_now - out_prev) as u128;
        }

        self.previous = Some(current);
        self.previous_us = timestamp_us;
        let seconds = (elapsed_us as f64) / 1_000_000.0;
        NetRateSample::Ready {
            down_bps: (down_bytes as f64) / seconds,
            up_bps: (up_bytes as f64) / seconds,
        }
    }

    fn rebase(&mut self, current: BTreeMap<u64, (u64, u64)>, timestamp_us: u64) -> NetRateSample {
        self.previous = Some(current);
        self.previous_us = timestamp_us;
        NetRateSample::WarmingUp
    }
}

/// 基于系统时间片差分的处理器总占用率计算器（`kernel` 时间在 Windows 下已包含 `idle` 时间）。
#[derive(Default)]
pub struct CpuCalculator {
    previous: Option<(u64, u64, u64)>,
    previous_us: u64,
}

impl CpuCalculator {
    pub fn reset(&mut self) {
        self.previous = None;
        self.previous_us = 0;
    }

    pub fn sample(&mut self, idle: u64, kernel: u64, user: u64, timestamp_us: u64) -> Option<u8> {
        let Some((prev_idle, prev_kernel, prev_user)) = self.previous else {
            self.previous = Some((idle, kernel, user));
            self.previous_us = timestamp_us;
            return None;
        };
        if timestamp_us <= self.previous_us {
            self.previous = Some((idle, kernel, user));
            self.previous_us = timestamp_us;
            return None;
        }
        let elapsed_us = timestamp_us - self.previous_us;
        if elapsed_us > MAX_SAMPLE_GAP_US {
            self.previous = Some((idle, kernel, user));
            self.previous_us = timestamp_us;
            return None;
        }
        if elapsed_us < MIN_SAMPLE_GAP_US {
            return None;
        }
        let (Some(idle_delta), Some(kernel_delta), Some(user_delta)) = (
            idle.checked_sub(prev_idle),
            kernel.checked_sub(prev_kernel),
            user.checked_sub(prev_user),
        ) else {
            self.previous = Some((idle, kernel, user));
            self.previous_us = timestamp_us;
            return None;
        };
        self.previous = Some((idle, kernel, user));
        self.previous_us = timestamp_us;

        let total = kernel_delta.saturating_add(user_delta);
        if total == 0 {
            return None;
        }
        let busy = total.saturating_sub(idle_delta);
        let ratio = (busy as f64 / total as f64) * 100.0;
        Some(ratio.round().clamp(0.0, 100.0) as u8)
    }
}

/// 根据物理内存总量与可用量计算占用百分比（0..=100）。
pub fn compute_memory_percent(total_phys: u64, avail_phys: u64) -> Option<u8> {
    if total_phys == 0 || avail_phys > total_phys {
        return None;
    }
    let used = total_phys - avail_phys;
    let ratio = (used as f64 / total_phys as f64) * 100.0;
    Some(ratio.round().clamp(0.0, 100.0) as u8)
}

/// 判定网卡行是否属于当前活动的物理以太网或 Wi-Fi 接口。
pub fn is_active_physical_adapter(
    oper_status: u32,
    if_type: u32,
    tunnel_type: u32,
    status_flags: u8,
    media_connect_state: u32,
) -> bool {
    const HARDWARE_INTERFACE: u8 = 0x01;
    // 过滤驱动(0x02)、已断开(0x08)、已暂停(0x10)、低电(0x20)、虚拟端点(0x40/0x80)一律排除
    const REJECTED_FLAGS: u8 = 0x02 | 0x08 | 0x10 | 0x20 | 0x40 | 0x80;
    oper_status == 1
        && (if_type == 6 || if_type == 71)
        && tunnel_type == 0
        && (status_flags & HARDWARE_INTERFACE) != 0
        && (status_flags & REJECTED_FLAGS) == 0
        && media_connect_state != 2
}

/// 发送给灵动岛前端的轻量遥测快照。
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TelemetrySnapshot {
    /// "ready" | "warmingUp" | "unavailable" | "disabled"
    pub net_state: String,
    pub down_bps: Option<u64>,
    pub up_bps: Option<u64>,
    pub cpu_percent: Option<u8>,
    pub memory_percent: Option<u8>,
}

/// 组合三种采样器的有状态采集器，单线程持有。
pub struct SystemSampler {
    origin: Instant,
    net: NetRateCalculator,
    cpu: CpuCalculator,
}

impl Default for SystemSampler {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
            net: NetRateCalculator::default(),
            cpu: CpuCalculator::default(),
        }
    }
}

impl SystemSampler {
    pub fn reset(&mut self) {
        self.net.reset();
        self.cpu.reset();
    }

    pub fn sample(&mut self, want_net: bool, want_cpu: bool, want_memory: bool) -> TelemetrySnapshot {
        let now_us = self.origin.elapsed().as_micros().min(u64::MAX as u128) as u64;

        let (net_state, down_bps, up_bps) = if !want_net {
            self.net.reset();
            ("disabled".to_string(), None, None)
        } else {
            match platform::read_network_counters() {
                Some(counters) => match self.net.sample(&counters, now_us) {
                    NetRateSample::Ready { down_bps, up_bps } => (
                        "ready".to_string(),
                        Some(down_bps.max(0.0).round() as u64),
                        Some(up_bps.max(0.0).round() as u64),
                    ),
                    NetRateSample::WarmingUp => ("warmingUp".to_string(), None, None),
                    NetRateSample::Unavailable => ("unavailable".to_string(), None, None),
                },
                None => {
                    self.net.reset();
                    ("unavailable".to_string(), None, None)
                }
            }
        };

        let cpu_percent = if !want_cpu {
            self.cpu.reset();
            None
        } else if let Some((idle, kernel, user)) = platform::read_system_times() {
            self.cpu.sample(idle, kernel, user, now_us)
        } else {
            self.cpu.reset();
            None
        };

        let memory_percent = if !want_memory {
            None
        } else {
            platform::read_memory_status()
                .and_then(|(total, avail)| compute_memory_percent(total, avail))
        };

        TelemetrySnapshot {
            net_state,
            down_bps,
            up_bps,
            cpu_percent,
            memory_percent,
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::{is_active_physical_adapter, AdapterCounter};
    use core::ffi::c_void;
    use std::mem::{offset_of, size_of};

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    /// 对应 Windows SDK `netioapi.h` 中的 `MIB_IF_ROW2`（64 位布局固定为 1352 字节）。
    #[repr(C)]
    struct MibIfRow2 {
        interface_luid: u64,
        interface_index: u32,
        interface_guid: Guid,
        alias: [u16; 257],
        description: [u16; 257],
        physical_address_length: u32,
        physical_address: [u8; 32],
        permanent_physical_address: [u8; 32],
        mtu: u32,
        if_type: u32,
        tunnel_type: u32,
        media_type: u32,
        physical_medium_type: u32,
        access_type: u32,
        direction_type: u32,
        interface_and_oper_status_flags: u8,
        oper_status: u32,
        admin_status: u32,
        media_connect_state: u32,
        network_guid: Guid,
        connection_type: u32,
        transmit_link_speed: u64,
        receive_link_speed: u64,
        in_octets: u64,
        in_ucast_pkts: u64,
        in_nucast_pkts: u64,
        in_discards: u64,
        in_errors: u64,
        in_unknown_protos: u64,
        in_ucast_octets: u64,
        in_multicast_octets: u64,
        in_broadcast_octets: u64,
        out_octets: u64,
        out_ucast_pkts: u64,
        out_nucast_pkts: u64,
        out_discards: u64,
        out_errors: u64,
        out_ucast_octets: u64,
        out_multicast_octets: u64,
        out_broadcast_octets: u64,
        out_qlen: u64,
    }

    #[repr(C)]
    struct MibIfTable2 {
        num_entries: u32,
        table: [MibIfRow2; 1],
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    impl FileTime {
        fn to_u64(self) -> u64 {
            ((self.high as u64) << 32) | (self.low as u64)
        }
    }

    #[repr(C)]
    struct MemoryStatusEx {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page_file: u64,
        avail_page_file: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended_virtual: u64,
    }

    const _: () = {
        assert!(size_of::<MibIfRow2>() == 1352);
        assert!(offset_of!(MibIfRow2, in_octets) == 1208);
        assert!(offset_of!(MibIfRow2, out_octets) == 1280);
        assert!(offset_of!(MibIfTable2, table) == 8);
        assert!(size_of::<MemoryStatusEx>() == 64);
    };

    #[link(name = "iphlpapi")]
    extern "system" {
        fn GetIfTable2(table: *mut *mut MibIfTable2) -> u32;
        fn FreeMibTable(memory: *mut c_void);
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetSystemTimes(
            idle_time: *mut FileTime,
            kernel_time: *mut FileTime,
            user_time: *mut FileTime,
        ) -> i32;
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
    }

    pub fn read_network_counters() -> Option<Vec<AdapterCounter>> {
        if size_of::<MibIfRow2>() != 1352
            || offset_of!(MibIfRow2, in_octets) != 1208
            || offset_of!(MibIfTable2, table) != 8
        {
            return None;
        }
        let mut table: *mut MibIfTable2 = core::ptr::null_mut();
        unsafe {
            let status = GetIfTable2(&mut table);
            if status != 0 || table.is_null() {
                if !table.is_null() {
                    FreeMibTable(table as *mut c_void);
                }
                return None;
            }
            let result = (|| {
                let count = (*table).num_entries as usize;
                if count > 16_384 {
                    return None;
                }
                let first = core::ptr::addr_of!((*table).table) as *const MibIfRow2;
                let rows = core::slice::from_raw_parts(first, count);
                let mut counters = Vec::new();
                for row in rows {
                    if !is_active_physical_adapter(
                        row.oper_status,
                        row.if_type,
                        row.tunnel_type,
                        row.interface_and_oper_status_flags,
                        row.media_connect_state,
                    ) {
                        continue;
                    }
                    if row.interface_luid != 0 {
                        counters.push(AdapterCounter {
                            id: row.interface_luid,
                            in_octets: row.in_octets,
                            out_octets: row.out_octets,
                        });
                    }
                }
                Some(counters)
            })();
            FreeMibTable(table as *mut c_void);
            result
        }
    }

    pub fn read_system_times() -> Option<(u64, u64, u64)> {
        let mut idle = FileTime::default();
        let mut kernel = FileTime::default();
        let mut user = FileTime::default();
        let ok = unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) };
        if ok == 0 {
            None
        } else {
            Some((idle.to_u64(), kernel.to_u64(), user.to_u64()))
        }
    }

    pub fn read_memory_status() -> Option<(u64, u64)> {
        let mut status = MemoryStatusEx {
            length: size_of::<MemoryStatusEx>() as u32,
            memory_load: 0,
            total_phys: 0,
            avail_phys: 0,
            total_page_file: 0,
            avail_page_file: 0,
            total_virtual: 0,
            avail_virtual: 0,
            avail_extended_virtual: 0,
        };
        let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
        if ok == 0 || status.total_phys == 0 {
            None
        } else {
            Some((status.total_phys, status.avail_phys))
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::AdapterCounter;
    pub fn read_network_counters() -> Option<Vec<AdapterCounter>> {
        None
    }
    pub fn read_system_times() -> Option<(u64, u64, u64)> {
        None
    }
    pub fn read_memory_status() -> Option<(u64, u64)> {
        None
    }
}

/// 启动后台每秒按需遥测线程。关闭微读数、隐藏窗口或全屏收为细线时自动休眠。
pub fn start(app: tauri::AppHandle) {
    use crate::{windows, AppState};
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    use tauri::{Emitter, Manager};
    std::thread::Builder::new()
        .name("island-telemetry".into())
        .spawn(move || {
            let mut sampler = SystemSampler::default();
            let mut was_active = false;
            while app.state::<AppState>().running.load(Ordering::Relaxed) {
                let ui = app.state::<AppState>().ui.snapshot();
                let appearance = &ui.island_appearance;
                let any_metric = appearance.telemetry_net
                    || appearance.telemetry_cpu
                    || appearance.telemetry_memory;
                let mode_enabled = matches!(appearance.telemetry_mode.as_str(), "smart" | "always")
                    && any_metric;
                let bar_visible = app
                    .get_webview_window(windows::BAR)
                    .and_then(|w| w.is_visible().ok())
                    .unwrap_or(false);
                let active = mode_enabled && bar_visible && !ui.fullscreen_suppressed;

                if active {
                    was_active = true;
                    let snapshot = sampler.sample(
                        appearance.telemetry_net,
                        appearance.telemetry_cpu,
                        appearance.telemetry_memory,
                    );
                    let _ = app.emit_to(windows::BAR, TELEMETRY_EVENT, &snapshot);
                } else if was_active {
                    was_active = false;
                    sampler.reset();
                }

                std::thread::sleep(Duration::from_millis(1000));
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn net_calculator_warms_up_then_computes_exact_rates_and_rebases_on_change() {
        let mut calc = NetRateCalculator::default();
        let a1 = [AdapterCounter { id: 0x10, in_octets: 1_000, out_octets: 500 }];
        assert_eq!(calc.sample(&a1, 1_000_000), NetRateSample::WarmingUp);

        // 间隔过短（50ms）保持预热，不冲掉原基线
        let a_short = [AdapterCounter { id: 0x10, in_octets: 2_000, out_octets: 600 }];
        assert_eq!(calc.sample(&a_short, 1_050_000), NetRateSample::WarmingUp);

        // 1 秒后（相对于首次 1_000_000us）：下载 +102_400 字节，上传 +20_480 字节
        let a2 = [AdapterCounter { id: 0x10, in_octets: 103_400, out_octets: 20_980 }];
        match calc.sample(&a2, 2_000_000) {
            NetRateSample::Ready { down_bps, up_bps } => {
                assert!((down_bps - 102_400.0).abs() < 0.01);
                assert!((up_bps - 20_480.0).abs() < 0.01);
            }
            other => panic!("expected Ready, got {other:?}"),
        }

        // 计数器回绕重置时自动 rebase，不产生负数或超大溢出值
        let a_reset = [AdapterCounter { id: 0x10, in_octets: 100, out_octets: 50 }];
        assert_eq!(calc.sample(&a_reset, 3_000_000), NetRateSample::WarmingUp);

        // 活动网卡集合变化时自动 rebase
        let multi = [
            AdapterCounter { id: 0x10, in_octets: 200, out_octets: 100 },
            AdapterCounter { id: 0x20, in_octets: 5_000, out_octets: 1_000 },
        ];
        assert_eq!(calc.sample(&multi, 4_000_000), NetRateSample::WarmingUp);

        // 采样间隔超过 10 秒自动 rebase
        let multi_late = [
            AdapterCounter { id: 0x10, in_octets: 900, out_octets: 300 },
            AdapterCounter { id: 0x20, in_octets: 9_000, out_octets: 2_000 },
        ];
        assert_eq!(calc.sample(&multi_late, 15_000_000), NetRateSample::WarmingUp);
    }

    #[test]
    fn physical_adapter_filter_excludes_vpn_tunnels_virtual_and_disconnected() {
        // 标准活动物理 Wi-Fi (71) 与以太网 (6)
        assert!(is_active_physical_adapter(1, 71, 0, 0x01, 1));
        assert!(is_active_physical_adapter(1, 6, 0, 0x01, 1));
        // 隧道接口（VPN 等）必须排除
        assert!(!is_active_physical_adapter(1, 6, 1, 0x01, 1));
        assert!(!is_active_physical_adapter(1, 131, 0, 0x01, 1));
        // 非硬件接口（虚拟网卡 flags 缺少 0x01）必须排除
        assert!(!is_active_physical_adapter(1, 6, 0, 0x00, 1));
        // 含过滤驱动或暂停标记必须排除
        assert!(!is_active_physical_adapter(1, 6, 0, 0x01 | 0x02, 1));
        // 媒体已断开或非 Up 状态必须排除
        assert!(!is_active_physical_adapter(2, 6, 0, 0x01, 1));
        assert!(!is_active_physical_adapter(1, 6, 0, 0x01, 2));
    }

    #[test]
    fn cpu_calculator_subtracts_idle_from_kernel_plus_user() {
        let mut cpu = CpuCalculator::default();
        assert_eq!(cpu.sample(1_000, 1_200, 300, 1_000_000), None);
        // 经过 1s：idle +600, kernel +700（含 idle 600 + 内核忙 100）, user +300 => total=1000, busy=400 => 40%
        assert_eq!(cpu.sample(1_600, 1_900, 600, 2_000_000), Some(40));
        // 全空闲：idle +1000, kernel +1000, user +0 => 0%
        assert_eq!(cpu.sample(2_600, 2_900, 600, 3_000_000), Some(0));
        // 满载：idle +0, kernel +400, user +600 => 100%
        assert_eq!(cpu.sample(2_600, 3_300, 1_200, 4_000_000), Some(100));
    }

    #[test]
    fn memory_percentage_handles_bounds_safely() {
        assert_eq!(compute_memory_percent(0, 0), None);
        assert_eq!(compute_memory_percent(100, 120), None);
        assert_eq!(compute_memory_percent(16_000, 4_000), Some(75));
        assert_eq!(compute_memory_percent(16_000, 16_000), Some(0));
        assert_eq!(compute_memory_percent(16_000, 0), Some(100));
    }

    #[test]
    #[ignore]
    fn read_only_windows_telemetry_smoke() {
        let mut sampler = SystemSampler::default();
        let first = sampler.sample(true, true, true);
        assert!(matches!(first.net_state.as_str(), "warmingUp" | "unavailable"));
        assert!(first.memory_percent.is_some_and(|v| v <= 100));
        std::thread::sleep(Duration::from_millis(220));
        let second = sampler.sample(true, true, true);
        assert!(matches!(second.net_state.as_str(), "ready" | "unavailable"));
        assert!(second.cpu_percent.is_some_and(|v| v <= 100));
        assert!(second.memory_percent.is_some_and(|v| v <= 100));
    }
}
