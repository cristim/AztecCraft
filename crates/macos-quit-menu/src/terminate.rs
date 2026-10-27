//! Vetoes `-[NSApplication terminate:]` (Dock Quit, logout, shutdown) so the unsaved-changes
//! guard runs first (#261).
//!
//! winit's private `WinitApplicationDelegate` implements no `applicationShouldTerminate:`, so
//! AppKit never asks the app before exiting. This module adds that method to the class at
//! runtime with `class_addMethod`. The method is add-only and never overwrites: if winit ever
//! implements the selector, installation fails loudly instead of shadowing it. The hook returns
//! `NSTerminateLater` after asking one window to close; the app answers later through
//! [`finish_terminate`]. Unlike swizzling, nothing winit relies on is replaced.
#![allow(unsafe_code)]

use std::cell::Cell;
use std::fmt;
use std::io::Write;
use std::panic::{UnwindSafe, catch_unwind};
use std::sync::Mutex;

use objc2::encode::Encode;
use objc2::ffi::class_addMethod;
use objc2::runtime::{AnyClass, AnyObject, Imp, NSObjectProtocol, Sel};
use objc2::{MainThreadMarker, sel};
use objc2_app_kit::{NSApplication, NSApplicationTerminateReply, NSWindow, NSWindowStyleMask};

use crate::terminate_core::{TerminateError, TerminateState, WindowFacts, choose_close_target};

const WINIT_DELEGATE: &std::ffi::CStr = c"WinitApplicationDelegate";

static STATE: Mutex<TerminateState> = Mutex::new(TerminateState::new());

fn lock_state() -> std::sync::MutexGuard<'static, TerminateState> {
    // The state is two plain flags; a poisoned lock still holds a consistent value.
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Why the terminate hook could not be installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminateHookError {
    NotMainThread,
    /// winit's delegate class does not exist (or the application has no delegate).
    ClassMissing,
    /// The application delegate is not an instance of the class being patched.
    DelegateMismatch,
    /// The class no longer has the shape winit 0.30.13 gave it.
    ShapeChanged,
    /// The class already implements `applicationShouldTerminate:`.
    AlreadyImplemented,
}

impl fmt::Display for TerminateHookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self {
            Self::NotMainThread => "the terminate hook must be installed from the main thread",
            Self::ClassMissing => "winit's application delegate class was not found",
            Self::DelegateMismatch => "the application delegate is not winit's delegate class",
            Self::ShapeChanged => "winit's application delegate class changed shape",
            Self::AlreadyImplemented => {
                "winit's application delegate already implements applicationShouldTerminate:"
            }
        };
        write!(
            f,
            "macOS Dock Quit, logout and shutdown cannot be routed to the unsaved-changes guard: \
             {what}; see known-issues (#261)"
        )
    }
}

impl std::error::Error for TerminateHookError {}

fn window_facts(window: &NSWindow) -> WindowFacts {
    WindowFacts {
        is_key: window.isKeyWindow(),
        is_main: window.isMainWindow(),
        is_visible: window.isVisible(),
        is_miniaturized: window.isMiniaturized(),
        is_closable: window.styleMask().contains(NSWindowStyleMask::Closable),
    }
}

fn reply_for_terminate(mtm: MainThreadMarker) -> NSApplicationTerminateReply {
    if lock_state().owed() {
        return NSApplicationTerminateReply::TerminateLater;
    }
    if lock_state().queued() {
        // The queued reply would answer a new request; refuse until it is delivered.
        let _ = writeln!(
            std::io::stderr(),
            "terminate declined while a reply is still queued, see known-issues (#268)"
        );
        return NSApplicationTerminateReply::TerminateCancel;
    }
    let app = NSApplication::sharedApplication(mtm);
    if app.modalWindow().is_some() {
        // An owed reply can only be sent from a later frame, which a nested modal loop (the
        // file dialogs run inside a frame) never lets run.
        let _ = writeln!(
            std::io::stderr(),
            "terminate declined while a modal window is active, see known-issues (#268)"
        );
        return NSApplicationTerminateReply::TerminateCancel;
    }
    let windows: Vec<_> = app.windows().iter().collect();
    if windows.is_empty() {
        return NSApplicationTerminateReply::TerminateNow;
    }
    let facts: Vec<_> = windows.iter().map(|window| window_facts(window)).collect();
    let Some(target) = choose_close_target(&facts).and_then(|index| windows.get(index)) else {
        eprintln!("no closable window to guard; cancelling terminate, see known-issues (#261)");
        return NSApplicationTerminateReply::TerminateCancel;
    };
    lock_state().begin();
    app.unhide(None);
    if target.isMiniaturized() {
        target.deminiaturize(None);
    }
    // `activate` exists only on macOS 14+; calling it elsewhere raises an Objective-C exception,
    // which `catch_unwind` cannot stop.
    if app.respondsToSelector(sel!(activate)) {
        app.activate();
    } else {
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    }
    target.performClose(None);
    NSApplicationTerminateReply::TerminateLater
}

