// Bramapa tilemap shader: one instance per non-empty cell, quad built from vertex_index.

struct Uniforms {
    view_proj: mat4x4<f32>,
    time: f32,
    atlas_cols: f32,
    atlas_rows: f32,
    depth_norm: f32,
    tile_w: f32,
    tile_h: f32,
    cell_w: f32,
    cell_h: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var atlas_tex: texture_2d<f32>;
@group(1) @binding(1) var atlas_smp: sampler;

struct Instance {
    @location(0) cell: vec2<u32>,   // absolute cell x, y
    @location(1) atlas: vec2<u32>,  // col, row
    @location(2) anim: vec2<u32>,   // frames, fps
    @location(3) misc: vec2<u32>,   // flags (bit0 mirror), layer (0 terrain, 1 overlay, 2 object)
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Instance) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let c = corners[vi];
    let x = f32(inst.cell.x);
    let y = f32(inst.cell.y);
    let top = vec2<f32>((x - y) * u.tile_w * 0.5, (x + y) * u.tile_h * 0.5);
    let tl = vec2<f32>(top.x - u.cell_w * 0.5, top.y + u.tile_h - u.cell_h);
    let world = tl + c * vec2<f32>(u.cell_w, u.cell_h);
    var clip = u.view_proj * vec4<f32>(world, 0.0, 1.0);

    let layer = inst.misc.y;
    var z = 0.999;
    if (layer == 1u) {
        z = 0.998;
    } else if (layer == 2u) {
        z = 0.9 - (x + y + 1.0) * u.depth_norm * 0.8;
    }
    clip.z = z * clip.w;

    var frame = 0u;
    if (inst.anim.x > 1u && inst.anim.y > 0u) {
        frame = u32(floor(u.time * f32(inst.anim.y))) % inst.anim.x;
    }
    var cu = c.x;
    if ((inst.misc.x & 1u) != 0u) {
        cu = 1.0 - cu;
    }
    let uv = vec2<f32>((f32(inst.atlas.x + frame) + cu) / u.atlas_cols, (f32(inst.atlas.y) + c.y) / u.atlas_rows);

    var out: VsOut;
    out.pos = clip;
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let color = textureSample(atlas_tex, atlas_smp, in.uv);
    if (color.a < 0.5) {
        discard;
    }
    return vec4<f32>(color.rgb, 1.0);
}
