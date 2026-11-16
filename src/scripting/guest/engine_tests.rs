use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use wasmtime::{Instance, Module, Store};

use super::*;

const SPIN: &str = r#"(module (func (export "run") (loop $l (br $l))))"#;
/// A dense arithmetic loop, the one `measure_fuel_throughput` uses to size the fuel budget.
const SPIN_FAST: &str = r#"(module (func (export "run") (local i64)
    (loop $l (local.set 0 (i64.add (local.get 0) (i64.const 1))) (br $l))))"#;

fn small(wall_ms: u64) -> RunLimits {
    RunLimits {
        memory_bytes: 4 * 1024 * 1024,
        wall: Duration::from_millis(wall_ms),
    }
}

fn instantiate(
    host: &HostEngine,
    store: &mut Store<RunState>,
    wat_text: &str,
) -> wasmtime::Result<Instance> {
    let module = Module::new(host.engine(), wat::parse_str(wat_text)?)?;
    Instance::new(store, &module, &[])
}

fn call_run(
    host: &HostEngine,
    limits: &RunLimits,
    draining: Arc<AtomicBool>,
    text: &str,
) -> RunError {
    let mut store = host.new_store(limits, draining).unwrap();
    let instance = instantiate(host, &mut store, text).unwrap();
    let run = instance
        .get_typed_func::<(), ()>(&mut store, "run")
        .unwrap();
    classify(&run.call(&mut store, ()).unwrap_err())
}

#[test]
fn fuel_ends_a_spin_when_no_ticker_runs() {
    let host = HostEngine::with_ticker(false).unwrap();
    let limits = small(100);
    let start = Instant::now();
    let err = call_run(&host, &limits, Arc::default(), SPIN_FAST);
    assert_eq!(err, RunError::OutOfFuel);
    assert!(
        start.elapsed() < Duration::from_secs(20),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn epoch_wall_limit_stops_a_spin_with_ample_fuel() {
    let host = HostEngine::new().unwrap();
    let limits = RunLimits {
        wall: Duration::from_millis(100),
        ..small(100)
    };
    let start = Instant::now();
    let mut store = host.new_store(&limits, Arc::default()).unwrap();
    store.set_fuel(u64::MAX / 2).unwrap();
    let instance = instantiate(&host, &mut store, SPIN).unwrap();
    let run = instance
        .get_typed_func::<(), ()>(&mut store, "run")
        .unwrap();
    let err = classify(&run.call(&mut store, ()).unwrap_err());
    assert_eq!(err, RunError::WallLimit);
    assert!(start.elapsed() >= Duration::from_millis(90));
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn draining_cancels_through_the_callback_not_the_first_tick() {
    let host = HostEngine::new().unwrap();
    let draining = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&draining);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        flag.store(true, Ordering::Relaxed);
    });
    let limits = RunLimits {
        wall: Duration::from_secs(20),
        ..small(100)
    };
    let mut store = host.new_store(&limits, draining).unwrap();
    store.set_fuel(u64::MAX / 2).unwrap();
    let instance = instantiate(&host, &mut store, SPIN).unwrap();
    let run = instance
        .get_typed_func::<(), ()>(&mut store, "run")
        .unwrap();
    let start = Instant::now();
    let err = classify(&run.call(&mut store, ()).unwrap_err());
    // A bare epoch trap would end the run after one 10 ms tick and not be Cancelled.
    assert_eq!(err, RunError::Cancelled);
    assert!(
        start.elapsed() >= Duration::from_millis(100),
        "{:?}",
        start.elapsed()
    );
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn fuel_budget_scales_with_the_wall_limit() {
    assert_eq!(small(1000).fuel(), 2 * FUEL_PER_SECOND);
    assert!(RunLimits::default().fuel() >= FUEL_PER_SECOND);
    assert_eq!(small(500).fuel(), FUEL_PER_SECOND);
    assert_eq!(small(0).fuel(), 1);
}
