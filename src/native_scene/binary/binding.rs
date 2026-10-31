use super::super::{mesh::PointOffset, source::SourceLeaf, *};
use super::{
    BinaryError, ErrorKind,
    admission::{Budget, Counts},
    canonical, fail, resource,
};
use crate::{
    maya_binary::{
        container::Container,
        semantic::{Data, WireFacts},
    },
    scene::NodeId,
};
use std::ops::Range;

mod custom;
mod effects;
mod geometry;
mod sites;

pub(super) struct Binding {
    pub(super) record: usize,
    pub(super) group: usize,
    pub(super) id: NativeId,
    transform: Option<usize>,
    pub(super) mesh: Option<usize>,
    fields: [Option<usize>; 5],
    visible: bool,
}

pub(super) struct Owners {
    indices: Vec<usize>,
}

impl Owners {
    pub(super) fn metadata_bytes(&self) -> Result<usize, BinaryError> {
        self.indices
            .capacity()
            .checked_mul(size_of::<usize>())
            .ok_or_else(resource)
    }

    fn name<'a>(facts: &'a WireFacts<'_>, index: usize) -> &'a [u8] {
        let Data::Create(create) = &facts.records[index].data else {
            unreachable!()
        };
        let name = &facts.container.source()[create.name.clone()];
        name.strip_prefix(b":").unwrap_or(name)
    }

    fn absolute_name(name: &[u8], offset: usize) -> Result<&[u8], BinaryError> {
        let name = name.strip_prefix(b":").unwrap_or(name);
        if name.split(|byte| *byte == b':').any(|part| part.is_empty()) || name.contains(&b'|') {
            return Err(fail(ErrorKind::Unsupported, offset));
        }
        Ok(name)
    }

    fn new(facts: &WireFacts<'_>, budget: &mut Budget) -> Result<Self, BinaryError> {
        let count = facts
            .records
            .iter()
            .filter(|record| matches!(record.data, Data::Create(_)))
            .count();
        let mut indices = budget.vector(count)?;
        for (index, record) in facts.records.iter().enumerate() {
            if let Data::Create(create) = &record.data {
                Self::absolute_name(
                    &facts.container.source()[create.name.clone()],
                    create.name.start,
                )?;
                indices.push(index);
            }
        }
        indices.sort_unstable_by(|a, b| Self::name(facts, *a).cmp(Self::name(facts, *b)));
        if indices
            .windows(2)
            .any(|pair| Self::name(facts, pair[0]) == Self::name(facts, pair[1]))
        {
            return Err(fail(ErrorKind::Unsupported, 0));
        }
        Ok(Self { indices })
    }

    pub(super) fn resolve_name(
        &self,
        facts: &WireFacts<'_>,
        raw_owner: &[u8],
        offset: usize,
    ) -> Result<Option<usize>, BinaryError> {
        let owner = Self::absolute_name(raw_owner, offset)?;
        if owner.is_empty() {
            return Err(fail(ErrorKind::Unsupported, offset));
        }
        let index = self
            .indices
            .binary_search_by(|index| Self::name(facts, *index).cmp(owner))
            .ok()
            .map(|index| self.indices[index]);
        if index.is_none() && !raw_owner.starts_with(b":") {
            return Err(fail(ErrorKind::Unsupported, offset));
        }
        Ok(index)
    }

    pub(super) fn resolve(
        &self,
        facts: &WireFacts<'_>,
        range: &Range<usize>,
    ) -> Result<(Option<usize>, Range<usize>), BinaryError> {
        let endpoint = &facts.container.source()[range.clone()];
        let dot = endpoint
            .iter()
            .position(|byte| *byte == b'.')
            .ok_or_else(|| fail(ErrorKind::Unsupported, range.start))?;
        let raw_owner = &endpoint[..dot];
        if dot + 1 == endpoint.len() {
            return Err(fail(ErrorKind::Unsupported, range.start));
        }
        let index = self.resolve_name(facts, raw_owner, range.start)?;
        Ok((index, range.start + dot + 1..range.end))
    }
}

