//! Python の値の表示（str()・repr()）と計算を真似る（exifread の値の表示を旧版と同じにするため）。
//!
//! 旧版の exifread は値を Python の str() で文字列にしていたので、分数の約分・浮動小数点数の最短の桁・
//! bytes の表示・余りの符号などを Python に合わせる。

/// 最大公約数（Python の math.gcd。負の数は絶対値で）。
pub(crate) fn gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.unsigned_abs(), b.unsigned_abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a as i64
}

/// Python の Fraction(n, d) と同じに約分する（分母は正）。
pub(crate) fn reduce(n: i64, d: i64) -> (i64, i64) {
    let g = gcd(n, d).max(1) * d.signum();
    (n / g, d / g)
}

/// exifread の Ratio の str（分母が 0 なら約分しない）。
pub(crate) fn ratio_str(n: i64, d: i64) -> String {
    let (n, d) = if d == 0 { (n, d) } else { reduce(n, d) };
    if d == 1 {
        n.to_string()
    } else {
        format!("{n}/{d}")
    }
}

/// Python の repr(float)（最短で元に戻る桁。指数が -4 未満か 16 以上なら指数表記）。
pub(crate) fn float_repr(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.into();
    }
    // "1.2345e-7" のような最短の表記から、数字の並びと指数を取り出す
    let sci = format!("{x:e}");
    let (mantissa, exponent) = sci.split_once('e').expect("指数表記");
    let exponent: i32 = exponent.parse().expect("指数");
    let negative = mantissa.starts_with('-');
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let sign = if negative { "-" } else { "" };
    if (-4..16).contains(&exponent) {
        let point = exponent + 1; // 小数点の前の桁の数
        let text = if point <= 0 {
            format!("0.{}{digits}", "0".repeat((-point) as usize))
        } else if point as usize >= digits.len() {
            format!("{digits}{}.0", "0".repeat(point as usize - digits.len()))
        } else {
            format!("{}.{}", &digits[..point as usize], &digits[point as usize..])
        };
        format!("{sign}{text}")
    } else {
        let rest = if digits.len() > 1 { format!(".{}", &digits[1..]) } else { String::new() };
        let exp_sign = if exponent < 0 { '-' } else { '+' };
        format!("{sign}{}{rest}e{exp_sign}{:02}", &digits[..1], exponent.abs())
    }
}

/// Python の repr(bytes)。
pub(crate) fn bytes_repr(bytes: &[u8]) -> String {
    let quote = if bytes.contains(&b'\'') && !bytes.contains(&b'"') { '"' } else { '\'' };
    let mut out = format!("b{quote}");
    for &b in bytes {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'\t' => out.push_str("\\t"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            _ if char::from(b) == quote => {
                out.push('\\');
                out.push(quote);
            }
            0x20..=0x7E => out.push(char::from(b)),
            _ => out.push_str(&format!("\\x{b:02x}")),
        }
    }
    out.push(quote);
    out
}

/// Python の repr(1 文字の str)。
pub(crate) fn char_repr(c: char) -> String {
    let quote = if c == '\'' { '"' } else { '\'' };
    let body = match c {
        '\\' => "\\\\".to_string(),
        '\t' => "\\t".to_string(),
        '\n' => "\\n".to_string(),
        '\r' => "\\r".to_string(),
        '\0'..='\x1f' | '\x7f'..='\u{a0}' | '\u{ad}' => format!("\\x{:02x}", c as u32),
        _ => c.to_string(),
    };
    format!("{quote}{body}{quote}")
}

/// Python の a % b（余りの符号は b と同じ）。
pub(crate) fn py_mod(a: i64, b: i64) -> i64 {
    let r = a % b;
    if r != 0 && (r < 0) != (b < 0) {
        r + b
    } else {
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reprs() {
        assert_eq!(float_repr(1.5), "1.5");
        assert_eq!(float_repr(2.0), "2.0");
        assert_eq!(float_repr(-0.0), "-0.0");
        assert_eq!(float_repr(1e16), "1e+16");
        assert_eq!(float_repr(1.5e-5), "1.5e-05");
        assert_eq!(float_repr(0.0001), "0.0001");
        assert_eq!(float_repr(123456.75), "123456.75");
        assert_eq!(float_repr(f64::from(0.1f32)), "0.10000000149011612");
        assert_eq!(float_repr(4.0 / 3.0), "1.3333333333333333");
        assert_eq!(ratio_str(4, 8), "1/2");
        assert_eq!(ratio_str(3, -6), "-1/2");
        assert_eq!(ratio_str(6, 3), "2");
        assert_eq!(ratio_str(5, 0), "5/0");
        assert_eq!(ratio_str(0, 7), "0");
        assert_eq!(bytes_repr(b"a'b\x80\\"), "b\"a'b\\x80\\\\\"");
        assert_eq!(char_repr('a'), "'a'");
        assert_eq!(char_repr('\''), "\"'\"");
    }

    #[test]
    fn modulo_and_gcd_follow_python() {
        assert_eq!((py_mod(7, 3), py_mod(-7, 3), py_mod(7, -3), py_mod(-7, -3)), (1, 2, -2, -1));
        assert_eq!((gcd(12, -18), gcd(0, 5), gcd(0, 0)), (6, 5, 0));
        assert_eq!(reduce(-4, -8), (1, 2));
        assert_eq!(float_repr(f64::NAN), "nan");
        assert_eq!(
            (float_repr(f64::INFINITY), float_repr(f64::NEG_INFINITY)),
            ("inf".to_string(), "-inf".to_string())
        );
        assert_eq!(float_repr(1.0e-5), "1e-05");
        assert_eq!(bytes_repr(b"tab\tnl\n"), "b'tab\\tnl\\n'");
        assert_eq!(char_repr('\u{1f}'), "'\\x1f'");
    }
}
