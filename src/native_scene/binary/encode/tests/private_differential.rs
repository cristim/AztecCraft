//! Private differential check for default-leaf insertion (issue #19).
//!
//! Strips one Maya-written `t` `DBL3` at a time from a private file, reopens the stripped bytes,
//! sets the node's translation to the stripped record's exact words and requires the save to
//! reproduce the original bytes. The stripper below walks the container itself and shares no code
//! with the production splice or planner. It shows the mechanism matches Maya's own writer in the
//! observed contexts; it does not show that Maya reads files it never wrote.

use super::super::encode;
use crate::native_scene::{Channel, LinearUnit, TransformField, binary::decode};
use std::sync::Arc;

const T_PAYLOAD: [u8; 3] = *b"t\0\x20";

struct Walked {
    /// Header start of every enclosing group, outermost first.
    ancestors: Vec<usize>,
    /// Header start and end including padding of a `t` `DBL3` with the canonical 27-byte payload.
    start: usize,
    end: usize,
    /// Auxiliary words of the `DBLE` siblings.
    sibling_dble_aux: Vec<u32>,
}

#[derive(Default)]
struct Survey {
    t_records: Vec<Walked>,
    /// (aux, padding is all zero) of every `DBL3` chunk in the file.
    dbl3: Vec<(u32, bool, usize)>,
}

fn be32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn be64(bytes: &[u8], at: usize) -> usize {
    usize::try_from(u64::from_be_bytes(bytes[at..at + 8].try_into().unwrap())).unwrap()
}

fn walk(
    bytes: &[u8],
    start: usize,
    end: usize,
    align: usize,
    up: &mut Vec<usize>,
    out: &mut Survey,
) {
    let mut siblings = Vec::new();
    let mut offset = start;
    while offset < end {
        let tag: [u8; 4] = bytes[offset..offset + 4].try_into().unwrap();
        let aux = be32(bytes, offset + 4);
        let size = be64(bytes, offset + 8);
        let payload = offset + 16;
        let next = payload + size.div_ceil(align) * align;
        siblings.push((tag, offset, aux, size, next));
        if tag == *b"FOR8" || tag == *b"LIS8" {
            let child_align = match &bytes[payload..payload + 4] {
                b"Maya" | b"CONS" => 4,
                _ => 8,
            };
            up.push(offset);
            walk(bytes, payload + 4, payload + size, child_align, up, out);
            up.pop();
        }
        offset = next;
    }
    for &(tag, at, aux, size, next) in &siblings {
        if tag != *b"DBL3" {
            continue;
        }
        let zero = bytes[at + 16 + size..next].iter().all(|b| *b == 0);
        out.dbl3.push((aux, zero, at));
        if size == 27 && bytes[at + 16..at + 19] == T_PAYLOAD {
            out.t_records.push(Walked {
                ancestors: up.clone(),
                start: at,
                end: next,
                sibling_dble_aux: siblings
                    .iter()
                    .filter(|s| s.0 == *b"DBLE")
                    .map(|s| s.2)
                    .collect(),
            });
        }
    }
}

fn survey(bytes: &[u8]) -> Survey {
    let mut out = Survey::default();
    let size = be64(bytes, 8);
    assert_eq!(&bytes[..4], b"FOR8");
    assert_eq!(&bytes[16..20], b"Maya");
    let mut up = vec![0];
    walk(bytes, 20, 16 + size, 4, &mut up, &mut out);
    out
}

/// `bytes` without the record and with every enclosing size word reduced by its length.
fn strip(bytes: &[u8], record: &Walked) -> Vec<u8> {
    let removed = record.end - record.start;
    let mut out = [&bytes[..record.start], &bytes[record.end..]].concat();
    for &ancestor in &record.ancestors {
        let size = be64(&out, ancestor + 8) - removed;
        out[ancestor + 8..ancestor + 16].copy_from_slice(&(size as u64).to_be_bytes());
    }
    out
}

#[derive(Default)]
struct Tally {
    tried: usize,
    identical: usize,
    elided_identical: usize,
    refused_aux: usize,
    refused_pad: usize,
    refused_other: usize,
    stripped_undecodable: usize,
    skipped_sole_dbl3: usize,
    skipped_nonstandard: usize,
    skipped_non_finite: usize,
    skipped_units_files: usize,
    files: usize,
}

