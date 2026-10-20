use wire::{chunk, creation, group, mesh_payload_with_uv, scene, triple};

pub(crate) fn appearance_specimen() -> Vec<u8> {
    appearance_specimen_with_users(1)
}

pub(crate) fn appearance_specimen_with_connections(extra: &[u8]) -> Vec<u8> {
    appearance_specimen_with_fields(Fields {
        connections: extra,
        ..Default::default()
    })
}

#[derive(Default)]
pub(crate) struct Fields<'a> {
    pub shape: &'a [u8],
    pub surface: &'a [u8],
    pub shader: &'a [u8],
    pub file: &'a [u8],
    pub color_management: &'a [u8],
    pub connections: &'a [u8],
    pub groups: &'a [u8],
    /// The mesh carries an empty UV set (no points, no ids).
    pub empty_mesh_uv: bool,
}

fn appearance_shape(name: &[u8], parent: &[u8], extra: &[u8], empty_mesh_uv: bool) -> Vec<u8> {
    let attribute = |tag: &[u8; 4], key: &[u8], kind: u8, value: &[u8]| {
        chunk(tag, &[key, &[0, kind], value].concat(), 8)
    };
    let string =
        |key: &[u8], kind, value: &[u8]| attribute(b"STR ", key, kind, &[value, &[0]].concat());
    let mut uv = Vec::new();
    let uv_words = [
        0,
        0,
        1f32.to_bits(),
        0,
        0,
        1f32.to_bits(),
        0.5f32.to_bits(),
        0.5f32.to_bits(),
    ];
    for word in uv_words {
        uv.extend(word.to_be_bytes());
    }
    let mut component = Vec::new();
    for word in [1, u32::from_be_bytes(*b"CMDF"), 1, 0, 1] {
        component.extend(word.to_be_bytes());
    }
    group(
        b"DMSH",
        &[
            creation(name, Some(parent)),
            attribute(b"CMP#", b"iog[0].og[0].gcl", 0x20, &component),
            attribute(b"FLGS", b"uvst[0].uvsp", 0x28, &4u32.to_be_bytes()),
            attribute(b"FLT2", b"uvst[0].uvsp[0:3]", 0x20, &uv),
            string(b"uvst[0].uvsn", 0x24, b"uv"),
            string(b"cuvs", 0x20, b"uv"),
            chunk(
                b"MESH",
                &if empty_mesh_uv {
                    mesh_payload_with_uv(&[0, 1, 0x6000_0002, 0, 1, 0x6000_0002], &[], &[])
                } else {
                    mesh_payload_with_uv(
                        &[0, 1, 0x6000_0002, 0, 1, 0x6000_0002],
                        &uv_words,
                        &[0, 1, 2, 3, 1, 2],
                    )
                },
                8,
            ),
            extra.to_vec(),
        ]
        .concat(),
        4,
    )
}

