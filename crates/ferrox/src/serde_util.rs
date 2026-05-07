/// Custom serde for f64 bounds that maps ±infinity to ±1e308 in JSON.
///
/// JSON has no infinity literal. This module round-trips finite values exactly
/// and maps `f64::INFINITY` ↔ `1e308` and `f64::NEG_INFINITY` ↔ `-1e308`.
/// Apply with `#[serde(with = "crate::serde_util::f64_inf")]` on lb/ub fields.
#[allow(dead_code)]
pub mod f64_inf {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    const INF_PROXY: f64 = 1e308;

    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn serialize<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
        let proxy = if *v == f64::INFINITY {
            INF_PROXY
        } else if *v == f64::NEG_INFINITY {
            -INF_PROXY
        } else {
            *v
        };
        proxy.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        let v = f64::deserialize(d)?;
        Ok(if v >= INF_PROXY {
            f64::INFINITY
        } else if v <= -INF_PROXY {
            f64::NEG_INFINITY
        } else {
            v
        })
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    struct Bound {
        #[serde(with = "super::f64_inf")]
        v: f64,
    }

    #[test]
    fn finite_value_round_trips() {
        let b = Bound { v: 3.5 };
        let s = serde_json::to_string(&b).unwrap();
        let r: Bound = serde_json::from_str(&s).unwrap();
        assert!((r.v - 3.5).abs() < f64::EPSILON);
    }

    #[test]
    fn positive_infinity_round_trips() {
        let b = Bound { v: f64::INFINITY };
        let s = serde_json::to_string(&b).unwrap();
        // The serialized form must be plain JSON (no infinity literal).
        assert!(!s.contains("inf") && !s.contains("Infinity"));
        let r: Bound = serde_json::from_str(&s).unwrap();
        assert!(r.v.is_infinite() && r.v.is_sign_positive());
    }

    #[test]
    fn negative_infinity_round_trips() {
        let b = Bound {
            v: f64::NEG_INFINITY,
        };
        let s = serde_json::to_string(&b).unwrap();
        let r: Bound = serde_json::from_str(&s).unwrap();
        assert!(r.v.is_infinite() && r.v.is_sign_negative());
    }
}
