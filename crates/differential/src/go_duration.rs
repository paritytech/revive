use std::time::Duration;

/// Parse a go formatted duration.
///
/// Sources:
/// - https://crates.io/crates/go-parse-duration (fixed an utf8 bug)
/// - https://github.com/golang/go/blob/master/src/time/format.go
pub fn parse_go_duration(value: &str) -> Result<Duration, String> {
    parse_duration(value).map(|ns| Duration::from_nanos(ns.abs() as u64))
}

fn parse_duration(string: &str) -> Result<i64, String> {
    // [-+]?([0-9]*(\.[0-9]*)?[a-z]+)+
    let mut s = string;
    let mut d: i64 = 0; // duration to be returned
    let mut neg = false;

    // Consume [-+]?
    if s != "" {
        let c = *s.as_bytes().get(0).unwrap();
        if c == b'-' || c == b'+' {
            neg = c == b'-';
            s = &s[1..];
        }
    }
    // Special case: if all that is left is "0", this is zero.
    if s == "0" {
        return Ok(0);
    }
    if s == "" {
        return Err(format!("invalid duration: {string}"));
    }
    while s != "" {
        // integers before, after decimal point
        let mut v: i64;
        let mut f: i64 = 0;
        // value = v + f / scale
        let mut scale: f64 = 1f64;

        // The next character must be [0-9.]
        let c = *s.as_bytes().get(0).unwrap();
        if !(c == b'.' || b'0' <= c && c <= b'9') {
            return Err(format!("invalid duration: {string}"));
        }
        // Consume [0-9]*
        let pl = s.len();
        match leading_int(s) {
            Ok((_v, _s)) => {
                v = _v;
                s = _s;
            }
            Err(_) => {
                return Err(format!("invalid duration: {string}"));
            }
        }
        let pre = pl != s.len(); // whether we consume anything before a period

        // Consume (\.[0-9]*)?
        let mut post = false;
        if s != "" && *s.as_bytes().get(0).unwrap() == b'.' {
            s = &s[1..];
            let pl = s.len();
            match leading_fraction(s) {
                (f_, scale_, s_) => {
                    f = f_;
                    scale = scale_;
                    s = s_;
                }
            }
            post = pl != s.len();
        }
        if !pre && !post {
            // no digits (e.g. ".s" or "-.s")
            return Err(format!("invalid duration: {string}"));
        }

        // Consume unit.
        let mut i = 0;
        while i < s.len() {
            let c = *s.as_bytes().get(i).unwrap();
            if c == b'.' || b'0' <= c && c <= b'9' {
                break;
            }
            i += 1;
        }
        if i == 0 {
            return Err(format!("missing unit in duration: {string}"));
        }
        let u = &s[..i];
        s = &s[i..];
        let unit = match u {
            "ns" => 1i64,
            "us" => 1000i64,
            "µs" => 1000i64, // U+00B5 = micro symbol
            "μs" => 1000i64, // U+03BC = Greek letter mu
            "ms" => 1000000i64,
            "s" => 1000000000i64,
            "m" => 60000000000i64,
            "h" => 3600000000000i64,
            _ => {
                return Err(format!("unknown unit {u} in duration {string}"));
            }
        };
        if v > (1 << 63 - 1) / unit {
            // overflow
            return Err(format!("invalid duration {string}"));
        }
        v *= unit;
        if f > 0 {
            // f64 is needed to be nanosecond accurate for fractions of hours.
            // v >= 0 && (f*unit/scale) <= 3.6e+12 (ns/h, h is the largest unit)
            v += (f as f64 * (unit as f64 / scale)) as i64;
            if v < 0 {
                // overflow
                return Err(format!("invalid duration {string}"));
            }
        }
        d += v;
        if d < 0 {
            // overflow
            return Err(format!("invalid duration {string}"));
        }
    }
    if neg {
        d = -d;
    }
    Ok(d)
}

fn leading_int(s: &str) -> Result<(i64, &str), String> {
    let mut x = 0;
    let mut i = 0;
    while i < s.len() {
        let c = s.chars().nth(i).unwrap();
        if c < '0' || c > '9' {
            break;
        }
        if x > (1 << 63 - 1) / 10 {
            return Err("overflow".into());
        }
        let d = i64::from(c.to_digit(10).unwrap());
        x = x * 10 + d;
        if x < 0 {
            // overflow
            return Err("overflow".into());
        }
        i += 1;
    }
    Ok((x, &s[i..]))
}

fn leading_fraction(s: &str) -> (i64, f64, &str) {
    let mut i = 0;
    let mut x = 0i64;
    let mut scale = 1f64;
    let mut overflow = false;
    while i < s.len() {
        let c = s.chars().nth(i).unwrap();
        if c < '0' || c > '9' {
            break;
        }
        if overflow {
            continue;
        }
        if x > (1 << 63 - 1) / 10 {
            // It's possible for overflow to give a positive number, so take care.
            overflow = true;
            continue;
        }
        let d = i64::from(c.to_digit(10).unwrap());
        let y = x * 10 + d;
        if y < 0 {
            overflow = true;
            continue;
        }
        x = y;
        scale *= 10f64;
        i += 1;
    }
    (x, scale, &s[i..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("8.731µs"), Ok(8731));
        assert_eq!(parse_duration("50ns"), Ok(50));
        assert_eq!(parse_duration("3ms"), Ok(3000000));
        assert_eq!(parse_duration("2us"), Ok(2000));
        assert_eq!(parse_duration("4.0s"), Ok(4000000000));
        assert_eq!(parse_duration("1h45m"), Ok(6300000000000));
        assert_eq!(
            parse_duration("1"),
            Err(String::from("missing unit in duration: 1")),
        );
    }
}
