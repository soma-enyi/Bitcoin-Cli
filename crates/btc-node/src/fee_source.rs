//! Fee rates from sources other than the node's own estimator, for nodes that cannot
//! estimate (such as a restricted gateway). Every number here is an approximation.

use std::collections::BTreeMap;

use crate::backend::BlockFeeStats;
use crate::error::NodeError;

/// An Esplora-compatible `fee-estimates` endpoint: confirmation target in blocks -> sat/vB.
pub const DEFAULT_PUBLIC_URL: &str = "https://blockstream.info/api/fee-estimates";

/// Nodes do not relay transactions below 1 sat/vB, so a lower estimate is raised to it.
pub const RELAY_MINIMUM: f64 = 1.0;

/// Anything above this is a bug or a hostile server, not a fee rate.
const MAX_PLAUSIBLE: f64 = 10_000.0;

fn rpc(message: impl Into<String>) -> NodeError {
    NodeError::Rpc(message.into())
}

/// Parses `{"1": 2.1, "6": 1.1, ...}`, dropping nothing silently: any bad entry is an error.
pub fn parse_estimates(json: &str) -> Result<BTreeMap<u16, f64>, NodeError> {
    let map: BTreeMap<String, f64> = serde_json::from_str(json).map_err(|e| {
        rpc(format!(
            "the fee service returned something unexpected: {e}"
        ))
    })?;
    let mut out = BTreeMap::new();
    for (target, rate) in map {
        let target: u16 = target
            .parse()
            .map_err(|_| rpc(format!("the fee service returned a bad target `{target}`")))?;
        if !rate.is_finite() || rate <= 0.0 || rate > MAX_PLAUSIBLE {
            return Err(rpc(format!(
                "the fee service returned an implausible rate {rate} sat/vB"
            )));
        }
        out.insert(target, rate);
    }
    if out.is_empty() {
        return Err(rpc("the fee service returned no estimates"));
    }
    Ok(out)
}

/// The rate for `target` blocks: the largest listed target not above it (cheaper targets are
/// never used, so the estimate errs on the side of confirming), at least the relay minimum.
pub fn rate_for_target(estimates: &BTreeMap<u16, f64>, target: u16) -> f64 {
    let rate = estimates
        .range(..=target)
        .next_back()
        .or_else(|| estimates.iter().next())
        .map(|(_, rate)| *rate)
        .unwrap_or(RELAY_MINIMUM);
    rate.max(RELAY_MINIMUM)
}

fn is_loopback(url: &str) -> bool {
    ["//127.0.0.1", "//localhost", "//[::1]"]
        .iter()
        .any(|h| url.contains(h))
}

/// Fetches the public estimates and picks the rate for `target`.
pub fn fetch_public(url: &str, target: u16) -> Result<f64, NodeError> {
    let secure = url.starts_with("https://") || (url.starts_with("http://") && is_loopback(url));
    if !secure {
        return Err(rpc("the fee service URL must be https://"));
    }
    let response = bitreq::Request::new(bitreq::Method::Get, url)
        .with_timeout(15)
        .send()
        .map_err(|e| rpc(format!("could not reach the fee service: {e}")))?;
    if response.status_code != 200 {
        return Err(rpc(format!(
            "the fee service answered HTTP {}",
            response.status_code
        )));
    }
    let body = response
        .as_str()
        .map_err(|e| rpc(format!("the fee service sent unreadable data: {e}")))?;
    Ok(rate_for_target(&parse_estimates(body)?, target))
}

/// Which share of recent block space paid at most the rate we pick. A short target needs to
/// beat more of what recently confirmed; a long one can undercut almost all of it.
pub fn percentile_for_target(target: u16) -> f64 {
    match target {
        0..=2 => 25.0,
        3..=6 => 10.0,
        7..=24 => 5.0,
        _ => 1.0,
    }
}

/// The fee rate below which `percent` of the block space (weighted by vsize) paid.
pub fn weighted_percentile(samples: &[(f64, u64)], percent: f64) -> Option<f64> {
    let mut sorted: Vec<(f64, u64)> = samples
        .iter()
        .copied()
        .filter(|(rate, vsize)| rate.is_finite() && *rate >= 0.0 && *vsize > 0)
        .collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let total: u64 = sorted.iter().map(|(_, v)| v).sum();
    let wanted = total as f64 * percent / 100.0;
    let mut seen = 0.0;
    for (rate, vsize) in &sorted {
        seen += *vsize as f64;
        if seen >= wanted {
            return Some(*rate);
        }
    }
    sorted.last().map(|(rate, _)| *rate)
}

/// An estimate from one block's fee statistics. A short target beats the 25th percentile of what
/// the block paid, a medium one the 10th, a long one only the cheapest transaction it confirmed.
/// Never below the relay minimum.
pub fn estimate_from_block_stats(stats: &BlockFeeStats, target: u16) -> f64 {
    let rate = match target {
        0..=2 => stats.percentiles[1],
        3..=24 => stats.percentiles[0],
        _ => stats.min_sat_vb.min(stats.percentiles[0]),
    };
    rate.max(RELAY_MINIMUM)
}

