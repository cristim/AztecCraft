#![forbid(unsafe_code)]

mod app;
mod editor_operations;
#[cfg(feature = "mcp")]
mod mcp;
mod tool_icons;
mod viewport_renderer;
mod viewport_ui;

fn main() -> eframe::Result {
    let mcp = match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [] => false,
        [argument] if argument == "--mcp-stdio" && cfg!(feature = "mcp") => true,
        _ => {
            eprintln!("Usage: azteccraft [--mcp-stdio (requires the mcp Cargo feature)]");
            std::process::exit(2);
        }
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
            if mcp {
                editor.start_mcp(&cc.egui_ctx)?;
            }
            #[cfg(not(feature = "mcp"))]
            let _ = mcp;
            Ok(Box::new(editor))
        }),
    )
}
