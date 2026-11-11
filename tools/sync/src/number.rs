//! Numbers as text: the shortest text that reads back as the same number, never rounded (decisions 0011, 0014).

/// Integral floats up to this size are written without a decimal point; above it, `i64` and `f64` stop agreeing.
const MAX_EXACT_INT: f64 = 9_007_199_254_740_992.0; // 2^53

/// Writes a float as TOML: `1`, `0.5`, `-0.0`, `1e-7`, `inf`, `nan`. Reads back bit for bit (NaN payloads aside).
pub fn float(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if x == 0.0 && x.is_sign_negative() {
        return "-0.0".into();
    }
    if x.fract() == 0.0 && x.abs() <= MAX_EXACT_INT {
        return format!("{}", x as i64);
    }
    // Rust's float formatting is already the shortest exact text; the exponent form keeps very small or large
    // numbers short (5e-324 instead of 320 digits).
    let plain = format!("{x}");
    let exp = format!("{x:e}");
    if exp.len() < plain.len() { exp } else { plain }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn back(text: &str) -> f64 {
        let t: toml::Table = format!("v = {text}").parse().unwrap();
        match &t["v"] {
            toml::Value::Float(f) => *f,
            toml::Value::Integer(i) => *i as f64,
            other => panic!("not a number: {other:?}"),
        }
    }

    #[test]
    fn every_hard_case_reads_back_bit_for_bit() {
        let cases = [
            0.0,
            -0.0,
            1.0,
            -1.0,
            0.5,
            0.1 + 0.2,
            1e-6,
            -1e-7,
            5e-324,
            2.2250738585072014e-308,
            f64::MAX,
            f64::MIN,
            1e21,
            MAX_EXACT_INT,
            MAX_EXACT_INT + 2.0,
            123456.789,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f32::MAX as f64,
            (18.0f32 / 255.0) as f64,
        ];
        for x in cases {
            let text = float(x);
            assert_eq!(
                back(&text).to_bits(),
                x.to_bits(),
                "{x:?} written as {text}"
            );
        }
        assert!(back(&float(f64::NAN)).is_nan());
    }

    #[test]
    fn forms_are_short_and_readable() {
        assert_eq!(float(1.0), "1");
        assert_eq!(float(-0.0), "-0.0");
        assert_eq!(float(0.5), "0.5");
        assert_eq!(float(1e-7), "1e-7");
        assert_eq!(float(5e-324), "5e-324");
        assert_eq!(float(1e21), "1e21");
        assert_eq!(float(0.1 + 0.2), "0.30000000000000004");
    }
}
