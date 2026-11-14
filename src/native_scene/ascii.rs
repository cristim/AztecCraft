use super::{
    AngularUnit, DagEdge, GeometryViewFacts, LinearUnit, NativeCurveShape, NativeError, NativeId,
    NativeMesh, NativeMeshShape, NativeScene, NativeTransform, NativeTransformNode, Scalar,
    TransformExtras, Units, UvSet,
    admission::Budget,
    authored::BUILTINS,
    curve::CurveSource,
    source::{SourceFormat, SourceRetention},
};
pub use crate::maya_ascii::{AsciiError, ErrorKind};
use crate::{
    maya_ascii::{
        Work,
        lex::{self, Statement, Token, TokenKind},
    },
    scene::NodeId,
};
pub use document::{AsciiDecoded, AsciiDocumentError, AuthoredProjection, decode_document};
use std::{ops::Range, sync::Arc};
use uuid::Uuid;

fn error(kind: ErrorKind, offset: usize) -> AsciiError {
    AsciiError {
        kind,
        offset: Some(offset),
    }
}
fn resource() -> AsciiError {
    AsciiError {
        kind: ErrorKind::ResourceLimit,
        offset: None,
    }
}
fn add(a: usize, b: usize) -> Result<usize, AsciiError> {
    a.checked_add(b).ok_or_else(resource)
}
fn mul(a: usize, b: usize) -> Result<usize, AsciiError> {
    a.checked_mul(b).ok_or_else(resource)
}

fn literal_equal(a: &[u8], b: &[u8], work: &mut Work) -> Result<bool, AsciiError> {
    fn next(bytes: &mut std::slice::Iter<'_, u8>) -> Result<Option<u8>, AsciiError> {
        let Some(&byte) = bytes.next() else {
            return Ok(None);
        };
        if byte != b'\\' {
            return Ok(Some(byte));
        }
        match bytes.next() {
            Some(b'n') => Ok(Some(b'\n')),
            Some(b't') => Ok(Some(b'\t')),
            Some(b'"') => Ok(Some(b'"')),
            Some(b'\\') => Ok(Some(b'\\')),
            _ => Err(error(ErrorKind::Malformed, 0)),
        }
    }
    work.charge(add(a.len(), b.len())?)?;
    let (mut a, mut b) = (a.iter(), b.iter());
    loop {
        let pair = (next(&mut a)?, next(&mut b)?);
        if pair.0 != pair.1 {
            return Ok(false);
        }
        if pair.0.is_none() {
            return Ok(true);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Transform(usize),
    Curve(usize),
    Mesh(usize),
    Camera,
    LightLinker,
    LayerManager,
    DisplayLayer,
    RenderLayerManager,
    RenderLayer,
    Script,
    Builtin,
    /// Index into the material side table's lamberts / shading groups.
    Lambert(usize),
    ShadingEngine(usize),
}

/// Owner attribute bits (1..=1024 are channels): a transform that already has a mesh child, or a
/// curve child. A mesh needs a transform with no other shape; several curves may share one.
/// The bits share one word with the channel bits, so every new bit must be distinct.
const HAS_MESH: u32 = 2048;
const HAS_CURVE: u32 = 4096;
const HAS_IT: u32 = 8192;
const HAS_OPM: u32 = 16384;
/// A mesh owner's bit; mesh.rs uses 8192..=262144 on the same word.
const HAS_DEFAULT_LINK: u32 = 524288;

struct Owner {
    name: Range<usize>,
    kind: Kind,
    attributes: u32,
    uuid: Option<Uuid>,
    keyability: bool,
}

struct Cursor<'a, 'w> {
    source: &'a [u8],
    tokens: &'a [Token],
    position: usize,
    offset: usize,
    work: &'w mut Work,
}

impl<'a> Cursor<'a, '_> {
    fn next(&mut self) -> Result<&'a Token, AsciiError> {
        let token = self
            .tokens
            .get(self.position)
            .ok_or_else(|| error(ErrorKind::Malformed, self.offset))?;
        self.work.charge(1)?;
        self.position = add(self.position, 1)?;
        Ok(token)
    }
    fn word(&mut self) -> Result<&'a str, AsciiError> {
        let token = self.next()?;
        if token.kind != TokenKind::Word {
            return Err(error(ErrorKind::Malformed, token.span.start));
        }
        token.text(self.source, self.work)
    }
    fn literal(&mut self) -> Result<(&'a str, Range<usize>), AsciiError> {
        let token = self.next()?;
        if token.kind != TokenKind::Quoted {
            return Err(error(ErrorKind::Malformed, token.span.start));
        }
        let text = token.text(self.source, self.work)?;
        let end = token.span.end.checked_sub(1).ok_or_else(resource)?;
        let value = text
            .get(1..text.len().checked_sub(1).ok_or_else(resource)?)
            .ok_or_else(|| error(ErrorKind::Malformed, token.span.start))?;
        Ok((value, add(token.span.start, 1)?..end))
    }
    fn peek(&mut self) -> Result<Option<(TokenKind, &'a str)>, AsciiError> {
        let Some(token) = self.tokens.get(self.position) else {
            return Ok(None);
        };
        self.work.charge(1)?;
        Ok(Some((token.kind, token.text(self.source, self.work)?)))
    }
    fn done(&self) -> Result<(), AsciiError> {
        if let Some(token) = self.tokens.get(self.position) {
            return Err(error(ErrorKind::Unsupported, token.span.start));
        }
        Ok(())
    }
    fn count(&mut self) -> Result<usize, AsciiError> {
        let text = self.word()?;
        self.work.charge(text.len())?;
        let digits = text.strip_prefix('+').unwrap_or(text);
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(error(ErrorKind::Malformed, self.offset));
        }
        text.parse::<u32>()
            .map(|n| n as usize)
            .map_err(|_| error(ErrorKind::Malformed, self.offset))
    }
    fn integer(&mut self) -> Result<i32, AsciiError> {
        let text = self.word()?;
        self.work.charge(text.len())?;
        let digits = text
            .strip_prefix('+')
            .or_else(|| text.strip_prefix('-'))
            .unwrap_or(text);
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(error(ErrorKind::Malformed, self.offset));
        }
        text.parse()
            .map_err(|_| error(ErrorKind::Malformed, self.offset))
    }
    fn number(&mut self) -> Result<f64, AsciiError> {
        let text = self.word()?;
        self.work.charge(mul(2, text.len())?)?;
        if !numeric(text.as_bytes()) {
            return Err(error(ErrorKind::Malformed, self.offset));
        }
        let number: f64 = text
            .parse()
            .map_err(|_| error(ErrorKind::Malformed, self.offset))?;
        if !number.is_finite() {
            return Err(error(ErrorKind::Malformed, self.offset));
        }
        Ok(number)
    }
    fn boolean(&mut self) -> Result<bool, AsciiError> {
        match self.word()? {
            "yes" | "on" | "true" | "1" => Ok(true),
            "no" | "off" | "false" | "0" => Ok(false),
            _ => Err(error(ErrorKind::Malformed, self.offset)),
        }
    }
    fn triple(&mut self) -> Result<[f64; 3], AsciiError> {
        Ok([self.number()?, self.number()?, self.number()?])
    }
    fn string(&mut self) -> Result<(), AsciiError> {
        self.literal().map(|_| ())
    }
    fn string_body(&mut self) -> Result<(), AsciiError> {
        if self
            .tokens
            .get(self.position)
            .is_some_and(|t| t.kind == TokenKind::Quoted)
        {
            return self.string();
        }
        if self.next()?.kind != TokenKind::LeftParen {
            return Err(error(ErrorKind::Unsupported, self.offset));
        }
        self.string()?;
        loop {
            match self.next()?.kind {
                TokenKind::RightParen => return Ok(()),
                TokenKind::Plus => self.string()?,
                _ => return Err(error(ErrorKind::Unsupported, self.offset)),
            }
        }
    }
}