#[test]
#[ignore = "requires explicit private manifest"]
fn private_stripped_translate_records_reinsert_byte_identically() {
    let Some(manifest) = std::env::var_os("AZTECCRAFT_MB_FIXTURE_MANIFEST") else {
        println!("skipped: AZTECCRAFT_MB_FIXTURE_MANIFEST is not set");
        return;
    };
    let manifest = std::path::Path::new(&manifest);
    let directory = manifest.parent().unwrap().canonicalize().unwrap();
    let entries: serde_json::Value =
        serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
    let mut total = Tally::default();
    for (number, entry) in entries.as_array().unwrap().iter().enumerate() {
        let path = directory
            .join(entry["relative_path"].as_str().unwrap())
            .canonicalize()
            .unwrap();
        assert!(path.starts_with(&directory));
        let bytes = std::fs::read(path).unwrap();
        let original = decode(Arc::<[u8]>::from(bytes.clone())).unwrap();
        if original.units().linear != LinearUnit::Centimeter {
            println!("file {number}: skipped, linear unit is not centimetre");
            total.skipped_units_files += 1;
            continue;
        }
        total.files += 1;
        let found = survey(&bytes);
        let mut tally = Tally::default();
        for record in &found.t_records {
            if found.dbl3.len() < 2 {
                tally.skipped_sole_dbl3 += 1;
                continue;
            }
            let removed = record.end - record.start;
            if removed % 8 != 0 {
                tally.skipped_nonstandard += 1;
                continue;
            }
            let words: [f64; 3] = std::array::from_fn(|i| {
                let at = record.start + 19 + 8 * i;
                f64::from_be_bytes(bytes[at..at + 8].try_into().unwrap())
            });
            if words.iter().any(|w| !w.is_finite()) {
                tally.skipped_non_finite += 1;
                continue;
            }
            let leaf = original
                .source
                .as_ref()
                .unwrap()
                .leaves
                .iter()
                .find(|leaf| {
                    leaf.field == TransformField::Translation
                        && leaf.numeric.start == record.start + 19
                })
                .expect("every walked t record is a decoded leaf");
            let index = original
                .nodes()
                .iter()
                .position(|node| node.transform_id == leaf.owner)
                .unwrap();
            let stripped = strip(&bytes, record);
            let Ok(mut scene) = decode(Arc::<[u8]>::from(stripped.clone())) else {
                tally.tried += 1;
                tally.stripped_undecodable += 1;
                continue;
            };
            tally.tried += 1;
            assert_eq!(scene.nodes()[index].name(), original.nodes()[index].name());
            let id = scene.nodes()[index].id();
            for (channel, word) in [
                Channel::TranslateX,
                Channel::TranslateY,
                Channel::TranslateZ,
            ]
            .into_iter()
            .zip(words)
            {
                scene.set_channel(id, channel, word).unwrap();
            }
            // Setting +0.0 on an absent leaf is a no-op, so this branch only checks that a no-op
            // save is byte-identical to the stripped file. It has never run on real data.
            let all_zero = words.iter().all(|w| w.to_bits() == 0);
            match encode(&scene) {
                Ok(output) => {
                    let kind = if all_zero { "elided" } else { "inserted" };
                    let expected: &[u8] = if all_zero { &stripped } else { &bytes };
                    let differs = output.iter().zip(expected).position(|(a, b)| a != b);
                    assert!(
                        differs.is_none() && output.len() == expected.len(),
                        "byte identity failed for a {kind} record: record offset {}, \
                         first differing offset {differs:?}, lengths {} vs {}",
                        record.start,
                        output.len(),
                        expected.len()
                    );
                    tally.identical += 1;
                    tally.elided_identical += usize::from(all_zero);
                }
                Err(_) => {
                    let remaining: Vec<_> =
                        found.dbl3.iter().filter(|c| c.2 != record.start).collect();
                    let aux = remaining[0].0;
                    if remaining.iter().any(|c| c.0 != aux)
                        || record.sibling_dble_aux.iter().any(|a| *a != aux)
                    {
                        tally.refused_aux += 1;
                    } else if remaining.iter().any(|c| !c.1) {
                        tally.refused_pad += 1;
                    } else {
                        tally.refused_other += 1;
                    }
                }
            }
        }
        println!(
            "file {number}: tried {} identical {} (elided {}) refused aux {} pad {} other {} \
             undecodable {} skipped sole-DBL3 {} nonstandard {} non-finite {}",
            tally.tried,
            tally.identical,
            tally.elided_identical,
            tally.refused_aux,
            tally.refused_pad,
            tally.refused_other,
            tally.stripped_undecodable,
            tally.skipped_sole_dbl3,
            tally.skipped_nonstandard,
            tally.skipped_non_finite,
        );
        total.tried += tally.tried;
        total.identical += tally.identical;
        total.elided_identical += tally.elided_identical;
        total.refused_aux += tally.refused_aux;
        total.refused_pad += tally.refused_pad;
        total.refused_other += tally.refused_other;
        total.stripped_undecodable += tally.stripped_undecodable;
        total.skipped_sole_dbl3 += tally.skipped_sole_dbl3;
        total.skipped_nonstandard += tally.skipped_nonstandard;
        total.skipped_non_finite += tally.skipped_non_finite;
    }
    println!(
        "total: files {} (skipped by unit {}) tried {} identical {} (elided {}) refused aux {} \
         pad {} other {} undecodable {} skipped sole-DBL3 {} nonstandard {} non-finite {}",
        total.files,
        total.skipped_units_files,
        total.tried,
        total.identical,
        total.elided_identical,
        total.refused_aux,
        total.refused_pad,
        total.refused_other,
        total.stripped_undecodable,
        total.skipped_sole_dbl3,
        total.skipped_nonstandard,
        total.skipped_non_finite,
    );
    assert!(total.tried > 0, "no stripped record was tried");
    assert_eq!(
        total.stripped_undecodable, 0,
        "a stripped file did not decode"
    );
    let refused = total.refused_aux + total.refused_pad + total.refused_other;
    assert_eq!(
        total.identical + refused + total.stripped_undecodable,
        total.tried,
        "every tried record must be identical, refused or undecodable"
    );
    let expected = match std::env::var("AZTECCRAFT_MB_EXPECTED_REFUSALS") {
        Err(std::env::VarError::NotPresent) => 0,
        Ok(text) => text.trim().parse::<usize>().unwrap_or_else(|_| {
            panic!("AZTECCRAFT_MB_EXPECTED_REFUSALS must be a non-negative integer")
        }),
        Err(_) => panic!("AZTECCRAFT_MB_EXPECTED_REFUSALS must be valid unicode"),
    };
    assert_eq!(refused, expected, "unexpected number of refused records");
}
