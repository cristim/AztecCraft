//! Wire shape of `create_primitive`: one `primitive` tag selects the variant, and each variant
//! accepts only the options that primitive has. Bounds come from `primitive_options`.

use azteccraft::{
    primitive_options::{
        Axis, DEFAULT_AXIS, DEFAULT_AXIS_DIVISIONS, DEFAULT_CUBE_DIVISIONS, DEFAULT_CUBE_SIZE_CM,
        DEFAULT_CYLINDER_CONE_HEIGHT_DIVISIONS, DEFAULT_HEIGHT_CM, DEFAULT_PLANE_DIVISIONS,
        DEFAULT_PLANE_SIZE_CM, DEFAULT_PLATONIC_RADIUS_CM, DEFAULT_RADIUS_CM, DEFAULT_ROUND_CAP,
        DEFAULT_SECTION_RADIUS_CM, DEFAULT_SPHERE_HEIGHT_DIVISIONS, DEFAULT_SUB_CAPS,
        DEFAULT_TORUS_HEIGHT_DIVISIONS, DEFAULT_TORUS_TWIST_DEG, MAX_DIVISIONS, MAX_SIZE_CM,
        MIN_AXIS_DIVISIONS, MIN_CAP_DIVISIONS, MIN_DIVISIONS, MIN_SIZE_CM,
        MIN_SPHERE_HEIGHT_DIVISIONS, PlatonicSolidKind, PrimitiveOptions,
    },
    scene::Primitive,
};
use rmcp::{
    model::ErrorData,
    schemars::{self, JsonSchema},
};
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};

#[derive(Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "PrimitiveAxis", inline)]
enum AxisInput {
    X,
    Y,
    Z,
}

impl From<Axis> for AxisInput {
    fn from(axis: Axis) -> Self {
        match axis {
            Axis::X => AxisInput::X,
            Axis::Y => AxisInput::Y,
            Axis::Z => AxisInput::Z,
        }
    }
}

impl From<AxisInput> for Axis {
    fn from(axis: AxisInput) -> Self {
        match axis {
            AxisInput::X => Axis::X,
            AxisInput::Y => Axis::Y,
            AxisInput::Z => Axis::Z,
        }
    }
}

/// Wire names of `PlatonicSolidKind`; the schema lists them as an enum.
#[derive(Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "PlatonicSolidType", inline)]
enum SolidInput {
    Dodecahedron,
    Icosahedron,
    Octahedron,
    Tetrahedron,
    Cube,
}

impl From<SolidInput> for PlatonicSolidKind {
    fn from(solid: SolidInput) -> Self {
        match solid {
            SolidInput::Dodecahedron => Self::Dodecahedron,
            SolidInput::Icosahedron => Self::Icosahedron,
            SolidInput::Octahedron => Self::Octahedron,
            SolidInput::Tetrahedron => Self::Tetrahedron,
            SolidInput::Cube => Self::Cube,
        }
    }
}

/// Unlike `Option<T>`, an explicit `null` is an error, not "use the defaults". Only a JSON object
/// is accepted: serde would otherwise also read an array as positional struct fields.
fn present<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Option<T>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    if !value.is_object() {
        return Err(serde::de::Error::custom("options must be an object"));
    }
    serde_json::from_value(value)
        .map(Some)
        .map_err(serde::de::Error::custom)
}