fn numeric(bytes: &[u8]) -> bool {
    let mut position = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let start = position;
    while bytes.get(position).is_some_and(u8::is_ascii_digit) {
        position += 1;
    }
    let mut digits = position - start;
    if bytes.get(position) == Some(&b'.') {
        position += 1;
        let start = position;
        while bytes.get(position).is_some_and(u8::is_ascii_digit) {
            position += 1;
        }
        digits += position - start;
    }
    if digits == 0 {
        return false;
    }
    if matches!(bytes.get(position), Some(b'e' | b'E')) {
        position += 1;
        if matches!(bytes.get(position), Some(b'+' | b'-')) {
            position += 1;
        }
        let start = position;
        while bytes.get(position).is_some_and(u8::is_ascii_digit) {
            position += 1;
        }
        if position == start {
            return false;
        }
    }
    position == bytes.len()
}

#[derive(Default)]
struct AttrFlags<'a> {
    attribute: Option<&'a str>,
    size: Option<usize>,
    value_type: Option<&'a str>,
    keyability: bool,
}

fn attr_flags<'a>(c: &mut Cursor<'a, '_>) -> Result<AttrFlags<'a>, AsciiError> {
    let mut flags = AttrFlags::default();
    while let Some((kind, raw)) = c.peek()? {
        match (kind, raw) {
            (TokenKind::Quoted, _) if flags.attribute.is_none() => {
                flags.attribute = Some(c.literal()?.0)
            }
            (TokenKind::Word, "-s") if flags.size.is_none() => {
                c.next()?;
                flags.size = Some(c.count()?);
            }
            (TokenKind::Word, "-type") if flags.value_type.is_none() => {
                c.next()?;
                flags.value_type = Some(c.literal()?.0);
            }
            (TokenKind::Word, "-k") if !flags.keyability => {
                c.next()?;
                if c.word()? != "off" {
                    return Err(error(ErrorKind::Unsupported, c.offset));
                }
                flags.keyability = true;
            }
            (TokenKind::Word, raw)
                if raw.starts_with('-')
                    && raw.as_bytes().get(1).is_some_and(u8::is_ascii_alphabetic) =>
            {
                return Err(error(ErrorKind::Unsupported, c.offset));
            }
            _ => break,
        }
    }
    if flags.attribute.is_none() {
        return Err(error(ErrorKind::Malformed, c.offset));
    }
    Ok(flags)
}

