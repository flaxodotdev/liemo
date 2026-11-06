#version 300 es
precision highp float;

uniform sampler2D u_scene;
uniform vec4 u_regions[8];
uniform int u_region_count;
uniform vec2 u_cell;

in vec2 v_uv;
out vec4 outColor;

const int TAPS = 3;

vec4 cellAverage(vec2 center, vec2 cell) {
  vec4 sum = vec4(0.0);
  for (int y = 0; y < TAPS; y++) {
    for (int x = 0; x < TAPS; x++) {
      vec2 offset =
        (vec2(float(x), float(y)) + 0.5) / float(TAPS) - 0.5;
      sum += texture(u_scene, center + offset * cell);
    }
  }
  return sum / float(TAPS * TAPS);
}

void main() {
  vec4 result = texture(u_scene, v_uv);
  vec2 cell = max(u_cell, vec2(0.0005));

  for (int i = 0; i < 8; i++) {
    if (i >= u_region_count) {
      continue;
    }
    vec4 region = u_regions[i];
    vec2 lo = region.xy;
    vec2 hi = region.xy + region.zw;
    vec2 hit = step(lo, v_uv) * step(v_uv, hi);
    if (hit.x * hit.y < 0.5) {
      continue;
    }
    vec2 center = lo + (floor((v_uv - lo) / cell) + 0.5) * cell;
    result = vec4(cellAverage(center, cell).rgb, 1.0);
  }

  outColor = result;
}