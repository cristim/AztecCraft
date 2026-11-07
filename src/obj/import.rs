//! Streaming Wavefront OBJ reader: pure data in, pure data out, no scene types.
//!
//! Every limit below is PROVISIONAL (Maya help is silent) and listed in the "Application policy
//! constants" table in `.project-docs/development.md`. Errors carry a keyword class and a
//! 1-based line number only, never file text.

mod error;
pub use error::{Limit, ObjImportError, SyntaxKind, UnsupportedKind};

use std::{
    io::{self, BufRead, BufReader, Read},
    path::Path,
};

pub const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_LINE_BYTES: usize = 64 * 1024;
pub const MAX_VERTICES: usize = 262_144;
pub const MAX_FACES: usize = 262_144;
pub const MAX_CORNERS: usize = 1_048_576;
pub const MAX_CORNERS_PER_FACE: usize = 1024;
pub const MAX_OBJECTS: usize = 1024;
pub const MAX_NAME_BYTES: usize = 256;
/// Largest accepted coordinate magnitude, chosen so f32 rounding and world math stay finite.
pub const MAX_COORD: f64 = 1e12;

/// Records that are recognised, counted and dropped. Normals and UVs are not imported.
const IGNORED: &[&str] = &[
    "vp",
    "usemtl",
    "mtllib",
    "s",
    "l",
    "p",
    "mg",
    "maplib",
    "usemap",
    "bevel",
    "c_interp",
    "d_interp",
    "lod",
    "shadow_obj",
    "trace_obj",
    "ctech",
    "stech",
];
/// Free-form geometry: refused because dropping it would silently lose shapes.
const FREE_FORM: &[&str] = &[
    "cstype", "deg", "curv", "curv2", "surf", "parm", "trim", "hole", "scrv", "sp", "end", "con",
    "step", "bmat",
];

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedObj {
    pub vertices: Vec<[f32; 3]>,
    /// Objects that own at least one face, in file order. An empty name means the file gave none.
    pub objects: Vec<ParsedObject>,
    pub ignored_records: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedObject {
    pub name: String,
    /// Global 0-based vertex indices, each face with distinct corners.
    pub faces: Vec<Vec<u32>>,
}

/// Reads and parses a regular `.obj` file. Empty object names (no `o`/`g`, or a bare one) take the
/// file stem.
pub fn read(path: &Path) -> Result<ParsedObj, ObjImportError> {
    // `open_regular_file` reports a non-regular file as `InvalidInput`.
    let (file, _) = crate::document::open_regular_file(path).map_err(|error| {
        if error.kind() == io::ErrorKind::InvalidInput {
            ObjImportError::NotRegularFile
        } else {
            ObjImportError::Io(error.kind())
        }
    })?;
    let mut parsed = parse(BufReader::new(file))?;
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = truncate_to_boundary(&stem, MAX_NAME_BYTES);
    for object in &mut parsed.objects {
        if object.name.is_empty() {
            object.name = stem.to_owned();
        }
    }
    Ok(parsed)
}

fn truncate_to_boundary(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(..end).unwrap_or_default()
}

#[derive(Default)]
struct State {
    vertices: Vec<[f32; 3]>,
    objects: Vec<ParsedObject>,
    current: Option<ParsedObject>,
    texcoords: usize,
    normals: usize,
    faces: usize,
    corners: usize,
    ignored: usize,
    scratch: Vec<u32>,
}

pub fn parse(reader: impl BufRead) -> Result<ParsedObj, ObjImportError> {
    let mut reader = reader.take(MAX_FILE_BYTES + 1);
    let mut state = State::default();
    let mut logical: Vec<u8> = Vec::new();
    let mut raw: Vec<u8> = Vec::new();
    let mut physical = 0usize;
    let mut total = 0u64;
    loop {
        logical.clear();
        let line = physical.saturating_add(1);
        let mut any = false;
        loop {
            raw.clear();
            let read = (&mut reader)
                .take(MAX_LINE_BYTES as u64 + 2)
                .read_until(b'\n', &mut raw)
                .map_err(|error| ObjImportError::Io(error.kind()))?;
            if read == 0 {
                break;
            }
            any = true;
            physical = physical.saturating_add(1);
            total = total.saturating_add(read as u64);
            if total > MAX_FILE_BYTES {
                return Err(ObjImportError::TooLarge);
            }
            // A chunk holds at most MAX_LINE_BYTES of content plus CRLF. CR ends a line only as
            // part of CRLF; a bare CR (classic Mac files) is refused.
            let newline = raw.last() == Some(&b'\n');
            if newline {
                raw.pop();
                if raw.last() == Some(&b'\r') {
                    raw.pop();
                }
            }
            // Any CR left is bare, except a last byte that may begin a CRLF in the next chunk.
            let scanned = match raw.split_last() {
                Some((b'\r', rest)) if !newline => rest,
                _ => raw.as_slice(),
            };
            if scanned.contains(&b'\r') {
                return Err(ObjImportError::CrOnlyLineEndings { line: physical });
            }
            if physical == 1 && raw.starts_with(&[0xEF, 0xBB, 0xBF]) {
                raw.drain(..3);
            }
            let continued = raw.last() == Some(&b'\\');
            if continued {
                raw.pop();
            }
            if logical.len().saturating_add(raw.len()) > MAX_LINE_BYTES {
                return Err(ObjImportError::LineTooLong { line });
            }
            logical.extend_from_slice(&raw);
            if !continued {
                break;
            }
        }
        if !any {
            break;
        }
        let text =
            std::str::from_utf8(&logical).map_err(|_| ObjImportError::InvalidUtf8 { line })?;
        state.record(text, line)?;
    }
    state.finish()
}

impl State {
    fn record(&mut self, text: &str, line: usize) -> Result<(), ObjImportError> {
        let text = text.split('#').next().unwrap_or_default();
        let mut tokens = text.split_ascii_whitespace();
        let Some(keyword) = tokens.next() else {
            return Ok(());
        };
        match keyword {
            "v" => self.vertex(tokens, line),
            "f" => self.face(tokens, line),
            "o" => self.group(tokens.next().into_iter().collect(), line),
            "g" => self.group(tokens.collect(), line),
            "vt" => {
                self.texcoords = self.texcoords.saturating_add(1);
                self.ignored = self.ignored.saturating_add(1);
                Ok(())
            }
            "vn" => {
                self.normals = self.normals.saturating_add(1);
                self.ignored = self.ignored.saturating_add(1);
                Ok(())
            }
            other if IGNORED.contains(&other) => {
                self.ignored = self.ignored.saturating_add(1);
                Ok(())
            }
            other => Err(ObjImportError::Unsupported {
                line,
                what: if FREE_FORM.contains(&other) {
                    UnsupportedKind::FreeForm
                } else {
                    UnsupportedKind::UnknownKeyword
                },
            }),
        }
    }

    fn vertex<'a>(
        &mut self,
        mut tokens: impl Iterator<Item = &'a str>,
        line: usize,
    ) -> Result<(), ObjImportError> {
        let mut point = [0f32; 3];
        for slot in &mut point {
            let token = tokens.next().ok_or(ObjImportError::Syntax {
                line,
                what: SyntaxKind::VertexNeedsThreeNumbers,
            })?;
            *slot = coordinate(token, line)?;
        }
        // An optional w or a vertex colour: extra numbers are validated, then dropped.
        for token in tokens {
            coordinate(token, line)?;
        }
        if self.vertices.len() >= MAX_VERTICES {
            return Err(ObjImportError::LimitExceeded(Limit::Vertices));
        }
        reserve(&mut self.vertices, 1)?;
        self.vertices.push(point);
        Ok(())
    }

    fn group(&mut self, names: Vec<&str>, line: usize) -> Result<(), ObjImportError> {
        if names.len() > 1 {
            return Err(ObjImportError::Unsupported {
                line,
                what: UnsupportedKind::OverlappingGroups,
            });
        }
        let name = names.first().copied().unwrap_or_default();
        if name.len() > MAX_NAME_BYTES {
            return Err(ObjImportError::LimitExceeded(Limit::NameBytes));
        }
        self.close_current()?;
        self.current = Some(ParsedObject {
            name: name.to_owned(),
            faces: Vec::new(),
        });
        Ok(())
    }

    fn close_current(&mut self) -> Result<(), ObjImportError> {
        let Some(object) = self.current.take() else {
            return Ok(());
        };
        if object.faces.is_empty() {
            return Ok(());
        }
        if self.objects.len() >= MAX_OBJECTS {
            return Err(ObjImportError::LimitExceeded(Limit::Objects));
        }
        reserve(&mut self.objects, 1)?;
        self.objects.push(object);
        Ok(())
    }

    fn face<'a>(
        &mut self,
        tokens: impl Iterator<Item = &'a str>,
        line: usize,
    ) -> Result<(), ObjImportError> {
        let tokens: Vec<&str> = tokens.collect();
        if tokens.len() < 3 {
            return Err(ObjImportError::DegenerateFace { line });
        }
        if tokens.len() > MAX_CORNERS_PER_FACE {
            return Err(ObjImportError::LimitExceeded(Limit::CornersPerFace));
        }
        if self.faces >= MAX_FACES {
            return Err(ObjImportError::LimitExceeded(Limit::Faces));
        }
        // Both sums are bounded by the caps just checked.
        if self.corners + tokens.len() > MAX_CORNERS {
            return Err(ObjImportError::LimitExceeded(Limit::Corners));
        }
        let mut face: Vec<u32> = Vec::new();
        reserve(&mut face, tokens.len())?;
        for token in tokens {
            face.push(self.corner(token, line)?);
        }
        self.scratch.clear();
        reserve(&mut self.scratch, face.len())?;
        self.scratch.extend_from_slice(&face);
        self.scratch.sort_unstable();
        if self
            .scratch
            .windows(2)
            .any(|pair| matches!(pair, [a, b] if a == b))
        {
            return Err(ObjImportError::DegenerateFace { line });
        }
        let object = self.current.get_or_insert_with(|| ParsedObject {
            name: String::new(),
            faces: Vec::new(),
        });
        reserve(&mut object.faces, 1)?;
        object.faces.push(face);
        self.faces += 1;
        self.corners += self.scratch.len();
        Ok(())
    }

    /// Parses `v`, `v/vt`, `v//vn` or `v/vt/vn`; returns the 0-based vertex index. The `vt` and
    /// `vn` indices are range-checked and dropped.
    fn corner(&self, token: &str, line: usize) -> Result<u32, ObjImportError> {
        let bad = ObjImportError::Syntax {
            line,
            what: SyntaxKind::BadFaceCorner,
        };
        let mut parts = token.split('/');
        let vertex = parts.next().unwrap_or_default();
        let texcoord = parts.next();
        let normal = parts.next();
        if parts.next().is_some() {
            return Err(bad);
        }
        match (texcoord, normal) {
            (Some(""), None) | (_, Some("")) => return Err(bad),
            _ => {}
        }
        let resolved = resolve(vertex, self.vertices.len(), line)?;
        if let Some(texcoord) = texcoord.filter(|text| !text.is_empty()) {
            resolve(texcoord, self.texcoords, line)?;
        }
        if let Some(normal) = normal {
            resolve(normal, self.normals, line)?;
        }
        u32::try_from(resolved).map_err(|_| ObjImportError::LimitExceeded(Limit::Vertices))
    }

    fn finish(mut self) -> Result<ParsedObj, ObjImportError> {
        self.close_current()?;
        if self.objects.is_empty() {
            return Err(ObjImportError::NoFaces);
        }
        Ok(ParsedObj {
            vertices: self.vertices,
            objects: self.objects,
            ignored_records: self.ignored,
        })
    }
}

