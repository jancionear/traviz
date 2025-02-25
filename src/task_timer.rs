pub struct TaskTimer {
    start_time: std::time::Instant,
    task_name: String,
}

impl TaskTimer {
    pub fn new(task_name: impl AsRef<str>) -> Self {
        let start_time = std::time::Instant::now();
        println!("Task: {} started", task_name.as_ref());
        Self {
            start_time,
            task_name: task_name.as_ref().to_string(),
        }
    }

    pub fn stop(&self) {
        println!(
            "Task: {} finished in {:.1}ms",
            self.task_name,
            self.start_time.elapsed().as_secs_f64() * 1000.0
        );
    }
}
