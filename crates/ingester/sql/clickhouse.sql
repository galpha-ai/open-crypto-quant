CREATE DATABSE solana;

USE solana;

CREATE TABLE pumpfun (
    ts DateTime('UTC'),
    queue String,
    json String
) ENGINE = MergeTree
PRIMARY KEY (ts)
ORDER BY (ts);

CREATE TABLE events (
    ts DateTime('UTC'),
    queue String,
    json String
) ENGINE = MergeTree
PRIMARY KEY (ts)
ORDER BY (ts);


-- Create a materialized view to extract key fields from JSON
CREATE MATERIALIZED VIEW pumpfun_mv 
ENGINE = MergeTree()
PARTITION BY toDate(ts)
ORDER BY (toDate(ts), txType, signature, mint, traderPublicKey)
SETTINGS index_granularity = 8192
AS
SELECT
    ts,
    toDate(ts) AS day,
    queue,
    JSONExtract(json, 'txType', 'String') AS txType,
    JSONExtract(json, 'signature', 'String') AS signature,
    JSONExtract(json, 'mint', 'String') AS mint,
    JSONExtract(json, 'traderPublicKey', 'String') AS traderPublicKey,
    JSONExtract(json, 'tokenAmount', 'Float64') AS tokenAmount,
    JSONExtract(json, 'newTokenBalance', 'Nullable(Float64)') AS newTokenBalance,
    JSONExtract(json, 'bondingCurveKey', 'Nullable(String)') AS bondingCurveKey,
    JSONExtract(json, 'vTokensInBondingCurve', 'Float64') AS vTokensInBondingCurve,
    JSONExtract(json, 'vSolInBondingCurve', 'Float64') AS vSolInBondingCurve,
    JSONExtract(json, 'marketCapSol', 'Float64') AS marketCapSol,
    JSONExtract(json, 'timestamp', 'UInt64') AS timestamp,
    JSONExtract(json, 'slot', 'UInt64') AS slot,
    json AS raw_json  -- Keeping the original JSON for reference if needed
FROM pumpfun;
