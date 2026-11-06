#version 300 es
precision highp float;
// Rayna's GPU take on remocn's lens-zoom: the same lens magnification curve, blur sample
// weights, per-channel scale split and shake, sampled per pixel instead of stacked CSS copies.

uniform sampler2D u_scene;
uniform vec2 u_res;
uniform float u_base;      // scale / 100
uniform float u_theta;     // half the lens fov, radians
uniform float u_blur;      // remocn blur amount, 0..100
uniform float u_samples;
uniform float u_fade;
uniform float u_rot;       // radians
uniform vec2 u_shake;      // px
uniform float u_shake_rot; // radians

in vec2 v_uv;
out vec4 outColor;

float mag(float r) {
  if (u_theta < 1e-4) return 1.0;
  float peak = tan(u_theta) / u_theta;
  if (r <= 0.0) return peak;
  float a = r * u_theta;
  if (a >= 1.5698) return 1.0;
  float rs = tan(a) / tan(u_theta);
  if (rs <= 1e-6) return peak;
  return max(1.0, r / rs);
}

vec2 rot(vec2 v, float a) {
  float c = cos(a), s = sin(a);
  return vec2(c * v.x - s * v.y, s * v.x + c * v.y);
}

vec3 lens(vec2 p, float chan) {
  // p: px from the centre, after shake and channel scale.
  vec2 c = u_res * 0.5;
  vec3 acc = vec3(0.0);
  float total = 0.0;
  int n = int(u_samples);
  for (int j = 0; j < 16; j++) {
    if (j >= n) break;
    float w = pow(1.0 - float(j) / u_samples, u_fade);
    float g = n == 1 ? 1.0 : 1.0 + (u_blur / 100.0) * (float(j) / (u_samples - 1.0));
    float r = length(p) / (c.x * g);
    float k = g * mag(r) * u_base;
    vec2 q = rot(p, -u_rot) / k + c;
    vec2 uv = q / u_res;
    vec4 s = (uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0) ? vec4(0.0) : texture(u_scene, uv);
    acc += s.rgb * s.a * w;
    total += w;
  }
  return acc / total;
}

void main() {
  vec2 c = u_res * 0.5;
  vec2 p = v_uv * u_res - c;
  p = rot(p - u_shake, -u_shake_rot);
  float r = lens(p / 1.01, 0.0).r;
  float g = lens(p, 1.0).g;
  float b = lens(p / 0.99, 2.0).b;
  outColor = vec4(r, g, b, 1.0);
}