macro_rules! option_structs {
    ($($name:ident => $primitive:ident {
        $($(#[$meta:meta])* $field:ident : $ty:ty = $default:expr $(, $min:expr, $max:expr)?;)*
    })*) => {$(
        #[derive(Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields, default)]
        pub(super) struct $name {
            $(
                $(#[$meta])*
                $(#[schemars(range(min = $min, max = $max))])?
                $field: $ty,
            )*
            axis: AxisInput,
        }

        impl Default for $name {
            fn default() -> Self {
                Self { $($field: $default,)* axis: DEFAULT_AXIS.into() }
            }
        }
    )*};
}

option_structs! {
    CubeOptions => Cube {
        width: f64 = DEFAULT_CUBE_SIZE_CM, MIN_SIZE_CM, MAX_SIZE_CM;
        height: f64 = DEFAULT_CUBE_SIZE_CM, MIN_SIZE_CM, MAX_SIZE_CM;
        depth: f64 = DEFAULT_CUBE_SIZE_CM, MIN_SIZE_CM, MAX_SIZE_CM;
        subdivisions_width: u32 = DEFAULT_CUBE_DIVISIONS, MIN_DIVISIONS, MAX_DIVISIONS;
        subdivisions_height: u32 = DEFAULT_CUBE_DIVISIONS, MIN_DIVISIONS, MAX_DIVISIONS;
        subdivisions_depth: u32 = DEFAULT_CUBE_DIVISIONS, MIN_DIVISIONS, MAX_DIVISIONS;
    }
    PlaneOptions => Plane {
        width: f64 = DEFAULT_PLANE_SIZE_CM, MIN_SIZE_CM, MAX_SIZE_CM;
        height: f64 = DEFAULT_PLANE_SIZE_CM, MIN_SIZE_CM, MAX_SIZE_CM;
        subdivisions_width: u32 = DEFAULT_PLANE_DIVISIONS, MIN_DIVISIONS, MAX_DIVISIONS;
        subdivisions_height: u32 = DEFAULT_PLANE_DIVISIONS, MIN_DIVISIONS, MAX_DIVISIONS;
    }
    CylinderOptions => Cylinder {
        radius: f64 = DEFAULT_RADIUS_CM, MIN_SIZE_CM, MAX_SIZE_CM;
        height: f64 = DEFAULT_HEIGHT_CM, MIN_SIZE_CM, MAX_SIZE_CM;
        subdivisions_axis: u32 = DEFAULT_AXIS_DIVISIONS, MIN_AXIS_DIVISIONS, MAX_DIVISIONS;
        subdivisions_height: u32 = DEFAULT_CYLINDER_CONE_HEIGHT_DIVISIONS, MIN_DIVISIONS, MAX_DIVISIONS;
        /// Subdivisions on each cap (Maya `-subdivisionsCaps`); 0 keeps one n-gon per cap.
        subdivisions_caps: u32 = DEFAULT_SUB_CAPS, MIN_CAP_DIVISIONS, MAX_DIVISIONS;
        /// Domes the caps; has no effect while `subdivisions_caps` is 0.
        round_cap: bool = DEFAULT_ROUND_CAP;
    }
    ConeOptions => Cone {
        radius: f64 = DEFAULT_RADIUS_CM, MIN_SIZE_CM, MAX_SIZE_CM;
        height: f64 = DEFAULT_HEIGHT_CM, MIN_SIZE_CM, MAX_SIZE_CM;
        subdivisions_axis: u32 = DEFAULT_AXIS_DIVISIONS, MIN_AXIS_DIVISIONS, MAX_DIVISIONS;
        subdivisions_height: u32 = DEFAULT_CYLINDER_CONE_HEIGHT_DIVISIONS, MIN_DIVISIONS, MAX_DIVISIONS;
        /// Subdivisions on the base cap (Maya `-subdivisionsCap`, singular); 0 keeps one n-gon.
        subdivisions_caps: u32 = DEFAULT_SUB_CAPS, MIN_CAP_DIVISIONS, MAX_DIVISIONS;
        /// Domes the base cap; has no effect while `subdivisions_caps` is 0.
        round_cap: bool = DEFAULT_ROUND_CAP;
    }
    SphereOptions => Sphere {
        radius: f64 = DEFAULT_RADIUS_CM, MIN_SIZE_CM, MAX_SIZE_CM;
        subdivisions_axis: u32 = DEFAULT_AXIS_DIVISIONS, MIN_AXIS_DIVISIONS, MAX_DIVISIONS;
        subdivisions_height: u32 = DEFAULT_SPHERE_HEIGHT_DIVISIONS, MIN_SPHERE_HEIGHT_DIVISIONS, MAX_DIVISIONS;
    }
    TorusOptions => Torus {
        radius: f64 = DEFAULT_RADIUS_CM, MIN_SIZE_CM, MAX_SIZE_CM;
        section_radius: f64 = DEFAULT_SECTION_RADIUS_CM, MIN_SIZE_CM, MAX_SIZE_CM;
        /// Degrees; any finite angle turns every section of the tube by the same amount.
        twist: f64 = DEFAULT_TORUS_TWIST_DEG;
        subdivisions_axis: u32 = DEFAULT_AXIS_DIVISIONS, MIN_AXIS_DIVISIONS, MAX_DIVISIONS;
        subdivisions_height: u32 = DEFAULT_TORUS_HEIGHT_DIVISIONS, MIN_AXIS_DIVISIONS, MAX_DIVISIONS;
    }
    PlatonicSolidOptions => PlatonicSolid {
        /// Which solid to build; Maya's `-solidType` default is the dodecahedron. The cube is
        /// PROVISIONAL (#466): Maya's command page lists only the other four.
        solid: SolidInput = SolidInput::Dodecahedron;
        /// Circumradius (distance from the centre to every vertex).
        radius: f64 = DEFAULT_PLATONIC_RADIUS_CM, MIN_SIZE_CM, MAX_SIZE_CM;
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "primitive", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum CreateArgs {
    Cube {
        scene_token: String,
        name: Option<String>,
        #[serde(default, deserialize_with = "present")]
        #[schemars(with = "CubeOptions")]
        options: Option<CubeOptions>,
    },
    Plane {
        scene_token: String,
        name: Option<String>,
        #[serde(default, deserialize_with = "present")]
        #[schemars(with = "PlaneOptions")]
        options: Option<PlaneOptions>,
    },
    Sphere {
        scene_token: String,
        name: Option<String>,
        #[serde(default, deserialize_with = "present")]
        #[schemars(with = "SphereOptions")]
        options: Option<SphereOptions>,
    },
    Cylinder {
        scene_token: String,
        name: Option<String>,
        #[serde(default, deserialize_with = "present")]
        #[schemars(with = "CylinderOptions")]
        options: Option<CylinderOptions>,
    },
    Cone {
        scene_token: String,
        name: Option<String>,
        #[serde(default, deserialize_with = "present")]
        #[schemars(with = "ConeOptions")]
        options: Option<ConeOptions>,
    },
    Torus {
        scene_token: String,
        name: Option<String>,
        #[serde(default, deserialize_with = "present")]
        #[schemars(with = "TorusOptions")]
        options: Option<TorusOptions>,
    },
    PlatonicSolid {
        scene_token: String,
        name: Option<String>,
        #[serde(default, deserialize_with = "present")]
        #[schemars(with = "PlatonicSolidOptions")]
        options: Option<PlatonicSolidOptions>,
    },
}

pub(super) struct Parsed {
    pub scene_token: String,
    pub primitive: Primitive,
    pub options: Option<PrimitiveOptions>,
    pub name: Option<String>,
}

impl CreateArgs {
    /// Builds the core options and runs the core `validate`, so the wire and the editor agree.
    pub(super) fn into_parsed(self) -> Result<Parsed, ErrorData> {
        let (scene_token, name, primitive, options) = match self {
            Self::Cube {
                scene_token,
                name,
                options,
            } => (
                scene_token,
                name,
                Primitive::Cube,
                options.map(|o| PrimitiveOptions::Cube {
                    width: o.width,
                    height: o.height,
                    depth: o.depth,
                    sub_width: o.subdivisions_width,
                    sub_height: o.subdivisions_height,
                    sub_depth: o.subdivisions_depth,
                    axis: o.axis.into(),
                }),
            ),
            Self::Plane {
                scene_token,
                name,
                options,
            } => (
                scene_token,
                name,
                Primitive::Plane,
                options.map(|o| PrimitiveOptions::Plane {
                    width: o.width,
                    height: o.height,
                    sub_width: o.subdivisions_width,
                    sub_height: o.subdivisions_height,
                    axis: o.axis.into(),
                }),
            ),
            Self::Sphere {
                scene_token,
                name,
                options,
            } => (
                scene_token,
                name,
                Primitive::Sphere,
                options.map(|o| PrimitiveOptions::Sphere {
                    radius: o.radius,
                    sub_axis: o.subdivisions_axis,
                    sub_height: o.subdivisions_height,
                    axis: o.axis.into(),
                }),
            ),
            Self::Cylinder {
                scene_token,
                name,
                options,
            } => (
                scene_token,
                name,
                Primitive::Cylinder,
                options.map(|o| PrimitiveOptions::Cylinder {
                    radius: o.radius,
                    height: o.height,
                    sub_axis: o.subdivisions_axis,
                    sub_height: o.subdivisions_height,
                    sub_caps: o.subdivisions_caps,
                    round_cap: o.round_cap,
                    axis: o.axis.into(),
                }),
            ),
            Self::Cone {
                scene_token,
                name,
                options,
            } => (
                scene_token,
                name,
                Primitive::Cone,
                options.map(|o| PrimitiveOptions::Cone {
                    radius: o.radius,
                    height: o.height,
                    sub_axis: o.subdivisions_axis,
                    sub_height: o.subdivisions_height,
                    sub_caps: o.subdivisions_caps,
                    round_cap: o.round_cap,
                    axis: o.axis.into(),
                }),
            ),
            Self::Torus {
                scene_token,
                name,
                options,
            } => (
                scene_token,
                name,
                Primitive::Torus,
                options.map(|o| PrimitiveOptions::Torus {
                    radius: o.radius,
                    section_radius: o.section_radius,
                    twist: o.twist,
                    sub_axis: o.subdivisions_axis,
                    sub_height: o.subdivisions_height,
                    axis: o.axis.into(),
                }),
            ),
            Self::PlatonicSolid {
                scene_token,
                name,
                options,
            } => (
                scene_token,
                name,
                Primitive::PlatonicSolid,
                options.map(|o| PrimitiveOptions::PlatonicSolid {
                    solid: o.solid.into(),
                    radius: o.radius,
                    axis: o.axis.into(),
                }),
            ),
        };
        if let Some(options) = &options {
            options
                .validate(primitive)
                .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
        }
        Ok(Parsed {
            scene_token,
            primitive,
            options,
            name,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults_of(args: CreateArgs) -> Option<PrimitiveOptions> {
        args.into_parsed().expect("defaults validate").options
    }

    fn check(primitive: Primitive, args: CreateArgs) {
        assert_eq!(
            defaults_of(args),
            Some(PrimitiveOptions::default_for(primitive)),
            "{primitive:?}"
        );
    }

    fn t() -> String {
        "token".to_owned()
    }

    #[test]
    fn a_twist_beyond_the_f64_range_is_a_json_syntax_error_not_infinity() {
        let error = serde_json::from_str::<CreateArgs>(
            r#"{"primitive":"torus","scene_token":"t","options":{"twist":1e400}}"#,
        )
        .err()
        .expect("1e400 does not fit an f64");
        assert!(error.is_syntax(), "{error}");
        assert!(error.to_string().contains("number out of range"), "{error}");
    }

    #[test]
    fn wire_defaults_equal_core_defaults() {
        check(
            Primitive::Cube,
            CreateArgs::Cube {
                scene_token: t(),
                name: None,
                options: Some(CubeOptions::default()),
            },
        );
        check(
            Primitive::Plane,
            CreateArgs::Plane {
                scene_token: t(),
                name: None,
                options: Some(PlaneOptions::default()),
            },
        );
        check(
            Primitive::Sphere,
            CreateArgs::Sphere {
                scene_token: t(),
                name: None,
                options: Some(SphereOptions::default()),
            },
        );
        check(
            Primitive::Cylinder,
            CreateArgs::Cylinder {
                scene_token: t(),
                name: None,
                options: Some(CylinderOptions::default()),
            },
        );
        check(
            Primitive::Cone,
            CreateArgs::Cone {
                scene_token: t(),
                name: None,
                options: Some(ConeOptions::default()),
            },
        );
        check(
            Primitive::Torus,
            CreateArgs::Torus {
                scene_token: t(),
                name: None,
                options: Some(TorusOptions::default()),
            },
        );
        check(
            Primitive::PlatonicSolid,
            CreateArgs::PlatonicSolid {
                scene_token: t(),
                name: None,
                options: Some(PlatonicSolidOptions::default()),
            },
        );
    }
}
