// Independent oracle for Maya ASCII written by AztecCraft. Shares no code with src/.
use std::collections::{BTreeMap, BTreeSet};

pub struct Mesh {
    pub points: Vec<[f32; 3]>,
    pub edges: Vec<[usize; 2]>,
    pub faces: Vec<Vec<i64>>,
}
pub type Scene = BTreeMap<String, Mesh>;

/// Signed face token: `n >= 0` is edge n forward, `n < 0` is edge `-n-1` reversed
/// (never `abs`). `!n` equals `-n-1` for every negative n, including `i64::MIN`.
fn decode(token: i64) -> (i64, bool) {
    if token >= 0 {
        (token, false)
    } else {
        (!token, true)
    }
}

impl Mesh {
    fn directed(&self, token: i64) -> Result<(usize, usize), String> {
        let (id, flip) = decode(token);
        let edge = usize::try_from(id).ok().and_then(|id| self.edges.get(id));
        let [a, b] = *edge.ok_or(format!("face edge id {token} out of range"))?;
        Ok(if flip { (b, a) } else { (a, b) })
    }

    /// Vertex sequence of every face: the oriented start of each directed edge.
    pub fn loops(&self) -> Result<Vec<Vec<usize>>, String> {
        let mut used = BTreeSet::new();
        let mut loops = Vec::new();
        for (index, face) in self.faces.iter().enumerate() {
            let dirs = face
                .iter()
                .map(|t| self.directed(*t))
                .collect::<Result<Vec<_>, _>>()?;
            if dirs.len() < 3 {
                return Err(format!("face {index} has fewer than 3 edges"));
            }
            for (i, (_, end)) in dirs.iter().enumerate() {
                if dirs[(i + 1) % dirs.len()].0 != *end {
                    return Err(format!("face {index} loop does not close at edge {i}"));
                }
            }
            for dir in &dirs {
                if !used.insert(*dir) {
                    return Err(format!("directed edge {dir:?} used twice (face {index})"));
                }
            }
            loops.push(dirs.iter().map(|d| d.0).collect());
        }
        Ok(loops)
    }

    /// Closed: every edge is used once in each direction.
    /// Open: every edge is used by some face.
    pub fn check(&self, closed: bool) -> Result<(), String> {
        self.loops()?;
        let used: BTreeSet<_> = self
            .faces
            .iter()
            .flatten()
            .map(|t| self.directed(*t))
            .collect::<Result<_, _>>()?;
        let bad = self.edges.iter().find(|[a, b]| {
            let (fwd, rev) = (used.contains(&(*a, *b)), used.contains(&(*b, *a)));
            if closed { !(fwd && rev) } else { !(fwd || rev) }
        });
        match bad {
            None => Ok(()),
            Some(e) => Err(format!(
                "edge {e:?} violates the {} rule",
                if closed { "closed" } else { "open" }
            )),
        }
    }

    /// Divergence-theorem volume from fan triangulation; positive means outward winding.
    pub fn signed_volume(&self) -> Result<f64, String> {
        let p = |i: usize| self.points[i].map(f64::from);
        let mut six = 0.0;
        for face in self.loops()? {
            for k in 1..face.len() - 1 {
                let (a, b, c) = (p(face[0]), p(face[k]), p(face[k + 1]));
                six += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                    + a[2] * (b[0] * c[1] - b[1] * c[0]);
            }
        }
        Ok(six / 6.0)
    }
}

struct Tok {
    text: String,
    quoted: bool,
}

