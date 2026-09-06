// Per-pixel histogram of the final linear-light composition. Each workgroup
// covers an 8x8 tile; one thread atomically adds the pixel's luminance and
// each channel into shared counters, which are then added to the global
// 4x256 table (row 0 = luminance, rows 1..3 = R, G, B).
struct Bins {
    counts: array<atomic<u32>, 1024>,
}
@group(0) @binding(0) var linear_image: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> histogram: Bins;
var<workgroup> tile: array<atomic<u32>, 1024>;

@compute @workgroup_size(8, 8)
fn histogram_pass(@builtin(global_invocation_id) id: vec3<u32>,
                  @builtin(local_invocation_id) local: vec3<u32>,
                  @builtin(workgroup_id) group: vec3<u32>) {
    // Zero the tile counters, cooperatively across the 64 lanes.
    let lane = local.x + local.y * 8u;
    for (var base = lane; base < 1024u; base += 64u) {
        atomicStore(&tile[base], 0u);
    }
    workgroupBarrier();
    if all(id.xy < textureDimensions(linear_image)) {
        let pixel = textureLoad(linear_image, vec2<i32>(id.xy), 0);
        let rgb = clamp(pixel.rgb / max(pixel.a, 0.000001), vec3(0.0), vec3(1.0));
        let luminance = clamp(dot(rgb, vec3(0.2126, 0.7152, 0.0722)), 0.0, 1.0);
        let lr = u32(luminance * 255.0);
        atomicAdd(&tile[lr], 1u);
        atomicAdd(&tile[256u + u32(rgb.r * 255.0)], 1u);
        atomicAdd(&tile[512u + u32(rgb.g * 255.0)], 1u);
        atomicAdd(&tile[768u + u32(rgb.b * 255.0)], 1u);
    }
    workgroupBarrier();
    // Each lane flushes the tile bins it owns into the global table.
    for (var base = lane; base < 1024u; base += 64u) {
        let value = atomicLoad(&tile[base]);
        if value != 0u {
            atomicAdd(&histogram.counts[base], value);
        }
    }
}