pub(crate) fn appearance_specimen_with_fields(fields: Fields<'_>) -> Vec<u8> {
    let attribute = |tag: &[u8; 4], key: &[u8], kind: u8, value: &[u8]| {
        chunk(tag, &[key, &[0, kind], value].concat(), 8)
    };
    let string =
        |key: &[u8], kind, value: &[u8]| attribute(b"STR ", key, kind, &[value, &[0]].concat());
    let connection =
        |mode, from: &[u8], to: &[u8]| chunk(b"CWFL", &[&[mode], from, &[0], to, &[0]].concat(), 8);
    let groups = [
        group(
            b"XFRM",
            &[creation(b"object", None), triple(b"t", [1.0, 2.0, 3.0])].concat(),
            4,
        ),
        appearance_shape(b"shape", b"object", fields.shape, fields.empty_mesh_uv),
        group(b"GPID", &creation(b"identity", None), 4),
        group(b"GPID", &creation(b"othergid", None), 4),
        group(
            b"SHAD",
            &[creation(b"surface", None), fields.surface.to_vec()].concat(),
            4,
        ),
        group(
            b"RPHO",
            &[
                creation(b"shader", None),
                attribute(b"DBLE", b"dc", 0x24, &0.8f64.to_be_bytes()),
                attribute(b"FLT3", b"ambc", 0x20, &[0; 12]),
                attribute(b"FLT3", b"sc", 0x20, &[0; 12]),
                attribute(b"DBLE", b"rfl", 0x20, &0.5f64.to_be_bytes()),
                fields.shader.to_vec(),
            ]
            .concat(),
            4,
        ),
        group(
            b"RTFT",
            &[
                creation(b"texture", None),
                string(b"ftn", 0x20, b"test.png"),
                string(b"cs", 0x20, b"sRGB"),
                fields.file.to_vec(),
            ]
            .concat(),
            4,
        ),
        group(b"RPL2", &creation(b"placement", None), 4),
        group(
            b"SLCT",
            &[
                chunk(b"SLCT", b":defaultColorMgtGlobals", 8),
                attribute(b"DBLE", b"cfe", 0x20, &1f64.to_be_bytes()),
                string(b"cfp", 0x20, b"test-config"),
                string(b"wsn", 0x20, b"test-space"),
                fields.color_management.to_vec(),
            ]
            .concat(),
            4,
        ),
    ]
    .concat();
    let mut links = [
        connection(1, b"shape.iog.og[0]", b"surface.dsm"),
        connection(0, b"identity.id", b"shape.iog.og[0].gid"),
        connection(1, b"identity.msg", b"surface.gn"),
        connection(0, b"surface.mwc", b"shape.iog.og[0].gco"),
        connection(0, b"shader.oc", b"surface.ss"),
        connection(0, b"texture.oc", b"shader.c"),
        connection(1, b"surface.pa", b":partition.st"),
        connection(1, b"shader.msg", b":materials.s"),
    ]
    .concat();
    for (source, destination) in [
        ("o", "uv"),
        ("ofu", "ofu"),
        ("ofv", "ofv"),
        ("rf", "rf"),
        ("reu", "reu"),
        ("rev", "rev"),
        ("vt1", "vt1"),
        ("vt2", "vt2"),
        ("vt3", "vt3"),
        ("vc1", "vc1"),
        ("ofs", "fs"),
    ] {
        links.extend(connection(
            0,
            format!("placement.{source}").as_bytes(),
            format!("texture.{destination}").as_bytes(),
        ));
    }
    for (source, destination) in [
        ("cme", "cme"),
        ("cfe", "cmcf"),
        ("cfp", "cmcp"),
        ("wsn", "ws"),
    ] {
        links.extend(connection(
            0,
            format!(":defaultColorMgtGlobals.{source}").as_bytes(),
            format!("texture.{destination}").as_bytes(),
        ));
    }
    links.extend(fields.connections);
    scene(
        &[
            groups,
            fields.groups.to_vec(),
            group(b"CONS", &group(b"CONN", &links, 4), 4),
        ]
        .concat(),
    )
    .to_vec()
}

pub(crate) fn appearance_specimen_with_users(users: usize) -> Vec<u8> {
    assert!((1..=3).contains(&users));
    if users == 1 {
        return appearance_specimen_with_connections(&[]);
    }
    let mut groups = Vec::new();
    let mut links = Vec::new();
    let attribute = |tag: &[u8; 4], key: &[u8], kind: u8, value: &[u8]| {
        chunk(tag, &[key, &[0, kind], value].concat(), 8)
    };
    let connection =
        |mode, from: &[u8], to: &[u8]| chunk(b"CWFL", &[&[mode], from, &[0], to, &[0]].concat(), 8);
    for ordinal in 2..=users {
        let object = format!("object{ordinal}");
        let shape = format!("shape{ordinal}");
        let identity = format!("identity{ordinal}");
        let surface = format!("surface{ordinal}");
        let extra = if ordinal == 3 {
            attribute(b"STR ", b"cuvs", 0x20, b"other\0")
        } else {
            Vec::new()
        };
        groups.extend(group(b"XFRM", &creation(object.as_bytes(), None), 4));
        groups.extend(appearance_shape(
            shape.as_bytes(),
            object.as_bytes(),
            &extra,
            false,
        ));
        groups.extend(group(b"GPID", &creation(identity.as_bytes(), None), 4));
        groups.extend(group(b"SHAD", &creation(surface.as_bytes(), None), 4));
        for (mode, source, destination) in [
            (1, format!("{shape}.iog.og[0]"), format!("{surface}.dsm")),
            (
                0,
                format!("{identity}.id"),
                format!("{shape}.iog.og[0].gid"),
            ),
            (1, format!("{identity}.msg"), format!("{surface}.gn")),
            (
                0,
                format!("{surface}.mwc"),
                format!("{shape}.iog.og[0].gco"),
            ),
            (0, "shader.oc".into(), format!("{surface}.ss")),
        ] {
            links.extend(connection(mode, source.as_bytes(), destination.as_bytes()));
        }
    }
    appearance_specimen_with_fields(Fields {
        groups: &groups,
        connections: &links,
        ..Default::default()
    })
}
