// The hint panel: one premultiplied image, faded as a whole.

struct Out {
    @builtin(position) pos: vec4f,
    @location(0) uv: vec2f,
    @location(1) alpha: f32,
}

@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var pixels: sampler;

@vertex
fn vs(@location(0) pos: vec2f, @location(1) uv: vec2f, @location(2) alpha: f32) -> Out {
    return Out(vec4f(pos, 0.0, 1.0), uv, alpha);
}

@fragment
fn fs(in: Out) -> @location(0) vec4f {
    return textureSample(image, pixels, in.uv) * in.alpha;
}
