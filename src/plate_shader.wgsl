// Camera image plane (#459). Layouts are fixed by src/image_plane/draw.rs; uv has v down.

struct PlateParams {
    mode: u32,
    output_srgb: u32,
    alpha_gain: f32,
    pad: f32,
}

@group(0) @binding(0) var<uniform> params: PlateParams;
@group(0) @binding(1) var plate_texture: texture_2d<f32>;
@group(0) @binding(2) var plate_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vertex(@location(0) position: vec4<f32>, @location(1) uv: vec2<f32>) -> VertexOutput {
    return VertexOutput(position, uv);
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

// Texels are the decoded bytes (Rgba8Unorm, straight alpha). Modes: 0 RGBA, 1 RGB, 2 Luminance
// (Rec. 709 on the encoded values), 3 Alpha as grey; 1 to 3 are opaque. An sRGB target encodes
// what we write, so output_srgb writes linear values and the stored bytes equal the texels.
// PROVISIONAL: bilinear filtering mixes the encoded values; an Rgba8UnormSrgb texture would
// filter in linear light, so scaled plates differ slightly at edges.
@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(plate_texture, plate_sampler, input.uv);
    var rgb = texel.rgb;
    var alpha = clamp(texel.a * params.alpha_gain, 0.0, 1.0);
    if params.mode == 1u {
        alpha = 1.0;
    } else if params.mode == 2u {
        rgb = vec3<f32>(dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722)));
        alpha = 1.0;
    } else if params.mode == 3u {
        rgb = vec3<f32>(alpha);
        alpha = 1.0;
    }
    if params.output_srgb == 1u {
        rgb = srgb_to_linear(rgb);
    }
    return vec4<f32>(rgb, alpha);
}

// Outline: white edges (PROVISIONAL).
@fragment
fn outline(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}
