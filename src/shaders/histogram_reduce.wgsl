@group(0) @binding(0) var<storage, read> tiles: array<u32>;
@group(0) @binding(1) var<storage, read_write> total: array<u32>;
@compute @workgroup_size(8, 8)
fn reduce(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= 256u || id.y >= 4u { return; }
    let bin = id.y * 256u + id.x;
    var sum = 0u;
    for (var i = bin; i < arrayLength(&tiles); i += 1024u) { sum += tiles[i]; }
    total[bin] = sum;
}
