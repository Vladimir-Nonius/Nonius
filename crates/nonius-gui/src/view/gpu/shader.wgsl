// Отрисовка детали: свет от камеры с обеих сторон поверхности, рёбра — ровным цветом.

struct Draw {
    view_proj: mat4x4<f32>,
    color: vec4<f32>,
    // Направление на свет, в осях модели.
    light: vec4<f32>,
    // x — сдвиг глубины (плюс — назад), y — 1 для освещения, 0 для ровного цвета.
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Draw;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
};

@vertex
fn vs_main(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>) -> Out {
    var out: Out;
    var p = u.view_proj * vec4<f32>(pos, 1.0);
    p.z = p.z + u.params.x * p.w;
    out.position = p;
    out.normal = normal;
    return out;
}

@fragment
fn fs_main(in: Out) -> @location(0) vec4<f32> {
    if (u.params.y < 0.5) {
        return u.color;
    }
    let n = normalize(in.normal);
    let l = normalize(u.light.xyz);
    let key = abs(dot(n, l));
    // Заполняющий свет сверху, чтобы грани в тени не сливались.
    let fill = 0.5 + 0.5 * n.y;
    let shade = 0.30 + 0.60 * key + 0.10 * fill;
    return vec4<f32>(u.color.rgb * shade, u.color.a);
}
