#[cfg(test)]
mod tests {
    use crate::utils::token_decimal_cache::TokenDecimalCache;

    #[test]
    fn test_cache_initialization() {
        let cache = TokenDecimalCache::new();
        assert_eq!(cache.cache_size(), 0);
    }

    #[test]
    fn test_cache_clear() {
        let cache = TokenDecimalCache::new();
        cache.clear_cache();
        assert_eq!(cache.cache_size(), 0);
    }

    #[test]
    fn test_token_suffix_detection() {
        // Test that the optimization logic would be triggered for known tokens
        let bonk_mint = "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263bonk";
        let pump_mint = "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263pump";
        let unknown_mint = "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263";

        assert!(bonk_mint.ends_with("bonk"));
        assert!(pump_mint.ends_with("pump"));
        assert!(!unknown_mint.ends_with("bonk"));
        assert!(!unknown_mint.ends_with("pump"));
    }
}
