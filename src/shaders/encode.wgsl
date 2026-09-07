@group(0) @binding(0) var linear_image: texture_2d<f32>;
@group(0) @binding(1) var display: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var png_output: texture_storage_2d<rgba8unorm, write>;
// Tile origin, empty flag, and first bin in the cached histogram table.
@group(0) @binding(3) var<uniform> tile: vec4<u32>;
@group(0) @binding(4) var<storage, read_write> histogram: array<atomic<u32>>;
var<workgroup> bins: array<atomic<u32>, 1024>;
fn srgb(linear: vec3<f32>) -> vec3<f32> {
    let c = clamp(linear, vec3(0.0), vec3(1.0));
    return select(1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, 12.92 * c, c <= vec3(0.0031308));
}
@compute @workgroup_size(8, 8)
fn encode(@builtin(local_invocation_id) local: vec3<u32>, @builtin(workgroup_id) group: vec3<u32>) {
    let lane = local.x + local.y * 8u;
    for (var bin = lane; bin < 1024u; bin += 64u) { atomicStore(&bins[bin], 0u); }
    workgroupBarrier();
    // Each lane processes 16 pixels: share the final linear load between
    // output encoding and histogram, amortizing clears and global atomics.
    var previous: array<u32, 4>;
    var counts: array<u32, 4>;
    for (var sy = 0u; sy < 4u; sy += 1u) {
        for (var sx = 0u; sx < 4u; sx += 1u) {
            let at = group.xy * 32u + local.xy + vec2(sx, sy) * 8u;
            let destination = at + tile.xy;
            if all(destination < textureDimensions(display)) {
                var pixel = vec4(0.0);
                if tile.z == 0u { pixel = textureLoad(linear_image, vec2<i32>(at), 0); }
                let linear = pixel.rgb / max(pixel.a, 0.000001);
                let color = srgb(linear);
                // egui samples gamma-encoded premultiplied pixels; PNG requires straight alpha.
                textureStore(display, vec2<i32>(destination), vec4(color * pixel.a, pixel.a));
                textureStore(png_output, vec2<i32>(destination), vec4(color, pixel.a));
                if pixel.a > 0.0 {
                    let rgb = clamp(linear, vec3(0.0), vec3(1.0));
                    let luma = clamp(dot(rgb, vec3(0.2126, 0.7152, 0.0722)), 0.0, 1.0);
                    let next = vec4(u32(luma * 255.0), 256u + u32(rgb.r * 255.0), 512u + u32(rgb.g * 255.0), 768u + u32(rgb.b * 255.0));
                    // Flat/smooth regions often repeat a bin within this lane.
                    for (var channel = 0u; channel < 4u; channel += 1u) {
                        if counts[channel] > 0u && previous[channel] != next[channel] {
                            atomicAdd(&bins[previous[channel]], counts[channel]);
                            counts[channel] = 0u;
                        }
                        previous[channel] = next[channel];
                        counts[channel] += 1u;
                    }
                }
            }
        }
    }
    for (var channel = 0u; channel < 4u; channel += 1u) {
        if counts[channel] > 0u { atomicAdd(&bins[previous[channel]], counts[channel]); }
    }
    workgroupBarrier();
    for (var bin = lane; bin < 1024u; bin += 64u) {
        let count = atomicLoad(&bins[bin]);
        if count > 0u { atomicAdd(&histogram[tile.w + bin], count); }
    }
}
