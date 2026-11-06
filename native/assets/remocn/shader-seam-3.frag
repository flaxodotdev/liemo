#version 300 es
precision highp float;
uniform vec2 iResolution;
uniform sampler2D tScene;
uniform sampler2D tField;
uniform float uPresence;
uniform float uDissolve;
uniform float uSoftness;
uniform float uDetail;
out vec4 fragColor;

float hash(vec2 p) {
  return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}
float noise(vec2 p) {
  vec2 i = floor(p), f = fract(p);
  f = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash(i), hash(i + vec2(1, 0)), f.x),
    mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), f.x), f.y);
}
void main() {
  vec2 uv = gl_FragCoord.xy / iResolution;
  vec2 p = gl_FragCoord.xy / iResolution.y;
  vec3 field = texture(tField, uv).rgb;
  float energy = max(field.r, max(field.g, field.b));
  // Static, continuous noise breaks up the field without making it travel.
  float grain = noise(p * 7.0) * 0.6 + noise(p * 19.0) * 0.28
    + noise(p * 43.0) * 0.12;
  float density = mix(smoothstep(0.025, 0.95, energy), grain, uDetail * 0.55);
  float appear = mix(1.0 + uSoftness, -uSoftness, uPresence);
  float disappear = mix(-uSoftness, 1.0 + uSoftness, uDissolve);
  float alpha = smoothstep(appear - uSoftness, appear + uSoftness, density)
    * smoothstep(disappear - uSoftness, disappear + uSoftness, density);
  vec3 material = texture(tScene, uv).rgb;
  fragColor = vec4(material * alpha, alpha);
}
