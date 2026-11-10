struct Out {
    @builtin(position) pos: vec4f,
    @location(0) edge: f32,
    @location(1) alpha: f32,
    @location(2) color: vec3f,
    @location(3) glow: f32,
}

@vertex
fn vs(
    @location(0) pos: vec2f,
    @location(1) edge: f32,
    @location(2) alpha: f32,
    @location(3) color: vec3f,
    @location(4) glow: f32,
) -> Out {
    return Out(vec4f(pos, 0.0, 1.0), edge, alpha, color, glow);
}

// A bright, nearly white core inside a soft glow of the trail color.
@fragment
fn fs(in: Out) -> @location(0) vec4f {
    let d = abs(in.edge);
    let core = 1.0 - smoothstep(0.2, 0.3, d);
    let glow = pow(1.0 - d, 2.5) * in.glow;
    let rgb = mix(in.color, vec3f(1.0), core * 0.8);
    let a = clamp(core + glow, 0.0, 1.0) * in.alpha;
    // Premultiplied alpha.
    return vec4f(rgb * a, a);
}
