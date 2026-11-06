#version 300 es
precision highp float;

uniform sampler2D u_from;
uniform sampler2D u_to;
uniform float u_progress;
uniform float u_aspect;
uniform vec2 u_grid;
uniform float u_wave;
uniform float u_lift;
uniform float u_gap;
uniform vec3 u_tint;
uniform vec2 u_dir;
uniform float u_radial;

in vec2 v_uv;
out vec4 outColor;

float hash(vec2 p) {
  return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
}

float waveCoord(vec2 p) {
  vec2 off = (p - 0.5) * vec2(u_aspect, 1.0);
  float ring = length(off) / (0.5 * sqrt(u_aspect * u_aspect + 1.0));
  float sweep = clamp(dot(p - 0.5, u_dir) + 0.5, 0.0, 1.0);
  return mix(sweep, ring, u_radial);
}

void main() {
  float front = u_progress * (1.0 + 6.0 * u_wave) - 3.0 * u_wave;

  float selfPass =
    smoothstep(-u_wave * 0.4, u_wave * 0.4, front - waveCoord(v_uv));
  vec4 base = mix(texture(u_from, v_uv), texture(u_to, v_uv), selfPass);

  float edge = 0.5 - u_gap * 0.5;
  vec2 anchor = floor(v_uv * u_grid);

  vec4 face = vec4(0.0);
  float depth = -1.0;

  for (int oy = -1; oy <= 1; oy++) {
    for (int ox = -1; ox <= 1; ox++) {
      vec2 cell = anchor + vec2(float(ox), float(oy));
      vec2 centre = (cell + 0.5) / u_grid;

      float stagger = (hash(cell) - 0.5) * u_wave * 0.4;
      float w = front - waveCoord(centre) + stagger;
      float band = w / u_wave;
      float pulse = exp(-band * band);
      if (pulse <= depth) {
        continue;
      }

      float rise = pulse * u_lift;
      vec2 lean = centre - 0.5;
      lean = lean / max(length(lean), 0.0001) * rise * 0.15;
      float grow = 1.0 + rise * 0.5;

      vec2 q = (v_uv - centre) * u_grid;
      vec2 top = (q - lean) / grow;
      float onTop = max(abs(top.x), abs(top.y));
      float onFoot = max(abs(q.x), abs(q.y));
      bool lit = onTop <= edge;
      if (!lit && onFoot > edge) {
        continue;
      }
      depth = pulse;

      float pass = smoothstep(-u_wave * 0.4, u_wave * 0.4, w);
      vec2 local = lit ? top : clamp(top, -edge, edge);
      vec2 src = centre + local / u_grid;
      vec4 col = mix(texture(u_from, src), texture(u_to, src), pass);

      if (lit) {
        col.rgb = mix(col.rgb, u_tint, pulse * 0.12);
        col.rgb *= 1.0 + pulse * 0.06;
      } else {
        col.rgb = mix(col.rgb, u_tint, pulse * 0.06);
        col.rgb *= 1.0 - pulse * 0.24;
      }

      face = col;
    }
  }

  outColor = face + base * (1.0 - face.a);
}