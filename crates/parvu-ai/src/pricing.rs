//! Estimated cost of a model call from its token usage. Prices are US
//! dollars per million tokens, for the models ParVu suggests; other models
//! have no estimate (local models cost nothing anyway).

use crate::types::Usage;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    /// Writing the 5-minute cache.
    pub cache_write: f64,
}

const fn p(input: f64, output: f64, cache_read: f64) -> Price {
    Price { input, output, cache_read, cache_write: input * 1.25 }
}

/// (model id or prefix ending in '-', price)
const TABLE: &[(&str, Price)] = &[
    ("claude-opus-5-5", p(4.0, 20.0, 0.20)),
    ("claude-sonnet-5-5", p(2.0, 10.0, 0.10)),
    ("claude-haiku-5-5", p(0.10, 0.50, 0.01)),
    ("claude-opus-5", p(5.0, 25.0, 0.50)),
    ("claude-sonnet-5", p(2.0, 10.0, 0.20)),
    ("claude-fable-5-2", p(8.0, 40.0, 0.20)),
];

pub fn price(model: &str) -> Option<Price> {
    TABLE.iter().find(|(m, _)| *m == model).map(|(_, p)| *p)
}

pub fn cost_usd(model: &str, u: &Usage) -> Option<f64> {
    let p = price(model)?;
    let m = 1_000_000.0;
    Some(
        u.input_tokens as f64 * p.input / m
            + u.output_tokens as f64 * p.output / m
            + u.cache_read_tokens as f64 * p.cache_read / m
            + u.cache_write_tokens as f64 * p.cache_write / m,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn costs() {
        let u = Usage { input_tokens: 1_000_000, output_tokens: 100_000, cache_read_tokens: 0, cache_write_tokens: 0 };
        assert!((cost_usd("claude-opus-5-5", &u).unwrap() - 6.0).abs() < 1e-9);
        assert_eq!(cost_usd("llama3.1:8b", &u), None);
    }
}