const FIELDS: [TransformField; 5] = [
    TransformField::Translation,
    TransformField::Rotation,
    TransformField::Scale,
    TransformField::RotatePivot,
    TransformField::ScalePivot,
];

fn field(key: &[u8]) -> Option<usize> {
    [b"t".as_slice(), b"r", b"s", b"rp", b"sp"]
        .iter()
        .position(|candidate| *candidate == key)
}

fn name<'a>(facts: &'a WireFacts<'_>, binding: &Binding) -> &'a [u8] {
    let Data::Create(create) = &facts.records[binding.record].data else {
        unreachable!()
    };
    &facts.container.source()[create.name.clone()]
}

fn text(bytes: &[u8], offset: usize) -> Result<&str, BinaryError> {
    std::str::from_utf8(bytes)
        .ok()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| fail(ErrorKind::Unsupported, offset))
}

fn owner_index(bindings: &[Binding], group: usize) -> Option<usize> {
    bindings
        .binary_search_by_key(&group, |binding| binding.group)
        .ok()
}

pub(super) fn children(
    container: &Container,
    group: usize,
) -> impl Iterator<Item = &crate::maya_binary::container::Chunk> {
    let end = container.chunk(group).unwrap().payload_range().end;
    let stop = container
        .chunks()
        .partition_point(|chunk| chunk.header_range().start < end);
    container.chunks()[group + 1..stop]
        .iter()
        .filter(move |chunk| chunk.parent() == Some(group))
}

fn units(container: &Container, budget: &mut Budget) -> Result<Units, BinaryError> {
    let mut linear = None;
    let mut angular = None;
    let mut time = None;
    for chunk in container.chunks() {
        if !chunk
            .parent()
            .and_then(|p| container.chunk(p))
            .is_some_and(|p| p.form() == Some(*b"HEAD"))
        {
            continue;
        }
        let bytes = &container.source()[chunk.payload_range()];
        let offset = chunk.payload_range().start;
        match &chunk.tag() {
            b"LUNI" => {
                if linear.is_some() {
                    return Err(fail(ErrorKind::Malformed, offset));
                }
                linear = Some(match bytes {
                    b"cm" => LinearUnit::Centimeter,
                    b"mm" => LinearUnit::Millimeter,
                    b"m" => LinearUnit::Meter,
                    b"in" => LinearUnit::Inch,
                    b"ft" => LinearUnit::Foot,
                    _ => return Err(fail(ErrorKind::Unsupported, offset)),
                });
            }
            b"AUNI" => {
                if angular.is_some() {
                    return Err(fail(ErrorKind::Malformed, offset));
                }
                angular = Some(match bytes {
                    b"deg" => AngularUnit::Degree,
                    b"rad" => AngularUnit::Radian,
                    _ => return Err(fail(ErrorKind::Unsupported, offset)),
                });
            }
            b"TUNI" => {
                if time.is_some() || !matches!(bytes, b"film" | b"ntsc") {
                    return Err(fail(ErrorKind::Unsupported, offset));
                }
                time = Some(budget.string(text(bytes, offset)?)?);
            }
            _ => {}
        }
    }
    Ok(Units {
        linear: linear.ok_or_else(|| fail(ErrorKind::Malformed, 0))?,
        angular: angular.ok_or_else(|| fail(ErrorKind::Malformed, 0))?,
        time: time.ok_or_else(|| fail(ErrorKind::Malformed, 0))?,
    })
}

