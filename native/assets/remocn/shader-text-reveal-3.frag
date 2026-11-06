#version 300 es
precision highp float;
uniform sampler2D uText;
uniform sampler2D uHero;
uniform vec2 uResolution;
uniform vec4 uBounds;
uniform float uEnter;
uniform float uExit;
uniform float uScale;
uniform float uTime;
uniform float uIntensity;
out vec4 fragColor;

void main() {
  vec2 uv = (gl_FragCoord.xy / uResolution - 0.5) / uScale + 0.5;
  vec2 q = (uv - uBounds.xy) / uBounds.zw;
  float flow = sin(q.x * 7.0 + q.y * 5.0 - uTime * 3.2) * 0.065
    + sin(q.x * 13.0 - q.y * 8.0 + uTime * 2.1) * 0.025;
  float rank = q.x * 0.64 + (1.0 - q.y) * 0.36 + flow;
  float d = rank - mix(-0.15, 1.65, uEnter);
  float edge = exp(-pow((d + 0.1) * 6.0, 2.0));
  // Refraction affects the glyph itself, concentrated inside the wet leading edge.
  vec3 field = texture(uHero, uv).rgb;
  vec2 warp = vec2(field.r - field.b, field.g - field.r);
  vec2 sampleUV = uv + warp * edge * uIntensity * 2.5 / uResolution;
  float glyph = texture(uText, sampleUV).a;
  float appear = 1.0 - smoothstep(-0.025, 0.09, d);
  float exitRank = q.x * 0.65 + (1.0 - q.y) * 0.5 + flow;
  float leave = smoothstep(-0.12, 0.06, exitRank - mix(-0.25, 1.45, uExit));

  vec3 liquid = texture(uHero, sampleUV).rgb;
  vec3 silver = vec3(0.96, 0.925, 1.0);
  float settled = 1.0 - smoothstep(-0.58, -0.16, d);
  vec3 color = mix(liquid, silver, settled);
  color = mix(silver, color, uIntensity);
  float alpha = glyph * appear * leave;
  fragColor = vec4(color * alpha, alpha);
}