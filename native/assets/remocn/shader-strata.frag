
precision highp float;

uniform vec2 u_res;
uniform float u_time;
uniform float u_layers;
uniform float u_amp;
uniform vec3 u_c0;
uniform vec3 u_c1;
uniform vec3 u_c2;
uniform vec3 u_c3;
uniform vec3 u_accent;
uniform float u_accentAmt;

float hash(vec2 p) {
  return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
}

float noise(vec2 p) {
  vec2 i = floor(p);
  vec2 f = fract(p);
  vec2 u = f * f * (3.0 - 2.0 * f);
  return mix(
    mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x),
    mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x),
    u.y
  );
}

float fbm(vec2 p) {
  float v = 0.0;
  float a = 0.5;
  for (int i = 0; i < 4; i++) {
    v += a * noise(p);
    p *= 2.03;
    a *= 0.5;
  }
  return v;
}

void main() {
  vec2 uv = gl_FragCoord.xy / u_res;
  float aspect = u_res.x / u_res.y;

  // Deeper layers drift faster — parallax without per-band recursion.
  float drift = u_time * (0.012 + 0.05 * (1.0 - uv.y));
  float w = fbm(vec2(uv.x * 2.6 * aspect + drift, uv.y * 2.2 - u_time * 0.01));

  float yd = uv.y + (w - 0.5) * u_amp;
  float band = yd * u_layers;
  float idx = floor(band);
  float f = fract(band);

  // Per-band value jitter + a soft sediment shadow at each band's lower edge.
  float j = hash(vec2(idx, 7.0));
  float edge = smoothstep(0.0, 0.4, f) * (1.0 - 0.3 * smoothstep(0.8, 1.0, f));

  float depth = clamp(idx / u_layers, 0.0, 1.0);
  vec3 col = mix(u_c0, u_c1, smoothstep(0.0, 0.4, depth));
  col = mix(col, u_c2, smoothstep(0.35, 0.75, depth));
  col = mix(col, u_c3, smoothstep(0.7, 1.0, depth));
  col *= 0.84 + 0.16 * j;
  col *= 0.92 + 0.08 * edge;

  // The accent rises from the deep strata and thins toward the surface.
  float mask = (1.0 - uv.y);
  mask *= mask;
  col = mix(col, u_accent, clamp(u_accentAmt, 0.0, 1.0) * mask * 0.38);

  // A whisper of living grain, time-derived so it stays deterministic.
  col += (hash(gl_FragCoord.xy + floor(u_time * 9.0)) - 0.5) * 0.012;

  gl_FragColor = vec4(col, 1.0);
}
