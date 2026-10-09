//! JavaScript's `Number.prototype.toString()` for numeric literal names: `0x10` names `16`,
//! `1e21` names `1e+21`, and `1_000` names `1000`, as TypeScript spells a numeric property name.

/// The ECMAScript decimal spelling of `value` (Number::toString with radix 10).
pub(crate) fn js_number(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value == 0.0 {
        return "0".to_owned();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-Infinity" } else { "Infinity" }.to_owned();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    // Rust's `{:e}` prints the shortest digits that round-trip, like ECMAScript requires.
    let scientific = format!("{:e}", value.abs());
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let exponent: i64 = exponent.parse().unwrap_or(0);
    // ECMAScript writes the value as 0.d1d2…dk × 10^n.
    let k = digits.len() as i64;
    let n = exponent + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        let (whole, fraction) = digits.split_at(n as usize);
        format!("{whole}.{fraction}")
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let power = n - 1;
        let sign = if power < 0 { '-' } else { '+' };
        let (first, rest) = digits.split_at(1);
        if rest.is_empty() {
            format!("{first}e{sign}{}", power.abs())
        } else {
            format!("{first}.{rest}e{sign}{}", power.abs())
        }
    };
    format!("{sign}{body}")
}

#[cfg(test)]
mod tests {
    use super::js_number;

    #[test]
    fn spells_numbers_like_javascript() {
        let cases = [
            (16.0, "16"),
            (42.0, "42"),
            (1000.0, "1000"),
            (0.5, "0.5"),
            (1.5e-7, "1.5e-7"),
            (0.000001, "0.000001"),
            (1e21, "1e+21"),
            (1.25e21, "1.25e+21"),
            (123_456_789_012_345_680_000.0, "123456789012345680000"),
            (0.1 + 0.2, "0.30000000000000004"),
            (0.0, "0"),
            (-2.5, "-2.5"),
            (f64::INFINITY, "Infinity"),
            (f64::NAN, "NaN"),
        ];
        for (value, expected) in cases {
            assert_eq!(js_number(value), expected, "{value}");
        }
    }
}
