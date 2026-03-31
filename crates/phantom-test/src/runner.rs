use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::phantom::{Phantom, PhantomInner};

type TestFn = Box<dyn FnOnce(&Phantom) -> crate::Result<()> + Send>;

/// Result of a single test execution.
#[derive(Clone, Debug)]
pub enum TestResult {
    Passed(Duration),
    Failed(Duration, String),
}

/// Event sent from the test thread to the monitor.
pub(crate) enum RunnerEvent {
    TestStarted(usize),
    TestFinished(usize, TestResult),
    SessionCreated(Arc<PhantomInner>, String),
    SessionEnded,
    Done,
}

/// A test runner that executes phantom integration tests, with optional
/// ratatui-based live monitor.
pub struct TestRunner {
    tests: Vec<(String, TestFn)>,
    monitor: Option<bool>,
}

impl TestRunner {
    pub fn new() -> Self {
        Self {
            tests: Vec::new(),
            monitor: None,
        }
    }

    /// Register a test.
    pub fn test(
        mut self,
        name: &str,
        f: impl FnOnce(&Phantom) -> crate::Result<()> + Send + 'static,
    ) -> Self {
        self.tests.push((name.to_string(), Box::new(f)));
        self
    }

    /// Force monitor mode on or off. If not set, auto-detects from
    /// `--monitor` in argv or the `PHANTOM_MONITOR` env var.
    pub fn monitor(mut self, enabled: bool) -> Self {
        self.monitor = Some(enabled);
        self
    }

    /// Run all tests and exit with the appropriate code.
    pub fn run(self) -> ! {
        let use_monitor = self.monitor.unwrap_or_else(|| {
            std::env::args().any(|a| a == "--monitor")
                || std::env::var("PHANTOM_MONITOR").is_ok()
        });

        #[cfg(feature = "monitor")]
        if use_monitor {
            crate::tui::run_with_tui(self.tests);
        }

        #[cfg(not(feature = "monitor"))]
        if use_monitor {
            eprintln!("Monitor mode requires the 'monitor' feature. Falling back to headless.");
        }

        if !use_monitor || cfg!(not(feature = "monitor")) {
            run_headless(self.tests);
        }

        unreachable!()
    }
}

/// Execute tests sequentially, printing results to stdout.
fn run_headless(tests: Vec<(String, TestFn)>) -> ! {
    let total = tests.len();
    let mut passed = 0;
    let mut failed = 0;
    let mut failures: Vec<(String, String)> = Vec::new();

    println!("\nphantom integration tests");
    println!("────────────────────────────\n");

    for (name, test_fn) in tests {
        let mut pt = match Phantom::new() {
            Ok(pt) => pt,
            Err(e) => {
                println!("  \x1b[31m✗\x1b[0m {name} (engine failed: {e})");
                failed += 1;
                failures.push((name, format!("engine failed: {e}")));
                continue;
            }
        };
        // No hook needed for headless mode
        let _ = &mut pt;

        let start = Instant::now();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| test_fn(&pt)));
        let elapsed = start.elapsed();

        match result {
            Ok(Ok(())) => {
                println!(
                    "  \x1b[32m✓\x1b[0m {} \x1b[2m{}\x1b[0m",
                    name,
                    format_duration(elapsed)
                );
                passed += 1;
            }
            Ok(Err(e)) => {
                println!(
                    "  \x1b[31m✗\x1b[0m {} \x1b[2m{}\x1b[0m",
                    name,
                    format_duration(elapsed)
                );
                println!("    {e}");
                failed += 1;
                failures.push((name, e.to_string()));
            }
            Err(panic_val) => {
                let msg = panic_message(&panic_val);
                println!(
                    "  \x1b[31m✗\x1b[0m {} \x1b[2m{}\x1b[0m",
                    name,
                    format_duration(elapsed)
                );
                println!("    PANIC: {msg}");
                failed += 1;
                failures.push((name, format!("PANIC: {msg}")));
            }
        }
    }

    println!("\n────────────────────────────");
    if failed == 0 {
        println!("\x1b[32m\x1b[1m{passed}/{total} passed\x1b[0m\n");
    } else {
        println!(
            "\x1b[32m{passed} passed\x1b[0m, \x1b[31m\x1b[1m{failed} failed\x1b[0m / {total} total\n"
        );
        for (name, msg) in &failures {
            println!("  \x1b[31m- {name}\x1b[0m: {msg}");
        }
        println!();
    }

    std::process::exit(if failed == 0 { 0 } else { 1 });
}

/// Run tests on a background thread, sending events via a channel.
pub(crate) fn run_tests_on_thread(
    tests: Vec<(String, TestFn)>,
    event_tx: crossbeam_channel::Sender<RunnerEvent>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("phantom-test-runner".into())
        .spawn(move || {
            for (idx, (_name, test_fn)) in tests.into_iter().enumerate() {
                let _ = event_tx.send(RunnerEvent::TestStarted(idx));

                let mut pt = match Phantom::new() {
                    Ok(pt) => pt,
                    Err(e) => {
                        let _ = event_tx.send(RunnerEvent::TestFinished(
                            idx,
                            TestResult::Failed(Duration::ZERO, format!("engine failed: {e}")),
                        ));
                        continue;
                    }
                };

                // Hook: notify the monitor when a session is created
                let tx = event_tx.clone();
                pt.on_session_created = Some(Arc::new(move |inner, session_name| {
                    let _ = tx.send(RunnerEvent::SessionCreated(inner, session_name));
                }));

                let start = Instant::now();
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| test_fn(&pt)));
                let elapsed = start.elapsed();

                let test_result = match result {
                    Ok(Ok(())) => TestResult::Passed(elapsed),
                    Ok(Err(e)) => TestResult::Failed(elapsed, e.to_string()),
                    Err(panic_val) => {
                        TestResult::Failed(elapsed, format!("PANIC: {}", panic_message(&panic_val)))
                    }
                };

                let _ = event_tx.send(RunnerEvent::SessionEnded);
                let _ = event_tx.send(RunnerEvent::TestFinished(idx, test_result));
            }
            let _ = event_tx.send(RunnerEvent::Done);
        })
        .expect("failed to spawn test runner thread")
}

pub(crate) fn format_duration(d: Duration) -> String {
    if d.as_secs() >= 1 {
        format!("{:.1}s", d.as_secs_f64())
    } else {
        format!("{}ms", d.as_millis())
    }
}

fn panic_message(val: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = val.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = val.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}