fn statements(text: &str) -> Result<Vec<Vec<Tok>>, String> {
    let (mut all, mut cur, mut word) = (Vec::new(), Vec::new(), String::new());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let comment = c == '/' && chars.peek() == Some(&'/');
        let quote = c == '"';
        if (quote || comment || c == ';' || c.is_whitespace()) && !word.is_empty() {
            cur.push(Tok {
                text: std::mem::take(&mut word),
                quoted: false,
            });
        }
        if quote {
            let mut s = String::new();
            loop {
                match chars.next().ok_or("unterminated string")? {
                    '"' => break,
                    '\\' => s.push(chars.next().ok_or("unterminated string")?),
                    c => s.push(c),
                }
            }
            cur.push(Tok {
                text: s,
                quoted: true,
            });
        } else if c == ';' {
            all.push(std::mem::take(&mut cur));
        } else if comment {
            chars.by_ref().find(|c| *c == '\n');
        } else if !c.is_whitespace() {
            word.push(c);
        }
    }
    if cur.is_empty() && word.is_empty() {
        Ok(all)
    } else {
        Err("statement not terminated by ';'".into())
    }
}

fn num<T: std::str::FromStr>(t: &str, what: &str) -> Result<T, String> {
    t.parse().map_err(|_| format!("{what}: bad number {t:?}"))
}

/// Parses `vals` as finite numbers (f32 text for `.vt`, f64 for channels and colours).
fn finite<T: std::str::FromStr + Copy + Into<f64>>(
    vals: &[&str],
    what: &str,
) -> Result<Vec<T>, String> {
    let one = |v: &&str| {
        let n = num::<T>(v, what)?;
        if n.into().is_finite() {
            Ok(n)
        } else {
            Err(format!("{what}: {v:?} is not finite"))
        }
    };
    vals.iter().map(one).collect()
}

struct SetAttr<'a> {
    size: Option<usize>,
    kind: Option<&'a str>,
    base: &'a str,
    range: Option<(usize, usize)>,
    values: Vec<&'a str>,
}

fn parse_set_attr(toks: &[Tok]) -> Result<SetAttr<'_>, String> {
    let (mut size, mut kind, mut attr, mut values) = (None, None, None, Vec::new());
    let mut it = toks.iter();
    while let Some(t) = it.next() {
        match (t.quoted, t.text.as_str()) {
            (false, "-s") if values.is_empty() => {
                size = Some(num(&it.next().ok_or("-s without size")?.text, "-s")?)
            }
            (false, "-type") if values.is_empty() => {
                kind = Some(it.next().ok_or("-type without name")?.text.as_str())
            }
            (true, a) if attr.is_none() && a.starts_with('.') => attr = Some(&a[1..]),
            (_, v) if attr.is_some() => values.push(v),
            (_, other) => return Err(format!("unexpected setAttr token {other:?}")),
        }
    }
    let attr = attr.ok_or("setAttr without attribute")?;
    let (base, range) = match attr.split_once('[') {
        None => (attr, None),
        Some((base, r)) => {
            let (lo, hi) = r
                .strip_suffix(']')
                .and_then(|r| r.split_once(':'))
                .ok_or("bad range")?;
            (base, Some((num(lo, "range")?, num(hi, "range")?)))
        }
    };
    Ok(SetAttr {
        size,
        kind,
        base,
        range,
        values,
    })
}

/// `-s N`, `[0:N-1]` and the value count must agree; returns N.
fn array_len(a: &SetAttr, per: Option<usize>) -> Result<usize, String> {
    let n = a.size.ok_or("array setAttr without -s")?;
    if n == 0 || a.range != Some((0, n - 1)) {
        return Err(format!(
            "{}: range {:?} does not match size {n}",
            a.base, a.range
        ));
    }
    match per {
        Some(per) if per.checked_mul(n) != Some(a.values.len()) => Err(format!(
            "{}: size {n} needs {per} values each, found {}",
            a.base,
            a.values.len()
        )),
        _ => Ok(n),
    }
}

fn faces(a: &SetAttr) -> Result<Vec<Vec<i64>>, String> {
    let (mut faces, mut it) = (Vec::new(), a.values.iter());
    while let Some(tag) = it.next() {
        let count: usize = num(it.next().ok_or("f without count")?, "f")?;
        if *tag != "f" {
            return Err(format!("unsupported polyFaces record {tag:?}"));
        }
        let ids = it
            .by_ref()
            .take(count)
            .map(|t| num::<i64>(t, "face id"))
            .collect::<Result<Vec<_>, _>>()?;
        if ids.len() != count {
            return Err("face record count mismatch".into());
        }
        faces.push(ids);
    }
    if faces.len() != array_len(a, None)? {
        return Err(format!(
            "fc size does not match its {} f records",
            faces.len()
        ));
    }
    Ok(faces)
}