pub(super) fn preflight_candidate(
    budget: &Budget,
    counts: &Counts,
    name_bytes: usize,
) -> Result<usize, BinaryError> {
    let scratch = counts.validation_bytes()?;
    let mut estimate = budget.snapshot();
    estimate.charge(scratch)?;
    estimate.preflight::<NativeTransformNode>(counts.transforms)?;
    estimate.preflight::<NativeMeshShape>(counts.shapes)?;
    estimate.preflight::<DagEdge>(counts.dag)?;
    estimate.preflight::<Range<usize>>(counts.atoms)?;
    estimate.preflight::<SourceLeaf>(counts.leaves)?;
    estimate.preflight::<[f32; 3]>(counts.points)?;
    estimate.preflight::<NativeEdge>(counts.edges)?;
    estimate.preflight::<NativeFace>(counts.faces)?;
    estimate.preflight::<DirectedEdge>(counts.corners)?;
    estimate.preflight::<PointOffset>(counts.offsets)?;
    estimate.charge(name_bytes)?;
    estimate.charge(4)?;
    Ok(scratch)
}

pub(super) fn construct(
    container: &Container,
    facts: &WireFacts<'_>,
    retained_bytes: usize,
    retained_appearance_bytes: usize,
) -> Result<NativeScene, BinaryError> {
    if let Some(chunk) = container
        .chunks()
        .iter()
        .find(|chunk| chunk.tag() == *b"FOR8" && chunk.form() == Some(*b"NCRV"))
    {
        return Err(fail(ErrorKind::Unsupported, chunk.header_range().start));
    }
    let initial = container
        .source()
        .len()
        .checked_add(container.metadata_bytes().ok_or_else(resource)?)
        .and_then(|n| n.checked_add(facts.metadata_bytes().ok()?))
        .and_then(|n| n.checked_add(retained_bytes))
        .and_then(|n| n.checked_add(retained_appearance_bytes))
        .ok_or_else(resource)?;
    let mut budget = Budget::new(initial)?;
    let count = facts
        .records
        .iter()
        .filter(|record| {
            matches!(record.data, Data::Create(_))
                && container.chunk(record.group).is_some_and(
                    |c| matches!(c.form(), Some(form) if form == *b"XFRM" || form == *b"DMSH"),
                )
        })
        .count();
    let mut bindings = budget.vector::<Binding>(count)?;
    let mut counts = Counts {
        chunks: container.chunks().len(),
        records: facts.records.len(),
        ..Counts::default()
    };
    for (index, record) in facts.records.iter().enumerate() {
        if !matches!(record.data, Data::Create(_)) {
            continue;
        }
        let form = container.chunk(record.group).unwrap().form();
        let transform = if form == Some(*b"XFRM") {
            let n = counts.transforms;
            counts.transforms += 1;
            Some(n)
        } else {
            None
        };
        if transform.is_none() && form != Some(*b"DMSH") {
            continue;
        }
        if transform.is_none() {
            counts.shapes += 1;
        }
        let id = NativeId(u64::try_from(bindings.len() + 1).map_err(|_| resource())?);
        bindings.push(Binding {
            record: index,
            group: record.group,
            id,
            transform,
            mesh: None,
            fields: [None; 5],
            visible: true,
        });
    }
    let mut names = budget.vector::<usize>(bindings.len())?;
    names.extend(0..bindings.len());
    names.sort_unstable_by(|a, b| name(facts, &bindings[*a]).cmp(name(facts, &bindings[*b])));
    for binding in &bindings {
        let value = name(facts, binding);
        let Data::Create(create) = &facts.records[binding.record].data else {
            unreachable!()
        };
        text(value, create.name.start)?;
        if value.iter().any(|b| matches!(b, b'|' | b':' | b'.')) {
            return Err(fail(ErrorKind::Unsupported, create.name.start));
        }
    }
    if names
        .windows(2)
        .any(|pair| name(facts, &bindings[pair[0]]) == name(facts, &bindings[pair[1]]))
    {
        return Err(fail(ErrorKind::Malformed, 0));
    }
    for binding in &bindings {
        let creation = container
            .chunk(facts.records[binding.record].chunk)
            .unwrap()
            .header_range()
            .start;
        if let Some(early) =
            children(container, binding.group).find(|chunk| chunk.header_range().start < creation)
        {
            return Err(fail(ErrorKind::Unsupported, early.payload_range().start));
        }
    }
    for (index, record) in facts.records.iter().enumerate() {
        let Some(owner) = owner_index(&bindings, record.group) else {
            continue;
        };
        if !matches!(record.data, Data::Create(_))
            && record.owner
                != Some(crate::maya_binary::semantic::Owner::Created(
                    bindings[owner].record,
                ))
        {
            return Err(fail(
                ErrorKind::Unsupported,
                container.chunk(record.chunk).unwrap().payload_range().start,
            ));
        }
        match &record.data {
            Data::Double(double) => {
                if let Some(slot) = field(&container.source()[double.key.clone()]) {
                    if bindings[owner].transform.is_none()
                        || container.chunk(record.chunk).unwrap().tag() != *b"DBL3"
                    {
                        return Err(fail(ErrorKind::Unsupported, double.key.start));
                    }
                    bindings[owner].fields[slot] = Some(index);
                } else if &container.source()[double.key.clone()] == b"v" {
                    let value = f64::from_be_bytes(
                        container.source()[double.numeric.clone()]
                            .try_into()
                            .map_err(|_| fail(ErrorKind::Malformed, double.numeric.start))?,
                    );
                    if bindings[owner].transform.is_none()
                        || double.kind != 0x24
                        || !matches!(value, 0.0 | 1.0)
                    {
                        return Err(fail(ErrorKind::Unsupported, double.key.start));
                    }
                    bindings[owner].visible = value != 0.0;
                }
            }
            Data::Mesh(mesh) => {
                if bindings[owner].mesh.replace(index).is_some() {
                    return Err(fail(ErrorKind::Malformed, mesh.key.start));
                }
                counts.points = counts
                    .points
                    .checked_add(mesh.positions.count / 3)
                    .ok_or_else(resource)?;
                counts.edges = counts
                    .edges
                    .checked_add(mesh.endpoints.count / 2)
                    .ok_or_else(resource)?;
                counts.corners = counts
                    .corners
                    .checked_add(mesh.corners.count)
                    .ok_or_else(resource)?;
                counts.faces = counts
                    .faces
                    .checked_add(
                        mesh.corners
                            .values(container.source())
                            .filter(|word| word >> 24 & 0x60 == 0x60)
                            .count(),
                    )
                    .ok_or_else(resource)?;
            }
            _ => {}
        }
    }
    counts.leaves = bindings
        .iter()
        .map(|binding| binding.fields.iter().flatten().count())
        .sum();
    counts.dag = bindings.len();
    let sites = sites::collect(container, &bindings, &mut budget)?;
    counts.sites = sites.len();
    counts.site_ancestors = sites.iter().map(|site| site.ancestors.len()).sum();
    counts.atoms = container
        .chunks()
        .iter()
        .map(|chunk| {
            if chunk.form().is_some() && !chunk.padding_range().is_empty() {
                2
            } else {
                1
            }
        })
        .sum();
    let owners = Owners::new(facts, &mut budget)?;
    counts.offsets = effects::inspect(container, facts, &bindings, &names, &owners, &mut budget)?;
    counts.admit_work()?;
    let scratch = preflight_candidate(
        &budget,
        &counts,
        bindings
            .iter()
            .map(|binding| name(facts, binding).len())
            .sum::<usize>(),
    )?;
    budget.charge(scratch)?;
    let units = units(container, &mut budget)?;
    let mut scene = NativeScene {
        nodes: budget.vector(counts.transforms)?,
        shapes: budget.vector(counts.shapes)?,
        curves: Vec::new(),
        dag: budget.vector(counts.dag)?,
        units,
        source: None,
        materials: Vec::new(),
        session_materials: Default::default(),
        session_joints: Default::default(),
        next_id: 1,
        next_native_id: 1,
    };
    let mut source = SourceRetention {
        bytes: container.source().clone(),
        format: SourceFormat::Binary,
        records: budget.vector(counts.atoms)?,
        leaves: budget.vector(counts.leaves)?,
        sites,
        appearance: None,
    };
    for chunk in container.chunks() {
        if chunk.form().is_some() {
            source
                .records
                .push(chunk.header_range().start..chunk.payload_range().start + 4);
            if !chunk.padding_range().is_empty() {
                source.records.push(chunk.padding_range());
            }
        } else {
            source
                .records
                .push(chunk.header_range().start..chunk.padding_range().end);
        }
    }
    source.records.sort_unstable_by_key(|atom| atom.start);
    for binding in &bindings {
        let record = &facts.records[binding.record];
        let Data::Create(create) = &record.data else {
            unreachable!()
        };
        let parent = if let Some(parent) = &create.parent {
            let index = names
                .binary_search_by(|index| {
                    name(facts, &bindings[*index]).cmp(&container.source()[parent.clone()])
                })
                .map_err(|_| fail(ErrorKind::Unsupported, parent.start))?;
            let parent = &bindings[names[index]];
            if parent.transform.is_none() {
                return Err(fail(ErrorKind::Unsupported, create.name.start));
            }
            Some(parent.id)
        } else if binding.transform.is_some() {
            None
        } else {
            return Err(fail(ErrorKind::Unsupported, create.name.start));
        };
        scene.dag.push(DagEdge {
            parent,
            child: binding.id,
        });
        let view = GeometryViewFacts {
            geometry: Some(StaticGeometry),
            local_visibility: Some(binding.visible),
        };
        let name = budget.string(text(name(facts, binding), create.name.start)?)?;
        if let Some(index) = binding.transform {
            let mut values: [[Scalar; 3]; 5] = std::array::from_fn(|slot| {
                std::array::from_fn(|_| Scalar::new(if slot == 2 { 1.0 } else { 0.0 }))
            });
            for (slot, assignment) in binding.fields.iter().enumerate() {
                let Some(index) = assignment else {
                    continue;
                };
                let record = &facts.records[*index];
                let Data::Double(double) = &record.data else {
                    unreachable!()
                };
                let leaf = source.leaves.len();
                source.leaves.push(SourceLeaf {
                    payload: container.chunk(record.chunk).unwrap().payload_range(),
                    numeric: double.numeric.clone(),
                    owner: binding.id,
                    field: FIELDS[slot],
                    storage: FIELDS[slot].storage(),
                });
                for (component, word) in container.source()[double.numeric.clone()]
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .enumerate()
                {
                    let internal = f64::from_be_bytes(*word);
                    values[slot][component] = Scalar {
                        value: FIELDS[slot]
                            .storage()
                            .declared(internal, &scene.units)
                            .map_err(|e| canonical(e, Some(double.numeric.start)))?,
                        origin: Some(ScalarOrigin::Binary { leaf, component }),
                        replaced: false,
                    };
                }
            }
            let [translation, rotation, scale, rotate_pivot, scale_pivot] = values;
            let [tx, ty, tz] = translation;
            let [rx, ry, rz] = rotation;
            let [sx, sy, sz] = scale;
            scene.nodes.push(NativeTransformNode {
                id: NodeId::new(u64::try_from(index + 1).map_err(|_| resource())?),
                transform_id: binding.id,
                transform_uuid: None,
                name,
                channels: [tx, ty, tz, rx, ry, rz, sx, sy, sz],
                rotate_pivot,
                scale_pivot,
                restrictions: Vec::new(),
                unsupported_transform: Vec::new(),
                extras: TransformExtras::default(),
                view,
            });
        } else {
            let mesh = geometry::construct(container, facts, binding, &mut budget)?;
            scene.shapes.push(NativeMeshShape {
                id: binding.id,
                uuid: None,
                name,
                mesh,
                restrictions: Vec::new(),
                view,
            });
        }
    }
    let (appearance, materials) = super::appearance::bind(
        container,
        facts,
        &bindings,
        &owners,
        &scene,
        &mut budget,
        retained_appearance_bytes,
    )?;
    source.appearance = Some(appearance);
    scene.materials = materials;
    scene.source = Some(source);
    NativeScene::from_binary_candidate(scene).map_err(|error| canonical(error, None))
}
