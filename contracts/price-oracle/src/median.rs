use soroban_sdk::{contracterror, Vec};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum MedianError {
    EmptyInput = 10,
    /// Arithmetic operation overflow detected.
    ArithmeticOverflow = 11,
}

/// Sort a Vec<i128> using insertion sort (no_std compatible).
#[allow(dead_code)]
fn sort_prices(prices: &mut Vec<i128>) {
    let len = prices.len();
    for i in 1..len {
        let mut j = i;
        while j > 0 {
            let a = prices.get(j - 1).unwrap();
            let b = prices.get(j).unwrap();
            if a > b {
                prices.set(j - 1, b);
                prices.set(j, a);
                j -= 1;
            } else {
                break;
            }
        }
    }
}

/// Returns the median of the provided prices.
/// - 0 inputs  → Err(MedianError::EmptyInput)
/// - 1 input   → returns that value
/// - odd count → returns the middle value
/// - even count → returns the average of the two middle values
#[allow(dead_code)]
pub fn calculate_median(mut prices: Vec<i128>) -> Result<i128, MedianError> {
    let len = prices.len();
    if len == 0 {
        return Err(MedianError::EmptyInput);
    }
    sort_prices(&mut prices);
    let mid = len / 2;
    if len % 2 == 1 {
        Ok(prices.get(mid).unwrap())
    } else {
        let lo = prices.get(mid - 1).unwrap();
        let hi = prices.get(mid).unwrap();
        let sum = lo.checked_add(hi).ok_or(MedianError::ArithmeticOverflow)?;
        Ok(sum.checked_div(2).ok_or(MedianError::ArithmeticOverflow)?)
    }
}

/// Value at multiset index `target` (0-based) in an ascending `(value, count)`
/// vector, located via cumulative counts.
fn value_at(pairs: &Vec<(i128, u32)>, target: u64) -> i128 {
    let len = pairs.len();
    let mut cum: u64 = 0;
    let mut last: i128 = 0;
    for i in 0..len {
        let (v, c) = pairs.get(i).unwrap();
        last = v;
        cum += c as u64;
        if target < cum {
            return v;
        }
    }
    // `target` is always < total by construction; fall back to the largest value.
    last
}

/// Median of a multiset represented as compacted `(value, count)` pairs.
///
/// The insertion sort below runs only over the DISTINCT values, while `count`
/// preserves each value's true multiplicity. The result is therefore identical
/// to sorting the full expanded multiset — this is the gas saving from
/// "vector compacting" without altering the consensus median.
#[allow(dead_code)]
pub fn calculate_median_compacted(mut pairs: Vec<(i128, u32)>) -> Result<i128, MedianError> {
    let len = pairs.len();
    if len == 0 {
        return Err(MedianError::EmptyInput);
    }

    // Total number of original rows across all buckets.
    let mut total: u64 = 0;
    for i in 0..len {
        let (_, c) = pairs.get(i).unwrap();
        total = total
            .checked_add(c as u64)
            .ok_or(MedianError::ArithmeticOverflow)?;
    }
    if total == 0 {
        return Err(MedianError::EmptyInput);
    }

    // Insertion sort over DISTINCT values only (ascending by value).
    for i in 1..len {
        let mut j = i;
        while j > 0 {
            let (va, _) = pairs.get(j - 1).unwrap();
            let (vb, _) = pairs.get(j).unwrap();
            if va > vb {
                let a = pairs.get(j - 1).unwrap();
                let b = pairs.get(j).unwrap();
                pairs.set(j - 1, b);
                pairs.set(j, a);
                j -= 1;
            } else {
                break;
            }
        }
    }

    if total % 2 == 1 {
        Ok(value_at(&pairs, total / 2))
    } else {
        let lo = value_at(&pairs, total / 2 - 1);
        let hi = value_at(&pairs, total / 2);
        let sum = lo.checked_add(hi).ok_or(MedianError::ArithmeticOverflow)?;
        Ok(sum.checked_div(2).ok_or(MedianError::ArithmeticOverflow)?)
    }
}

#[cfg(test)]
mod median_tests {
    use crate::median::{calculate_median, MedianError};
    use soroban_sdk::{vec, Env};

    #[test]
    fn test_odd_number_median() {
        let env = Env::default();
        let prices = vec![&env, 748_i128, 750_i128, 752_i128];
        assert_eq!(calculate_median(prices), Ok(750));
    }

    #[test]
    fn test_even_number_median() {
        let env = Env::default();
        let prices = vec![&env, 740_i128, 750_i128, 760_i128, 770_i128];
        assert_eq!(calculate_median(prices), Ok(755));
    }

    #[test]
    fn test_single_input_returns_itself() {
        let env = Env::default();
        let prices = vec![&env, 999_i128];
        assert_eq!(calculate_median(prices), Ok(999));
    }

    #[test]
    fn test_empty_input_returns_error() {
        let env = Env::default();
        let prices = soroban_sdk::Vec::<i128>::new(&env);
        assert_eq!(calculate_median(prices), Err(MedianError::EmptyInput));
    }

    #[test]
    fn test_compacted_matches_expanded_with_duplicates() {
        // Five votes of 750 and one of 900: true median is 750.
        // Naive dedup would give median(750, 900) = 825 — the bug we avoid.
        let env = Env::default();
        let pairs = vec![&env, (750_i128, 5_u32), (900_i128, 1_u32)];
        assert_eq!(crate::median::calculate_median_compacted(pairs), Ok(750));
    }

    #[test]
    fn test_compacted_even_total() {
        // Expanded multiset: [740,740,760,770] → median = (740+760)/2 = 750.
        let env = Env::default();
        let pairs = vec![&env, (740_i128, 2_u32), (760_i128, 1_u32), (770_i128, 1_u32)];
        assert_eq!(crate::median::calculate_median_compacted(pairs), Ok(750));
    }

    #[test]
    fn test_compacted_unsorted_input() {
        // [800,800,750,900,900,900] sorted → median = (800+900)/2 = 850.
        let env = Env::default();
        let pairs = vec![&env, (800_i128, 2_u32), (750_i128, 1_u32), (900_i128, 3_u32)];
        assert_eq!(crate::median::calculate_median_compacted(pairs), Ok(850));
    }

    #[test]
    fn test_compacted_single_bucket() {
        let env = Env::default();
        let pairs = vec![&env, (999_i128, 4_u32)];
        assert_eq!(crate::median::calculate_median_compacted(pairs), Ok(999));
    }

    #[test]
    fn test_compacted_empty_returns_error() {
        let env = Env::default();
        let pairs = soroban_sdk::Vec::<(i128, u32)>::new(&env);
        assert_eq!(
            crate::median::calculate_median_compacted(pairs),
            Err(MedianError::EmptyInput)
        );
    }
}
