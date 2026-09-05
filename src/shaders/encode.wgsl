@group(0) @binding(0) var linear_image: texture_2d<f32>;
@group(0) @binding(1) var display: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var export: texture_storage_2d<rgba8unorm, write>;
fn srgb(linear: vec3<f32>) -> vec3<f32> {
    let c = clamp(linear, vec3(0.0), vec3(1.0));
    return select(1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, 12.92 * c, c <= vec3(0.0031308));
}
@compute @workgroup_size(8, 8)
fn encode(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= textureDimensions(display)) { return; }
    let at = vec2<i32>(id.xy);
    let pixel = textureLoad(linear_image, at, 0);
    let color = srgb(pixel.rgb / max(pixel.a, 0.000001));
    // egui samples gamma-encoded premultiplied pixels; PNG requires straight alpha.
    textureStore(display, at, vec4(color * pixel.a, pixel.a));
    textureStore(export, at, vec4(color, pixel.a));
}
