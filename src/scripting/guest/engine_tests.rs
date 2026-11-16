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

fn call_i32(host: &HostEngine, limits: &RunLimits, text: &str, name: &str) -> i32 {
    let mut store = host.new_store(limits, Arc::default()).unwrap();
    let instance = instantiate(host, &mut store, text).unwrap();
    let f = instance
        .get_typed_func::<(), i32>(&mut store, name)
        .unwrap();
    f.call(&mut store, ()).unwrap()
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

#[test]
fn memory_growth_past_the_cap_returns_minus_one() {
    let host = HostEngine::new().unwrap();
    let text = r#"(module (memory 1)
        (func (export "ok") (result i32) (memory.grow (i32.const 10)))
        (func (export "over") (result i32) (memory.grow (i32.const 1000))))"#;
    // 4 MiB cap = 64 pages.
    assert_eq!(call_i32(&host, &small(1000), text, "ok"), 1);
    assert_eq!(call_i32(&host, &small(1000), text, "over"), -1);
}

#[test]
fn initial_memory_above_the_cap_is_refused() {
    let host = HostEngine::new().unwrap();
    let mut store = host.new_store(&small(1000), Arc::default()).unwrap();
    assert!(instantiate(&host, &mut store, "(module (memory 1000))").is_err());
}

#[test]
fn table_growth_past_the_cap_returns_minus_one() {
    let host = HostEngine::new().unwrap();
    let text = format!(
        r#"(module (table 1 funcref)
        (func (export "over") (result i32) (table.grow (ref.null func) (i32.const {}))))"#,
        TABLE_ELEMENTS + 1
    );
    assert_eq!(call_i32(&host, &small(1000), &text, "over"), -1);
}

#[test]
fn instance_memory_and_table_counts_are_capped() {
    let host = HostEngine::new().unwrap();
    let mut store = host.new_store(&small(1000), Arc::default()).unwrap();
    assert!(
        instantiate(
            &host,
            &mut store,
            "(module (table 1 funcref) (table 1 funcref))"
        )
        .is_err()
    );
    let mut store = host.new_store(&small(1000), Arc::default()).unwrap();
    assert!(instantiate(&host, &mut store, "(module)").is_ok());
    assert!(instantiate(&host, &mut store, "(module)").is_err());
}

#[test]
fn infinite_recursion_traps_on_the_guest_thread() {
    let outcome = run_on_guest_thread(|| {
        let host = HostEngine::new().unwrap();
        let text = r#"(module (func $f (export "run") (call $f)))"#;
        call_run(&host, &RunLimits::default(), Arc::default(), text)
    });
    assert_eq!(outcome, Some(RunError::StackOverflow));
}

#[test]
fn a_panicking_guest_thread_is_reported_as_none() {
    let outcome: Option<()> = run_on_guest_thread(|| panic!("probe"));
    assert_eq!(outcome, None);
}

#[test]
fn unneeded_proposals_are_rejected() {
    let host = HostEngine::new().unwrap();
    let cases = [
        (
            "simd",
            "(module (func (result v128) v128.const i32x4 0 0 0 0))",
        ),
        (
            "relaxed simd",
            "(module (func (param v128) (result v128) (i32x4.relaxed_trunc_f32x4_s (local.get 0))))",
        ),
        ("tail call", "(module (func $f return_call $f))"),
        ("multi memory", "(module (memory 1) (memory 1))"),
        (
            "extended const",
            "(module (global i32 (i32.add (i32.const 1) (i32.const 2))))",
        ),
    ];
    for (name, text) in cases {
        let bytes = wat::parse_str(text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            Module::new(host.engine(), bytes).is_err(),
            "{name} accepted"
        );
    }
}

#[test]
fn a_short_legitimate_run_finishes_under_default_limits() {
    let host = HostEngine::new().unwrap();
    let text = r#"(module (func (export "run") (local i32)
        (loop $l (local.set 0 (i32.add (local.get 0) (i32.const 1)))
          (br_if $l (i32.lt_u (local.get 0) (i32.const 1000000))))))"#;
    let mut store = host
        .new_store(&RunLimits::default(), Arc::default())
        .unwrap();
    let instance = instantiate(&host, &mut store, text).unwrap();
    let run = instance
        .get_typed_func::<(), ()>(&mut store, "run")
        .unwrap();
    run.call(&mut store, ()).unwrap();
}

#[test]
fn threads_are_not_enabled() {
    let host = HostEngine::new().unwrap();
    let shared = r#"(module (memory 1 1 shared))"#;
    assert!(Module::new(host.engine(), wat::parse_str(shared).unwrap()).is_err());
}

#[test]
fn trap_classification_is_typed() {
    let host = HostEngine::new().unwrap();
    let text = r#"(module (func (export "run") unreachable))"#;
    assert_eq!(
        call_run(&host, &RunLimits::default(), Arc::default(), text),
        RunError::Trap
    );
}

#[test]
#[ignore = "prints the fuel throughput used to size FUEL_PER_SECOND"]
fn measure_fuel_throughput() {
    let host = HostEngine::with_ticker(false).unwrap();
    let mut store = host
        .new_store(&RunLimits::default(), Arc::default())
        .unwrap();
    let budget = 10_000_000_000u64;
    store.set_fuel(budget).unwrap();
    let instance = instantiate(&host, &mut store, SPIN).unwrap();
    let run = instance
        .get_typed_func::<(), ()>(&mut store, "run")
        .unwrap();
    let start = Instant::now();
    let err = run.call(&mut store, ()).unwrap_err();
    let secs = start.elapsed().as_secs_f64();
    println!(
        "{:?}: fast loop, {budget} fuel in {secs:.2}s = {:.0} fuel/s",
        classify(&err),
        budget as f64 / secs
    );
}
