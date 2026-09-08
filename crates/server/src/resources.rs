//! Process measurements sampled once per second, away from audio processing.
use serde::Serialize;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Default, Serialize)]
pub struct Stats {
    pub cpu_percent: Option<f64>,
    pub resident_bytes: Option<u64>,
    pub sampled_at_ms: f64,
}

pub fn start() -> Arc<Mutex<Stats>> {
    let stats = Arc::new(Mutex::new(Stats::default()));
    let shared = stats.clone();
    std::thread::Builder::new()
        .name("pr0-resource-stats".into())
        .spawn(move || {
            let mut previous = None;
            loop {
                let time = Instant::now();
                let cpu = cpu_seconds();
                let percent = cpu.zip(previous).and_then(|(cpu, (last_cpu, last_time))| {
                    cpu_percent(cpu - last_cpu, time.duration_since(last_time).as_secs_f64())
                });
                previous = cpu.map(|cpu| (cpu, time));
                *shared.lock().unwrap() = Stats {
                    cpu_percent: percent,
                    resident_bytes: resident_bytes(),
                    sampled_at_ms: crate::audio::monotonic_ms(),
                };
                std::thread::sleep(Duration::from_secs(1));
            }
        })
        .expect("Start resource monitor");
    stats
}

fn cpu_percent(cpu_seconds: f64, elapsed: f64) -> Option<f64> {
    (cpu_seconds.is_finite() && cpu_seconds >= 0. && elapsed > 0.)
        .then_some(cpu_seconds / elapsed * 100.)
}

#[cfg(unix)]
fn cpu_seconds() -> Option<f64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: getrusage initializes the correctly sized structure on success.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return None;
    }
    let usage = unsafe { usage.assume_init() };
    Some(
        usage.ru_utime.tv_sec as f64
            + usage.ru_stime.tv_sec as f64
            + (usage.ru_utime.tv_usec + usage.ru_stime.tv_usec) as f64 / 1e6,
    )
}
#[cfg(not(unix))]
fn cpu_seconds() -> Option<f64> {
    None
}

#[cfg(target_os = "macos")]
#[allow(deprecated)] // Use the existing libc binding to the stable native task_info API.
fn resident_bytes() -> Option<u64> {
    let mut info = std::mem::MaybeUninit::<libc::mach_task_basic_info>::uninit();
    let mut count = libc::MACH_TASK_BASIC_INFO_COUNT;
    // SAFETY: flavor, output structure and count match the Mach task_info ABI.
    let result = unsafe {
        libc::task_info(
            libc::mach_task_self(),
            libc::MACH_TASK_BASIC_INFO,
            info.as_mut_ptr().cast(),
            &mut count,
        )
    };
    if result != libc::KERN_SUCCESS {
        return None;
    }
    Some(unsafe { info.assume_init() }.resident_size)
}
#[cfg(target_os = "linux")]
fn resident_bytes() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = text.split_whitespace().nth(1)?.parse().ok()?;
    // SAFETY: sysconf has no pointer arguments and is called off the audio worker.
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    (page_size > 0).then(|| pages * page_size as u64)
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn resident_bytes() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_percent_represents_consumed_cores_not_machine_capacity() {
        assert_eq!(cpu_percent(0.5, 2.), Some(25.));
        assert_eq!(cpu_percent(3., 1.), Some(300.));
        assert_eq!(cpu_percent(-1., 1.), None);
        assert_eq!(cpu_percent(0., 0.), None);
    }
    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn current_process_reports_real_resident_memory_and_cpu_time() {
        assert!(resident_bytes().unwrap() > 0);
        assert!(cpu_seconds().unwrap() >= 0.);
    }
}
