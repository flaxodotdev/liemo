#version 300 es
precision highp float;

uniform sampler2D u_scene;
uniform sampler2D u_atlas;
uniform vec2 u_cell;
uniform float u_glyph_count;
uniform float u_colored;
uniform vec3 u_ink;
uniform float u_intensity;

in vec2 v_uv;
out vec4 outColor;

const int TAPS = 3;

vec4 cellAverage(vec2 center, vec2 cell) {
  vec4 sum = vec4(0.0);
  for (int y = 0; y < TAPS; y++) {
    for (int x = 0; x < TAPS; x++) {
      vec2 offset = (vec2(float(x), float(y)) + 0.5) / float(TAPS) - 0.5;
      sum += texture(u_scene, center + offset * cell);
    }
  }
  return sum / float(TAPS * TAPS);
}

void main() {
  vec2 cell = max(u_cell, vec2(0.001));
  vec2 origin = floor(v_uv / cell) * cell;
  vec4 sampled = cellAverage(origin + cell * 0.5, cell);

  float luma = dot(sampled.rgb, vec3(0.299, 0.587, 0.114));
  float index = min(floor(luma * u_glyph_count), u_glyph_count - 1.0);

  vec2 local = clamp((v_uv - origin) / cell, 0.008, 0.992);
  float mask =
    texture(u_atlas, vec2((index + local.x) / u_glyph_count, local.y)).r;

  vec3 glyph = mix(u_ink, sampled.rgb, u_colored) * mask;
  vec4 source = texture(u_scene, v_uv);
  float amount = clamp(u_intensity, 0.0, 1.0);

  outColor = vec4(
    mix(source.rgb, glyph, amount),
    mix(source.a, max(mask, sampled.a), amount)
  );
}