/// How many blocks to read for `target`: a short target follows the latest few blocks, a long
/// one smooths over a day.
pub fn default_window(target: u16) -> u16 {
    match target {
        0..=2 => 3,
        3..=6 => 6,
        7..=24 => 24,
        _ => 144,
    }
}

/// The middle value, so one odd block (say an empty one) cannot drag the estimate.
pub fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let mid = values.len() / 2;
    Some(if values.len() % 2 == 1 {
        values[mid]
    } else {
        (values[mid - 1] + values[mid]) / 2.0
    })
}

/// An estimate from `(sat/vB, vsize)` samples of recently confirmed transactions.
pub fn estimate_from_samples(samples: &[(f64, u64)], target: u16) -> Result<f64, NodeError> {
    weighted_percentile(samples, percentile_for_target(target))
        .map(|rate| rate.max(RELAY_MINIMUM))
        .ok_or_else(|| rpc("the recent blocks contained no transactions with fee data"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn estimates() -> BTreeMap<u16, f64> {
        parse_estimates(r#"{"1":2.1,"2":2.1,"6":1.1,"25":0.3,"144":0.25,"1008":0.1}"#).unwrap()
    }

    #[test]
    fn longer_targets_get_lower_rates_and_never_below_the_relay_minimum() {
        let e = estimates();
        assert_eq!(rate_for_target(&e, 1), 2.1);
        assert_eq!(rate_for_target(&e, 6), 1.1);
        // 100 blocks is not listed: use the largest target below it (25), then raise to the floor.
        assert_eq!(rate_for_target(&e, 100), 1.0);
        assert_eq!(rate_for_target(&e, 1008), 1.0);
    }

    #[test]
    fn an_unlisted_target_uses_the_nearest_lower_one_so_it_errs_towards_confirming() {
        let e = parse_estimates(r#"{"1":50.0,"3":20.0,"10":8.0}"#).unwrap();
        assert_eq!(rate_for_target(&e, 2), 50.0);
        assert_eq!(rate_for_target(&e, 9), 20.0);
        assert_eq!(rate_for_target(&e, 500), 8.0);
    }

    #[test]
    fn rejects_garbage_and_implausible_rates() {
        assert!(parse_estimates("not json").is_err());
        assert!(parse_estimates("{}").is_err());
        assert!(parse_estimates(r#"{"x": 1.0}"#).is_err());
        assert!(parse_estimates(r#"{"1": -3.0}"#).is_err());
        assert!(parse_estimates(r#"{"1": 99999999.0}"#).is_err());
        assert!(parse_estimates(r#"{"1": 0.0}"#).is_err());
    }

    #[test]
    fn the_public_url_must_be_https_unless_it_is_this_machine() {
        assert!(fetch_public("http://example.com/fees", 6).is_err());
        assert!(fetch_public("ftp://example.com/fees", 6).is_err());
    }

    #[test]
    fn percentiles_are_weighted_by_block_space() {
        // 100 vB at 1 sat/vB, 900 vB at 10 sat/vB: the 5th percentile is the cheap tx, the 50th the dear one.
        let samples = [(1.0, 100), (10.0, 900)];
        assert_eq!(weighted_percentile(&samples, 5.0), Some(1.0));
        assert_eq!(weighted_percentile(&samples, 50.0), Some(10.0));
        assert_eq!(weighted_percentile(&[], 50.0), None);
    }

    #[test]
    fn block_stats_estimate_follows_the_target_and_respects_the_relay_minimum() {
        let stats = BlockFeeStats {
            height: 1,
            hash: String::new(),
            tx_count: 10,
            min_sat_vb: 0.5,
            avg_sat_vb: 8.0,
            max_sat_vb: 90.0,
            percentiles: [3.0, 5.0, 7.0, 12.0, 20.0],
        };
        assert_eq!(estimate_from_block_stats(&stats, 1), 5.0);
        assert_eq!(estimate_from_block_stats(&stats, 6), 3.0);
        // The cheapest transaction paid 0.5, which a node would not relay: raised to 1.
        assert_eq!(estimate_from_block_stats(&stats, 100), 1.0);
    }

    #[test]
    fn the_window_grows_with_the_target_and_the_median_ignores_one_odd_block() {
        assert!(default_window(1) < default_window(6));
        assert!(default_window(6) < default_window(24));
        assert!(default_window(24) < default_window(1008));
        assert_eq!(median(&mut [9.0, 1.0, 5.0]), Some(5.0));
        assert_eq!(median(&mut [1.0, 1.0, 40.0, 3.0]), Some(2.0));
        assert_eq!(median(&mut []), None);
    }

    #[test]
    fn shorter_targets_use_higher_percentiles() {
        assert!(percentile_for_target(1) > percentile_for_target(6));
        assert!(percentile_for_target(6) > percentile_for_target(24));
        assert!(percentile_for_target(24) > percentile_for_target(1008));
    }

    #[test]
    fn block_estimates_are_raised_to_the_relay_minimum() {
        assert_eq!(estimate_from_samples(&[(0.2, 1000)], 6).unwrap(), 1.0);
        assert_eq!(estimate_from_samples(&[(7.0, 1000)], 1).unwrap(), 7.0);
        assert!(estimate_from_samples(&[], 6).is_err());
    }
}