impl AttrFlags<'_> {
    fn plain(&self, c: &Cursor<'_, '_>) -> Result<(), AsciiError> {
        if self.size.is_some() || self.value_type.is_some() || self.keyability {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        Ok(())
    }
    fn typed(&self, expected: &str, c: &Cursor<'_, '_>) -> Result<(), AsciiError> {
        if self.size.is_some() || self.value_type != Some(expected) || self.keyability {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        Ok(())
    }
    fn allocation(&self, c: &Cursor<'_, '_>) -> Result<(), AsciiError> {
        if self.size.is_none() || self.value_type.is_some() || self.keyability {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        c.done()
    }
}

struct Builder<'a, 'b> {
    source: &'a [u8],
    budget: &'b mut Budget,
    scene: NativeScene,
    owners: Vec<Owner>,
    selected: Option<usize>,
    units: bool,
    /// Whether `createNode mesh` is admitted. False for the plain `decode`; `decode_document`
    /// sets it and adopts the result only after proving it byte-identical to the writer's output.
    meshes: bool,
    mesh_parts: Vec<mesh::MeshParts>,
    materials: material::MaterialParts,
    requires: bool,
    file_info: Vec<Range<usize>>,
    connections: u8,
    relationships: u8,
    default_links: usize,
}

impl Builder<'_, '_> {
    fn find(&self, name: &str, work: &mut Work) -> Result<Option<usize>, AsciiError> {
        for (index, owner) in self.owners.iter().enumerate() {
            work.charge(add(1, name.len())?)?;
            if self.source.get(owner.name.clone()) == Some(name.as_bytes()) {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }
    fn owner(&self, c: &Cursor<'_, '_>) -> Result<usize, AsciiError> {
        self.selected
            .ok_or_else(|| error(ErrorKind::Malformed, c.offset))
    }
    fn mark(&mut self, owner: usize, bit: u32, c: &Cursor<'_, '_>) -> Result<(), AsciiError> {
        let record = self
            .owners
            .get_mut(owner)
            .ok_or_else(|| error(ErrorKind::Malformed, c.offset))?;
        if record.attributes & bit != 0 {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        record.attributes |= bit;
        Ok(())
    }
    fn next_native_id(&self) -> Result<NativeId, AsciiError> {
        let used = add(
            add(self.scene.nodes.len(), self.scene.curves.len())?,
            self.scene.shapes.len(),
        )?;
        let id = u64::try_from(used)
            .map_err(|_| resource())?
            .checked_add(1)
            .ok_or_else(resource)?;
        Ok(NativeId(id))
    }
    fn create(&mut self, c: &mut Cursor<'_, '_>) -> Result<(), AsciiError> {
        let class = c.word()?;
        let (mut name, mut parent, mut shared) = (None, None, false);
        while c.position < c.tokens.len() {
            match c.word()? {
                "-n" if name.is_none() => name = Some(c.literal()?),
                "-p" if parent.is_none() => parent = Some(c.literal()?.0),
                "-s" if !shared => shared = true,
                _ => return Err(error(ErrorKind::Unsupported, c.offset)),
            }
        }
        let (name, range) = name.ok_or_else(|| error(ErrorKind::Malformed, c.offset))?;
        c.work.charge(add(
            name.len(),
            mul(
                add(self.scene.nodes.len(), BUILTINS.len())?,
                add(1, name.len())?,
            )?,
        )?)?;
        if self.scene.validate_new_name(None, name).is_err()
            || BUILTINS.contains(&name)
            || self.find(name, c.work)?.is_some()
        {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        let parent = parent
            .map(|name| {
                self.find(name, c.work)?
                    .ok_or_else(|| error(ErrorKind::Unsupported, c.offset))
            })
            .transpose()?;
        let kind = match (class, name) {
            ("transform", _) => {
                if shared && !matches!(name, "persp" | "top" | "front" | "side") {
                    return Err(error(ErrorKind::Unsupported, c.offset));
                }
                let parent_id = self.parent(parent, c)?;
                let id = self.next_native_id()?;
                let node_id = NodeId::new(
                    u64::try_from(self.scene.nodes.len())
                        .map_err(|_| resource())?
                        .checked_add(1)
                        .ok_or_else(resource)?,
                );
                let index = self.scene.nodes.len();
                self.scene.nodes.push(NativeTransformNode {
                    id: node_id,
                    transform_id: id,
                    transform_uuid: None,
                    name: self.budget.string(name).map_err(|_| resource())?,
                    channels: NativeTransform::default().values().map(Scalar::new),
                    rotate_pivot: [0.0; 3].map(Scalar::new),
                    scale_pivot: [0.0; 3].map(Scalar::new),
                    restrictions: Vec::new(),
                    unsupported_transform: Vec::new(),
                    extras: TransformExtras::default(),
                    view: GeometryViewFacts::authored(),
                });
                self.scene.dag.push(DagEdge {
                    parent: parent_id,
                    child: id,
                });
                Kind::Transform(index)
            }
            ("nurbsCurve", _) if !shared => {
                let parent_id = self
                    .parent(parent, c)?
                    .ok_or_else(|| error(ErrorKind::Unsupported, c.offset))?;
                if let Some(parent_index) = parent {
                    let owner = &mut self.owners[parent_index];
                    if owner.attributes & HAS_MESH != 0 {
                        return Err(error(ErrorKind::Unsupported, c.offset));
                    }
                    owner.attributes |= HAS_CURVE;
                }
                let id = self.next_native_id()?;
                let index = self.scene.curves.len();
                self.scene.curves.push(NativeCurveShape {
                    id,
                    uuid: None,
                    name: self.budget.string(name).map_err(|_| resource())?,
                    degree: 3,
                    control_vertices: Vec::new(),
                    knots: Vec::new(),
                    source: CurveSource {
                        owner: id,
                        statement: 0..0,
                    },
                    view: GeometryViewFacts::authored(),
                });
                self.scene.dag.push(DagEdge {
                    parent: Some(parent_id),
                    child: id,
                });
                Kind::Curve(index)
            }
            ("mesh", _) if self.meshes && !shared => {
                let parent_index = parent.ok_or_else(|| error(ErrorKind::Unsupported, c.offset))?;
                let parent_id = self
                    .parent(parent, c)?
                    .ok_or_else(|| error(ErrorKind::Unsupported, c.offset))?;
                let parent_name = self.source.get(self.owners[parent_index].name.clone());
                // A shared transform is one of the four default camera transforms.
                if parent_name
                    .is_none_or(|text| matches!(text, b"persp" | b"top" | b"front" | b"side"))
                {
                    return Err(error(ErrorKind::Unsupported, c.offset));
                }
                if self.owners[parent_index].attributes & HAS_CURVE != 0 {
                    return Err(error(ErrorKind::Unsupported, c.offset));
                }
                self.mark(parent_index, HAS_MESH, c)?;
                let id = self.next_native_id()?;
                let index = self.scene.shapes.len();
                self.scene.shapes.push(NativeMeshShape {
                    id,
                    uuid: None,
                    name: self.budget.string(name).map_err(|_| resource())?,
                    mesh: NativeMesh::from_parts(
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        Some(false),
                    )
                    .map_err(|_| error(ErrorKind::Malformed, c.offset))?,
                    restrictions: Vec::new(),
                    view: GeometryViewFacts::authored(),
                });
                self.scene.dag.push(DagEdge {
                    parent: Some(parent_id),
                    child: id,
                });
                self.mesh_parts.push(mesh::MeshParts::default());
                Kind::Mesh(index)
            }
            ("lambert" | "shadingEngine", _) if !shared && parent.is_none() => self
                .material_node(class, name, c)?
                .ok_or_else(|| error(ErrorKind::Unsupported, c.offset))?,
            ("camera", "perspShape" | "topShape" | "frontShape" | "sideShape") if shared => {
                let parent = parent.ok_or_else(|| error(ErrorKind::Unsupported, c.offset))?;
                let expected = name
                    .strip_suffix("Shape")
                    .ok_or_else(|| error(ErrorKind::Unsupported, c.offset))?;
                if !matches!(self.owners[parent].kind, Kind::Transform(_))
                    || self.source.get(self.owners[parent].name.clone())
                        != Some(expected.as_bytes())
                {
                    return Err(error(ErrorKind::Unsupported, c.offset));
                }
                Kind::Camera
            }
            ("lightLinker", "lightLinker1") if shared && parent.is_none() => Kind::LightLinker,
            ("displayLayerManager", "layerManager") if !shared && parent.is_none() => {
                Kind::LayerManager
            }
            ("displayLayer", "defaultLayer") if !shared && parent.is_none() => Kind::DisplayLayer,
            ("renderLayerManager", "renderLayerManager") if !shared && parent.is_none() => {
                Kind::RenderLayerManager
            }
            ("renderLayer", "defaultRenderLayer") if !shared && parent.is_none() => {
                Kind::RenderLayer
            }
            ("script", "uiConfigurationScriptNode" | "sceneConfigurationScriptNode")
                if !shared && parent.is_none() =>
            {
                Kind::Script
            }
            _ => return Err(error(ErrorKind::Unsupported, c.offset)),
        };
        if self.owners.len() == self.owners.capacity() {
            return Err(resource());
        }
        self.selected = Some(self.owners.len());
        self.owners.push(Owner {
            name: range,
            kind,
            attributes: 0,
            uuid: None,
            keyability: false,
        });
        Ok(())
    }
    fn parent(
        &self,
        parent: Option<usize>,
        c: &Cursor<'_, '_>,
    ) -> Result<Option<NativeId>, AsciiError> {
        parent
            .map(|index| match self.owners[index].kind {
                Kind::Transform(node) => Ok(self.scene.nodes[node].transform_id),
                _ => Err(error(ErrorKind::Unsupported, c.offset)),
            })
            .transpose()
    }
    fn select(&mut self, c: &mut Cursor<'_, '_>) -> Result<(), AsciiError> {
        if c.word()? != "-ne" {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        let token = c.next()?;
        if token.kind != TokenKind::Word {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        let raw = token.text(c.source, c.work)?;
        c.work.charge(mul(BUILTINS.len(), add(1, raw.len())?)?)?;
        let name = raw
            .strip_prefix(':')
            .filter(|name| BUILTINS[..13].contains(name))
            .ok_or_else(|| error(ErrorKind::Unsupported, c.offset))?;
        c.done()?;
        if let Some(index) = self.find(name, c.work)? {
            self.selected = Some(index);
            return Ok(());
        }
        if self.owners.len() == self.owners.capacity() {
            return Err(resource());
        }
        self.selected = Some(self.owners.len());
        self.owners.push(Owner {
            name: add(token.span.start, 1)?..token.span.end,
            kind: Kind::Builtin,
            attributes: 0,
            uuid: None,
            keyability: false,
        });
        Ok(())
    }
    fn rename(&mut self, c: &mut Cursor<'_, '_>) -> Result<(), AsciiError> {
        let owner = self.owner(c)?;
        if matches!(
            self.owners[owner].kind,
            Kind::Builtin | Kind::Lambert(_) | Kind::ShadingEngine(_)
        ) || c.word()? != "-uid"
            || self.owners[owner].uuid.is_some()
        {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        let text = c.literal()?.0;
        c.work.charge(mul(2, text.len())?)?;
        if text.len() != 36
            || !text.bytes().enumerate().all(|(i, b)| {
                if [8, 13, 18, 23].contains(&i) {
                    b == b'-'
                } else {
                    b.is_ascii_hexdigit()
                }
            })
        {
            return Err(error(ErrorKind::Malformed, c.offset));
        }
        let uid = Uuid::parse_str(text).map_err(|_| error(ErrorKind::Malformed, c.offset))?;
        c.done()?;
        for other in &self.owners {
            c.work.charge(1)?;
            if other.uuid == Some(uid) {
                return Err(error(ErrorKind::Unsupported, c.offset));
            }
        }
        match self.owners[owner].kind {
            Kind::Transform(index) => self.scene.nodes[index].transform_uuid = Some(uid),
            Kind::Curve(index) => self.scene.curves[index].uuid = Some(uid),
            Kind::Mesh(index) => self.scene.shapes[index].uuid = Some(uid),
            _ => (),
        }
        self.owners[owner].uuid = Some(uid);
        Ok(())
    }
    fn set_attr(
        &mut self,
        c: &mut Cursor<'_, '_>,
        retained: Range<usize>,
    ) -> Result<(), AsciiError> {
        let owner = self.owner(c)?;
        let flags = attr_flags(c)?;
        let attribute = flags
            .attribute
            .ok_or_else(|| error(ErrorKind::Malformed, c.offset))?;
        match self.owners[owner].kind {
            Kind::Transform(index) => {
                if !self.units {
                    return Err(error(ErrorKind::Unsupported, c.offset));
                }
                match attribute {
                    ".t" | ".r" | ".s" | ".rp" | ".sp" => {
                        flags.typed("double3", c)?;
                        let value = c.triple()?.map(Scalar::new);
                        c.done()?;
                        let bit = match attribute {
                            ".t" => 1,
                            ".r" => 2,
                            ".s" => 4,
                            ".rp" => 8,
                            _ => 16,
                        };
                        self.mark(owner, bit, c)?;
                        let node = &mut self.scene.nodes[index];
                        match attribute {
                            ".t" => node.channels[..3].clone_from_slice(&value),
                            ".r" => node.channels[3..6].clone_from_slice(&value),
                            ".s" => node.channels[6..].clone_from_slice(&value),
                            ".rp" => node.rotate_pivot = value,
                            _ => node.scale_pivot = value,
                        }
                    }
                    ".ro" => {
                        flags.plain(c)?;
                        let order = u8::try_from(c.integer()?)
                            .ok()
                            .filter(|order| *order <= 5)
                            .ok_or_else(|| error(ErrorKind::Malformed, c.offset))?;
                        c.done()?;
                        self.mark(owner, 64, c)?;
                        self.scene.nodes[index].extras.rotate_order = Some(order);
                    }
                    ".it" => {
                        flags.plain(c)?;
                        let inherits = c.boolean()?;
                        c.done()?;
                        self.mark(owner, HAS_IT, c)?;
                        self.scene.nodes[index].extras.inherits_transform = Some(inherits);
                    }
                    ".opm" => {
                        flags.typed("matrix", c)?;
                        let mut value = [0.0; 16];
                        for slot in &mut value {
                            *slot = c.number()?;
                        }
                        c.done()?;
                        self.mark(owner, HAS_OPM, c)?;
                        self.scene.nodes[index].extras.offset_parent_matrix = Some(value);
                    }
                    ".ra" | ".sh" | ".rpt" | ".spt" => {
                        flags.typed("double3", c)?;
                        let value = c.triple()?;
                        c.done()?;
                        let bit = match attribute {
                            ".ra" => 128,
                            ".sh" => 256,
                            ".rpt" => 512,
                            _ => 1024,
                        };
                        self.mark(owner, bit, c)?;
                        let extras = &mut self.scene.nodes[index].extras;
                        match attribute {
                            ".ra" => extras.rotate_axis = Some(value),
                            ".sh" => extras.shear = Some(value),
                            ".rpt" => extras.rotate_pivot_translate = Some(value),
                            _ => extras.scale_pivot_translate = Some(value),
                        }
                    }
                    ".v" => {
                        flags.plain(c)?;
                        let visible = c.boolean()?;
                        c.done()?;
                        self.mark(owner, 32, c)?;
                        self.scene.nodes[index].view.local_visibility = Some(visible);
                    }
                    _ => return Err(error(ErrorKind::Unsupported, c.offset)),
                }
            }
            Kind::Curve(index) => {
                if !self.units {
                    return Err(error(ErrorKind::Unsupported, c.offset));
                }
                match attribute {
                    ".cc" => {
                        flags.typed("nurbsCurve", c)?;
                        if self.owners[owner].attributes & 1 != 0 {
                            return Err(error(ErrorKind::Unsupported, c.offset));
                        }
                        let degree = c.count()?;
                        let spans = c.count()?;
                        let form = c.count()?;
                        let rational = c.boolean()?;
                        let dimension = c.count()?;
                        if !(1..=3).contains(&degree) || form != 0 || rational || dimension != 3 {
                            return Err(error(ErrorKind::Unsupported, c.offset));
                        }
                        let knot_count = c.count()?;
                        if knot_count > c.tokens.len().saturating_sub(c.position) {
                            return Err(error(ErrorKind::Malformed, c.offset));
                        }
                        let mut knots = self
                            .budget
                            .vector::<f64>(knot_count)
                            .map_err(|_| resource())?;
                        for _ in 0..knot_count {
                            knots.push(c.number()?);
                        }
                        let cv_count = c.count()?;
                        if cv_count <= degree
                            || cv_count.checked_sub(degree) != Some(spans)
                            || cv_count.checked_add(degree - 1) != Some(knot_count)
                            || mul(cv_count, 3)? != c.tokens.len().saturating_sub(c.position)
                        {
                            return Err(error(ErrorKind::Malformed, c.offset));
                        }
                        let mut cvs = self
                            .budget
                            .vector::<[f64; 3]>(cv_count)
                            .map_err(|_| resource())?;
                        for _ in 0..cv_count {
                            cvs.push(c.triple()?);
                        }
                        c.done()?;
                        self.mark(owner, 1, c)?;
                        self.scene.curves[index].degree = u32::try_from(degree)
                            .map_err(|_| error(ErrorKind::Malformed, c.offset))?;
                        self.scene.curves[index].knots = knots;
                        self.scene.curves[index].control_vertices = cvs;
                        self.scene.curves[index].source.statement = retained;
                    }
                    ".v" if flags.keyability => {
                        if flags.size.is_some()
                            || flags.value_type.is_some()
                            || self.owners[owner].keyability
                        {
                            return Err(error(ErrorKind::Unsupported, c.offset));
                        }
                        c.done()?;
                        self.owners[owner].keyability = true;
                    }
                    ".v" => {
                        flags.plain(c)?;
                        let visible = c.boolean()?;
                        c.done()?;
                        self.mark(owner, 2, c)?;
                        self.scene.curves[index].view.local_visibility = Some(visible);
                    }
                    _ => return Err(error(ErrorKind::Unsupported, c.offset)),
                }
            }
            Kind::Camera => {
                let bit = match attribute {
                    ".v" => {
                        if flags.size.is_some()
                            || flags.value_type.is_some()
                            || (flags.keyability && self.owners[owner].keyability)
                        {
                            return Err(error(ErrorKind::Unsupported, c.offset));
                        }
                        c.boolean()?;
                        self.owners[owner].keyability |= flags.keyability;
                        1
                    }
                    ".rnd" | ".o" => {
                        flags.plain(c)?;
                        c.boolean()?;
                        if attribute == ".rnd" { 2 } else { 4 }
                    }
                    ".fl" | ".coi" | ".ow" => {
                        flags.plain(c)?;
                        c.number()?;
                        match attribute {
                            ".fl" => 8,
                            ".coi" => 16,
                            _ => 32,
                        }
                    }
                    ".imn" | ".den" | ".man" | ".hc" => {
                        flags.typed("string", c)?;
                        c.string()?;
                        match attribute {
                            ".imn" => 64,
                            ".den" => 128,
                            ".man" => 256,
                            _ => 512,
                        }
                    }
                    _ => return Err(error(ErrorKind::Unsupported, c.offset)),
                };
                c.done()?;
                self.mark(owner, bit, c)?;
            }
            Kind::LightLinker => {
                let bit = match attribute {
                    ".lnk" => 1,
                    ".slnk" => 2,
                    _ => return Err(error(ErrorKind::Unsupported, c.offset)),
                };
                flags.allocation(c)?;
                self.mark(owner, bit, c)?;
            }
            Kind::RenderLayer if attribute == ".g" => {
                flags.plain(c)?;
                c.boolean()?;
                c.done()?;
                self.mark(owner, 1, c)?;
            }
            Kind::Script => {
                let bit = match attribute {
                    ".b" => {
                        flags.typed("string", c)?;
                        c.string_body()?;
                        1
                    }
                    ".st" => {
                        flags.plain(c)?;
                        c.integer()?;
                        2
                    }
                    _ => return Err(error(ErrorKind::Unsupported, c.offset)),
                };
                c.done()?;
                self.mark(owner, bit, c)?;
            }
            Kind::Builtin => {
                let name = self
                    .source
                    .get(self.owners[owner].name.clone())
                    .ok_or_else(|| error(ErrorKind::Malformed, c.offset))?;
                c.work.charge(name.len())?;
                let bit = match (name, attribute) {
                    (b"time1", ".o") | (b"defaultResolution", ".pa") => {
                        flags.plain(c)?;
                        c.number()?;
                        1
                    }
                    (b"time1", ".unw") => {
                        flags.plain(c)?;
                        c.integer()?;
                        2
                    }
                    (b"hardwareRenderingGlobals", ".otfna") => {
                        flags.typed("stringArray", c)?;
                        let count = c.count()?;
                        if count != c.tokens.len().saturating_sub(c.position) {
                            return Err(error(ErrorKind::Malformed, c.offset));
                        }
                        for _ in 0..count {
                            c.string()?;
                        }
                        1
                    }
                    (b"hardwareRenderingGlobals", ".otfva") => {
                        flags.typed("Int32Array", c)?;
                        let count = c.count()?;
                        if count != c.tokens.len().saturating_sub(c.position) {
                            return Err(error(ErrorKind::Malformed, c.offset));
                        }
                        for _ in 0..count {
                            c.integer()?;
                        }
                        2
                    }
                    (b"hardwareRenderingGlobals", ".fprt") => {
                        flags.plain(c)?;
                        c.boolean()?;
                        4
                    }
                    (b"renderPartition", ".st")
                    | (b"defaultShaderList1", ".s")
                    | (b"postProcessList1", ".p")
                    | (b"ikSystem", ".sol") => {
                        flags.allocation(c)?;
                        1
                    }
                    (b"initialShadingGroup" | b"initialParticleSE", ".ro") => {
                        flags.plain(c)?;
                        c.boolean()?;
                        1
                    }
                    (b"defaultRenderGlobals", ".outf") => {
                        flags.plain(c)?;
                        c.integer()?;
                        1
                    }
                    (b"hardwareRenderGlobals", ".ctrs" | ".btrs") => {
                        flags.plain(c)?;
                        c.integer()?;
                        if attribute == ".ctrs" { 1 } else { 2 }
                    }
                    _ => return Err(error(ErrorKind::Unsupported, c.offset)),
                };
                c.done()?;
                self.mark(owner, bit, c)?;
            }
            Kind::Mesh(index) => self.mesh_attribute(owner, index, &flags, c, attribute)?,
            Kind::Lambert(_) | Kind::ShadingEngine(_) => {
                self.material_attribute(owner, &flags, c, attribute)?
            }
            _ => return Err(error(ErrorKind::Unsupported, c.offset)),
        }
        Ok(())
    }
    fn metadata_owner(
        &self,
        name: &str,
        expected: Kind,
        c: &mut Cursor<'_, '_>,
    ) -> Result<(), AsciiError> {
        let owner = self
            .find(name, c.work)?
            .ok_or_else(|| error(ErrorKind::Unsupported, c.offset))?;
        if self.owners[owner].kind != expected {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        Ok(())
    }
    fn connect(&mut self, c: &mut Cursor<'_, '_>) -> Result<(), AsciiError> {
        let from = c.literal()?.0;
        let to = c.literal()?.0;
        if let (Some(shape), ":initialShadingGroup.dsm") = (from.strip_suffix(".iog"), to) {
            return self.default_link(shape, c);
        }
        let bit = match (from, to) {
            ("layerManager.dli[0]", "defaultLayer.id") => {
                self.metadata_owner("layerManager", Kind::LayerManager, c)?;
                self.metadata_owner("defaultLayer", Kind::DisplayLayer, c)?;
                1
            }
            ("renderLayerManager.rlmi[0]", "defaultRenderLayer.rlid") => {
                self.metadata_owner("renderLayerManager", Kind::RenderLayerManager, c)?;
                self.metadata_owner("defaultRenderLayer", Kind::RenderLayer, c)?;
                2
            }
            ("defaultRenderLayer.msg", ":defaultRenderingList1.r") => {
                self.metadata_owner("defaultRenderLayer", Kind::RenderLayer, c)?;
                if c.word()? != "-na" {
                    return Err(error(ErrorKind::Unsupported, c.offset));
                }
                4
            }
            _ => return Err(error(ErrorKind::Unsupported, c.offset)),
        };
        c.done()?;
        if self.connections & bit != 0 {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        self.connections |= bit;
        Ok(())
    }
    /// `connectAttr "<mesh>.iog" ":initialShadingGroup.dsm" -na;`: the shape joins the default
    /// shading group. `instObjGroups` into `dagSetMembers` is documented (Maya 2027 Nodes/objectSet
    /// and Nodes/shadingEngine pages: a DAG node's instObjGroups into dagSetMembers puts the whole
    /// object in the set; connectAttr -na: CommandsPython/connectAttr). The `-na` placement is
    /// PROVISIONAL (not compared with Maya, #5). Counted only: the read-back gate decides whether
    /// a file with links is admitted, and it is exactly as wide as the writer's output (#156).
    fn default_link(&mut self, shape: &str, c: &mut Cursor<'_, '_>) -> Result<(), AsciiError> {
        let owner = self
            .find(shape, c.work)?
            .ok_or_else(|| error(ErrorKind::Unsupported, c.offset))?;
        if !matches!(self.owners[owner].kind, Kind::Mesh(_)) || c.word()? != "-na" {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        self.mark(owner, HAS_DEFAULT_LINK, c)?;
        self.default_links = add(self.default_links, 1)?;
        Ok(())
    }
    fn relationship(&mut self, c: &mut Cursor<'_, '_>) -> Result<(), AsciiError> {
        let kind = c.literal()?.0;
        if c.literal()?.0 != ":lightLinker1" {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        self.metadata_owner("lightLinker1", Kind::LightLinker, c)?;
        let tail = c.literal()?.0;
        if c.literal()?.0 != ":defaultLightSet.message" {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        let bit = match (kind, tail) {
            ("link", ":initialShadingGroup.message") => 1,
            ("link", ":initialParticleSE.message") => 2,
            ("shadowLink", ":initialShadingGroup.message") => 4,
            ("shadowLink", ":initialParticleSE.message") => 8,
            _ => return Err(error(ErrorKind::Unsupported, c.offset)),
        };
        c.done()?;
        if self.relationships & bit != 0 {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        self.relationships |= bit;
        Ok(())
    }
    fn command(&mut self, c: &mut Cursor<'_, '_>, statement: &Statement) -> Result<(), AsciiError> {
        match c.word()? {
            "requires" => {
                if self.requires || c.word()? != "maya" {
                    return Err(error(ErrorKind::Unsupported, c.offset));
                }
                c.string()?;
                c.done()?;
                self.requires = true;
            }
            "fileInfo" => {
                let (_, key) = c.literal()?;
                c.string()?;
                c.done()?;
                let value = self
                    .source
                    .get(key.clone())
                    .ok_or_else(|| error(ErrorKind::Malformed, c.offset))?;
                for previous in &self.file_info {
                    c.work.charge(add(1, value.len())?)?;
                    let previous = self
                        .source
                        .get(previous.clone())
                        .ok_or_else(|| error(ErrorKind::Malformed, c.offset))?;
                    if literal_equal(previous, value, c.work)? {
                        return Err(error(ErrorKind::Unsupported, c.offset));
                    }
                }
                if self.file_info.len() == self.file_info.capacity() {
                    return Err(resource());
                }
                self.file_info.push(key);
            }
            "currentUnit" => {
                if self.units {
                    return Err(error(ErrorKind::Unsupported, c.offset));
                }
                let mut flags = 0;
                let mut units = Units::default();
                while c.position < c.tokens.len() {
                    let bit = match (c.word()?, c.word()?) {
                        ("-l", unit) => {
                            units.linear = match unit {
                                "millimeter" => LinearUnit::Millimeter,
                                "centimeter" => LinearUnit::Centimeter,
                                "meter" => LinearUnit::Meter,
                                "inch" => LinearUnit::Inch,
                                "foot" => LinearUnit::Foot,
                                _ => return Err(error(ErrorKind::Unsupported, c.offset)),
                            };
                            1
                        }
                        ("-a", unit) => {
                            units.angular = match unit {
                                "degree" => AngularUnit::Degree,
                                "radian" => AngularUnit::Radian,
                                _ => return Err(error(ErrorKind::Unsupported, c.offset)),
                            };
                            2
                        }
                        ("-t", unit @ ("film" | "ntsc")) => {
                            units.time = self.budget.string(unit).map_err(|_| resource())?;
                            4
                        }
                        _ => return Err(error(ErrorKind::Unsupported, c.offset)),
                    };
                    if flags & bit != 0 {
                        return Err(error(ErrorKind::Unsupported, c.offset));
                    }
                    flags |= bit;
                }
                if flags != 7 {
                    return Err(error(ErrorKind::Unsupported, c.offset));
                }
                self.scene.units = units;
                self.units = true;
            }
            "createNode" => self.create(c)?,
            "rename" => self.rename(c)?,
            "select" => self.select(c)?,
            "setAttr" => self.set_attr(c, statement.retained.clone())?,
            "connectAttr" => self.connect(c)?,
            "relationship" => self.relationship(c)?,
            _ => return Err(error(ErrorKind::Unsupported, c.offset)),
        }
        c.done()
    }
}

fn validation_admission(
    scene: &NativeScene,
    records: usize,
    budget: &mut Budget,
    work: &mut Work,
) -> Result<(), AsciiError> {
    // Covers repeated identity/highwater, scalar, source-record and CV/knot validation visits.
    const LINEAR_VISITS: usize = 32;
    let transforms = scene.nodes.len();
    let identities = add(add(transforms, scene.curves.len())?, scene.shapes.len())?;
    let dag = scene.dag.len();
    let scratch =
        super::admission::validation_bytes(transforms, identities, dag).map_err(|_| resource())?;
    budget.charge(scratch).map_err(|_| resource())?;
    let mut linear = add(add(identities, dag)?, records)?;
    for curve in &scene.curves {
        linear = add(
            linear,
            add(mul(3, curve.control_vertices.len())?, curve.knots.len())?,
        )?;
    }
    work.charge(add(
        add(
            mul(mul(2, identities)?, dag)?,
            mul(scene.curves.len(), records)?,
        )?,
        mul(LINEAR_VISITS, add(linear, mul(15, transforms)?)?)?,
    )?)
}

pub fn decode(source: Arc<[u8]>) -> Result<NativeScene, AsciiError> {
    decode_inner(source, false)
}

/// Test helper: decode that admits `createNode mesh` without the read-back gate. Production
/// callers go through `decode_document`.
#[cfg(test)]
pub(super) fn decode_admitting_meshes(source: Arc<[u8]>) -> Result<NativeScene, AsciiError> {
    decode_inner(source, true)
}

fn decode_inner(source: Arc<[u8]>, meshes: bool) -> Result<NativeScene, AsciiError> {
    decode_with_uvs(source, meshes).map(|decoded| decoded.scene)
}

/// What the gate's decode read besides the sourced scene. A sourced scene cannot hold UVs or
/// session materials, so the caller attaches them to the source-free candidate.
struct Decoded {
    scene: NativeScene,
    /// Each shape's UV set, parallel to the scene's shapes.
    uvs: Vec<Option<UvSet>>,
    /// How many default shading-group links were read.
    links: usize,
    /// The material statements, `None` when the file has none.
    materials: Option<material::ParsedMaterials>,
}

fn decode_with_uvs(source: Arc<[u8]>, meshes: bool) -> Result<Decoded, AsciiError> {
    let lex::Records {
        source,
        statements,
        tokens,
        trailing,
        mut budget,
        mut work,
    } = lex::inspect(source)?;
    let (mut owners, mut transforms, mut curves, mut shapes, mut file_info) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    let (mut lamberts, mut engines) = (0usize, 0usize);
    for statement in &statements {
        let mut c = Cursor {
            source: &source,
            tokens: tokens
                .get(statement.tokens.clone())
                .ok_or_else(|| error(ErrorKind::Malformed, statement.command.start))?,
            position: 0,
            offset: statement.command.start,
            work: &mut work,
        };
        match c.word()? {
            "createNode" => {
                owners = add(owners, 1)?;
                match c.word()? {
                    "transform" => transforms = add(transforms, 1)?,
                    "nurbsCurve" => curves = add(curves, 1)?,
                    "mesh" => shapes = add(shapes, 1)?,
                    "lambert" if meshes => lamberts = add(lamberts, 1)?,
                    "shadingEngine" if meshes => engines = add(engines, 1)?,
                    _ => (),
                }
            }
            "fileInfo" => file_info = add(file_info, 1)?,
            _ => (),
        }
    }
    let dag = add(add(transforms, curves)?, shapes)?;
    let records = add(statements.len(), usize::from(!trailing.is_empty()))?;
    let mut retention = SourceRetention {
        has_removals: false,
        sites: Vec::new(),
        bytes: source.clone(),
        format: SourceFormat::Ascii,
        records: budget.vector(records).map_err(|_| resource())?,
        leaves: Vec::new(),
        appearance: None,
    };
    work.charge(records)?;
    for statement in &statements {
        retention.records.push(statement.retained.clone());
    }
    if !trailing.is_empty() {
        retention.records.push(trailing);
    }
    let scene = NativeScene {
        nodes: budget.vector(transforms).map_err(|_| resource())?,
        curves: budget.vector(curves).map_err(|_| resource())?,
        shapes: budget.vector(shapes).map_err(|_| resource())?,
        dag: budget.vector(dag).map_err(|_| resource())?,
        source: Some(retention),
        units: Units::default(),
        ..NativeScene::default()
    };
    let owner_table = budget
        .vector(add(owners, BUILTINS.len())?)
        .map_err(|_| resource())?;
    let info_table = budget.vector(file_info).map_err(|_| resource())?;
    let mesh_parts = budget.vector(shapes).map_err(|_| resource())?;
    let materials = material::MaterialParts::with_capacity(&mut budget, lamberts, engines)?;
    let mut builder = Builder {
        source: &source,
        budget: &mut budget,
        scene,
        owners: owner_table,
        selected: None,
        units: false,
        meshes,
        mesh_parts,
        materials,
        requires: false,
        file_info: info_table,
        connections: 0,
        relationships: 0,
        default_links: 0,
    };
    for statement in &statements {
        let mut c = Cursor {
            source: &source,
            tokens: tokens
                .get(statement.tokens.clone())
                .ok_or_else(|| error(ErrorKind::Malformed, statement.command.start))?,
            position: 0,
            offset: statement.command.start,
            work: &mut work,
        };
        builder.command(&mut c, statement)?;
    }
    if !builder.units {
        return Err(error(ErrorKind::Unsupported, 0));
    }
    // Whether `.cc` CVs are written in the declared linear unit is unverified against the Maya
    // reference (#5); curves are refused rather than guessed. Meshes never reach here with
    // other units: `decode_document` requires writer-identical bytes (centimeter/degree/film).
    if builder.scene.units.linear != LinearUnit::Centimeter && !builder.scene.curves.is_empty() {
        return Err(error(ErrorKind::Unsupported, 0));
    }
    let uvs = builder.finish_meshes()?;
    let materials = builder.finish_materials()?;
    validation_admission(&builder.scene, records, builder.budget, &mut work)?;
    let scene = NativeScene::from_source_candidate(builder.scene).map_err(|e| match e {
        NativeError::ResourceLimit => resource(),
        NativeError::UnsupportedView(_) => AsciiError {
            kind: ErrorKind::Unsupported,
            offset: None,
        },
        _ => AsciiError {
            kind: ErrorKind::Malformed,
            offset: None,
        },
    })?;
    Ok(Decoded {
        scene,
        uvs,
        links: builder.default_links,
        materials,
    })
}

#[cfg(test)]
mod degree_tests;
mod document;
mod material;
mod mesh;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod units_tests;
pub(crate) mod write;
#[cfg(test)]
pub(super) use write::{AsciiWriteRefusal, emit_geometry_prefix};
