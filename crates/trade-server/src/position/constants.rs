pub(crate) const POSITION_DUST_THRESHOLD: f64 = 1e-6;
/// Minimum order size threshold - orders with remaining size below this are considered filled.
/// Polymarket minimum order size is $0.01, so we use a slightly smaller value.
pub(crate) const ORDER_DUST_THRESHOLD: f64 = 0.005;
