// Keyframes: a clip property that changes over the clip. Each animated property holds keys at
// times measured from the clip's start; between two keys the value moves in a straight line,
// eases in and out, or holds until the next key. Speed keys make a speed ramp: the source runs
// at the keyed speed, so where the clip is in its footage is the integral of speed.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Ease {
    /// A straight line to the next key.
    #[default]
    Linear,
    /// Slow out of this key and into the next.
    Smooth,
    /// Stay at this value until the next key.
    Hold,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Key {
    /// Seconds from the clip's start.
    pub t: f64,
    pub v: f64,
    /// How the value travels from this key to the next.
    #[serde(default, skip_serializing_if = "is_linear")]
    pub ease: Ease,
}
fn is_linear(e: &Ease) -> bool {
    *e == Ease::Linear
}

/// Animated properties by name: x, y, scale, rot, opacity, volume, speed, and an effect's
/// strength or numeric parameter as `fx.<uid>.amount` / `fx.<uid>.<param>`.
pub type Keys = BTreeMap<String, Vec<Key>>;

/// The clip properties that can be keyed, with their labels.
pub const PROPS: &[(&str, &str)] = &[
    ("x", "Position X"),
    ("y", "Position Y"),
    ("scale", "Scale"),
    ("rot", "Rotation"),
    ("opacity", "Opacity"),
    ("volume", "Volume"),
    ("speed", "Speed"),
];

pub fn prop_label(prop: &str) -> String {
    if let Some((_, l)) = PROPS.iter().find(|(p, _)| *p == prop) {
        return l.to_string();
    }
    match prop.rsplit_once('.') {
        Some((_, "amount")) => "Effect strength".into(),
        Some((_, p)) => format!("Effect {p}"),
        None => prop.into(),
    }
}

/// Two keys closer than this are the same key (half a frame at 60 fps).
pub const SAME: f64 = 1.0 / 120.0;

/// Smoothstep, and its integral from 0 to `u`.
fn smooth(u: f64) -> f64 {
    u * u * (3.0 - 2.0 * u)
}
fn smooth_int(u: f64) -> f64 {
    u * u * u - u * u * u * u / 2.0
}

/// The value at `lt` seconds into the clip. Before the first key and after the last, the
/// nearest key's value holds.
pub fn value_at(keys: &[Key], lt: f64) -> Option<f64> {
    let first = keys.first()?;
    if lt <= first.t {
        return Some(first.v);
    }
    let i = keys.partition_point(|k| k.t <= lt);
    if i >= keys.len() {
        return Some(keys[keys.len() - 1].v);
    }
    let (a, b) = (keys[i - 1], keys[i]);
    let span = (b.t - a.t).max(1e-9);
    let u = ((lt - a.t) / span).clamp(0.0, 1.0);
    Some(match a.ease {
        Ease::Linear => a.v + (b.v - a.v) * u,
        Ease::Smooth => a.v + (b.v - a.v) * smooth(u),
        Ease::Hold => a.v,
    })
}

/// ∫ value dt over one segment, from its start to `u` (0–1) of the way along.
fn segment_int(a: &Key, b: &Key, u: f64) -> f64 {
    let span = b.t - a.t;
    let d = b.v - a.v;
    span * match a.ease {
        Ease::Linear => a.v * u + d * u * u / 2.0,
        Ease::Smooth => a.v * u + d * smooth_int(u),
        Ease::Hold => a.v * u,
    }
}

/// ∫ value dt from 0 to `lt`, holding the end values outside the keys. Exact for every ease.
pub fn integral(keys: &[Key], lt: f64) -> f64 {
    let Some(first) = keys.first() else { return 0.0 };
    if lt <= 0.0 {
        return 0.0;
    }
    let mut sum = first.v * first.t.clamp(0.0, lt);
    if lt <= first.t {
        return first.v * lt;
    }
    for w in keys.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if b.t <= 0.0 {
            continue;
        }
        // Part of the segment before 0 doesn't count.
        let u0 = if a.t < 0.0 { (-a.t / (b.t - a.t).max(1e-9)).clamp(0.0, 1.0) } else { 0.0 };
        if lt < b.t {
            let u = ((lt - a.t) / (b.t - a.t).max(1e-9)).clamp(0.0, 1.0);
            return sum + segment_int(a, b, u) - segment_int(a, b, u0);
        }
        sum += segment_int(a, b, 1.0) - segment_int(a, b, u0);
    }
    let last = keys[keys.len() - 1];
    sum + last.v * (lt - last.t.max(0.0))
}

/// Put a key at `t` (replacing one already there), keeping keys in time order.
pub fn upsert(keys: &mut Vec<Key>, t: f64, v: f64) {
    if let Some(k) = keys.iter_mut().find(|k| (k.t - t).abs() < SAME) {
        k.v = v;
        return;
    }
    let ease = keys.iter().rev().find(|k| k.t < t).map(|k| k.ease).unwrap_or_default();
    let i = keys.partition_point(|k| k.t < t);
    keys.insert(i, Key { t, v, ease });
}

/// Whether there's a key at `t`.
pub fn has_key_at(keys: &[Key], t: f64) -> bool {
    keys.iter().any(|k| (k.t - t).abs() < SAME)
}

/// Every keyed moment of a clip, merged across properties, in order.
pub fn key_times(all: &Keys) -> Vec<f64> {
    let mut ts: Vec<f64> = all.values().flatten().map(|k| k.t).collect();
    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|a, b| (*a - *b).abs() < SAME);
    ts
}

/// Move every key by `dt` (trimming the clip's start keeps keys on the same footage).
pub fn shift(all: &mut Keys, dt: f64) {
    for v in all.values_mut() {
        for k in v.iter_mut() {
            k.t -= dt;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn k(t: f64, v: f64, ease: Ease) -> Key {
        Key { t, v, ease }
    }
    #[test]
    fn interpolates() {
        let keys = vec![k(1.0, 0.0, Ease::Linear), k(3.0, 10.0, Ease::Hold), k(4.0, 0.0, Ease::Linear)];
        assert_eq!(value_at(&keys, 0.0), Some(0.0));
        assert_eq!(value_at(&keys, 2.0), Some(5.0));
        assert_eq!(value_at(&keys, 3.5), Some(10.0));
        assert_eq!(value_at(&keys, 9.0), Some(0.0));
    }
    #[test]
    fn integrates_like_summing() {
        let keys = vec![k(0.5, 1.0, Ease::Smooth), k(2.0, 3.0, Ease::Linear), k(3.0, 0.5, Ease::Hold), k(3.5, 2.0, Ease::Linear)];
        for lt in [0.2, 1.0, 2.0, 2.7, 3.2, 5.0] {
            let n = 200_000;
            let dt = lt / n as f64;
            let sum: f64 = (0..n).map(|i| value_at(&keys, (i as f64 + 0.5) * dt).unwrap() * dt).sum();
            assert!((integral(&keys, lt) - sum).abs() < 1e-4, "lt {lt}: {} vs {sum}", integral(&keys, lt));
        }
    }
    #[test]
    fn integrates_keys_before_zero() {
        let keys = vec![k(-1.0, 0.0, Ease::Linear), k(1.0, 2.0, Ease::Linear)];
        // From 0 to 1 the value goes 1 → 2.
        assert!((integral(&keys, 1.0) - 1.5).abs() < 1e-9);
    }
}
