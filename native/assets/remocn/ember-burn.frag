#version 300 es
precision highp float;

uniform sampler2D u_from;
uniform sampler2D u_to;
uniform float u_progress;
uniform float u_aspect;
uniform float u_patches;
uniform float u_softness;
uniform float u_content;
uniform float u_heat;
uniform vec3 u_glow;
uniform float u_embers;

in vec2 v_uv;
out vec4 outColor;

const float EMBER_GRID = 110.0;

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
  float sum = 0.0;
  float amp = 0.5;
  for (int i = 0; i < 5; i++) {
    sum += noise(p) * amp;
    p *= 2.03;
    amp *= 0.5;
  }
  return sum * 1.032;
}

float luma(vec3 c) {
  return dot(c, vec3(0.2126, 0.7152, 0.0722));
}

void main() {
  vec2 skewed = vec2(v_uv.x * u_aspect, v_uv.y);
  float alive =
    smoothstep(0.0, 0.06, u_progress) * smoothstep(1.0, 0.94, u_progress);

  float brightness = luma(texture(u_from, v_uv).rgb);
  float field = clamp(
    clamp(fbm(skewed * u_patches), 0.0, 1.0) *
      (1.0 - u_content * brightness * 0.9),
    0.0,
    1.0
  );

  float soft = max(u_softness, 0.004);
  float margin = soft * 3.0 + 0.02;
  float front = mix(-margin, 1.0 + margin, pow(u_progress, 1.2));
  float depth = field - front;

  float intact = smoothstep(-soft, soft, depth);
  float arrived = 1.0 - intact;

  vec2 shimmer = vec2(
    noise(skewed * u_patches * 2.4 + vec2(0.0, u_progress * 3.1)) - 0.5,
    noise(skewed * u_patches * 2.4 + vec2(5.2, 1.7 + u_progress * 4.3)) - 0.5
  );
  float heatBand = depth / (soft * 6.0);
  float boil = exp(-heatBand * heatBand) * alive * u_heat * 0.14;
  vec4 from = texture(u_from, v_uv + shimmer * boil * intact);
  vec4 to = texture(u_to, v_uv - shimmer * boil * arrived);

  float ramp = depth / (soft * 2.2);
  float charBand = (ramp - 0.55) * 1.5;
  float moltenBand = (ramp + 0.55) * 1.5;
  float rimBand = depth / (soft * 1.1);
  float charred = intact * exp(-charBand * charBand) * alive;
  float molten = arrived * exp(-moltenBand * moltenBand) * alive;
  float rim = exp(-rimBand * rimBand);

  float since = clamp(-depth / (soft * 3.0), 0.0, 1.0);
  float emberBand = depth / (soft * 2.6);
  vec2 emberSpace = skewed + vec2(0.0, u_progress * 0.42);
  float cellSeed = hash(floor(emberSpace * EMBER_GRID));
  float ember =
    step(1.0 - u_embers * 0.08, cellSeed) *
    arrived *
    exp(-emberBand * emberBand) *
    alive;
  vec3 sparkSource = texture(u_from, v_uv + vec2(0.0, since * 0.07)).rgb;

  vec4 color = mix(to, from, intact);
  color.rgb *= 1.0 - charred * 0.92;
  color.rgb += (color.rgb * 1.2 + u_glow * 0.6) * molten;

  vec3 spark = mix(u_glow, sparkSource * 1.4 + u_glow * 0.3, 0.5);
  float light = min(rim * 0.85, 1.4);
  color.rgb += u_glow * light + spark * ember * 1.3;
  color.a = clamp(color.a + light + molten + ember, 0.0, 1.0);

  outColor = color;
}