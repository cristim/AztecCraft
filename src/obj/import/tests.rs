use super::*;

#[test]
fn limits_are_pinned_to_their_provisional_values() {
    assert_eq!(MAX_FILE_BYTES, 32 * 1024 * 1024);
    assert_eq!(MAX_LINE_BYTES, 64 * 1024);
    assert_eq!(MAX_VERTICES, 262_144);
    assert_eq!(MAX_FACES, 262_144);
    assert_eq!(MAX_CORNERS, 1_048_576);
    assert_eq!(MAX_CORNERS_PER_FACE, 1024);
    assert_eq!(MAX_OBJECTS, 1024);
    assert_eq!(MAX_NAME_BYTES, 256);
    assert_eq!(MAX_COORD, 1e12);
}