thread_local! {
    /// Set while `should_terminate` runs: its own `guarded` answers a panic there, so the global
    /// panic hook must not queue a second reply.
    static IN_HOOK: Cell<bool> = const { Cell::new(false) };
}

struct InHook;

impl InHook {
    fn enter() -> Self {
        let _ = IN_HOOK.try_with(|flag| flag.set(true));
        Self
    }
}

impl Drop for InHook {
    fn drop(&mut self) {
        let _ = IN_HOOK.try_with(|flag| flag.set(false));
    }
}

extern "C" fn should_terminate(
    _this: *mut AnyObject,
    _cmd: Sel,
    _sender: *mut AnyObject,
) -> NSApplicationTerminateReply {
    let _in_hook = InHook::enter();
    guarded(|| match MainThreadMarker::new() {
        Some(mtm) => reply_for_terminate(mtm),
        None => NSApplicationTerminateReply::TerminateCancel,
    })
}

/// Unwinding out of an Objective-C callback is undefined behaviour, so any panic cancels the
/// terminate and first clears an owed state the panicking code may have created, otherwise
/// nothing would ever answer it.
fn guarded(
    body: impl FnOnce() -> NSApplicationTerminateReply + UnwindSafe,
) -> NSApplicationTerminateReply {
    catch_unwind(body).unwrap_or_else(|_| {
        let _ = lock_state().reply(false, |_| {});
        // `eprintln!` itself panics on a closed stderr, which must not escape here.
        let _ = writeln!(
            std::io::stderr(),
            "terminate hook panicked; cancelling terminate, see known-issues (#261)"
        );
        NSApplicationTerminateReply::TerminateCancel
    })
}

/// Declines an owed terminate when the main thread panics outside `should_terminate`. winit
/// catches a frame panic and stops its loop, but AppKit keeps waiting for a reply that no later
/// frame will send, so the process would hang instead of exiting on the panic (#270). Other
/// threads do not stop the app, and a quit the user can still finish must not be cancelled.
fn answer_owed_on_panic() {
    if MainThreadMarker::new().is_none() || IN_HOOK.try_with(Cell::get).unwrap_or(false) {
        return;
    }
    if !terminate_owed() {
        return;
    }
    if finish_terminate(false).is_ok() {
        let _ = writeln!(
            std::io::stderr(),
            "panic while a terminate reply was owed; declining it so the process can exit, \
             see known-issues (#270)"
        );
    }
}

/// Chains a panic hook that runs [`answer_owed_on_panic`] before the previous hook. Installed
/// by [`install_terminate_guard`]; public only so the integration test can install it.
#[doc(hidden)]
pub fn install_panic_answer() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        answer_owed_on_panic();
        previous(info);
    }));
}

fn is_instance_of(object: &AnyObject, class: &AnyClass) -> bool {
    let mut current = Some(object.class());
    while let Some(candidate) = current {
        if std::ptr::eq(candidate, class) {
            return true;
        }
        current = candidate.superclass();
    }
    false
}

