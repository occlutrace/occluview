// OccluView mesh shader: flat-shaded or vertex-color, depth-tested.
//
// Vertex format matches occluview_core::Vertex (#[repr(C)]):
//   position: [f32; 3]  @ offset 0
//   normal:   [f32; 3]  @ offset 12
//   color:    [u8; 4]   @ offset 24
//
// Camera uniforms (mat4 view + mat4 projection + vec3 light_dir) come from a
// single uniform buffer at group 0 binding 0.

struct Camera {
    view: mat4x4<f32>,
    projection: mat4x4<f32>,
    light_dir: vec3<f32>,
    _pad0: f32,
    camera_pos: vec3<f32>,
    _pad1: f32,
}

@group(0) @binding(0) var<uniform> camera: Camera;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<u32>,
};

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    let world_pos = vec4<f32>(in.position, 1.0);
    out.clip_pos = camera.projection * camera.view * world_pos;
    // Normalize u8 color channels to 0..1. wgsl has no direct u32->f32 on
    // vectors, so unpack element by element.
    out.color = vec3<f32>(
        f32(in.color.r) / 255.0,
        f32(in.color.g) / 255.0,
        f32(in.color.b) / 255.0,
    );
    out.normal = in.normal;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Lambertian shading with a soft ambient. If the normal is zero (STL with
    // no normals), fall back to a flat mid-grey so the silhouette is visible.
    var n = in.normal;
    let n_len = length(n);
    if (n_len < 0.001) {
        n = vec3<f32>(0.0, 0.0, 1.0);
    } else {
        n = normalize(n);
    }
    let l = normalize(camera.light_dir);
    let ndotl = max(dot(n, l), 0.0);
    let ambient = 0.35;
    let lit = ambient + (1.0 - ambient) * ndotl;
    let rgb = in.color * lit;
    return vec4<f32>(rgb, 1.0);
}
