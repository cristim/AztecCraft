//! Session-material statements of the authored ASCII subset (#156), read into a side table.
//!
//! Per session material `lambertN` (N >= 2) the layout is `createNode lambert -n "lambertN";`,
//! `setAttr ".c" -type "float3" r g b;`, `createNode shadingEngine -n "lambertNSG";`,
//! `setAttr ".ro" yes;`, then the connects `"lambertN.oc" "lambertNSG.ss"`,
//! `"lambertNSG.pa" ":renderPartition.st" -na`, `"lambertN.msg" ":defaultShaderList1.s" -na`, and
//! per assigned shape `"<shape>.iog" "lambertNSG.dsm" -na` (an unassigned shape keeps its link to
//! `:initialShadingGroup`; never both). Documentation (Maya 2027): Nodes/lambert (`color` `c`
//! float3, `outColor` `oc`), Nodes/shadingEngine (`surfaceShader` `ss`), Nodes/objectSet
//! (`dagSetMembers` `dsm`, `partition` `pa`, `renderableOnlySet` `ro`; a DAG node's
//! `instObjGroups` into `dsm` puts the whole object in the set) and CommandsPython/connectAttr
//! (`-na`). PROVISIONAL, not on those pages and not compared with Maya (#5): the statement
//! order, `.ro yes`, the `-na` placements, the `:renderPartition.st` and
//! `:defaultShaderList1.s` connects, and the omission of `materialInfo` and `.ihi`.
//!
//! The gate attaches the table and compares re-emitted bytes. A colour channel must be the
//! exact text Rust prints for its `f32`, so nothing the writer would not emit is read.
use super::{AsciiError, Builder, Cursor, ErrorKind, Kind, add, error, mul, numeric, resource};
use crate::native_scene::{
    NativeId, NativeMeshShape, NativeScene,
    admission::Budget,
    session_material::{
        Lambert, MAX_SESSION_MATERIALS, SessionMaterialId, SessionMaterials, ShadingGroup,
        lambert_number,
    },
};
use std::collections::BTreeMap;

/// Lambert owner bits: colour, `oc` into a group's `ss`, `msg` into `:defaultShaderList1.s`.
const COLOR: u32 = 1;
const SHADER_OUT: u32 = 2;
const SHADER_LIST: u32 = 4;
/// Group owner bits: `ro`, `pa` into `:renderPartition.st`, a lambert wired to `ss`.
const RENDERABLE: u32 = 1;
const PARTITION: u32 = 2;
const SURFACE: u32 = 4;
const COMPLETE: u32 = 7;

pub(super) struct LambertParts {
    name: String,
    color: Option<[f64; 3]>,
}

pub(super) struct GroupParts {
    name: String,
    material: Option<usize>,
}

/// What the material statements said, in file order: lamberts, shading groups and the
/// (shape index, group index) pairs of the explicit memberships.
#[derive(Default)]
pub(super) struct MaterialParts {
    lamberts: Vec<LambertParts>,
    groups: Vec<GroupParts>,
    assigned: Vec<(usize, usize)>,
}

impl MaterialParts {
    pub(super) fn with_capacity(
        budget: &mut Budget,
        lamberts: usize,
        groups: usize,
        shapes: usize,
    ) -> Result<Self, AsciiError> {
        // Index 0 of the table is the default pair, so a file holds at most one fewer.
        if lamberts >= MAX_SESSION_MATERIALS || groups >= MAX_SESSION_MATERIALS {
            return Err(resource());
        }
        Ok(Self {
            lamberts: budget.vector(lamberts).map_err(|_| resource())?,
            groups: budget.vector(groups).map_err(|_| resource())?,
            assigned: budget
                .vector(if groups == 0 { 0 } else { shapes })
                .map_err(|_| resource())?,
        })
    }
}

/// The table a file's material statements describe. Material and group `i` are those at table
/// index `i + 1`: index 0 is the default pair no file mentions.
pub(super) struct ParsedMaterials {
    table: SessionMaterials,
    /// (shape index in the decoded scene, group index in the table).
    assigned: Vec<(usize, usize)>,
}

impl ParsedMaterials {
    /// Stores the table on the source-free candidate; `validate` still checks it.
    pub(super) fn attach(
        self,
        shapes: &[NativeMeshShape],
        candidate: &mut NativeScene,
    ) -> Result<(), AsciiError> {
        let mut table = self.table;
        let mut assignments: BTreeMap<NativeId, usize> = BTreeMap::new();
        for (shape, group) in self.assigned {
            let shape = shapes
                .get(shape)
                .ok_or_else(|| error(ErrorKind::Malformed, 0))?;
            assignments.insert(shape.id(), group);
        }
        table.assignments = assignments;
        candidate.attach_authored_session_materials(table);
        Ok(())
    }
}

