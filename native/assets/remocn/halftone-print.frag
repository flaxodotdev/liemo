#version 300 es
precision highp float;

uniform sampler2D u_scene;
uniform float u_aspect;
uniform float u_cell;
uniform float u_angle;
uniform float u_shift;
uniform vec3 u_paper;

in vec2 v_uv;
out vec4 outColor;

const vec3 CYAN_ABSORB = vec3(0.92, 0.12, 0.02);
const vec3 MAGENTA_ABSORB = vec3(0.10, 0.88, 0.06);
const vec3 YELLOW_ABSORB = vec3(0.03, 0.10, 0.90);

vec2 toSquare(vec2 uv) {
  return vec2(uv.x * u_aspect, uv.y);
}

vec2 fromSquare(vec2 p) {
  return vec2(p.x / u_aspect, p.y);
}

float screenInk(float angle, vec2 drift, vec3 pick) {
  vec2 p = toSquare(v_uv);
  float s = sin(angle);
  float c = cos(angle);
  vec2 r = vec2(p.x * c - p.y * s, p.x * s + p.y * c);

  vec2 center = (floor(r / u_cell) + 0.5) * u_cell;
  vec2 unrotated =
    vec2(center.x * c + center.y * s, -center.x * s + center.y * c);

  vec4 texel = texture(u_scene, fromSquare(unrotated + drift));
  vec3 straight = texel.rgb / max(texel.a, 0.004);
  float ink = clamp(1.0 - dot(straight, pick), 0.0, 1.0) * texel.a;

  float radius = sqrt(ink) * u_cell * 0.72;
  float aa = u_cell * 0.045;
  float coverage = 1.0 - smoothstep(radius - aa, radius + aa, length(r - center));
  return coverage * step(0.002, ink);
}

void main() {
  vec4 source = texture(u_scene, v_uv);

  float cyan =
    screenInk(u_angle + 0.2618, u_shift * vec2(1.0, 0.0), vec3(1.0, 0.0, 0.0));
  float magenta =
    screenInk(u_angle + 1.3090, u_shift * vec2(-0.5, 0.866), vec3(0.0, 1.0, 0.0));
  float yellow =
    screenInk(u_angle, u_shift * vec2(-0.5, -0.866), vec3(0.0, 0.0, 1.0));

  vec3 color = u_paper;
  color *= 1.0 - cyan * CYAN_ABSORB;
  color *= 1.0 - magenta * MAGENTA_ABSORB;
  color *= 1.0 - yellow * YELLOW_ABSORB;

  outColor = vec4(color * source.a, source.a);
}