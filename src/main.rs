#![forbid(unsafe_code)]

mod app;
mod editor_operations;
#[cfg(feature = "mcp")]
mod mcp;
mod tool_icons;
mod viewport_renderer;
mod viewport_ui;

const USAGE: &str = "Usage: azteccraft [--mcp-stdio [--recovery-dir ABSOLUTE_LOCAL_PATH]] (both flags require the mcp Cargo feature)";

/// Opt-in flags only; anything else is a usage error. `--recovery-dir` names where recovery
/// files may be written (off without it), must be an absolute path on a local disk and is only
/// reachable through the MCP session, so it requires `--mcp-stdio`.
fn parse_args(
    args: impl IntoIterator<Item = std::ffi::OsString>,
) -> Result<(bool, Option<std::path::PathBuf>), ()> {
    let (mut mcp, mut recovery_dir) = (false, None);
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--mcp-stdio") if cfg!(feature = "mcp") && !mcp => mcp = true,
            Some("--recovery-dir") if cfg!(feature = "mcp") && recovery_dir.is_none() => {
                match args.next().map(std::path::PathBuf::from) {
                    Some(path) if path.is_absolute() => recovery_dir = Some(path),
                    _ => return Err(()),
                }
            }
            _ => return Err(()),
        }
    }
    if recovery_dir.is_some() && !mcp {
        return Err(());
    }
    Ok((mcp, recovery_dir))
}

fn main() -> eframe::Result {
    let Ok((mcp, recovery_dir)) = parse_args(std::env::args_os().skip(1)) else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    eframe::run_native(
        "AztecCraft | Untitled",
        eframe::NativeOptions {
            depth_buffer: 32,
            stencil_buffer: 0,
            multisampling: 0,
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([1440.0, 900.0])
                .with_min_inner_size([960.0, 640.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            // Command-Q, the Quit item, Dock Quit and logout all send `terminate:`; the hook
            // routes them through the close guard (#261). The call prints its own error; `?`
            // aborts the launch if winit's delegate shape changed.
            #[cfg(target_os = "macos")]
            macos_quit_menu::install_terminate_guard()?;
            let mut editor = app::Editor::new(cc)?;
            editor.install_shutdown_signal(&cc.egui_ctx)?;
            #[cfg(feature = "mcp")]
            {
                if let Some(dir) = recovery_dir {
                    editor.enable_recovery(dir)?;
                }
                if mcp {
                    editor.start_mcp(&cc.egui_ctx)?;
                }
            }
            #[cfg(not(feature = "mcp"))]
            let _ = (mcp, recovery_dir);
            Ok(Box::new(editor))
        }),
    )
}

#[cfg(all(test, feature = "mcp"))]
mod tests {
    use super::parse_args;
    use std::ffi::OsString;

    fn parse(args: &[&str]) -> Result<(bool, Option<std::path::PathBuf>), ()> {
        parse_args(args.iter().map(OsString::from))
    }

    #[test]
    fn flags_are_opt_in_and_strict() {
        assert_eq!(parse(&[]), Ok((false, None)));
        assert_eq!(parse(&["--mcp-stdio"]), Ok((true, None)));
        #[cfg(unix)]
        {
            let both = parse(&["--mcp-stdio", "--recovery-dir", "/var/recovery"]);
            assert_eq!(both, Ok((true, Some("/var/recovery".into()))));
            let reversed = parse(&["--recovery-dir", "/var/recovery", "--mcp-stdio"]);
            assert_eq!(reversed, both);
        }
        for bad in [
            &["--recovery-dir", "/var/recovery"][..],
            &["--mcp-stdio", "--recovery-dir"],
            &["--mcp-stdio", "--recovery-dir", "relative"],
            &["--mcp-stdio", "--recovery-dir", ""],
            &["--mcp-stdio", "--recovery-dir", "--mcp-stdio"],
            &["--mcp-stdio", "--mcp-stdio"],
            &["--recovery-dir=/x", "--mcp-stdio"],
            &["extra"],
        ] {
            assert_eq!(parse(bad), Err(()), "{bad:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_non_utf8_argument_is_a_usage_error_not_a_panic() {
        use std::os::unix::ffi::OsStringExt;
        assert_eq!(parse_args([OsString::from_vec(vec![0xff, 0xfe])]), Err(()));
    }
}
