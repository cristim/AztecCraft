//! Session-material statements of the authored ASCII subset (#156), read into a side table.
//!
//! Per session material `lambertN` (N >= 2) the layout is `createNode lambert -n "lambertN";`,
//! `setAttr ".c" -type "float3" r g b;`, `createNode shadingEngine -n "lambertNSG";`,
//! `setAttr ".ro" yes;`, then the connects (read in a later change). Documentation (Maya 2027):
//! Nodes/lambert (`color` `c` float3), Nodes/shadingEngine and Nodes/objectSet
//! (`renderableOnlySet` `ro`). PROVISIONAL, not on those pages and not compared with Maya (#5):
//! the statement order, `.ro yes`, and the omission of `materialInfo` and `.ihi`.
//!
//! The gate attaches the table and compares re-emitted bytes. A colour channel must be the
//! exact text Rust prints for its `f32`, so nothing the writer would not emit is read.
use super::{AsciiError, Builder, Cursor, ErrorKind, Kind, add, error, mul, numeric, resource};
use crate::native_scene::{
    NativeScene,
    admission::Budget,
    session_material::{
        Lambert, MAX_SESSION_MATERIALS, SessionMaterialId, SessionMaterials, ShadingGroup,
        lambert_number,
    },
};

/// Owner bit: a lambert's colour or a group's `ro`, each needed once.
const SEEN: u32 = 1;

pub(super) struct LambertParts {
    name: String,
    color: Option<[f64; 3]>,
}

pub(super) struct GroupParts {
    name: String,
}

/// What the material statements said, in file order.
#[derive(Default)]
pub(super) struct MaterialParts {
    lamberts: Vec<LambertParts>,
    groups: Vec<GroupParts>,
}

impl MaterialParts {
    pub(super) fn with_capacity(
        budget: &mut Budget,
        lamberts: usize,
        groups: usize,
    ) -> Result<Self, AsciiError> {
        // Index 0 of the table is the default pair, so a file holds at most one fewer.
        if lamberts >= MAX_SESSION_MATERIALS || groups >= MAX_SESSION_MATERIALS {
            return Err(resource());
        }
        Ok(Self {
            lamberts: budget.vector(lamberts).map_err(|_| resource())?,
            groups: budget.vector(groups).map_err(|_| resource())?,
        })
    }
}

/// The table a file's material statements describe. Material and group `i` are those at table
/// index `i + 1`: index 0 is the default pair no file mentions.
pub(super) struct ParsedMaterials {
    table: SessionMaterials,
}

impl ParsedMaterials {
    /// Stores the table on the source-free candidate; `validate` still checks it.
    pub(super) fn attach(self, candidate: &mut NativeScene) {
        candidate.attach_authored_session_materials(self.table);
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
fn material_name(name: &str, group: bool) -> bool {
    name.ends_with("SG") == group && lambert_number(name).is_some_and(|number| number >= 2)
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
            parts.groups.push(GroupParts { name: stored });
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
                self.mark(owner, SEEN, c)?;
                self.materials.lamberts[index].color = Some(color);
            }
            (Kind::ShadingEngine(_), ".ro") => {
                flags.plain(c)?;
                if c.word()? != "yes" {
                    return Err(error(ErrorKind::Unsupported, c.offset));
                }
                c.done()?;
                self.mark(owner, SEEN, c)?;
            }
            _ => return Err(error(ErrorKind::Unsupported, c.offset)),
        }
        Ok(())
    }

    /// The side table once every statement was read, or `None` for a file with no material
    /// statements. Anything partial is refused: each material is a complete, paired block.
    pub(super) fn finish_materials(&mut self) -> Result<Option<ParsedMaterials>, AsciiError> {
        let parts = std::mem::take(&mut self.materials);
        if parts.lamberts.is_empty() && parts.groups.is_empty() {
            return Ok(None);
        }
        let malformed = || error(ErrorKind::Malformed, 0);
        if parts.lamberts.len() != parts.groups.len()
            || self.owners.iter().any(|owner| {
                matches!(owner.kind, Kind::Lambert(_) | Kind::ShadingEngine(_))
                    && owner.attributes != SEEN
            })
        {
            return Err(malformed());
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
            // Each group is declared with its lambert, in the same order.
            if group.name.strip_suffix("SG") != Some(lambert.name.as_str()) {
                return Err(error(ErrorKind::Unsupported, 0));
            }
            table.materials.push(Lambert {
                name: lambert.name,
                color: lambert.color.ok_or_else(malformed)?,
            });
            table.groups.push(ShadingGroup {
                name: group.name,
                material: id,
            });
        }
        Ok(Some(ParsedMaterials { table }))
    }
}