impl Cursor<'_, '_> {
    /// One colour channel: finite, within 0 through 1 and written exactly as Rust prints the
    /// `f32` it parses to, which is what the writer emits.
    fn colour_channel(&mut self) -> Result<f64, AsciiError> {
        let text = self.word()?;
        self.work.charge(mul(2, text.len())?)?;
        if !numeric(text.as_bytes()) {
            return Err(error(ErrorKind::Malformed, self.offset));
        }
        let value: f32 = text
            .parse()
            .map_err(|_| error(ErrorKind::Malformed, self.offset))?;
        if !(0.0..=1.0).contains(&value) || value.to_string() != text {
            return Err(error(ErrorKind::Unsupported, self.offset));
        }
        Ok(f64::from(value))
    }
}

/// `lambertN`'s `N`, for a name that is exactly `lambert<N>` (N >= 2: `lambert1` is Maya's own)
/// or, with `group`, `lambert<N>SG`.
pub(super) fn material_name(name: &str, group: bool) -> bool {
    name.ends_with("SG") == group && lambert_number(name).is_some_and(|number| number >= 2)
}

fn net_added(c: &mut Cursor<'_, '_>) -> Result<(), AsciiError> {
    if c.word()? != "-na" {
        return Err(error(ErrorKind::Unsupported, c.offset));
    }
    Ok(())
}

