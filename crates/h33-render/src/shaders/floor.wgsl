// ============================================================
//  Floor shader — dalle du hangar + grille procédurale antialiasée
// ============================================================

struct Camera {
    view_proj : mat4x4<f32>,
    cam_pos   : vec4<f32>,
};
@group(0) @binding(0) var<uniform> camera : Camera;

struct VertexInput {
    @location(0) pos    : vec3<f32>,
    @location(1) normal : vec3<f32>,
    @location(2) color  : vec3<f32>,
};

struct VertexOutput {
    @builtin(position) pos : vec4<f32>,
    @location(0) world     : vec3<f32>,
};

@vertex
fn vs_main(in : VertexInput) -> VertexOutput {
    var out : VertexOutput;
    out.pos   = camera.view_proj * vec4<f32>(in.pos, 1.0);
    out.world = in.pos;
    return out;
}

@fragment
fn fs_main(in : VertexOutput) -> @location(0) vec4<f32> {
    // Base : béton sombre avec légère variation spatiale.
    var base = vec3<f32>(0.10, 0.105, 0.12);
    let checker = select(0.0, 0.008,
        (floor(in.world.x) + floor(in.world.z)) % 2.0 == 0.0);
    base = base + vec3<f32>(checker);

    // Lignes de grille : fines tous les 1 m, marquées tous les 8 m.
    let d1 = abs(fract(in.world.xz - 0.5) - 0.5) / fwidth(in.world.xz);
    let line = 1.0 - min(min(d1.x, d1.y), 1.0);
    let d8 = abs(fract(in.world.xz / 8.0 - 0.5) - 0.5) / fwidth(in.world.xz / 8.0);
    let line_major = 1.0 - min(min(d8.x, d8.y), 1.0);

    base = mix(base, vec3<f32>(0.16, 0.175, 0.21), line * 0.55);
    base = mix(base, vec3<f32>(0.25, 0.55, 0.75), line_major * 0.5);

    // Vignette douce centrée hangar.
    let r = length(in.world.xz) / 26.0;
    let vign = clamp(r * r, 0.0, 1.0);
    base = base * (1.0 - vign * 0.25);

    return vec4<f32>(base, 1.0);
}
