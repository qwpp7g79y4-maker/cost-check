//! Decimal amounts as the reports show them.
//!
//! `ci-bill` stores USD as micros and prints cents, half up, away from zero.
//! The same steps live here so the Markdown matches `ci-bill report --md`.

use crate::Fault;

/// Parse a plain decimal into an integer at `scale` digits after the dot.
/// Half up, away from zero, when there are extra digits.
pub fn parse_fixed(raw: &str, scale: u32) -> Result<i128, Fault> {
    if scale > 9 {
        return Err(Fault::new("geen bedrag"));
    }
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(Fault::new("geen bedrag"));
    }
    let neg = raw.starts_with('-');
    let raw = raw
        .strip_prefix('+')
        .or_else(|| raw.strip_prefix('-'))
        .unwrap_or(raw);
    if raw.is_empty() || raw.starts_with('.') {
        return Err(Fault::new("geen bedrag"));
    }
    let (whole, frac) = match raw.split_once('.') {
        Some((whole, frac)) => (whole, frac),
        None => (raw, ""),
    };
    if whole.is_empty()
        || !whole.chars().all(|c| c.is_ascii_digit())
        || !frac.chars().all(|c| c.is_ascii_digit())
        || whole.len() > 15
    {
        return Err(Fault::new("geen bedrag"));
    }
    let whole_n: i128 = whole.parse().map_err(|_| Fault::new("geen bedrag"))?;
    let factor = 10i128
        .checked_pow(scale)
        .ok_or_else(|| Fault::new("geen bedrag"))?;
    let mut acc = whole_n
        .checked_mul(factor)
        .ok_or_else(|| Fault::new("geen bedrag"))?;
    let mut round = 0i128;
    for (i, ch) in frac.chars().enumerate() {
        let digit = i128::from(ch as u8 - b'0');
        if (i as u32) < scale {
            let pow = 10i128.pow(scale - 1 - i as u32);
            acc = acc
                .checked_add(digit * pow)
                .ok_or_else(|| Fault::new("geen bedrag"))?;
        } else if i as u32 == scale {
            round = digit;
        }
    }
    if round >= 5 {
        acc = acc
            .checked_add(1)
            .ok_or_else(|| Fault::new("geen bedrag"))?;
    }
    if neg {
        acc = acc.checked_neg().ok_or_else(|| Fault::new("geen bedrag"))?;
    }
    Ok(acc)
}

pub fn format_fixed(value: i128, scale: u32) -> String {
    let neg = value < 0;
    let abs = value.unsigned_abs();
    let div = 10u128.pow(scale);
    let whole = abs / div;
    let frac = abs % div;
    let sign = if neg && abs != 0 { "-" } else { "" };
    format!("{sign}{whole}.{frac:0width$}", width = scale as usize)
}

/// `284.208000` → `284.21`, the same half-up rule as `ci-bill`.
pub fn format_cents_str(raw: &str) -> Result<String, Fault> {
    Ok(cents_from_micros(parse_fixed(raw, 6)?))
}

pub fn cents_from_micros(micros: i128) -> String {
    let neg = micros < 0;
    let abs = micros.unsigned_abs();
    let mut cents = abs / 10_000;
    let rem = abs % 10_000;
    if rem >= 5_000 {
        cents += 1;
    }
    let sign = if neg && cents != 0 { "-" } else { "" };
    format!("{sign}{}.{:02}", cents / 100, cents % 100)
}

/// Keep sub-cent amounts such as `0.003`. Whole cents stay at two decimals.
pub fn display_amount(raw: &str) -> Result<String, Fault> {
    let micros = parse_fixed(raw, 6)?;
    if micros % 10_000 == 0 {
        Ok(format_fixed(micros / 10_000, 2))
    } else {
        let text = format_fixed(micros, 6);
        Ok(trim_zeros(&text))
    }
}

fn trim_zeros(text: &str) -> String {
    let (sign, rest) = if let Some(rest) = text.strip_prefix('-') {
        ("-", rest)
    } else {
        ("", text)
    };
    let Some((whole, frac)) = rest.split_once('.') else {
        return text.to_string();
    };
    let frac = frac.trim_end_matches('0');
    if frac.is_empty() {
        format!("{sign}{whole}.00")
    } else {
        format!("{sign}{whole}.{frac}")
    }
}

