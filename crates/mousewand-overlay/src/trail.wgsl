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

// A solid body of the trail color, a little lighter down the middle, inside
// a soft glow. `edge` runs from -1 to 1 across the ribbon.
@fragment
fn fs(in: Out) -> @location(0) vec4f {
    let d = abs(in.edge);
    // One pixel of anti-aliasing at the body's edge, however wide it is.
    let aa = max(fwidth(in.edge), 1e-4);
    let body = 1.0 - smoothstep(0.32 - aa, 0.32 + aa, d);
    let glow = pow(1.0 - d, 2.0) * in.glow * 0.7;
    let light = 1.0 - smoothstep(0.0, 0.2, d);
    let rgb = mix(in.color, vec3f(1.0), light * 0.3);
    let a = clamp(body + glow, 0.0, 1.0) * in.alpha;
    // Premultiplied alpha.
    return vec4f(rgb * a, a);
}