impl Builder<'_, '_> {
    /// `createNode lambert` / `createNode shadingEngine`: the owner kind for a well-named node,
    /// or `None` for any other class. Only the gate's decode admits them.
    pub(super) fn material_node(
        &mut self,
        class: &str,
        name: &str,
        c: &Cursor<'_, '_>,
    ) -> Result<Option<Kind>, AsciiError> {
        let group = match class {
            "lambert" => false,
            "shadingEngine" => true,
            _ => return Ok(None),
        };
        if !self.meshes || !material_name(name, group) {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        let parts = &mut self.materials;
        let stored = self.budget.string(name).map_err(|_| resource())?;
        let kind = if group {
            if parts.groups.len() == parts.groups.capacity() {
                return Err(resource());
            }
            parts.groups.push(GroupParts {
                name: stored,
                material: None,
            });
            Kind::ShadingEngine(parts.groups.len() - 1)
        } else {
            if parts.lamberts.len() == parts.lamberts.capacity() {
                return Err(resource());
            }
            parts.lamberts.push(LambertParts {
                name: stored,
                color: None,
            });
            Kind::Lambert(parts.lamberts.len() - 1)
        };
        Ok(Some(kind))
    }

    /// `setAttr ".c" -type "float3" r g b` on a lambert, `setAttr ".ro" yes` on a group.
    pub(super) fn material_attribute(
        &mut self,
        owner: usize,
        flags: &super::AttrFlags<'_>,
        c: &mut Cursor<'_, '_>,
        attribute: &str,
    ) -> Result<(), AsciiError> {
        match (self.owners[owner].kind, attribute) {
            (Kind::Lambert(index), ".c") => {
                flags.typed("float3", c)?;
                let color = [
                    c.colour_channel()?,
                    c.colour_channel()?,
                    c.colour_channel()?,
                ];
                c.done()?;
                self.mark(owner, COLOR, c)?;
                self.materials.lamberts[index].color = Some(color);
            }
            (Kind::ShadingEngine(_), ".ro") => {
                flags.plain(c)?;
                if c.word()? != "yes" {
                    return Err(error(ErrorKind::Unsupported, c.offset));
                }
                c.done()?;
                self.mark(owner, RENDERABLE, c)?;
            }
            _ => return Err(error(ErrorKind::Unsupported, c.offset)),
        }
        Ok(())
    }

    fn named(&self, name: &str, c: &mut Cursor<'_, '_>) -> Result<(usize, Kind), AsciiError> {
        let owner = self
            .find(name, c.work)?
            .ok_or_else(|| error(ErrorKind::Unsupported, c.offset))?;
        Ok((owner, self.owners[owner].kind))
    }

    /// The material connects other than membership; `Ok(false)` when `from`/`to` are not one.
    pub(super) fn material_connect(
        &mut self,
        from: &str,
        to: &str,
        c: &mut Cursor<'_, '_>,
    ) -> Result<bool, AsciiError> {
        if let (Some(lambert), Some(group)) = (from.strip_suffix(".oc"), to.strip_suffix(".ss")) {
            let (lambert_owner, lambert_kind) = self.named(lambert, c)?;
            let (group_owner, group_kind) = self.named(group, c)?;
            let (Kind::Lambert(material), Kind::ShadingEngine(index)) = (lambert_kind, group_kind)
            else {
                return Err(error(ErrorKind::Unsupported, c.offset));
            };
            self.mark(lambert_owner, SHADER_OUT, c)?;
            self.mark(group_owner, SURFACE, c)?;
            self.materials.groups[index].material = Some(material);
            return Ok(true);
        }
        if let (Some(group), ":renderPartition.st") = (from.strip_suffix(".pa"), to) {
            let (owner, kind) = self.named(group, c)?;
            if !matches!(kind, Kind::ShadingEngine(_)) {
                return Err(error(ErrorKind::Unsupported, c.offset));
            }
            net_added(c)?;
            self.mark(owner, PARTITION, c)?;
            return Ok(true);
        }
        if let (Some(lambert), ":defaultShaderList1.s") = (from.strip_suffix(".msg"), to) {
            let (owner, kind) = self.named(lambert, c)?;
            if !matches!(kind, Kind::Lambert(_)) {
                return Err(error(ErrorKind::Unsupported, c.offset));
            }
            net_added(c)?;
            self.mark(owner, SHADER_LIST, c)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// `"<shape>.iog" "<group>.dsm" -na` into a session shading group: the shape's one
    /// membership, so a shape already linked anywhere (default or other group) is refused.
    pub(super) fn group_membership(
        &mut self,
        shape: usize,
        group: &str,
        c: &mut Cursor<'_, '_>,
    ) -> Result<(), AsciiError> {
        let (_, kind) = self.named(group, c)?;
        let Kind::ShadingEngine(index) = kind else {
            return Err(error(ErrorKind::Unsupported, c.offset));
        };
        let Kind::Mesh(mesh) = self.owners[shape].kind else {
            return Err(error(ErrorKind::Unsupported, c.offset));
        };
        net_added(c)?;
        self.mark(shape, super::HAS_DEFAULT_LINK, c)?;
        if self.materials.assigned.len() == self.materials.assigned.capacity() {
            return Err(resource());
        }
        // The table's group 0 is the default one, so group `index` of the file is `index + 1`.
        self.materials.assigned.push((mesh, add(index, 1)?));
        Ok(())
    }

    /// The side table once every statement was read, or `None` for a file with no material
    /// statements. Anything partial is refused: each material is a complete, paired block.
    pub(super) fn finish_materials(&mut self) -> Result<Option<ParsedMaterials>, AsciiError> {
        let parts = std::mem::take(&mut self.materials);
        if parts.lamberts.is_empty() && parts.groups.is_empty() {
            return Ok(None);
        }
        // A block missing a part is one the writer would not emit, not a damaged statement.
        let incomplete = || AsciiError {
            kind: ErrorKind::Unsupported,
            offset: None,
        };
        if parts.lamberts.len() != parts.groups.len()
            || self.owners.iter().any(|owner| {
                matches!(owner.kind, Kind::Lambert(_) | Kind::ShadingEngine(_))
                    && owner.attributes != COMPLETE
            })
        {
            return Err(incomplete());
        }
        let mut table = SessionMaterials::default();
        let wanted = add(parts.lamberts.len(), 1)?;
        table
            .materials
            .try_reserve(wanted)
            .and(table.groups.try_reserve(wanted))
            .map_err(|_| resource())?;
        for (index, (lambert, group)) in parts.lamberts.into_iter().zip(parts.groups).enumerate() {
            let id = SessionMaterialId::from_index(add(index, 1)?);
            // `oc` must join the lambert and the group declared with it, in the same order.
            if group.material != Some(index)
                || group.name.strip_suffix("SG") != Some(lambert.name.as_str())
            {
                return Err(error(ErrorKind::Unsupported, 0));
            }
            table.materials.push(Lambert {
                name: lambert.name,
                color: lambert.color.ok_or_else(incomplete)?,
            });
            table.groups.push(ShadingGroup {
                name: group.name,
                material: id,
            });
        }
        Ok(Some(ParsedMaterials {
            table,
            assigned: parts.assigned,
        }))
    }
}
