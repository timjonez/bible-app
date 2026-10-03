pub fn see_all_label(code: &str, count: usize) -> String {
    if count == 0 {
        format!("See all {code} in the KJV")
    } else {
        format!("See all {code} in the KJV ({})", with_commas(count))
    }
}

fn with_commas(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    let len = digits.len();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn see_all_includes_code_and_count() {
        assert_eq!(see_all_label("H430", 0), "See all H430 in the KJV");
        assert_eq!(
            see_all_label("H430", 2249),
            "See all H430 in the KJV (2,249)"
        );
        assert_eq!(see_all_label("G26", 106), "See all G26 in the KJV (106)");
        assert_eq!(with_commas(2602), "2,602");
        assert_eq!(with_commas(12), "12");
    }
}
