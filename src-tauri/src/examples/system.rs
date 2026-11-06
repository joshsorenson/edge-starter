//! Example: this Mac's CPU, memory and network use. A background thread
//! samples every 2 seconds and keeps the last 2 minutes for charts.

use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;
use sysinfo::{Networks, System};

const EVERY: Duration = Duration::from_secs(2);
const KEEP: usize = 60; // 2 minutes

#[derive(Debug, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    /// Unix ms.
    pub at: i64,
    /// 0–100, all cores.
    pub cpu: f32,
    pub mem_used: u64,
    /// Bytes per second across all interfaces.
    pub down: u64,
    pub up: u64,
}

#[derive(Debug, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub samples: Vec<Sample>,
    pub mem_total: u64,
    pub swap_used: u64,
    pub cores: Vec<f32>,
    pub cpu_name: String,
    /// Top processes by CPU: (name, cpu %, memory bytes).
    pub top: Vec<(String, f32, u64)>,
    pub uptime_secs: u64,
    pub load: [f64; 3],
}

static STATE: Mutex<Option<(VecDeque<Sample>, Snapshot)>> = Mutex::new(None);

/// Start the sampler (once, at launch).
pub fn start() {
    std::thread::spawn(|| {
        let mut sys = System::new_all();
        let mut nets = Networks::new_with_refreshed_list();
        let mut tick = 0u32;
        loop {
            std::thread::sleep(EVERY);
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            nets.refresh(true);
            let (rx, tx) = nets.iter().filter(|(name, _)| !name.starts_with("lo")).fold((0, 0), |(r, t), (_, n)| (r + n.received(), t + n.transmitted()));
            let secs = EVERY.as_secs_f64();
            let s = Sample {
                at: chrono::Utc::now().timestamp_millis(),
                cpu: sys.global_cpu_usage(),
                mem_used: sys.used_memory(),
                down: (rx as f64 / secs) as u64,
                up: (tx as f64 / secs) as u64,
            };
            // Processes are heavier; refresh them every 3rd tick (6 s).
            if tick % 3 == 0 {
                sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
            }
            tick = tick.wrapping_add(1);
            let mut top: Vec<(String, f32, u64)> = {
                // Group helpers (e.g. many "Google Chrome Helper") under one name.
                let mut by: std::collections::HashMap<String, (f32, u64)> = std::collections::HashMap::new();
                for p in sys.processes().values() {
                    let name = p.name().to_string_lossy().split(" Helper").next().unwrap_or("").trim().to_string();
                    let e = by.entry(name).or_default();
                    e.0 += p.cpu_usage();
                    e.1 += p.memory();
                }
                by.into_iter().map(|(n, (c, m))| (n, c, m)).collect()
            };
            top.sort_by(|a, b| b.1.total_cmp(&a.1));
            top.truncate(6);
            let load = System::load_average();
            let mut g = STATE.lock().unwrap();
            let (hist, snap) = g.get_or_insert_with(Default::default);
            hist.push_back(s);
            while hist.len() > KEEP {
                hist.pop_front();
            }
            *snap = Snapshot {
                samples: hist.iter().cloned().collect(),
                mem_total: sys.total_memory(),
                swap_used: sys.used_swap(),
                cores: sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
                cpu_name: sys.cpus().first().map(|c| c.brand().to_string()).unwrap_or_default(),
                top,
                uptime_secs: System::uptime(),
                load: [load.one, load.five, load.fifteen],
            };
        }
    });
}

pub fn snapshot() -> Snapshot {
    STATE.lock().unwrap().as_ref().map(|(_, s)| s.clone()).unwrap_or_default()
}
