#version 300 es
precision highp float;

uniform sampler2D u_from;
uniform sampler2D u_to;
uniform float u_progress;
uniform float u_intensity;
uniform float u_slices;
uniform float u_rgb_split;
uniform float u_block_noise;

in vec2 v_uv;
out vec4 outColor;

const float PI = 3.141592653589793;
const float STEPS = 22.0;
const float BLOCK_COLUMNS = 16.0;

float hash(vec2 p) {
  return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
}

vec4 sampleSplit(sampler2D tex, vec2 uv, float split) {
  vec4 center = texture(tex, uv);
  float r = texture(tex, vec2(uv.x + split, uv.y)).r;
  float b = texture(tex, vec2(uv.x - split, uv.y)).b;
  return vec4(r, center.g, b, center.a);
}

void main() {
  float energy = pow(max(sin(u_progress * PI), 0.0), 0.7);
  float clock = floor(u_progress * STEPS);

  float row = floor(v_uv.y * u_slices);
  float torn = step(0.45, hash(vec2(row, clock)));
  float amount = hash(vec2(row + 17.0, clock * 1.7 + 3.0)) - 0.5;
  float sliceShift = amount * 0.3 * u_intensity * energy * torn;

  vec2 block = vec2(floor(v_uv.x * BLOCK_COLUMNS), row);
  float corrupt = step(
    1.0 - u_block_noise * 0.5 * energy,
    hash(block + clock * 7.3)
  );
  float blockShift =
    (hash(block + clock * 3.1 + 5.0) - 0.5) * 0.2 * u_intensity * corrupt;

  vec2 uv = vec2(fract(v_uv.x + sliceShift + blockShift), v_uv.y);
  float split = u_rgb_split * 0.018 * energy * (0.35 + abs(amount));

  float cut = 0.5 + (hash(vec2(row, 91.0)) - 0.5) * 0.32;
  float handed = step(cut, u_progress);

  vec4 from = sampleSplit(u_from, uv, split);
  vec4 to = sampleSplit(u_to, uv, split);
  vec4 color = mix(from, to, handed);
  vec4 leaked = mix(to, from, handed);

  float leak = corrupt * step(0.55, hash(block + clock * 11.0 + 13.0));
  color = mix(color, leaked, leak * 0.8);

  color.rgb +=
    vec3(0.05, 0.015, 0.09) *
    energy *
    torn *
    hash(vec2(row, clock + 41.0)) *
    color.a;

  outColor = color;
}