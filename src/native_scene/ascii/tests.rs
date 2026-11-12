use super::*;
use std::fs::OpenOptions;
use std::io::Read;

const CC: &str =
    "setAttr \".cc\" -type \"nurbsCurve\" 3 1 0 no 3 6 0 0 0 1 1 1 4 0 -0 0 1 2 3 4 -3 2 6 0 -1;";
const CREATE: &str =
    "createNode transform -n \"curve1\"; createNode nurbsCurve -n \"curveShape1\" -p \"curve1\";";

fn input(body: &str) -> Arc<[u8]> {
    Arc::from(format!("//Maya ASCII 2016 scene\nrequires maya \"2016\"; currentUnit -l centimeter -a degree -t film; {body}").into_bytes())
}

fn specimen() -> String {
    format!("{CREATE} setAttr -k off \".v\"; {CC}")
}

#[test]
fn declarative_curve_owns_exact_f64_knots_source_and_known_visibility() {
    let source = input(&specimen());
    let scene = decode(source.clone()).unwrap();
    assert_eq!(scene.nodes().len(), 1);
    assert!(scene.shapes().is_empty());
    assert_eq!(scene.curves().len(), 1);
    let curve = &scene.curves()[0];
    assert_eq!(curve.name(), "curveShape1");
    assert_eq!(
        curve.control_vertices(),
        [
            [0.0, -0.0, 0.0],
            [1.0, 2.0, 3.0],
            [4.0, -3.0, 2.0],
            [6.0, 0.0, -1.0]
        ]
    );
    assert_eq!(
        curve.control_vertices()[0][1].to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(curve.knots(), [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
    assert_eq!(curve.domain().unwrap(), [0.0, 1.0]);
    assert!(curve.view.visible().unwrap());
    assert_eq!(scene.units(), &Units::default());
    let retention = scene.source.as_ref().unwrap();
    assert!(Arc::ptr_eq(&source, &retention.bytes));
    assert!(retention.leaves.is_empty());
    assert!(retention.appearance.is_none());
    assert!(retention.records.contains(&curve.source.statement));
    assert_eq!(curve.source.owner, curve.id());
    assert!(
        retention
            .bytes
            .get(curve.source.statement.clone())
            .unwrap()
            .ends_with(CC.as_bytes())
    );
    scene.validate().unwrap();
    let mut scene = scene;
    assert_eq!(
        scene.set_channel(scene.nodes[0].id, super::super::Channel::TranslateX, 1.0),
        Err(NativeError::ReadOnlyFormat)
    );
    assert_eq!(
        super::super::binary::encode(&scene).unwrap_err().kind(),
        super::super::binary::ErrorKind::Unsupported
    );
}

#[test]
fn declared_hierarchy_channels_pivots_and_uuids_remain_canonical() {
    let body = format!(
        "createNode transform -n \"root\"; rename -uid \"00000000-0000-4000-8000-000000000001\"; setAttr \".t\" -type \"double3\" 1.000000000001 -0 3; createNode transform -n \"curve1\" -p \"root\"; setAttr \".r\" -type \"double3\" 13 21 34; setAttr \".rp\" -type \"double3\" 1 2 3; setAttr \".sp\" -type \"double3\" -1 -2 -3; setAttr \".v\" no; createNode nurbsCurve -n \"curveShape1\" -p \"curve1\"; rename -uid \"00000000-0000-4000-8000-000000000002\"; {CC}"
    );
    let scene = decode(input(&body)).unwrap();
    assert_eq!(
        scene.nodes()[0].transform().translation,
        [1.000000000001, -0.0, 3.0]
    );
    assert_eq!(
        scene.nodes()[0].transform().translation[1].to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(scene.nodes()[1].rotate_pivot(), [1.0, 2.0, 3.0]);
    assert_eq!(scene.nodes()[1].scale_pivot(), [-1.0, -2.0, -3.0]);
    assert!(!scene.nodes[1].view.visible().unwrap());
    assert_eq!(
        scene.nodes()[0].transform_uuid(),
        Some(Uuid::parse_str("00000000-0000-4000-8000-000000000001").unwrap())
    );
    assert_eq!(
        scene.curves()[0].uuid(),
        Some(Uuid::parse_str("00000000-0000-4000-8000-000000000002").unwrap())
    );
    assert_eq!(scene.dag_edges().len(), 3);
    assert_eq!(
        scene.dag_edges()[1].parent(),
        Some(scene.nodes()[0].transform_id())
    );
    assert_eq!(scene.dag_edges()[2].child(), scene.curves()[0].id());
    assert_eq!(scene.next_id, 3);
    assert_eq!(scene.next_native_id, 4);
}

#[test]
fn inert_metadata_has_exact_owners_and_never_creates_script_nodes() {
    let body = format!("{CREATE} {CC}
        createNode script -n \"uiConfigurationScriptNode\";
        setAttr \".b\" -type \"string\" (\"createNode mesh -n \\\"hidden\\\"; `call`; $x;\"+\" // ;\"); setAttr \".st\" 3;
        createNode script -n \"sceneConfigurationScriptNode\"; setAttr \".b\" -type \"string\" \"file -open \\\"bad\\\";\"; setAttr \".st\" 6;
        createNode lightLinker -s -n \"lightLinker1\"; setAttr -s 2 \".lnk\"; setAttr -s 2 \".slnk\";
        createNode displayLayerManager -n \"layerManager\"; createNode displayLayer -n \"defaultLayer\";
        createNode renderLayerManager -n \"renderLayerManager\"; createNode renderLayer -n \"defaultRenderLayer\"; setAttr \".g\" yes;
        select -ne :hardwareRenderingGlobals; setAttr \".otfna\" -type \"stringArray\" 2 \"a\" \"b\"; setAttr \".otfva\" -type \"Int32Array\" 2 -1 2; setAttr \".fprt\" yes;
        select -ne :initialShadingGroup; setAttr \".ro\" yes; select -ne :initialParticleSE; setAttr \".ro\" yes;
        select -ne :renderPartition; setAttr -s 2 \".st\"; select -ne :defaultShaderList1; setAttr -s 4 \".s\";
        select -ne :postProcessList1; setAttr -s 2 \".p\"; select -ne :ikSystem; setAttr -s 4 \".sol\";
        select -ne :time1; setAttr \".o\" 1; setAttr \".unw\" 1;
        select -ne :defaultRenderGlobals; setAttr \".outf\" 3; select -ne :defaultResolution; setAttr \".pa\" 1;
        select -ne :hardwareRenderGlobals; setAttr \".ctrs\" 256; setAttr \".btrs\" 512;
        select -ne :renderGlobalsList1; select -ne :defaultRenderingList1;
        relationship \"link\" \":lightLinker1\" \":initialShadingGroup.message\" \":defaultLightSet.message\";
        relationship \"shadowLink\" \":lightLinker1\" \":initialParticleSE.message\" \":defaultLightSet.message\";
        connectAttr \"layerManager.dli[0]\" \"defaultLayer.id\"; connectAttr \"renderLayerManager.rlmi[0]\" \"defaultRenderLayer.rlid\";
        connectAttr \"defaultRenderLayer.msg\" \":defaultRenderingList1.r\" -na;");
    let source = input(&body);
    let scene = decode(source.clone()).unwrap();
    assert_eq!((scene.nodes().len(), scene.curves().len()), (1, 1));
    assert!(Arc::ptr_eq(&source, &scene.source.as_ref().unwrap().bytes));
    assert!(!format!("{scene:?}").contains("hidden"));
}

#[test]
fn unknown_effects_nodes_owners_and_connections_refuse_the_whole_candidate() {
    for suffix in [
        "createNode mesh -n \"mesh\";",
        "createNode expression -n \"expression\";",
        "requires plugin \"1\";",
        "setAttr \".cr\" -type \"nurbsCurve\" 0;",
        "setAttr \".cp[0]\" -type \"double3\" 1 2 3;",
        "connectAttr \"driver.output\" \"curveShape1.cr\";",
        "connectAttr \"time1.outTime\" \"curve1.tx\";",
        "select -ne :bogus;",
        "select -ne :curve1;",
        "select -ne ::time1;",
        "parent \"curve1\" \"other\";",
        "createNode script -n \"arbitraryScript\";",
        "createNode camera -n \"userCamera\" -p \"curve1\";",
        "createNode lightLinker -s -n \"otherLinker\";",
        "createNode transform -n \"time1\";",
        "createNode displayLayer -n \"bogusLayer\";",
        "createNode transform -n \"curveShape1\";",
        "select -ne :defaultResolution; setAttr \".unknown\" 1;",
        "select -ne :initialShadingGroup; setAttr \".b\" -type \"string\" \"a\";",
        "relationship \"link\" \":lightLinker1\" \"curve1.message\" \":defaultLightSet.message\";",
        "connectAttr \"defaultRenderLayer.msg\" \"curve1.tx\" -na;",
    ] {
        let source = input(&format!("{} {suffix}", specimen()));
        let before = source.clone();
        let error = decode(source).unwrap_err();
        assert_eq!(
            error.kind(),
            ErrorKind::Unsupported,
            "synthetic case {suffix}"
        );
        assert!(error.offset().is_none_or(|offset| offset <= before.len()));
    }
    for effect in ["setAttr \".rotateOrder\" 1;", "setAttr \".tx\" 1;"] {
        assert_eq!(
            decode(input(&format!(
                "createNode transform -n \"ancestor\"; {effect} {CREATE} {CC}"
            )))
            .unwrap_err()
            .kind(),
            ErrorKind::Unsupported
        );
    }
}

#[test]
fn duplicate_values_and_missing_or_wrong_parents_never_use_a_fallback_owner() {
    for body in [
        format!("{CREATE} {CC} {CC}"),
        format!("{CREATE} {CC} setAttr \".v\" yes; setAttr \".v\" no;"),
        format!("{CREATE} {CC} setAttr -k off \".v\"; setAttr -k off \".v\";"),
        "createNode transform -n \"curve1\"; setAttr \".t\" -type \"double3\" 0 0 0; setAttr \".t\" -type \"double3\" 1 0 0;".to_owned(),
        "createNode transform -n \"curve1\"; rename -uid \"00000000-0000-4000-8000-000000000001\"; rename -uid \"00000000-0000-4000-8000-000000000002\";".into(),
        "createNode transform -n \"a\"; rename -uid \"00000000-0000-4000-8000-000000000001\"; createNode transform -n \"b\"; rename -uid \"00000000-0000-4000-8000-000000000001\";".into(),
        format!("createNode transform -n \"other\"; createNode nurbsCurve -n \"curve\" -p \"missing\"; {CC}"),
        "createNode transform -n \"other\"; createNode transform -n \"curve1\" -p \"missing\";".to_owned(),
        format!("createNode nurbsCurve -n \"curve\"; {CC}"),
        format!("{CREATE} {CC} createNode transform -n \"child\" -p \"curveShape1\";"),
        "select -ne :time1; rename -uid \"00000000-0000-4000-8000-000000000001\";".into(),
        format!("{CREATE} {CC} currentUnit -l centimeter -a degree -t film;"),
        format!("{CREATE} {CC} fileInfo \"k\" \"a\"; fileInfo \"k\" \"b\";"),
        format!("{CREATE} {CC} fileInfo \"a\\n\" \"a\"; fileInfo \"a\n\" \"b\";"),
    ] {
        assert_eq!(decode(input(&body)).unwrap_err().kind(), ErrorKind::Unsupported);
    }
    assert!(decode(input("setAttr \".v\" yes;")).is_err());
}

#[test]
fn interior_knot_multiplicity_is_unsupported_but_bad_knots_stay_malformed() {
    let kinked = "setAttr \".cc\" -type \"nurbsCurve\" 3 3 0 no 3 8 0 0 0 1 1 2 2 2 6 0 0 0 1 0 0 2 0 0 3 0 0 4 0 0 5 0 0;";
    let error = decode(input(&format!("{CREATE} setAttr -k off \".v\"; {kinked}"))).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Unsupported);
    for knots in ["0 0 0 1 0.5 1", "0 0 0 1 1 0.5"] {
        let body = CC.replacen("0 0 0 1 1 1", knots, 1);
        let error = decode(input(&format!("{CREATE} {body}"))).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Malformed);
    }
}

#[test]
fn every_curve_header_count_numeric_word_and_metadata_arity_is_consumed() {
    for (before, after, expected) in [
        ("3 1 0 no 3", "4 1 0 no 3", ErrorKind::Unsupported),
        ("3 1 0 no 3", "3 1 1 no 3", ErrorKind::Unsupported),
        ("3 1 0 no 3", "3 1 0 yes 3", ErrorKind::Unsupported),
        ("3 1 0 no 3", "3 1 0 no 2", ErrorKind::Unsupported),
        ("3 1 0 no 3", "3 2 0 no 3", ErrorKind::Malformed),
        ("6 0 0 0 1 1 1", "6 0 0 0 0 0 0", ErrorKind::Malformed),
        ("6 0 0 0 1 1 1", "6 0 0 0 1 -1 1", ErrorKind::Malformed),
        ("0 -0 0", "NaN -0 0", ErrorKind::Malformed),
        ("0 -0 0", "1e999 -0 0", ErrorKind::Malformed),
        ("4 0 -0 0", "5 0 -0 0", ErrorKind::Malformed),
    ] {
        let body = format!("{CREATE} {}", CC.replacen(before, after, 1));
        assert_eq!(decode(input(&body)).unwrap_err().kind(), expected);
    }
    for suffix in [
        "select -ne :hardwareRenderingGlobals; setAttr \".otfna\" -type \"stringArray\" 2 \"a\";",
        "select -ne :hardwareRenderingGlobals; setAttr \".otfva\" -type \"Int32Array\" 1 1 2;",
        "select -ne :time1; setAttr \".unw\" 2147483648;",
        "createNode transform -n \"extra\"; rename -uid \"not-a-UUID\";",
        "createNode script -n \"uiConfigurationScriptNode\"; setAttr \".b\" -type \"string\" \"a\" \"b\";",
        "select -ne :defaultResolution; setAttr \".pa\" 1 2;",
        "select -ne :defaultResolution; setAttr \".pa\" (\"1\");",
        "select -ne :time1; setAttr \".o\" -type \"double\" 1;",
    ] {
        assert!(decode(input(&format!("{} {suffix}", specimen()))).is_err());
    }
    let missing_units: Arc<[u8]> =
        Arc::from(format!("//Maya ASCII 2016 scene\n{}", specimen()).into_bytes());
    assert_eq!(
        decode(missing_units).unwrap_err().kind(),
        ErrorKind::Unsupported
    );
    let meter = input(&specimen());
    let meter: Arc<[u8]> = Arc::from(
        String::from_utf8(meter.to_vec())
            .unwrap()
            .replace("-l centimeter", "-l meter")
            .into_bytes(),
    );
    assert_eq!(decode(meter).unwrap_err().kind(), ErrorKind::Unsupported);
}

#[test]
fn owner_lookup_work_refuses_before_expensive_canonical_validation() {
    let mut body = String::new();
    for index in 0..5000 {
        body.push_str(&format!("createNode transform -n \"node{index}\";"));
    }
    super::super::FULL_VALIDATIONS.with(|count| count.set(0));
    let error = decode(input(&body)).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::ResourceLimit);
    assert_eq!(error.offset(), None);
    super::super::FULL_VALIDATIONS.with(|count| assert_eq!(count.get(), 0));
}

#[test]
fn canonical_validation_preflight_accepts_exact_budget_and_work_boundaries() {
    let scene = decode(input(&specimen())).unwrap();
    let records = scene.source.as_ref().unwrap().records.len();
    let scratch = 4096 + 256 * (3 + 3 * 2 + 2);
    let validation_work = 2 * 2 * 2 + records + 32 * (2 + 2 + records + 3 * 4 + 6 + 15);
    let mut budget = Budget::new(128 * 1024 * 1024 - scratch).unwrap();
    let mut work = Work::new();
    work.charge(50_000_000 - validation_work).unwrap();
    validation_admission(&scene, records, &mut budget, &mut work).unwrap();
    assert!(budget.charge(1).is_err());
    assert_eq!(work.charge(1).unwrap_err().kind(), ErrorKind::ResourceLimit);
    let mut budget = Budget::new(128 * 1024 * 1024 - scratch + 1).unwrap();
    assert_eq!(
        validation_admission(&scene, records, &mut budget, &mut Work::new())
            .unwrap_err()
            .kind(),
        ErrorKind::ResourceLimit
    );
    let mut work = Work::new();
    work.charge(50_000_000 - validation_work + 1).unwrap();
    assert_eq!(
        validation_admission(&scene, records, &mut Budget::new(0).unwrap(), &mut work)
            .unwrap_err()
            .kind(),
        ErrorKind::ResourceLimit
    );
    assert_eq!(
        mul(usize::MAX, 2).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
    assert_eq!(
        add(usize::MAX, 1).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
}

#[test]
#[ignore = "requires pinned genuine MA and external independent source-word audit"]
fn private_genuine_ma_decodes_exact_curves_and_preserves_every_source_byte() {
    fn regular(path: &std::ffi::OsStr, cap: u64) -> Vec<u8> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
        }
        let file = options
            .open(path)
            .expect("cannot open explicit regular audit input");
        assert!(file.metadata().unwrap().is_file());
        let mut bytes = Vec::new();
        file.take(cap + 1).read_to_end(&mut bytes).unwrap();
        assert!(bytes.len() as u64 <= cap);
        bytes
    }
    #[derive(serde::Deserialize)]
    struct Audit {
        knots: Vec<String>,
        cvs: Vec<[String; 3]>,
    }
    let path =
        std::env::var_os("AZTECCRAFT_MA_CURVE_FIXTURE").expect("explicit pinned MA path required");
    let audit = std::env::var_os("AZTECCRAFT_MA_CURVE_AUDIT")
        .expect("explicit independent source-word audit required");
    let bytes: Arc<[u8]> = Arc::from(regular(&path, 82_189));
    assert_eq!(bytes.len(), 82_189);
    let audit: Audit = serde_json::from_slice(&regular(&audit, 4096)).unwrap();
    let scene = decode(bytes.clone()).unwrap();
    assert_eq!(
        (scene.nodes.len(), scene.curves.len(), scene.shapes.len()),
        (5, 1, 0)
    );
    let curve = &scene.curves[0];
    assert_eq!((curve.control_vertices.len(), curve.knots.len()), (8, 10));
    assert_eq!(curve.domain().unwrap(), [0.0, 5.0]);
    assert_eq!(audit.knots.len(), curve.knots.len());
    assert_eq!(audit.cvs.len(), curve.control_vertices.len());
    for (index, (actual, expected)) in curve.knots.iter().zip(audit.knots).enumerate() {
        assert!(
            actual.to_bits() == expected.parse::<f64>().unwrap().to_bits(),
            "knot ordinal {index}"
        );
    }
    for (index, (actual, expected)) in curve.control_vertices.iter().zip(audit.cvs).enumerate() {
        for component in 0..3 {
            assert!(
                actual[component].to_bits()
                    == expected[component].parse::<f64>().unwrap().to_bits(),
                "CV ordinal {index}, component {component}"
            );
        }
    }
    assert!(curve.view.visible().unwrap());
    assert_eq!(scene.units, Units::default());
    let source = scene.source.as_ref().unwrap();
    assert!(Arc::ptr_eq(&source.bytes, &bytes));
    assert!(source.bytes.as_ref() == bytes.as_ref());
    assert!(source.leaves.is_empty());
    source.validate().unwrap();
    assert!(source.records.contains(&curve.source.statement));
    assert_eq!(
        scene
            .nodes
            .iter()
            .filter(|node| !node.view.visible().unwrap())
            .count(),
        4
    );
}

#[test]
fn ascii_reports_local_visibility_for_transform_and_curve_shape() {
    let scene = decode(input(&specimen())).unwrap();
    assert_eq!(scene.nodes()[0].local_visibility(), Some(true));
    assert_eq!(scene.curves()[0].local_visibility(), Some(true));
    let scene = decode(input(&format!(
        "createNode transform -n \"curve1\"; setAttr \".v\" no; createNode nurbsCurve -n \"curveShape1\" -p \"curve1\"; {CC}"
    )))
    .unwrap();
    assert_eq!(scene.nodes()[0].local_visibility(), Some(false));
    assert_eq!(scene.curves()[0].local_visibility(), Some(true));
    let scene = decode(input(&format!("{CREATE} setAttr \".v\" no; {CC}"))).unwrap();
    assert_eq!(scene.nodes()[0].local_visibility(), Some(true));
    assert_eq!(scene.curves()[0].local_visibility(), Some(false));
}

#[test]
fn transform_attributes_outside_the_modeled_channels_are_refused_at_open() {
    for attribute in ["setAttr \".tx\" 1;", "setAttr \".inheritsTransform\" no;"] {
        let body = format!(
            "createNode transform -n \"curve1\"; {attribute} createNode nurbsCurve -n \"curveShape1\" -p \"curve1\"; {CC}"
        );
        assert_eq!(
            decode(input(&body)).unwrap_err().kind(),
            ErrorKind::Unsupported,
            "{attribute}"
        );
    }
}

fn transform_extras(attrs: &str) -> Result<crate::native_scene::NativeScene, AsciiError> {
    decode(input(&format!(
        "createNode transform -n \"ancestor\"; {attrs} {CREATE} {CC}"
    )))
}

#[test]
fn rotate_order_axis_shear_and_pivot_translations_are_read_into_extras() {
    let scene = transform_extras(
        "setAttr \".ro\" 4; setAttr \".ra\" -type \"double3\" 10 20 -30; \
         setAttr \".sh\" -type \"double3\" 0.5 0 0.25; \
         setAttr \".rpt\" -type \"double3\" 1 2 3; setAttr \".spt\" -type \"double3\" -1 0 2;",
    )
    .unwrap();
    let extras = scene.nodes()[0].extras();
    assert_eq!(extras.rotate_order, Some(4));
    assert_eq!(extras.rotate_axis, Some([10.0, 20.0, -30.0]));
    assert_eq!(extras.shear, Some([0.5, 0.0, 0.25]));
    assert_eq!(extras.rotate_pivot_translate, Some([1.0, 2.0, 3.0]));
    assert_eq!(extras.scale_pivot_translate, Some([-1.0, 0.0, 2.0]));
    let absent = transform_extras("").unwrap();
    assert_eq!(absent.nodes()[0].extras(), &Default::default());
    let internal = scene.internal_transform(scene.nodes()[0].id()).unwrap();
    assert!(internal.extras.is_some());
}

#[test]
fn extras_follow_scene_units_when_the_scene_units_differ() {
    let mut scene = transform_extras(
        "setAttr \".ra\" -type \"double3\" 1 0 0; setAttr \".rpt\" -type \"double3\" 1 0 0; \
         setAttr \".sh\" -type \"double3\" 0.5 0 0;",
    )
    .unwrap();
    scene.units.angular = crate::native_scene::AngularUnit::Radian;
    scene.units.linear = crate::native_scene::LinearUnit::Meter;
    let id = scene.nodes()[0].id();
    let e = scene.internal_transform(id).unwrap().extras.unwrap();
    assert_eq!(e.rotate_axis_radians, [1.0, 0.0, 0.0]);
    assert_eq!(e.rotate_pivot_translate_cm, [100.0, 0.0, 0.0]);
    assert_eq!(e.shear, [0.5, 0.0, 0.0]);
}

#[test]
fn malformed_or_unsupported_extras_are_typed_refusals() {
    let malformed = [
        "setAttr \".ro\" 6;",
        "setAttr \".ro\" -1;",
        "setAttr \".ro\" 1.5;",
        "setAttr \".ro\" x;",
        "setAttr \".sh\" -type \"double3\" 1 0;",
        "setAttr \".sh\" -type \"double3\" nan 0 0;",
        "setAttr \".ra\" -type \"double3\" inf 0 0;",
        "setAttr \".rpt\" -type \"double3\" 1 0 abc;",
    ];
    for attrs in malformed {
        assert_eq!(
            transform_extras(attrs).unwrap_err().kind(),
            ErrorKind::Malformed,
            "{attrs}"
        );
    }
    let unsupported = [
        "setAttr \".ro\" 1 2;",
        "setAttr \".spt\" -type \"double3\" 1 0 0 0;",
        "setAttr \".sh\" 1 0 0;",
        "setAttr \".sh\" -type \"float3\" 1 0 0;",
        "setAttr \".ro\" -type \"double3\" 1;",
        "setAttr \".ro\" 1; setAttr \".ro\" 2;",
        "setAttr \".ra\" -type \"double3\" 1 0 0; setAttr \".ra\" -type \"double3\" 1 0 0;",
        "setAttr \".sh\" -type \"double3\" 1 0 0; setAttr \".sh\" -type \"double3\" 0 0 0;",
        "setAttr \".rpt\" -type \"double3\" 1 0 0; setAttr \".rpt\" -type \"double3\" 1 0 0;",
        "setAttr \".spt\" -type \"double3\" 1 0 0; setAttr \".spt\" -type \"double3\" 1 0 0;",
    ];
    for attrs in unsupported {
        assert_eq!(
            transform_extras(attrs).unwrap_err().kind(),
            ErrorKind::Unsupported,
            "{attrs}"
        );
    }
}

#[test]
fn session_material_edits_are_read_only_on_ascii_scenes() {
    use crate::native_scene::{MaterialChoice, SessionMaterialId};
    let mut scene = decode(input(&specimen())).unwrap();
    let before = scene.clone();
    let shape = NativeId::from_decimal("1").unwrap();
    let zero = SessionMaterialId::from_index(0);
    let errors = [
        scene.create_session_material([0.5; 3]).map(drop),
        scene
            .assign_session_material(shape, MaterialChoice::Existing(zero))
            .map(drop),
        scene.set_session_material_color(zero, [0.5; 3]),
    ];
    for error in errors {
        assert_eq!(error, Err(NativeError::ReadOnlyFormat));
    }
    assert_eq!(scene, before);
}

const MESH_UID: &str = "rename -uid \"aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa\";";

fn mesh_scene(body: &str) -> Result<NativeScene, AsciiError> {
    decode_admitting_meshes(input(body))
}

#[test]
fn mesh_declaration_binds_a_shape_under_its_transform_and_keeps_ids_unique() {
    let scene = mesh_scene(&format!(
        "{} createNode transform -n \"box\"; createNode mesh -n \"boxShape\" -p \"box\"; {MESH_UID} \
         createNode transform -n \"second\"; createNode mesh -n \"secondShape\" -p \"second\";",
        specimen()
    ))
    .unwrap();
    scene.validate().unwrap();
    assert_eq!(scene.shapes().len(), 2);
    let shape = &scene.shapes()[0];
    assert_eq!(shape.name(), "boxShape");
    assert_eq!(
        shape.uuid(),
        Some(Uuid::from_u128(0xaaaa_aaaa_aaaa_aaaa_aaaa_aaaa_aaaa_aaaa))
    );
    assert!(shape.mesh().faces().is_empty());
    let box_node = scene.nodes.iter().find(|n| n.name() == "box").unwrap();
    assert!(
        scene
            .dag
            .iter()
            .any(|e| e.parent == Some(box_node.transform_id) && e.child == shape.id)
    );
    let mut ids: Vec<_> = scene
        .nodes
        .iter()
        .map(|n| n.transform_id)
        .chain(scene.curves.iter().map(|c| c.id))
        .chain(scene.shapes.iter().map(|s| s.id))
        .collect();
    let count = ids.len();
    ids.sort_by_key(|id| id.0);
    ids.dedup();
    assert_eq!(ids.len(), count);
}

#[test]
fn mesh_declaration_refuses_unowned_shared_duplicate_and_misparented_shapes() {
    let transform = "createNode transform -n \"a\";";
    let curve = "createNode nurbsCurve -n \"c\" -p \"a\";";
    let mesh = "createNode mesh -n \"m\" -p \"a\";";
    // Each body ends with the statement that must be refused.
    for (body, refused) in [
        // no parent, shared, a second shape under one transform, a non-transform parent
        (
            "createNode mesh -n \"aShape\";".to_string(),
            "createNode mesh -n \"aShape\";",
        ),
        (
            format!("{transform} createNode mesh -s -n \"aShape\" -p \"a\";"),
            "createNode mesh -s -n \"aShape\" -p \"a\";",
        ),
        (
            format!(
                "{transform} createNode mesh -n \"s1\" -p \"a\"; createNode mesh -n \"s2\" -p \"a\";"
            ),
            "createNode mesh -n \"s2\" -p \"a\";",
        ),
        (
            format!(
                "{transform} createNode mesh -n \"s1\" -p \"a\"; createNode mesh -n \"s2\" -p \"s1\";"
            ),
            "createNode mesh -n \"s2\" -p \"s1\";",
        ),
        (
            format!("{CREATE} createNode mesh -n \"s\" -p \"curveShape1\";"),
            "createNode mesh -n \"s\" -p \"curveShape1\";",
        ),
        // a transform may hold one shape: a curve then a mesh, and a mesh then a curve
        (format!("{transform} {curve} {mesh}"), mesh),
        (format!("{transform} {mesh} {curve}"), curve),
        // an unknown parent, a name already taken and a built-in camera transform as parent
        (
            "createNode mesh -n \"s\" -p \"missing\";".to_string(),
            "createNode mesh -n \"s\" -p \"missing\";",
        ),
        (
            format!("{transform} createNode mesh -n \"a\" -p \"a\";"),
            "createNode mesh -n \"a\" -p \"a\";",
        ),
        (
            "createNode transform -s -n \"persp\"; createNode mesh -n \"s\" -p \"persp\";"
                .to_string(),
            "createNode mesh -n \"s\" -p \"persp\";",
        ),
        // a mesh and a transform may not share a uuid
        (
            format!(
                "{transform} {MESH_UID} createNode transform -n \"b\"; createNode mesh -n \"bs\" -p \"b\"; {MESH_UID}"
            ),
            MESH_UID,
        ),
    ] {
        let error = mesh_scene(&body)
            .err()
            .unwrap_or_else(|| panic!("accepted: {body}"));
        assert_eq!(error.kind(), ErrorKind::Unsupported, "{body}");
        let text = String::from_utf8(input(&body).to_vec()).unwrap();
        let start = text.rfind(refused).unwrap();
        let offset = error
            .offset()
            .unwrap_or_else(|| panic!("no offset: {body}"));
        assert!(
            (start..=start + refused.len()).contains(&offset),
            "offset {offset} outside `{refused}` ({start}): {body}"
        );
    }
}

#[test]
fn public_decode_still_refuses_mesh_declarations() {
    let source = input("createNode transform -n \"a\"; createNode mesh -n \"aShape\" -p \"a\";");
    assert_eq!(decode(source).unwrap_err().kind(), ErrorKind::Unsupported);
}

#[test]
fn several_curves_may_still_share_one_transform() {
    let source = input(&format!(
        "createNode transform -n \"a\"; createNode nurbsCurve -n \"c1\" -p \"a\"; {CC} \
         createNode nurbsCurve -n \"c2\" -p \"a\"; {CC}"
    ));
    assert_eq!(decode(source).unwrap().curves().len(), 2);
}

#[test]
fn inherits_transform_and_offset_parent_matrix_are_read_into_extras() {
    let scene = transform_extras(
        "setAttr \".it\" no; setAttr \".opm\" -type \"matrix\" 1 0 0 0 0 1 0 0 0 0 1 0 2 3 4 1;",
    )
    .unwrap();
    let extras = scene.nodes()[0].extras();
    assert_eq!(extras.inherits_transform, Some(false));
    assert_eq!(
        extras.offset_parent_matrix,
        Some([
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 2.0, 3.0, 4.0, 1.0
        ])
    );
    let internal = scene.internal_transform(scene.nodes()[0].id()).unwrap();
    assert!(internal.extras.is_some_and(|e| !e.inherits_transform));
    // Explicit defaults are stated by the source but compose as the absent default.
    let stated = transform_extras(
        "setAttr \".it\" yes; setAttr \".opm\" -type \"matrix\" 1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1;",
    )
    .unwrap();
    assert_eq!(stated.nodes()[0].extras().inherits_transform, Some(true));
    assert!(stated.nodes()[0].extras().is_default());
    assert!(
        stated
            .internal_transform(stated.nodes()[0].id())
            .unwrap()
            .extras
            .is_none()
    );
}

#[test]
fn malformed_or_unsupported_it_and_opm_are_typed_refusals() {
    for attrs in [
        "setAttr \".it\" maybe;",
        "setAttr \".opm\" -type \"matrix\" 1 0 0 0 0 1 0 0 0 0 1 0 0 0 0;",
        "setAttr \".opm\" -type \"matrix\" 1 0 0 0 0 1 0 0 0 0 1 0 0 0 nan 1;",
    ] {
        assert_eq!(
            transform_extras(attrs).unwrap_err().kind(),
            ErrorKind::Malformed,
            "{attrs}"
        );
    }
    for attrs in [
        "setAttr \".it\" no; setAttr \".it\" no;",
        "setAttr \".opm\" -type \"matrix\" 1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1; setAttr \".opm\" -type \"matrix\" 1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1;",
        "setAttr \".opm\" 1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1;",
        "setAttr \".opm\" -type \"double3\" 1 0 0;",
        "setAttr \".opm\" -type \"matrix\" 1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1 1;",
        "connectAttr \"defaultRenderLayer.msg\" \"ancestor.it\";",
        "connectAttr \"defaultRenderLayer.msg\" \"ancestor.opm\";",
    ] {
        assert_eq!(
            transform_extras(attrs).unwrap_err().kind(),
            ErrorKind::Unsupported,
            "{attrs}"
        );
    }
    // A projective offset parses but is refused by validation, never silently dropped to affine.
    assert!(
        transform_extras("setAttr \".opm\" -type \"matrix\" 1 0 0 0.5 0 1 0 0 0 0 1 0 0 0 0 1;")
            .is_err()
    );
}

#[test]
fn it_and_opm_on_a_transform_that_parents_a_curve_do_not_collide_with_shape_bits() {
    let opm = "setAttr \".opm\" -type \"matrix\" 1 0 0 0 0 1 0 0 0 0 1 0 5 0 0 1;";
    for attrs in [
        "setAttr \".it\" no;",
        opm,
        &format!("setAttr \".it\" no; {opm}"),
    ] {
        let body = format!(
            "createNode transform -n \"curve1\"; {attrs} createNode nurbsCurve -n \"curveShape1\" -p \"curve1\"; {CC}"
        );
        let scene = decode(input(&body)).unwrap_or_else(|e| panic!("{attrs}: {e:?}"));
        assert_eq!(scene.curves().len(), 1, "{attrs}");
        assert!(!scene.nodes()[0].extras().is_default(), "{attrs}");
    }
}

#[test]
fn a_projective_offset_parent_matrix_fails_validation_with_its_own_error() {
    let error =
        transform_extras("setAttr \".opm\" -type \"matrix\" 1 0 0 0.5 0 1 0 0 0 0 1 0 0 0 0 1;")
            .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Malformed);
}
