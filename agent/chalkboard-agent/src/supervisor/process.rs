// SPDX-License-Identifier: MIT OR Apache-2.0

//! Process supervisor restarting crashed child services within 1 s (OS-006).

use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Supervisor managing the lifecycle of the renderer process (OS-006).
#[derive(Debug)]
pub struct ProcessSupervisor {
    program: String,
    args: Vec<String>,
    running: Arc<AtomicBool>,
    restart_count: Arc<AtomicU32>,
    child: Arc<Mutex<Option<Child>>>,
    last_restart: Arc<Mutex<Option<Instant>>>,
    worker_handle: Option<JoinHandle<()>>,
}

impl ProcessSupervisor {
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            running: Arc::new(AtomicBool::new(false)),
            restart_count: Arc::new(AtomicU32::new(0)),
            child: Arc::new(Mutex::new(None)),
            last_restart: Arc::new(Mutex::new(None)),
            worker_handle: None,
        }
    }

    /// Spawns the supervisor loop, which restarts the child within 1 s if terminated (OS-006).
    pub fn start(&mut self) -> std::io::Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        self.running.store(true, Ordering::SeqCst);
        let program = self.program.clone();
        let args = self.args.clone();
        let running = self.running.clone();
        let restart_count = self.restart_count.clone();
        let child_arc = self.child.clone();
        let last_restart_arc = self.last_restart.clone();

        // Spawn initial child
        let first_child = Command::new(&program).args(&args).spawn()?;
        let first_pid = first_child.id();
        *child_arc.lock().unwrap() = Some(first_child);
        let _ = first_pid;

        let handle = thread::spawn(move || {
            while running.load(Ordering::SeqCst) {
                let mut should_restart = false;
                {
                    let mut lock = child_arc.lock().unwrap();
                    if let Some(ref mut c) = *lock {
                        match c.try_wait() {
                            Ok(Some(_status)) => {
                                should_restart = true;
                            }
                            Ok(None) => {}
                            Err(_) => {
                                should_restart = true;
                            }
                        }
                    } else {
                        should_restart = true;
                    }
                    if should_restart {
                        *lock = None;
                    }
                }

                if should_restart && running.load(Ordering::SeqCst) {
                    let restart_start = Instant::now();
                    // OS-006: Must restart within 1 s. A small backoff of 50 ms avoids CPU thrashing.
                    thread::sleep(Duration::from_millis(50));

                    if let Ok(new_child) = Command::new(&program).args(&args).spawn() {
                        *child_arc.lock().unwrap() = Some(new_child);
                        restart_count.fetch_add(1, Ordering::SeqCst);
                        *last_restart_arc.lock().unwrap() = Some(restart_start);
                    }
                }

                thread::sleep(Duration::from_millis(20));
            }

            // Cleanup child on exit
            if let Some(mut c) = child_arc.lock().unwrap().take() {
                let _ = c.kill();
                let _ = c.wait();
            }
        });

        self.worker_handle = Some(handle);
        Ok(())
    }

    /// Kills the active child to test the supervisor's rapid recovery (OS-006).
    pub fn kill_child(&self) -> bool {
        let mut lock = self.child.lock().unwrap();
        if let Some(ref mut c) = *lock {
            c.kill().is_ok()
        } else {
            false
        }
    }

    pub fn child_pid(&self) -> Option<u32> {
        self.child.lock().unwrap().as_ref().map(|c| c.id())
    }

    pub fn restart_count(&self) -> u32 {
        self.restart_count.load(Ordering::SeqCst)
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(h) = self.worker_handle.take() {
            let _ = h.join();
        }
        if let Some(mut c) = self.child.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl Drop for ProcessSupervisor {
    fn drop(&mut self) {
        self.stop();
    }
}
