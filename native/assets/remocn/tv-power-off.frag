#version 300 es
precision highp float;

uniform sampler2D u_scene;
uniform vec2 u_resolution;
uniform float u_aspect;
uniform float u_progress;
uniform float u_gain;
uniform float u_afterglow;
uniform vec3 u_phosphor;

in vec2 v_uv;
out vec4 outColor;

const int TAPS = 32;
const int COLUMN_TAPS = 8;
const float GOLDEN = 0.6180339887;
const float COLLAPSE_END = 0.6000;
const float PINCH_END = 0.8200;
const float MIN_SQUASH = 0.00550;
const float MIN_PINCH = 0.00400;
const float CONCENTRATION = 0.3000;
const float DECAY_CURVE = 2.6000;
const float BLACKOUT_IN = 0.0600;

void main() {
  float p = u_progress;

  if (p <= 0.0) {
    outColor = texture(u_scene, v_uv);
    return;
  }
  if (p >= 1.0) {
    outColor = vec4(0.0, 0.0, 0.0, 1.0);
    return;
  }

  float collapse = clamp(p / COLLAPSE_END, 0.0, 1.0);
  float pinch = clamp((p - COLLAPSE_END) / (PINCH_END - COLLAPSE_END), 0.0, 1.0);
  float decayed = clamp((p - PINCH_END) / (1.0 - PINCH_END), 0.0, 1.0);

  vec2 scale = vec2(pow(MIN_PINCH, pinch), pow(MIN_SQUASH, collapse));
  vec2 centered = (v_uv - 0.5) / scale + 0.5;
  vec2 span = 1.0 / (scale * u_resolution);

  vec3 sum = vec3(0.0);
  for (int i = 0; i < TAPS; i++) {
    float fi = float(i);
    vec2 at = centered + vec2(
      (fract(fi * GOLDEN + 0.5) - 0.5) * span.x,
      ((fi + 0.5) / float(TAPS) - 0.5) * span.y
    );
    vec2 inside = step(vec2(0.0), at) * step(at, vec2(1.0));
    sum += texture(u_scene, at).rgb * inside.x * inside.y;
  }

  float concentrate = pow(1.0 / (scale.x * scale.y), CONCENTRATION);
  vec3 light =
    (sum / float(TAPS)) * (1.0 + (concentrate - 1.0) * max(u_gain, 0.0));

  float glow = max(u_afterglow, 0.0);
  float bleed = smoothstep(0.2, 0.8, collapse) * (1.0 - pinch) * glow;

  if (bleed > 0.0) {
    vec3 column = vec3(0.0);
    float cx = clamp(centered.x, 0.0, 1.0);
    for (int i = 0; i < COLUMN_TAPS; i++) {
      column +=
        texture(u_scene, vec2(cx, (float(i) + 0.5) / float(COLUMN_TAPS))).rgb;
    }
    column /= float(COLUMN_TAPS);

    float dy = v_uv.y - 0.5;
    float reach = 0.02 + 0.03 * collapse;
    light +=
      column *
      mix(vec3(1.0), u_phosphor, 0.65) *
      exp(-(dy * dy) / (reach * reach)) *
      bleed *
      0.9;
  }

  if (pinch > 0.0) {
    vec2 d = (v_uv - 0.5) * vec2(u_aspect, 1.0);
    float radius = max(0.045 * glow, 0.0005);
    light +=
      u_phosphor *
      exp(-dot(d, d) / (radius * radius)) *
      smoothstep(0.0, 0.5, pinch) *
      1.15;
  }

  light *= pow(max(1.0 - decayed, 0.0001), DECAY_CURVE);

  float blackout = smoothstep(0.0, BLACKOUT_IN, p);
  outColor = vec4(
    clamp(light, 0.0, 1.0),
    mix(texture(u_scene, v_uv).a, 1.0, blackout)
  );
}