fn reserve<T>(vec: &mut Vec<T>, additional: usize) -> Result<(), ObjImportError> {
    vec.try_reserve(additional)
        .map_err(|_| ObjImportError::LimitExceeded(Limit::Allocation))
}

fn coordinate(token: &str, line: usize) -> Result<f32, ObjImportError> {
    let value: f64 = token.parse().map_err(|_| ObjImportError::Syntax {
        line,
        what: SyntaxKind::BadNumber,
    })?;
    if !value.is_finite() || value.abs() > MAX_COORD {
        return Err(ObjImportError::CoordinateOutOfRange { line });
    }
    // |value| <= MAX_COORD is far below f32::MAX, so the cast stays finite; -0.0 is kept.
    Ok(value as f32)
}

/// Resolves a 1-based or negative (relative) OBJ index against the `count` records seen so far.
fn resolve(token: &str, count: usize, line: usize) -> Result<usize, ObjImportError> {
    if token.is_empty() || token.starts_with('+') {
        return Err(ObjImportError::Syntax {
            line,
            what: SyntaxKind::BadFaceCorner,
        });
    }
    let index: i64 = token.parse().map_err(|error: std::num::ParseIntError| {
        use std::num::IntErrorKind::{NegOverflow, PosOverflow};
        match error.kind() {
            PosOverflow | NegOverflow => ObjImportError::IndexOutOfRange { line },
            _ => ObjImportError::Syntax {
                line,
                what: SyntaxKind::BadFaceCorner,
            },
        }
    })?;
    let resolved = if index > 0 {
        usize::try_from(index)
            .ok()
            .and_then(|index| index.checked_sub(1))
    } else if index < 0 {
        usize::try_from(index.unsigned_abs())
            .ok()
            .and_then(|back| count.checked_sub(back))
    } else {
        None
    };
    resolved
        .filter(|resolved| *resolved < count)
        .ok_or(ObjImportError::IndexOutOfRange { line })
}

#[cfg(test)]
mod tests;
