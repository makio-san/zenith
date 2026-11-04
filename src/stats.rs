//! Sistem ölçümleri: ayrı bir iş parçacığında 1,5 sn'de bir örneklenir.

use std::collections::VecDeque;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use sysinfo::{Components, Disks, Networks, System};

#[derive(Clone, Default)]
pub struct Stats {
    pub cpu_name: String,
    pub cores: usize,
    pub os: String,
    pub host: String,
    pub gpu_name: String,
    pub cpu: f32,
    pub per_core: Vec<f32>,
    pub mem_used: u64,
    pub mem_total: u64,
    pub swap_used: u64,
    pub gpu: Option<f32>,
    pub temp: Option<f32>,
    pub rx: f64,
    pub tx: f64,
    pub disk_used: u64,
    pub disk_total: u64,
    pub uptime: u64,
    pub cpu_hist: VecDeque<f32>,
    pub ram_hist: VecDeque<f32>,
}

pub type Shared = Arc<Mutex<Stats>>;

pub fn start(ctx: egui::Context) -> Shared {
    let shared: Shared = Arc::new(Mutex::new(Stats::default()));
    let out = shared.clone();
    thread::spawn(move || {
        let mut sys = System::new_all();
        let mut nets = Networks::new_with_refreshed_list();
        let mut disks = Disks::new_with_refreshed_list();
        let mut comps = Components::new_with_refreshed_list();

        let cpu_name = sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default();
        let cores = sys.cpus().len();
        let os = System::long_os_version().unwrap_or_default();
        let host = System::host_name().unwrap_or_default();
        let gpu_name = gpu_name_mac().unwrap_or_else(|| "Unknown".into());

        let interval = Duration::from_millis(1500);
        loop {
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            nets.refresh();
            disks.refresh();
            comps.refresh();

            let (mut rx, mut tx) = (0u64, 0u64);
            for (_, d) in nets.iter() {
                rx += d.received();
                tx += d.transmitted();
            }
            let secs = interval.as_secs_f64();

            let root = disks
                .iter()
                .find(|d| d.mount_point().to_string_lossy() == "/")
                .or_else(|| disks.iter().next());
            let (disk_total, disk_used) = root
                .map(|d| (d.total_space(), d.total_space() - d.available_space()))
                .unwrap_or((0, 0));

            let temp = comps
                .iter()
                .filter(|c| {
                    let l = c.label().to_ascii_lowercase();
                    l.contains("cpu") || l.contains("soc") || l.contains("pmu tdie")
                })
                .map(|c| c.temperature())
                .fold(None, |m: Option<f32>, t| Some(m.map_or(t, |m| m.max(t))));

            let cpu = sys.global_cpu_usage();
            let mem_used = sys.used_memory();
            let mem_total = sys.total_memory();

            if let Ok(mut s) = out.lock() {
                s.cpu_name = cpu_name.clone();
                s.cores = cores;
                s.os = os.clone();
                s.host = host.clone();
                s.gpu_name = gpu_name.clone();
                s.cpu = cpu;
                s.per_core = sys.cpus().iter().map(|c| c.cpu_usage()).collect();
                s.mem_used = mem_used;
                s.mem_total = mem_total;
                s.swap_used = sys.used_swap();
                s.gpu = gpu_util_mac();
                s.temp = temp;
                s.rx = rx as f64 / secs;
                s.tx = tx as f64 / secs;
                s.disk_used = disk_used;
                s.disk_total = disk_total;
                s.uptime = System::uptime();
                s.cpu_hist.push_back(cpu);
                s.ram_hist.push_back(if mem_total > 0 { mem_used as f32 / mem_total as f32 * 100.0 } else { 0.0 });
                while s.cpu_hist.len() > 60 {
                    s.cpu_hist.pop_front();
                }
                while s.ram_hist.len() > 60 {
                    s.ram_hist.pop_front();
                }
            }
            ctx.request_repaint();
            thread::sleep(interval);
        }
    });
    shared
}

/// macOS: GPU kullanımını ioreg'den oku ("Device Utilization %").
fn gpu_util_mac() -> Option<f32> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let out = Command::new("ioreg").args(["-r", "-d", "1", "-w", "0", "-c", "IOAccelerator"]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let idx = text.find("\"Device Utilization %\"")?;
    let rest = &text[idx..];
    let eq = rest.find('=')?;
    let num: String = rest[eq + 1..].trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
    num.parse().ok()
}

/// macOS: GPU adı (system_profiler).
fn gpu_name_mac() -> Option<String> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let out = Command::new("system_profiler").args(["SPDisplaysDataType"]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .find_map(|l| l.trim().strip_prefix("Chipset Model:").map(|s| s.trim().to_string()))
}

pub fn fmt_bytes(n: u64) -> String {
    let n = n as f64;
    if n >= 1e9 { format!("{:.1} GB", n / 1e9) } else { format!("{:.0} MB", n / 1e6) }
}

pub fn fmt_rate(n: f64) -> String {
    if n >= 1e6 { format!("{:.1} MB/s", n / 1e6) } else { format!("{:.0} KB/s", n / 1e3) }
}

pub fn fmt_uptime(s: u64) -> String {
    let h = s / 3600;
    let m = (s % 3600) / 60;
    if h >= 24 { format!("{}d {}h", h / 24, h % 24) } else { format!("{}h {}m", h, m) }
}
