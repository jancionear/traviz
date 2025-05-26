use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
struct FunctionTiming {
    total_duration: Duration,
    call_count: u64,
}

pub struct Profiler {
    timings: Arc<Mutex<HashMap<String, FunctionTiming>>>,
    frame_count: Arc<AtomicU32>,
}

pub static GLOBAL_PROFILER: LazyLock<Profiler> = LazyLock::new(Profiler::new);

impl Profiler {
    fn new() -> Self {
        let timings = Arc::new(Mutex::new(HashMap::<String, FunctionTiming>::new()));
        let timings_clone = Arc::clone(&timings);
        let frame_count = Arc::new(AtomicU32::new(0));
        let frame_count_clone = Arc::clone(&frame_count);
        thread::spawn(move || {
            let mut last_report_time = Instant::now();
            loop {
                thread::sleep(Duration::from_secs(5));

                let interval_duration;
                let current_frames_val;
                let report_data: Vec<(String, FunctionTiming)>;

                // Try to hold the lock for as short as possible
                {
                    let mut timings_guard = timings_clone.lock().unwrap();
                    let now = Instant::now();

                    current_frames_val = frame_count_clone.swap(0, Ordering::Relaxed);
                    interval_duration = now.duration_since(last_report_time);

                    report_data = timings_guard
                        .iter()
                        .map(|(name, timing)| (name.clone(), timing.clone()))
                        .collect();

                    // Reset timings for the next interval
                    for (_, timing) in timings_guard.iter_mut() {
                        timing.total_duration = Duration::ZERO;
                        timing.call_count = 0;
                    }
                    last_report_time = now;
                }

                println!(
                    "[PROFILE] Report for the last {:.2}s:",
                    interval_duration.as_secs_f32()
                );

                let mut sorted_timings = report_data;
                sorted_timings.sort_by_key(|(name, _)| name.clone());

                for (name, timing) in sorted_timings {
                    let avg_duration_ms = if timing.call_count > 0 {
                        (timing.total_duration.as_secs_f64() * 1000.0) / timing.call_count as f64
                    } else {
                        0.0
                    };
                    println!(
                        "[PROFILE]  - {}: {:.3}ms total ({} calls, avg {:.3}ms/call)",
                        name,
                        timing.total_duration.as_secs_f64() * 1000.0,
                        timing.call_count,
                        avg_duration_ms
                    );
                }

                let fps = if interval_duration.as_secs_f32() > 0.0 {
                    current_frames_val as f32 / interval_duration.as_secs_f32()
                } else {
                    0.0
                };
                println!("[PROFILE]  - Average FPS: {:.2}", fps);
                println!("[PROFILE] --- End of Report ---");
            }
        });

        Self {
            timings,
            frame_count,
        }
    }

    pub fn start_timing(&self, fn_name: &'static str) -> TimingGuard {
        TimingGuard {
            fn_name,
            start_time: Some(Instant::now()),
            timings_map: Arc::clone(&self.timings),
        }
    }

    pub fn increment_frame_count(&self) {
        self.frame_count.fetch_add(1, Ordering::Relaxed);
    }
}

pub struct TimingGuard {
    fn_name: &'static str,
    start_time: Option<Instant>,
    timings_map: Arc<Mutex<HashMap<String, FunctionTiming>>>,
}

impl Drop for TimingGuard {
    fn drop(&mut self) {
        if let Some(start_time) = self.start_time {
            let duration = start_time.elapsed();
            let mut timings = self.timings_map.lock().unwrap();
            let entry = timings.entry(self.fn_name.to_string()).or_default();
            entry.total_duration += duration;
            entry.call_count += 1;
        }
    }
}
