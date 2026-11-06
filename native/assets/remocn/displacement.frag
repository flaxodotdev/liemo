#version 300 es
precision highp float;

uniform sampler2D u_from;
uniform sampler2D u_to;
uniform float u_progress;
uniform float u_aspect;
uniform vec2 u_cells;
uniform float u_shift;
uniform float u_aberration;
uniform float u_grain;
uniform float u_stagger;

in vec2 v_uv;
out vec4 outColor;

const float PI = 3.141592653589793;

float hash(vec2 p) {
  return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
}

void main() {
  vec2 cell = floor(v_uv * u_cells);
  float h1 = hash(cell);
  float h2 = hash(cell + 7.3);
  float h3 = hash(cell + 13.1);

  float delay = h3 * u_stagger;
  float phase =
    clamp((u_progress - delay) / max(1.0 - u_stagger, 0.1), 0.0, 1.0);
  float shear = sin(phase * PI);

  vec2 aim = vec2((h1 - 0.5) * 2.0, (h2 - 0.5) * 0.7);
  vec2 dir = aim / max(length(aim), 0.0001);
  vec2 slide = dir * shear * u_shift * 1.4;
  vec2 off = slide / u_cells;
  vec2 fringe = off * u_aberration * 0.35;

  float swap = smoothstep(0.34, 0.66, phase);

  vec4 mid = mix(
    texture(u_from, v_uv - off),
    texture(u_to, v_uv - off),
    swap
  );
  float red = mix(
    texture(u_from, v_uv - off - fringe).r,
    texture(u_to, v_uv - off - fringe).r,
    swap
  );
  float blue = mix(
    texture(u_from, v_uv - off + fringe).b,
    texture(u_to, v_uv - off + fringe).b,
    swap
  );

  vec4 col = vec4(red, mid.g, blue, mid.a);

  float tick = floor(u_progress * 30.0);
  float speck =
    hash(floor(v_uv * vec2(u_aspect, 1.0) * 460.0) + tick * 1.7) - 0.5;
  col.rgb += vec3(speck * u_grain * shear * 0.5) * col.a;
  col.rgb *= 1.0 - 0.12 * shear;

  outColor = col;
}