/// `amount * part / whole`, half up, at two decimals.
pub fn proportion(amount: &str, part: u64, whole: u64) -> Result<String, Fault> {
    if whole == 0 || part == 0 {
        return Ok("0.00".to_string());
    }
    let cents = parse_fixed(amount, 2)?;
    let numer = cents
        .checked_mul(i128::from(part))
        .ok_or_else(|| Fault::new("geen bedrag"))?;
    let div = i128::from(whole);
    let mut quote = numer / div;
    let rem = numer % div;
    if rem.abs() * 2 >= div.abs() {
        quote += if numer >= 0 { 1 } else { -1 };
    }
    Ok(format_fixed(quote, 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cents_match_ci_bill() {
        assert_eq!(format_cents_str("284.208000").unwrap(), "284.21");
        assert_eq!(format_cents_str("230.144000").unwrap(), "230.14");
        assert_eq!(format_cents_str("44.232000").unwrap(), "44.23");
        assert_eq!(format_cents_str("7.720000").unwrap(), "7.72");
        assert_eq!(format_cents_str("2.112000").unwrap(), "2.11");
        assert_eq!(format_cents_str("207.872000").unwrap(), "207.87");
        assert_eq!(format_cents_str("22.320000").unwrap(), "22.32");
        assert_eq!(format_cents_str("2.160000").unwrap(), "2.16");
        assert_eq!(format_cents_str("0.000000").unwrap(), "0.00");
    }

    #[test]
    fn display_keeps_a_sub_cent_amount() {
        assert_eq!(display_amount("0.003").unwrap(), "0.003");
        assert_eq!(display_amount("0.53").unwrap(), "0.53");
        assert_eq!(display_amount("1825.00").unwrap(), "1825.00");
        assert_eq!(display_amount("0").unwrap(), "0.00");
    }

    #[test]
    fn proportion_splits_a_tier() {
        assert_eq!(proportion("10.00", 2, 2).unwrap(), "10.00");
        assert_eq!(proportion("10.00", 1, 2).unwrap(), "5.00");
        assert_eq!(proportion("10.00", 0, 2).unwrap(), "0.00");
    }

    #[test]
    fn garbage_is_not_money() {
        assert!(parse_fixed("12.3.4", 2).is_err());
        assert!(parse_fixed("abc", 2).is_err());
        assert!(parse_fixed("", 2).is_err());
    }

    #[test]
    fn half_a_cent_goes_away_from_zero() {
        assert_eq!(format_cents_str("284.205000").unwrap(), "284.21");
        assert_eq!(format_cents_str("284.204999").unwrap(), "284.20");
        assert_eq!(format_cents_str("-0.005000").unwrap(), "-0.01");
        assert_eq!(format_cents_str("-0.004999").unwrap(), "0.00");
        assert_eq!(format_cents_str("1.005").unwrap(), "1.01");
        assert_eq!(format_cents_str("0.005").unwrap(), "0.01");
        assert_eq!(format_cents_str("-284.205").unwrap(), "-284.21");
        assert_eq!(cents_from_micros(5_000), "0.01");
        assert_eq!(cents_from_micros(4_999), "0.00");
        assert_eq!(cents_from_micros(-5_000), "-0.01");
    }

    #[test]
    fn half_a_cent_in_a_proportion_goes_away_from_zero() {
        // 0.01 * 1 / 2 = 0.005 cent-halves: exactly half a cent step.
        assert_eq!(proportion("0.01", 1, 2).unwrap(), "0.01");
        assert_eq!(proportion("-0.01", 1, 2).unwrap(), "-0.01");
        assert_eq!(proportion("0.01", 1, 3).unwrap(), "0.00");
        assert_eq!(proportion("0.02", 1, 3).unwrap(), "0.01");
        assert_eq!(proportion("0.03", 1, 2).unwrap(), "0.02");
        assert_eq!(proportion("1.00", 1, 3).unwrap(), "0.33");
        assert_eq!(proportion("2.00", 1, 3).unwrap(), "0.67");
    }
}
