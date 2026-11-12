//! Declared `currentUnit` admission (#17). Provisional until compared with the pinned Maya reference.
use super::*;
use crate::native_scene::{AngularUnit, LinearUnit};

const CC: &str =
    "setAttr \".cc\" -type \"nurbsCurve\" 3 1 0 no 3 6 0 0 0 1 1 1 4 0 -0 0 1 2 3 4 -3 2 6 0 -1;";
const NODE: &str = "createNode transform -n \"t1\"; setAttr \".t\" -type \"double3\" 1 2 3; \
     setAttr \".r\" -type \"double3\" 1.5707963267948966 0 0; \
     setAttr \".rp\" -type \"double3\" 0.5 0 0;";
const CURVE: &str = "createNode nurbsCurve -n \"cShape\" -p \"t1\";";

fn scene(units: &str, body: &str) -> Result<crate::native_scene::NativeScene, AsciiError> {
    decode(Arc::from(
        format!("//Maya ASCII 2016 scene\nrequires maya \"2016\"; currentUnit {units}; {body}")
            .into_bytes(),
    ))
}

#[test]
fn every_declared_linear_unit_scales_transform_values_to_centimeters() {
    for (unit, factor, kind) in [
        ("millimeter", 0.1, LinearUnit::Millimeter),
        ("centimeter", 1.0, LinearUnit::Centimeter),
        ("meter", 100.0, LinearUnit::Meter),
        ("inch", 2.54, LinearUnit::Inch),
        ("foot", 30.48, LinearUnit::Foot),
    ] {
        let scene = scene(&format!("-l {unit} -a degree -t film"), NODE).unwrap();
        assert_eq!(scene.units().linear, kind);
        let t = scene.internal_transform(scene.nodes()[0].id()).unwrap();
        assert_eq!(t.translation_cm, [factor, 2.0 * factor, 3.0 * factor]);
        assert_eq!(t.rotate_pivot_cm[0], 0.5 * factor);
        // Declared channels keep their declared values.
        assert_eq!(scene.nodes()[0].transform().translation, [1.0, 2.0, 3.0]);
    }
}

#[test]
fn radian_angular_unit_is_read_as_radians_and_time_ntsc_is_kept() {
    let scene = scene("-l centimeter -a radian -t ntsc", NODE).unwrap();
    assert_eq!(scene.units().angular, AngularUnit::Radian);
    assert_eq!(scene.units().time, "ntsc");
    let t = scene.internal_transform(scene.nodes()[0].id()).unwrap();
    assert_eq!(t.rotation_radians[0], std::f64::consts::FRAC_PI_2);
}

#[test]
fn radian_and_ntsc_are_admitted_with_curves_in_centimeters() {
    let scene = scene(
        "-l centimeter -a radian -t ntsc",
        &format!("{NODE} {CURVE} {CC}"),
    )
    .unwrap();
    assert_eq!(scene.curves().len(), 1);
    assert_eq!(scene.curves()[0].control_vertices()[1], [1.0, 2.0, 3.0]);
}

#[test]
fn non_centimeter_linear_unit_refuses_curves_instead_of_misscaling() {
    for unit in ["millimeter", "meter", "inch", "foot"] {
        let header = format!("-l {unit} -a degree -t film");
        let error = scene(&header, &format!("{NODE} {CURVE} {CC}")).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Unsupported, "{unit}");
    }
}

#[test]
fn unlisted_duplicate_missing_or_extra_unit_flags_refuse() {
    for header in [
        "-l kilometer -a degree -t film",
        "-l yard -a degree -t film",
        "-l mile -a degree -t film",
        "-l centimeter -a minute -t film",
        "-l centimeter -a degree -t pal",
        "-l centimeter -a degree -t sec",
        "-l centimeter -a degree",
        "-l centimeter -t film",
        "-l meter -l meter -a degree -t film",
        "-l centimeter -a degree -a radian -t film",
        "-l centimeter -a degree -t film -t film",
        "-l centimeter -a degree -t film -ua",
        "-l Meter -a degree -t film",
        "-a degree -t film -l",
        "",
    ] {
        let error = scene(header, NODE).unwrap_err();
        assert!(
            matches!(error.kind(), ErrorKind::Unsupported | ErrorKind::Malformed),
            "{header}: {:?}",
            error.kind()
        );
    }
    let twice =
        "currentUnit -l centimeter -a degree -t film; currentUnit -l centimeter -a degree -t film;";
    let twice = decode(Arc::from(
        format!("//Maya ASCII 2016 scene\nrequires maya \"2016\"; {twice} {NODE}").into_bytes(),
    ));
    assert_eq!(twice.unwrap_err().kind(), ErrorKind::Unsupported);
}

#[test]
fn transform_values_before_any_units_declaration_still_refuse() {
    let source = format!(
        "//Maya ASCII 2016 scene\nrequires maya \"2016\"; {NODE} currentUnit -l meter -a degree -t film;"
    );
    let error = decode(Arc::from(source.into_bytes())).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Unsupported);
}