/// `rename -uid` takes an 8-4-4-4-12 hexadecimal UUID.
fn uuid_ok(uid: &str) -> bool {
    uid.split('-').map(str::len).eq([8, 4, 4, 4, 12])
        && uid.chars().all(|c| c == '-' || c.is_ascii_hexdigit())
}

/// The first three statements are fixed: requires, currentUnit, then the marker.
fn check_header(statements: &[Vec<Tok>]) -> Result<(), String> {
    let text = |i: usize| -> Vec<&str> {
        let st = statements.get(i).map_or(&[][..], |s| &s[..]);
        st.iter().map(|t| t.text.as_str()).collect()
    };
    let units = [
        "currentUnit",
        "-l",
        "centimeter",
        "-a",
        "degree",
        "-t",
        "film",
    ];
    let ok = text(0).len() == 3
        && text(0)[..2] == ["requires", "maya"]
        && text(1) == units
        && text(2) == ["fileInfo", "AztecCraft", "1"];
    if ok {
        Ok(())
    } else {
        Err("header must be requires, currentUnit, fileInfo marker".into())
    }
}

fn create_node(args: &[Tok], types: &BTreeMap<String, String>) -> Result<(String, String), String> {
    let [ty, flag, node, rest @ ..] = args else {
        return Err("short createNode".into());
    };
    let known = matches!(
        ty.text.as_str(),
        "transform" | "mesh" | "lambert" | "shadingEngine"
    );
    if flag.text != "-n" || !known || node.text.is_empty() || types.contains_key(&node.text) {
        return Err(format!(
            "unsupported or duplicate createNode {:?} {:?}",
            ty.text, node.text
        ));
    }
    let parent = match rest {
        [] => None,
        [p, parent] if p.text == "-p" => Some(parent.text.as_str()),
        _ => return Err("bad createNode arguments".into()),
    };
    match (ty.text.as_str(), parent) {
        ("mesh", Some(p)) => match types.get(p).map(String::as_str) {
            Some("transform") => {}
            Some(_) => return Err(format!("mesh {} needs a transform parent", node.text)),
            None => return Err(format!("parent {p:?} not created yet")),
        },
        ("mesh", None) => return Err(format!("mesh {} needs a transform parent", node.text)),
        (_, Some(p)) => return Err(format!("{} must not have parent {p:?}", ty.text)),
        (_, None) => {}
    }
    Ok((ty.text.clone(), node.text.clone()))
}

