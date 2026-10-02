// ============================================================
//  Scene shader — cubes instanciés, lambert stylisé + brouillard
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
    // per-instance
    @location(3) model0 : vec4<f32>,
    @location(4) model1 : vec4<f32>,
    @location(5) model2 : vec4<f32>,
    @location(6) model3 : vec4<f32>,
    @location(7) tint   : vec4<f32>,
};

struct VertexOutput {
    @builtin(position) pos    : vec4<f32>,
    @location(0) world        : vec3<f32>,
    @location(1) normal       : vec3<f32>,
    @location(2) color        : vec3<f32>,
};

@vertex
fn vs_main(in : VertexInput) -> VertexOutput {
    let model = mat4x4<f32>(in.model0, in.model1, in.model2, in.model3);
    let world = model * vec4<f32>(in.pos, 1.0);
    var out : VertexOutput;
    out.pos    = camera.view_proj * world;
    out.world  = world.xyz;
    out.normal = normalize((model * vec4<f32>(in.normal, 0.0)).xyz);
    out.color  = in.color * in.tint.rgb;
    return out;
}

@fragment
fn fs_main(in : VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    // Soleil stylisé (chaud) + remplissage froid + hémisphérique.
    let sun_dir  = normalize(vec3<f32>(0.45, 1.0, 0.3));
    let fill_dir = normalize(vec3<f32>(-0.6, 0.25, -0.5));
    let lambert = max(dot(n, sun_dir), 0.0);
    let fill    = max(dot(n, fill_dir), 0.0) * 0.22;
    let hemi    = 0.5 + 0.5 * n.y; // ciel-sol
    let ambient = mix(0.30, 0.48, hemi);

    // Speculaire discret (type plastique/jouet).
    let view_dir = normalize(camera.cam_pos.xyz - in.world);
    let half_v   = normalize(sun_dir + view_dir);
    let spec     = pow(max(dot(n, half_v), 0.0), 32.0) * 0.18;

    var color = in.color * (ambient + lambert * 0.72 + fill) + vec3<f32>(spec);

    // Brouillard léger vers les bords du hangar.
    let fog = clamp((length(in.world.xz) - 18.0) / 70.0, 0.0, 0.45);
    let fog_color = vec3<f32>(0.055, 0.065, 0.085);
    color = mix(color, fog_color, fog);

    return vec4<f32>(color, 1.0);
}
