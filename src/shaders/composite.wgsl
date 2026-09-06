struct Parameters {
    tone: vec4<f32>,
    offset: vec2<i32>,
    blend: u32,
    padding: u32,
    // Master levels in linear light: (input black, input white, 1/gamma).
    levels: vec3<f32>,
    // Bit 0 RGB, 1 R, 2 G, 3 B.
    curve_mask: u32,
}
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var backdrop: texture_2d<f32>;
@group(0) @binding(2) var output: texture_storage_2d<rgba16float, write>;
@group(0) @binding(3) var<uniform> p: Parameters;
@group(0) @binding(4) var curves: texture_2d<f32>;

fn linear(s: vec3<f32>) -> vec3<f32> {
    return select(pow((s + 0.055) / 1.055, vec3(2.4)), s / 12.92, s <= vec3(0.04045));
}

// The CPU filled this 256x4 LUT from the same control points Curve::eval
// interpolates, so one lookup applies the whole curve. Rows: 0 RGB,
// 1 R, 2 G, 3 B.
fn curve_at(row: u32, x: f32) -> f32 {
    let index = u32(clamp(x, 0.0, 1.0) * 255.0 + 0.5);
    return textureLoad(curves, vec2<u32>(min(index, 255u), row), 0).r;
}

@compute @workgroup_size(8, 8)
fn composite(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= textureDimensions(output)) { return; }
    let at = vec2<i32>(id.xy);
    let below = textureLoad(backdrop, at, 0);
    let source_coord = at - p.offset;
    if any(source_coord < vec2(0)) || any(source_coord >= vec2<i32>(textureDimensions(source))) {
        textureStore(output, at, below); return;
    }
    let pixel = textureLoad(source, source_coord, 0);
    var rgb = linear(pixel.rgb) * exp2(p.tone.x);
    rgb = max((rgb - vec3(0.18)) * p.tone.y + vec3(0.18), vec3(0.0));
    let luminance = dot(rgb, vec3(0.2126, 0.7152, 0.0722));
    rgb = max(mix(vec3(luminance), rgb, p.tone.z), vec3(0.0));
    // Master levels, then monotone curves, all in linear light. Levels are
    // normalized into 0..1 before the curves so both share one domain.
    if p.curve_mask != 0u || p.levels.x != 0.0 || p.levels.y != 1.0 || p.levels.z != 1.0 {
        let t = clamp((rgb - vec3(p.levels.x)) / max(p.levels.y - p.levels.x, 0.01), vec3(0.0), vec3(1.0));
        let g = pow(t, vec3(p.levels.z));
        var c = g;
        if (p.curve_mask & 1u) != 0u {
            c = vec3(curve_at(0u, g.r), curve_at(0u, g.g), curve_at(0u, g.b));
        }
        if (p.curve_mask & 2u) != 0u { c.r = curve_at(1u, c.r); }
        if (p.curve_mask & 4u) != 0u { c.g = curve_at(2u, c.g); }
        if (p.curve_mask & 8u) != 0u { c.b = curve_at(3u, c.b); }
        // Map back out of the normalized levels domain, preserving range.
        rgb = c * (p.levels.y - p.levels.x) + vec3(p.levels.x);
    }
    let alpha = pixel.a * p.tone.w;
    let base = below.rgb / max(below.a, 0.000001);
    var blend = rgb;
    if p.blend == 1u { blend = base * rgb; }
    if p.blend == 2u { blend = vec3(1.0) - (vec3(1.0) - base) * (vec3(1.0) - rgb); }
    // Source-over with separable blending; accumulated RGB is premultiplied.
    let color = (1.0 - alpha) * below.rgb + alpha * ((1.0 - below.a) * rgb + below.a * blend);
    textureStore(output, at, vec4(color, alpha + below.a * (1.0 - alpha)));
}
