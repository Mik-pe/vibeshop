struct Parameters { tone: vec4<f32>, offset: vec2<i32>, blend: u32, padding: u32 }
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var backdrop: texture_2d<f32>;
@group(0) @binding(2) var output: texture_storage_2d<rgba16float, write>;
@group(0) @binding(3) var<uniform> p: Parameters;

fn linear(s: vec3<f32>) -> vec3<f32> {
    return select(pow((s + 0.055) / 1.055, vec3(2.4)), s / 12.92, s <= vec3(0.04045));
}
@compute @workgroup_size(8, 8)
fn composite(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= textureDimensions(output)) { return; }
    let at = vec2<i32>(id.xy);
    let below = textureLoad(backdrop, at, 0);
    let from = at - p.offset;
    if any(from < vec2(0)) || any(from >= vec2<i32>(textureDimensions(source))) {
        textureStore(output, at, below); return;
    }
    let pixel = textureLoad(source, from, 0);
    var rgb = linear(pixel.rgb) * exp2(p.tone.x);
    rgb = max((rgb - vec3(0.18)) * p.tone.y + vec3(0.18), vec3(0.0));
    let luminance = dot(rgb, vec3(0.2126, 0.7152, 0.0722));
    rgb = max(mix(vec3(luminance), rgb, p.tone.z), vec3(0.0));
    let alpha = pixel.a * p.tone.w;
    let base = below.rgb / max(below.a, 0.000001);
    var blend = rgb;
    if p.blend == 1u { blend = base * rgb; }
    if p.blend == 2u { blend = vec3(1.0) - (vec3(1.0) - base) * (vec3(1.0) - rgb); }
    // W3C source-over with separable blending; both accumulated terms are premultiplied.
    let color = (1.0 - alpha) * below.rgb + alpha * ((1.0 - below.a) * rgb + below.a * blend);
    textureStore(output, at, vec4(color, alpha + below.a * (1.0 - alpha)));
}
