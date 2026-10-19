#[path = "roster.rs"]
mod roster;

use roster::{Kind, name, word};
pub use roster::{Token, inspect_header};

pub struct Declarations {
    pub boundary: usize,
    pub shapes: Vec<String>,
    pub materials: Vec<(String, String, [String; 3])>,
    pub connections: Vec<Vec<Token>>,
}

pub fn inspect_declarations(bytes: &[u8]) -> Result<Declarations, String> {
    let roster::Roster {
        boundary,
        shapes,
        mut declared,
        tail,
    } = roster::inspect(bytes)?;
    let mut report = Declarations {
        boundary,
        shapes,
        materials: Vec::new(),
        connections: Vec::new(),
    };
    let mut pending: Option<(String, Option<[String; 3]>)> = None;
    let mut connections = false;
    for (kind, tokens) in tail {
        if connections && kind != Kind::Connection {
            return Err("declaration after connection".into());
        }
        match kind {
            Kind::Lambert => {
                let [command, node_type, flag, node] = tokens.as_slice() else {
                    return Err("Lambert declaration".into());
                };
                if pending.is_some()
                    || !word(command, "createNode")
                    || !word(node_type, "lambert")
                    || !word(flag, "-n")
                    || !node.quoted
                    || !name(&node.text)
                    || !declared.insert(node.text.clone())
                {
                    return Err("Lambert name/order".into());
                }
                pending = Some((node.text.clone(), None));
            }
            Kind::Color => {
                let [command, attr, flag, ty, r, g, b] = tokens.as_slice() else {
                    return Err("Lambert color".into());
                };
                let Some((_, color @ None)) = pending.as_mut() else {
                    return Err("color owner".into());
                };
                if !word(command, "setAttr")
                    || !attr.quoted
                    || attr.text != ".c"
                    || !word(flag, "-type")
                    || !ty.quoted
                    || ty.text != "float3"
                    || [r, g, b].iter().any(|value| {
                        value.quoted
                            || !value.text.parse::<f64>().is_ok_and(|number| {
                                number.is_finite() && (0.0..=1.0).contains(&number)
                            })
                    })
                {
                    return Err("Lambert color".into());
                }
                *color = Some([r.text.clone(), g.text.clone(), b.text.clone()]);
            }
            Kind::Group => {
                let [command, node_type, flag, node] = tokens.as_slice() else {
                    return Err("shadingEngine declaration".into());
                };
                let Some((material, Some(color))) = pending.take() else {
                    return Err("shadingEngine owner".into());
                };
                if !word(command, "createNode")
                    || !word(node_type, "shadingEngine")
                    || !word(flag, "-n")
                    || !node.quoted
                    || !name(&node.text)
                    || !declared.insert(node.text.clone())
                {
                    return Err("shadingEngine name/order".into());
                }
                report.materials.push((material, node.text.clone(), color));
            }
            Kind::Connection => {
                connections = true;
                report.connections.push(tokens);
            }
            _ => return Err("unexpected declaration kind".into()),
        }
    }
    if pending.is_some() {
        return Err("incomplete material pair".into());
    }
    Ok(report)
}
