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
            // winit installs Quit as `terminate:` before this runs; route it through the
            // window close guard so Command-Q cannot discard a dirty scene (#11).
            #[cfg(target_os = "macos")]
            macos_quit_menu::route_quit_to_close()?;
            let editor = app::Editor::new(cc)?;
            #[cfg(feature = "mcp")]
            let mut editor = editor;
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
