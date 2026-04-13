-- Make sure we are in the correct database
USE solana;

-- Drop the view if it exists, useful for iterative development
-- DROP VIEW IF EXISTS pumpfun_from_events_mv;

-- Create the new materialized view reading from the 'events' table
CREATE MATERIALIZED VIEW pumpfun_from_events_mv
ENGINE = MergeTree()
PARTITION BY toDate(ts)
ORDER BY (toDate(ts), txType, signature, mint, traderPublicKey)
SETTINGS index_granularity = 8192
AS
SELECT
    -- Base columns from the 'events' table
    ts,
    toDate(ts) AS day,
    queue,

    -- Extract fields from JSON, handling 'buy'/'sell' structure
    multiIf(
        JSONHas(json, 'buy'), JSONExtractString(json, 'buy', 'base', 'txType'),
        JSONHas(json, 'sell'), JSONExtractString(json, 'sell', 'base', 'txType'),
        '' -- Default or error case
    ) AS txType,

    multiIf(
        JSONHas(json, 'buy'), JSONExtractString(json, 'buy', 'base', 'signature'),
        JSONHas(json, 'sell'), JSONExtractString(json, 'sell', 'base', 'signature'),
        ''
    ) AS signature,

    multiIf(
        JSONHas(json, 'buy'), JSONExtractString(json, 'buy', 'mint'),
        JSONHas(json, 'sell'), JSONExtractString(json, 'sell', 'mint'),
        ''
    ) AS mint,

    multiIf(
        JSONHas(json, 'buy'), JSONExtractString(json, 'buy', 'base', 'traderPublicKey'),
        JSONHas(json, 'sell'), JSONExtractString(json, 'sell', 'base', 'traderPublicKey'),
        ''
    ) AS traderPublicKey,

    multiIf(
        JSONHas(json, 'buy'), JSONExtractFloat(json, 'buy', 'tokenAmount'),
        JSONHas(json, 'sell'), JSONExtractFloat(json, 'sell', 'tokenAmount'),
        0.0 -- Default Float64 value
    ) AS tokenAmount,

    multiIf(
        JSONHas(json, 'buy'), JSONExtract(json, 'buy', 'newTokenBalance', 'Nullable(Float64)'),
        JSONHas(json, 'sell'), JSONExtract(json, 'sell', 'newTokenBalance', 'Nullable(Float64)'),
        NULL -- Default Nullable value
    ) AS newTokenBalance,

    multiIf(
        JSONHas(json, 'buy'), JSONExtract(json, 'buy', 'bondingCurveKey', 'Nullable(String)'),
        JSONHas(json, 'sell'), JSONExtract(json, 'sell', 'bondingCurveKey', 'Nullable(String)'),
        NULL
    ) AS bondingCurveKey,

    multiIf(
        JSONHas(json, 'buy'), JSONExtractFloat(json, 'buy', 'vTokensInBondingCurve'),
        JSONHas(json, 'sell'), JSONExtractFloat(json, 'sell', 'vTokensInBondingCurve'),
        0.0
    ) AS vTokensInBondingCurve,

    multiIf(
        JSONHas(json, 'buy'), JSONExtractFloat(json, 'buy', 'vSolInBondingCurve'),
        JSONHas(json, 'sell'), JSONExtractFloat(json, 'sell', 'vSolInBondingCurve'),
        0.0
    ) AS vSolInBondingCurve,

    -- Note: Original MV schema has marketCapSol as Float64 (non-nullable).
    -- JSONExtractFloat returns 0.0 if the path doesn't exist or the value is null/not a number.
    multiIf(
        JSONHas(json, 'buy'), JSONExtractFloat(json, 'buy', 'marketCapSol'),
        JSONHas(json, 'sell'), JSONExtractFloat(json, 'sell', 'marketCapSol'),
        0.0
    ) AS marketCapSol,

    multiIf(
        JSONHas(json, 'buy'), JSONExtractUInt(json, 'buy', 'base', 'timestamp'),
        JSONHas(json, 'sell'), JSONExtractUInt(json, 'sell', 'base', 'timestamp'),
        0 -- Default UInt64 value
    ) AS timestamp,

     multiIf(
        JSONHas(json, 'buy'), JSONExtractUInt(json, 'buy', 'base', 'slot'),
        JSONHas(json, 'sell'), JSONExtractUInt(json, 'sell', 'base', 'slot'),
        0
    ) AS slot,

    json AS raw_json  -- Keep the original JSON
FROM events -- Read from the new source table
WHERE
    -- Filter for PumpFun transactions specifically using the 'dex' field within the JSON
    multiIf(
        JSONHas(json, 'buy', 'base', 'dex'), JSONExtractString(json, 'buy', 'base', 'dex'),
        JSONHas(json, 'sell', 'base', 'dex'), JSONExtractString(json, 'sell', 'base', 'dex'),
        '' -- Default if neither path exists
    ) = 'PumpFun';

-- Optional: Verify the schema of the new MV
-- DESCRIBE TABLE pumpfun_from_events_mv;
