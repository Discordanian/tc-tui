use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;
use sysinfo::System;

#[derive(Clone)]
pub struct SysSnapshot {
    pub cpu_count: usize,
    pub total_ram: f64,
    pub used_ram: f64,
    pub cpu_load: f32,
}

impl SysSnapshot {
    /// Placeholder snapshot used before the monitor thread produces its first
    /// real sample.
    pub fn empty() -> Self {
        SysSnapshot {
            cpu_count: 0,
            total_ram: 0.0,
            used_ram: 0.0,
            cpu_load: 0.0,
        }
    }
}

pub fn take_snapshot(sys: &System) -> SysSnapshot {
    let cpu_load = if sys.cpus().is_empty() {
        0.0
    } else {
        sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / sys.cpus().len() as f32
    };
    SysSnapshot {
        cpu_count: sys.cpus().len(),
        total_ram: sys.total_memory() as f64 / 1_073_741_824.0,
        used_ram: sys.used_memory() as f64 / 1_073_741_824.0,
        cpu_load,
    }
}

pub fn current_cpu_load(sys: &System) -> f32 {
    if sys.cpus().is_empty() {
        0.0
    } else {
        sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / sys.cpus().len() as f32
    }
}

/// Shared system state produced by [`spawn_system_monitor`] and consumed by any
/// frontend. Holds the latest snapshot plus a rolling CPU-load history.
#[derive(Clone)]
pub struct SysState {
    pub snapshot: SysSnapshot,
    pub cpu_history: Vec<f32>,
}

impl SysState {
    pub fn empty() -> Self {
        SysState {
            snapshot: SysSnapshot::empty(),
            cpu_history: Vec::new(),
        }
    }
}

/// Spawn a background thread that samples CPU/memory every `cpu_sample_secs`,
/// maintaining a rolling history of `cpu_history_len` load samples. Sending on
/// the paired refresh channel forces an immediate resample.
pub fn spawn_system_monitor(
    shared: Arc<Mutex<SysState>>,
    refresh_rx: mpsc::Receiver<()>,
    cpu_history_len: usize,
    cpu_sample_secs: u64,
) {
    thread::spawn(move || {
        let mut sys = System::new_all();
        sys.refresh_cpu_usage();
        sys.refresh_memory();

        // sysinfo needs a minimum interval between CPU refreshes to produce a
        // meaningful first reading.
        thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);

        let mut history: VecDeque<f32> = VecDeque::with_capacity(cpu_history_len);

        loop {
            sys.refresh_cpu_usage();
            sys.refresh_memory();

            let snap = take_snapshot(&sys);
            if cpu_history_len > 0 {
                if history.len() >= cpu_history_len {
                    history.pop_front();
                }
                history.push_back(snap.cpu_load);
            }

            if let Ok(mut s) = shared.lock() {
                s.snapshot = snap;
                s.cpu_history = history.iter().copied().collect();
            }

            match refresh_rx.recv_timeout(Duration::from_secs(cpu_sample_secs)) {
                Ok(()) => continue,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_cpu_count_matches_sysinfo() {
        let mut sys = System::new_all();
        sys.refresh_cpu_usage();
        let snap = take_snapshot(&sys);
        assert_eq!(snap.cpu_count, sys.cpus().len());
    }

    #[test]
    fn snapshot_total_ram_positive() {
        let mut sys = System::new_all();
        sys.refresh_memory();
        let snap = take_snapshot(&sys);
        assert!(snap.total_ram > 0.0);
    }

    #[test]
    fn snapshot_used_ram_lte_total() {
        let mut sys = System::new_all();
        sys.refresh_memory();
        let snap = take_snapshot(&sys);
        assert!(snap.used_ram <= snap.total_ram);
    }

    #[test]
    fn current_cpu_load_in_range() {
        let mut sys = System::new_all();
        sys.refresh_cpu_usage();
        let load = current_cpu_load(&sys);
        assert!((0.0..=100.0).contains(&load));
    }

    #[test]
    fn sys_state_empty_has_no_history() {
        let s = SysState::empty();
        assert!(s.cpu_history.is_empty());
        assert_eq!(s.snapshot.cpu_count, 0);
    }
}
