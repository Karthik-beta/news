//! 64-bit SimHash over word 3-shingles, for `documents.simhash` (near-duplicate text).

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Lower-cased alphanumeric words.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

pub fn simhash(text: &str) -> u64 {
    let w = words(text);
    let mut counts = [0i64; 64];
    let mut add = |s: &str| {
        let h = fnv1a(s.as_bytes());
        for (i, c) in counts.iter_mut().enumerate() {
            *c += if h >> i & 1 == 1 { 1 } else { -1 };
        }
    };
    if w.len() < 3 {
        add(&w.join(" "));
    } else {
        for s in w.windows(3) {
            add(&s.join(" "));
        }
    }
    counts
        .iter()
        .enumerate()
        .fold(0u64, |acc, (i, c)| if *c > 0 { acc | 1 << i } else { acc })
}

pub fn distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// `documents.simhash` is a signed `bigint`: the same 64 bits.
pub fn as_bigint(h: u64) -> i64 {
    i64::from_ne_bytes(h.to_ne_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_duplicates_are_close_and_others_far() {
        let a = "Apple reports fourth quarter results. Revenue rose to a record, and the board declared a dividend of 26 cents per share payable in November.";
        let b = "Apple reports fourth quarter results. Revenue rose to a record, and the board declared a dividend of 27 cents per share payable in November.";
        let c = "The Consumer Price Index for All Urban Consumers increased 0.2 percent in October on a seasonally adjusted basis.";
        assert_eq!(simhash(a), simhash(a));
        assert!(
            distance(simhash(a), simhash(b)) < 12,
            "{}",
            distance(simhash(a), simhash(b))
        );
        assert!(
            distance(simhash(a), simhash(c)) > 16,
            "{}",
            distance(simhash(a), simhash(c))
        );
        assert_eq!(as_bigint(u64::MAX), -1);
    }
}