/// Adds `applicationShouldTerminate:` to `class`. `delegate` is the application's delegate and
/// must be an instance of `class`. Checks the class still has winit's shape before touching it.
pub fn install_terminate_guard_in(
    class: &AnyClass,
    delegate: &AnyObject,
) -> Result<(), TerminateHookError> {
    MainThreadMarker::new().ok_or(TerminateHookError::NotMainThread)?;
    if !is_instance_of(delegate, class) {
        return Err(TerminateHookError::DelegateMismatch);
    }
    let shape_ok = class
        .superclass()
        .is_some_and(|superclass| superclass.name() == c"NSObject")
        && class
            .instance_method(sel!(applicationDidFinishLaunching:))
            .is_some()
        && class
            .instance_method(sel!(applicationWillTerminate:))
            .is_some();
    if !shape_ok {
        return Err(TerminateHookError::ShapeChanged);
    }
    let hook = sel!(applicationShouldTerminate:);
    // `Q@:@` for the real reply type: return value, receiver, selector, sender.
    let types = format!("{}@:@\0", NSApplicationTerminateReply::ENCODING);
    // SAFETY: `Imp` is `unsafe extern "C-unwind" fn()`, a type-erased function pointer. The real
    // signature of `should_terminate` is `(id, SEL, id) -> NSUInteger`, matching `types`; it
    // never unwinds (catch_unwind), so declaring it `extern "C"` is sound. Function pointers
    // have the same size and representation, and the runtime calls the IMP with that signature.
    let imp = unsafe {
        std::mem::transmute::<
            extern "C" fn(*mut AnyObject, Sel, *mut AnyObject) -> NSApplicationTerminateReply,
            Imp,
        >(should_terminate)
    };
    // SAFETY: `class` is a live class and `types` a NUL-terminated encoding. The
    // runtime copies the class pointer and the encoding; it does not mutate through `class`
    // beyond adding the method, which only this function does and only on the main thread.
    let added = unsafe {
        class_addMethod(
            std::ptr::from_ref(class).cast_mut(),
            hook,
            imp,
            types.as_ptr().cast(),
        )
    };
    if added.as_bool() {
        Ok(())
    } else {
        Err(TerminateHookError::AlreadyImplemented)
    }
}

/// Installs the hook on the running application's winit delegate. Native-only: the class
/// exists only inside a real winit process. Prints the error before returning it so a launch
/// that exits at startup leaves a trace.
pub fn install_terminate_guard() -> Result<(), TerminateHookError> {
    let result = MainThreadMarker::new()
        .ok_or(TerminateHookError::NotMainThread)
        .and_then(|mtm| {
            let class = AnyClass::get(WINIT_DELEGATE).ok_or(TerminateHookError::ClassMissing)?;
            let delegate = NSApplication::sharedApplication(mtm)
                .delegate()
                .ok_or(TerminateHookError::ClassMissing)?;
            let delegate: &AnyObject = (*delegate).as_ref();
            install_terminate_guard_in(class, delegate)
        });
    match &result {
        Ok(()) => install_panic_answer(),
        Err(error) => eprintln!("{error}"),
    }
    result
}

/// Whether a termination is waiting for the app's answer.
pub fn terminate_owed() -> bool {
    lock_state().owed()
}

/// Answers the owed termination. The AppKit reply runs on the main queue after the caller
/// returns (never inside an egui frame: `applicationWillTerminate:` re-enters winit's event
/// handler). `Err(NotOwed)` when nothing is owed.
pub fn finish_terminate(approved: bool) -> Result<(), TerminateError> {
    let mut state = lock_state();
    let mut to_send = None;
    state.reply(approved, |approved| to_send = Some(approved))?;
    state.mark_queued(approved);
    drop(state);
    if let Some(approved) = to_send {
        dispatch2::DispatchQueue::main().exec_async(move || {
            lock_state().delivered();
            if let Some(mtm) = MainThreadMarker::new() {
                NSApplication::sharedApplication(mtm).replyToApplicationShouldTerminate(approved);
            } else {
                eprintln!(
                    "terminate reply ran off the main thread; dropped, see known-issues (#261)"
                );
            }
        });
    }
    Ok(())
}

/// Whether an AppKit reply is queued and not yet delivered.
pub fn terminate_reply_queued() -> bool {
    lock_state().queued()
}

/// The answer in the queued AppKit reply, if one is queued. Lets the harness tell a decline
/// from an approval.
#[doc(hidden)]
pub fn terminate_reply_queued_answer() -> Option<bool> {
    lock_state().queued_answer()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Both tests drive the one process-wide state.
    static SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn a_panic_off_the_main_thread_leaves_an_owed_reply_alone() {
        let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        lock_state().begin();
        answer_owed_on_panic();
        let owed = terminate_owed();
        let queued = terminate_reply_queued();
        let _ = lock_state().reply(false, |_| {});
        assert!(owed, "a background panic must not answer");
        assert!(!queued);
    }

    #[test]
    fn a_panic_after_begin_cancels_and_clears_the_owed_state() {
        let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        let reply = guarded(|| {
            lock_state().begin();
            panic!("forced");
        });
        assert_eq!(reply, NSApplicationTerminateReply::TerminateCancel);
        assert!(!terminate_owed());
        assert!(!terminate_reply_queued());
    }
}
