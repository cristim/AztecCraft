//! The macOS terminate hook (#261) adds a method to winit's private delegate class, so its
//! shape is only known for one winit release.

const VERIFIED_WINIT: &str = "0.30.13";

#[test]
fn winit_stays_on_the_verified_version() {
    let lock = include_str!("../Cargo.lock");
    let versions: Vec<&str> = lock
        .split("[[package]]")
        .filter(|package| {
            package
                .lines()
                .any(|line| line.trim() == "name = \"winit\"")
        })
        .filter_map(|package| {
            package
                .lines()
                .find_map(|line| line.strip_prefix("version = \""))
                .and_then(|rest| rest.strip_suffix('"'))
        })
        .collect();
    assert_eq!(
        versions,
        [VERIFIED_WINIT],
        "winit changed; re-verify the WinitApplicationDelegate shape that \
         crates/macos-quit-menu/src/terminate.rs relies on (#261, known-issues.md) \
         before bumping VERIFIED_WINIT"
    );
}
