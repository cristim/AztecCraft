//! Runs on the main thread (`harness = false`) because menus need a `MainThreadMarker`.
//! The menu is hand-built in winit's shape; no NSApplication is created.

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod mac {
    use macos_quit_menu::{QuitMenuError, retarget_quit};
    use objc2::MainThreadMarker;
    use objc2::runtime::Sel;
    use objc2::sel;
    use objc2_app_kit::NSEventModifierFlags;
    use objc2_app_kit::{NSMenu, NSMenuItem};
    use objc2_foundation::NSString;

    fn item(
        mtm: MainThreadMarker,
        title: &str,
        action: Option<Sel>,
        key: &str,
    ) -> objc2::rc::Retained<NSMenuItem> {
        // SAFETY: the selectors are plain action names stored on the item, never invoked here.
        unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc(),
                &NSString::from_str(title),
                action,
                &NSString::from_str(key),
            )
        }
    }

    fn menu(
        mtm: MainThreadMarker,
        items: &[(&str, Option<Sel>, &str)],
    ) -> objc2::rc::Retained<NSMenu> {
        let main = NSMenu::new(mtm);
        let top = item(mtm, "App", None, "");
        let sub = NSMenu::new(mtm);
        for (title, action, key) in items {
            sub.addItem(&item(mtm, title, *action, key));
        }
        top.setSubmenu(Some(&sub));
        main.addItem(&top);
        main
    }

    fn snapshot(menu: &NSMenu) -> Vec<(String, Option<Sel>, String)> {
        let top = menu.itemArray().objectAtIndex(0);
        let sub = top.submenu().expect("submenu");
        sub.itemArray()
            .iter()
            .map(|i| {
                (
                    i.title().to_string(),
                    i.action(),
                    i.keyEquivalent().to_string(),
                )
            })
            .collect()
    }

    pub fn run() {
        let mtm = MainThreadMarker::new().expect("main thread");

        let m = menu(
            mtm,
            &[
                ("About X", Some(sel!(orderFrontStandardAboutPanel:)), ""),
                ("Hide X", Some(sel!(hide:)), "h"),
                ("Quit X", Some(sel!(terminate:)), "q"),
            ],
        );
        let before = snapshot(&m);
        retarget_quit(&m).expect("retarget");
        let after = snapshot(&m);
        assert_eq!(
            after[2],
            ("Quit X".into(), Some(sel!(performClose:)), "q".into())
        );
        assert_eq!(before[..2], after[..2]);
        println!("retarget_changes_only_quit ... ok");

        let m = menu(
            mtm,
            &[
                ("Hide X", Some(sel!(hide:)), "h"),
                ("Quit", Some(sel!(terminate:)), "w"),
            ],
        );
        let before = snapshot(&m);
        assert_eq!(retarget_quit(&m), Err(QuitMenuError::NotFound));
        assert_eq!(snapshot(&m), before);
        println!("missing_quit_is_an_error_and_mutates_nothing ... ok");

        let m = menu(
            mtm,
            &[
                ("Quit A", Some(sel!(terminate:)), "q"),
                ("Quit B", Some(sel!(terminate:)), "q"),
            ],
        );
        let before = snapshot(&m);
        assert_eq!(retarget_quit(&m), Err(QuitMenuError::Ambiguous));
        assert_eq!(snapshot(&m), before);
        println!("duplicate_quit_is_an_error_and_mutates_nothing ... ok");

        let m = menu(mtm, &[("Quit X", Some(sel!(terminate:)), "q")]);
        let quit = m
            .itemArray()
            .objectAtIndex(0)
            .submenu()
            .expect("submenu")
            .itemArray()
            .objectAtIndex(0);
        quit.setKeyEquivalentModifierMask(NSEventModifierFlags::Option);
        assert_eq!(retarget_quit(&m), Err(QuitMenuError::NotFound));
        assert_eq!(quit.action(), Some(sel!(terminate:)));
        println!("non_command_quit_is_not_matched ... ok");

        let empty = NSMenu::new(mtm);
        assert_eq!(retarget_quit(&empty), Err(QuitMenuError::NotFound));
        println!("empty_menu_is_not_found ... ok");

        for e in [
            QuitMenuError::NoMainMenu,
            QuitMenuError::NotFound,
            QuitMenuError::Ambiguous,
        ] {
            assert!(e.to_string().contains("#11"));
        }
        println!("errors_are_actionable ... ok");
    }
}

#[cfg(target_os = "macos")]
fn main() {
    mac::run();
}
