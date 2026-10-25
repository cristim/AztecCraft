//! Runs on the main thread (`harness = false`). A lookalike of winit's delegate class stands in
//! for `WinitApplicationDelegate`, which exists only inside a real winit process; the
//! production class lookup is covered by the native checklist, not here. The window cases need
//! a WindowServer session (an offscreen, never-shown NSWindow is created).

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod mac {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use macos_quit_menu::{
        TerminateError, TerminateHookError, finish_terminate, install_terminate_guard,
        install_terminate_guard_in, terminate_owed, terminate_reply_queued,
    };
    use objc2::encode::Encode;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, NSObject, Sel};
    use objc2::{ClassType, MainThreadMarker, msg_send, sel};
    use objc2_app_kit::{
        NSApplication, NSApplicationTerminateReply as Reply, NSBackingStoreType, NSWindow,
        NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    static PERFORM_CLOSE_CALLS: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn noop(_: *mut AnyObject, _: Sel, _: *mut AnyObject) {}

    extern "C" fn count_perform_close(_: *mut AnyObject, _: Sel, _: *mut AnyObject) {
        PERFORM_CLOSE_CALLS.fetch_add(1, Ordering::SeqCst);
    }

    fn lookalike(name: &'static std::ffi::CStr, with_shape: bool) -> &'static AnyClass {
        let mut builder = ClassBuilder::new(name, NSObject::class()).expect("unique class name");
        if with_shape {
            // SAFETY: both selectors take one object argument and return nothing, like `noop`.
            unsafe {
                builder.add_method(
                    sel!(applicationDidFinishLaunching:),
                    noop as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
                );
                builder.add_method(
                    sel!(applicationWillTerminate:),
                    noop as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
                );
            }
        }
        builder.register()
    }

    fn instance(class: &AnyClass) -> Retained<AnyObject> {
        // SAFETY: `new` is NSObject's allocate-and-init and returns an owned object.
        unsafe { msg_send![class, new] }
    }

    fn terminate(delegate: &AnyObject) -> Reply {
        // SAFETY: the installed method has the signature `(id, SEL, id) -> NSUInteger`.
        unsafe { msg_send![delegate, applicationShouldTerminate: std::ptr::null::<AnyObject>()] }
    }

    fn window() -> Retained<NSWindow> {
        let mut builder =
            ClassBuilder::new(c"AztecCraftTestWindow", NSWindow::class()).expect("unique name");
        // SAFETY: `performClose:` takes one object argument and returns nothing.
        unsafe {
            builder.add_method(
                sel!(performClose:),
                count_perform_close as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
        }
        let class = builder.register();
        let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(80.0, 60.0));
        // SAFETY: standard NSWindow designated initialiser on a freshly allocated instance.
        unsafe {
            let allocated: *mut AnyObject = msg_send![class, alloc];
            let window: *mut AnyObject = msg_send![
                allocated,
                initWithContentRect: rect,
                styleMask: NSWindowStyleMask::empty(),
                backing: NSBackingStoreType::Buffered,
                defer: true,
            ];
            Retained::from_raw(window.cast::<NSWindow>()).expect("window allocated")
        }
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn CGSessionCopyCurrentDictionary() -> *const std::ffi::c_void;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(object: *const std::ffi::c_void);
        static kCFRunLoopDefaultMode: *const std::ffi::c_void;
        fn CFRunLoopRunInMode(mode: *const std::ffi::c_void, seconds: f64, return_after: u8)
        -> i32;
    }

    /// Windows need a WindowServer connection; the session dictionary is NULL without one.
    fn has_gui_session() -> bool {
        // SAFETY: the call takes no arguments; a non-NULL result is an owned CF object that
        // is released exactly once.
        let session = unsafe { CGSessionCopyCurrentDictionary() };
        if session.is_null() {
            return false;
        }
        // SAFETY: `session` is the non-NULL object returned above.
        unsafe { CFRelease(session) };
        true
    }

    /// Runs the main queue, so a reply queued by `finish_terminate` is delivered.
    fn drain_main_queue() {
        // SAFETY: one 0.1 s default-mode pass of the main run loop runs the queued main-queue
        // block. The arguments match CFRunLoopRunInMode's C signature, and kCFRunLoopDefaultMode
        // is a constant CFStringRef. The pass waits the full 0.1 s because an earlier handled
        // source can end a return-after-source run before the queue runs.
        unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, 0) };
    }

    pub fn run() {
        let mtm = MainThreadMarker::new().expect("main thread");

        assert_eq!(Reply::ENCODING.to_string(), "Q", "reply type encoding");

        std::thread::spawn(|| {
            assert_eq!(
                install_terminate_guard(),
                Err(TerminateHookError::NotMainThread)
            );
        })
        .join()
        .expect("thread ran");
        assert_eq!(
            install_terminate_guard(),
            Err(TerminateHookError::ClassMissing),
            "winit's class does not exist in this process"
        );

        let bare = lookalike(c"AztecCraftBareDelegate", false);
        assert_eq!(
            install_terminate_guard_in(bare, &instance(bare)),
            Err(TerminateHookError::ShapeChanged)
        );
        let class = lookalike(c"AztecCraftWinitLookalike", true);
        assert_eq!(
            install_terminate_guard_in(class, &instance(bare)),
            Err(TerminateHookError::DelegateMismatch)
        );

        let delegate = instance(class);
        assert_eq!(install_terminate_guard_in(class, &delegate), Ok(()));
        assert_eq!(
            install_terminate_guard_in(class, &delegate),
            Err(TerminateHookError::AlreadyImplemented)
        );
        for error in [
            TerminateHookError::NotMainThread,
            TerminateHookError::ClassMissing,
            TerminateHookError::DelegateMismatch,
            TerminateHookError::ShapeChanged,
            TerminateHookError::AlreadyImplemented,
        ] {
            let text = error.to_string();
            assert!(
                text.contains("#261") && text.contains("known-issues"),
                "{text}"
            );
        }

        if !has_gui_session() {
            println!("terminate hook: terminate and window cases skipped, no GUI session");
            return;
        }

        // H1: no windows at all -> terminate now, nothing owed.
        assert_eq!(terminate(&delegate), Reply::TerminateNow);
        assert!(!terminate_owed());
        assert_eq!(finish_terminate(true), Err(TerminateError::NotOwed));

        // H4: a window that cannot close is never asked to; terminate is cancelled, not owed.
        let window = window();
        assert_eq!(terminate(&delegate), Reply::TerminateCancel);
        assert!(!terminate_owed());
        assert_eq!(PERFORM_CLOSE_CALLS.load(Ordering::SeqCst), 0);

        // H4: a closable window gets exactly one performClose:, however often terminate repeats.
        window.setStyleMask(NSWindowStyleMask::Closable);
        assert_eq!(terminate(&delegate), Reply::TerminateLater);
        assert!(terminate_owed());
        assert_eq!(PERFORM_CLOSE_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(terminate(&delegate), Reply::TerminateLater);
        assert_eq!(PERFORM_CLOSE_CALLS.load(Ordering::SeqCst), 1);

        assert_eq!(finish_terminate(false), Ok(()));
        assert!(!terminate_owed());
        assert_eq!(finish_terminate(false), Err(TerminateError::NotOwed));
        assert!(terminate_reply_queued());

        // #268: while the previous reply is still queued, a new terminate is declined. It owes
        // nothing and starts no close request, so the stale reply cannot answer it.
        assert_eq!(terminate(&delegate), Reply::TerminateCancel);
        assert!(!terminate_owed());
        assert_eq!(PERFORM_CLOSE_CALLS.load(Ordering::SeqCst), 1);
        // The queued block runs `replyToApplicationShouldTerminate:` with no terminate pending.
        drain_main_queue();
        assert!(!terminate_reply_queued());
        assert!(!terminate_owed());

        // Idle again: a new terminate begins normally.
        assert_eq!(terminate(&delegate), Reply::TerminateLater);
        assert!(terminate_owed());
        assert_eq!(PERFORM_CLOSE_CALLS.load(Ordering::SeqCst), 2);
        assert_eq!(finish_terminate(false), Ok(()));
        drain_main_queue();
        assert!(!terminate_reply_queued());

        // #268: while an AppKit modal is active the terminate is declined and nothing is
        // closed. The session shows the window and makes it key.
        let app = NSApplication::sharedApplication(mtm);
        let session = app.beginModalSessionForWindow(&window);
        assert_eq!(app.modalWindow().as_deref(), Some(&*window));
        assert_eq!(terminate(&delegate), Reply::TerminateCancel);
        assert!(!terminate_owed());
        assert_eq!(PERFORM_CLOSE_CALLS.load(Ordering::SeqCst), 2);
        // SAFETY: `session` was returned by `beginModalSessionForWindow:` above and is ended once.
        unsafe { app.endModalSession(session) };
        assert!(app.modalWindow().is_none());
        assert_eq!(terminate(&delegate), Reply::TerminateLater);
        assert!(terminate_owed());
        assert_eq!(PERFORM_CLOSE_CALLS.load(Ordering::SeqCst), 3);
        assert_eq!(finish_terminate(false), Ok(()));
        drain_main_queue();
        assert!(!terminate_reply_queued());
        window.orderOut(None);
        println!("terminate hook: ok");
    }
}

#[cfg(target_os = "macos")]
fn main() {
    mac::run();
}
