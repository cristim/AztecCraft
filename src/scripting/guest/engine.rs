//! Helper-side wasmtime engine for sandboxed scripting (#57, T2a): configuration, per-run store
//! limits, the epoch ticker, cancellation through the epoch callback and the guest thread.
//!
//! This file is shared into the `script-host` helper with `#[path]` (the package is bin-only); no
//! editor type is used here. Nothing in it grants a capability: it only bounds a guest.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder, Trap, UpdateDeadline};

/// Wasm call-stack budget (PROVISIONAL, plan 4.2).
pub const MAX_WASM_STACK: usize = 4 * 1024 * 1024;
/// Native frames the host may need on the guest thread besides the wasm stack.
pub const HOST_MARGIN: usize = 1024 * 1024;
/// Stack of the dedicated guest thread (PROVISIONAL).
pub const GUEST_THREAD_STACK: usize = 16 * 1024 * 1024;
const _: () = assert!(GUEST_THREAD_STACK >= MAX_WASM_STACK + HOST_MARGIN);

/// Epoch tick; the callback checks cancellation and the wall limit once per tick.
pub const EPOCH_TICK: Duration = Duration::from_millis(10);
/// Default wall limit of a run (PROVISIONAL).
pub const DEFAULT_WALL: Duration = Duration::from_secs(30);
/// Default guest memory cap (PROVISIONAL).
pub const DEFAULT_MEMORY_BYTES: usize = 256 * 1024 * 1024;
/// Table element cap per table (PROVISIONAL).
pub const TABLE_ELEMENTS: usize = 100_000;
/// Fuel units per second used to size fuel. PROVISIONAL: `measure_fuel_throughput` (ignored
/// test) saw 9.5e9 for a dense arithmetic loop on a quiet Apple-silicon host and 0.85e9 to
/// 0.94e9 on the same host under heavy load. Sizing from the highest rate keeps legitimate heavy
/// scripts from running out of fuel before the wall limit; fuel is only the backstop for a dead
/// ticker, so a slow-burning loop ends later than 2x wall.
pub const FUEL_PER_SECOND: u64 = 10_000_000_000;

/// Per-run limits.
#[derive(Clone, Copy, Debug)]
pub struct RunLimits {
    pub memory_bytes: usize,
    pub wall: Duration,
}

impl Default for RunLimits {
    fn default() -> Self {
        Self {
            memory_bytes: DEFAULT_MEMORY_BYTES,
            wall: DEFAULT_WALL,
        }
    }
}

impl RunLimits {
    /// About twice the instructions the guest can execute in the wall limit, so a run still ends
    /// by itself when the ticker thread has died.
    pub fn fuel(&self) -> u64 {
        let fuel = self
            .wall
            .as_nanos()
            .saturating_mul(2 * u128::from(FUEL_PER_SECOND))
            / 1_000_000_000;
        u64::try_from(fuel).unwrap_or(u64::MAX).max(1)
    }
}

/// Why a run was stopped by the host rather than by the guest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    Draining,
    WallLimit,
}

impl std::fmt::Display for StopReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Draining => f.write_str("run cancelled"),
            Self::WallLimit => f.write_str("wall time limit reached"),
        }
    }
}

impl std::error::Error for StopReason {}

/// Typed outcome of a failed guest call; never carries guest text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunError {
    Cancelled,
    WallLimit,
    OutOfFuel,
    StackOverflow,
    Trap,
    Other,
}

pub fn classify(err: &wasmtime::Error) -> RunError {
    match err.downcast_ref::<StopReason>() {
        Some(StopReason::Draining) => return RunError::Cancelled,
        Some(StopReason::WallLimit) => return RunError::WallLimit,
        None => {}
    }
    match err.downcast_ref::<Trap>() {
        Some(Trap::OutOfFuel) => RunError::OutOfFuel,
        Some(Trap::StackOverflow) => RunError::StackOverflow,
        Some(_) => RunError::Trap,
        None => RunError::Other,
    }
}

/// The engine configuration of the helper. Threads and the component model are absent because
/// the wasmtime features are not built; GC and memory64 are switched off explicitly.
pub fn engine_config() -> Config {
    let mut config = Config::new();
    config.consume_fuel(true);
    config.epoch_interruption(true);
    config.max_wasm_stack(MAX_WASM_STACK);
    // wasmtime requires max_wasm_stack <= async_stack_size even though no async stack is used.
    config.async_stack_size(MAX_WASM_STACK);
    config.wasm_gc(false);
    config.wasm_memory64(false);
    // PROVISIONAL until the T8a python.wasm pins its feature set: CPython-WASI from the WASI SDK
    // is expected to need only reference-types, multi-value, sign-extension, bulk memory and
    // mutable globals (wasmtime defaults); everything below is switched off.
    config.wasm_simd(false);
    config.wasm_relaxed_simd(false);
    config.wasm_tail_call(false);
    config.wasm_multi_memory(false);
    config.wasm_extended_const(false);
    config.debug_info(false);
    config
}

/// Engine plus the ticker thread that advances its epoch every [`EPOCH_TICK`].
pub struct HostEngine {
    engine: Engine,
    stop: Arc<AtomicBool>,
    ticker: Option<JoinHandle<()>>,
}

impl HostEngine {
    pub fn new() -> wasmtime::Result<Self> {
        Self::with_ticker(true)
    }

    /// `ticker == false` exists so tests can prove fuel ends a run when the ticker dies.
    pub fn with_ticker(ticker: bool) -> wasmtime::Result<Self> {
        let engine = Engine::new(&engine_config())?;
        let stop = Arc::new(AtomicBool::new(false));
        let handle = ticker.then(|| {
            let engine = engine.clone();
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(EPOCH_TICK);
                    engine.increment_epoch();
                }
            })
        });
        Ok(Self {
            engine,
            stop,
            ticker: handle,
        })
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// A fresh store for one run: limiter, fuel and the cancelling epoch callback installed.
    pub fn new_store(
        &self,
        limits: &RunLimits,
        draining: Arc<AtomicBool>,
    ) -> wasmtime::Result<Store<RunState>> {
        let state = RunState {
            limits: StoreLimitsBuilder::new()
                .memory_size(limits.memory_bytes)
                .table_elements(TABLE_ELEMENTS)
                .instances(1)
                .memories(1)
                .tables(1)
                .trap_on_grow_failure(false)
                .build(),
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|state| &mut state.limits);
        store.set_fuel(limits.fuel())?;
        let deadline = Instant::now() + limits.wall;
        store.set_epoch_deadline(1);
        store.epoch_deadline_callback(move |_| {
            if draining.load(Ordering::Relaxed) {
                Err(wasmtime::Error::new(StopReason::Draining))
            } else if Instant::now() >= deadline {
                Err(wasmtime::Error::new(StopReason::WallLimit))
            } else {
                Ok(UpdateDeadline::Continue(1))
            }
        });
        Ok(store)
    }
}

impl Drop for HostEngine {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.ticker.take() {
            let _ = handle.join();
        }
    }
}

/// Store data of one run.
pub struct RunState {
    limits: StoreLimits,
}

/// Runs `f` on a thread with [`GUEST_THREAD_STACK`] bytes of stack and returns its result, or
/// `None` when the thread could not be spawned or panicked.
pub fn run_on_guest_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    std::thread::Builder::new()
        .name("script-guest".into())
        .stack_size(GUEST_THREAD_STACK)
        .spawn(f)
        .ok()?
        .join()
        .ok()
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
