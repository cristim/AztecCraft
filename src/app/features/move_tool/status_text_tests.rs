//! The component Move status names its vertex count in the singular and the plural.
use super::vertex_count_text;

#[test]
fn the_vertex_count_reads_naturally_for_one_and_many() {
    assert_eq!(vertex_count_text(1), "1 vertex");
    assert_eq!(vertex_count_text(2), "2 vertices");
    assert_eq!(vertex_count_text(8), "8 vertices");
}