pub fn validate(text: &str) -> Result<Scene, String> {
    let all = statements(text)?;
    check_header(&all)?;
    let mut types: BTreeMap<String, String> = BTreeMap::new();
    let mut meshes: BTreeMap<String, Mesh> = BTreeMap::new();
    let (mut current, mut connected, mut seen) = (String::new(), false, BTreeSet::new());
    for st in &all[3..] {
        let name = st.first().map_or("", |t| t.text.as_str());
        let args = &st[1.min(st.len())..];
        let banned = ["python", "eval", "scriptNode", "-command"];
        let script =
            |t: &Tok| banned.contains(&t.text.trim_end_matches(|c: char| !c.is_alphanumeric()));
        if st.iter().any(script) {
            return Err(format!("script-capable token in {name:?}"));
        }
        match name {
            "createNode" => {
                if connected {
                    return Err("createNode after connectAttr".into());
                }
                let (ty, node) = create_node(args, &types)?;
                if ty == "mesh" {
                    meshes.insert(
                        node.clone(),
                        Mesh {
                            points: vec![],
                            edges: vec![],
                            faces: vec![],
                        },
                    );
                }
                types.insert(node.clone(), ty);
                current = node;
            }
            "rename" | "setAttr" if current.is_empty() => {
                return Err(format!("{name} before any createNode"));
            }
            "rename" => {
                let uid = args.get(1).map_or("", |t| t.text.as_str());
                if args.len() != 2 || args[0].text != "-uid" || !uuid_ok(uid) {
                    return Err("bad rename -uid".into());
                }
            }
            "setAttr" => {
                let a = parse_set_attr(args)?;
                if !seen.insert((current.clone(), a.base.to_string())) {
                    return Err(format!("{current}.{} set twice", a.base));
                }
                let ty = types[&current].as_str();
                match (ty, a.base, a.kind) {
                    ("transform", "t" | "r" | "s", Some("double3"))
                        if a.size.is_none() && a.values.len() == 3 =>
                    {
                        finite::<f64>(&a.values, "channel")?;
                    }
                    ("lambert", "c", Some("float3")) if a.size.is_none() && a.values.len() == 3 => {
                        finite::<f64>(&a.values, "colour")?;
                    }
                    ("mesh", "vt", None) => {
                        array_len(&a, Some(3))?;
                        let flat = finite::<f32>(&a.values, "vt")?;
                        let mesh = meshes.get_mut(&current).ok_or("mesh")?;
                        mesh.points = flat.chunks(3).map(|p| [p[0], p[1], p[2]]).collect();
                    }
                    ("mesh", "ed", None) => {
                        array_len(&a, Some(3))?;
                        let mesh = meshes.get_mut(&current).ok_or("mesh")?;
                        for e in a.values.chunks(3) {
                            let ends = [num::<usize>(e[0], "ed")?, num::<usize>(e[1], "ed")?];
                            if ends[0] == ends[1] {
                                return Err(format!("edge {ends:?} is a self-loop"));
                            }
                            if ends.iter().any(|v| *v >= mesh.points.len()) {
                                return Err(format!(
                                    "edge endpoint {ends:?} invalid for {} vertices",
                                    mesh.points.len()
                                ));
                            }
                            if e[2] != "0" && e[2] != "1" {
                                return Err(format!("edge smooth flag {:?} is not 0 or 1", e[2]));
                            }
                            mesh.edges.push(ends);
                        }
                    }
                    ("mesh", "fc", Some("polyFaces")) => {
                        let mesh = meshes.get_mut(&current).ok_or("mesh")?;
                        mesh.faces = faces(&a)?;
                    }
                    _ => return Err(format!("unsupported attribute {ty}.{}", a.base)),
                }
            }
            "connectAttr" => {
                connected = true;
                let [src, dst, rest @ ..] = args else {
                    return Err("short connectAttr".into());
                };
                let end = |t: &Tok| -> Result<(String, String), String> {
                    let (node, attr) =
                        t.text.split_once('.').ok_or("endpoint without attribute")?;
                    match types.get(node) {
                        Some(ty) => Ok((ty.clone(), attr.into())),
                        None if node == ":initialShadingGroup" => {
                            Ok(("shadingEngine".into(), attr.into()))
                        }
                        None => Err(format!("connection names unknown node {node:?}")),
                    }
                };
                let (s, d) = (end(src)?, end(dst)?);
                let na = rest.len() == 1 && rest[0].text == "-na";
                match (s.0.as_str(), s.1.as_str(), d.0.as_str(), d.1.as_str()) {
                    ("lambert", "oc", "shadingEngine", "ss") if rest.is_empty() => {}
                    ("mesh", "iog", "shadingEngine", "dsm") if na => {}
                    _ => {
                        return Err(format!(
                            "unsupported connection {} -> {}",
                            src.text, dst.text
                        ));
                    }
                }
            }
            other => return Err(format!("command {other:?} is not in the whitelist")),
        }
    }
    for (name, mesh) in &meshes {
        if mesh.points.is_empty() || mesh.edges.is_empty() || mesh.faces.is_empty() {
            return Err(format!("mesh {name} lacks vt, ed or fc"));
        }
        mesh.loops()?;
    }
    Ok(meshes)
}
