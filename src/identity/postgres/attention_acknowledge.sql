INSERT INTO relay_attention_receipts(workspace,user_id,kind,source,version,read_ms,reviewed_ms)
VALUES($1,$2,$3,$4,$5,(extract(epoch FROM clock_timestamp())*1000)::bigint,
 CASE WHEN $6 THEN (extract(epoch FROM clock_timestamp())*1000)::bigint ELSE NULL END)
ON CONFLICT(workspace,user_id,kind,source) DO UPDATE SET
 version=excluded.version,
 read_ms=CASE WHEN relay_attention_receipts.version=excluded.version
  THEN relay_attention_receipts.read_ms ELSE excluded.read_ms END,
 reviewed_ms=CASE WHEN relay_attention_receipts.version=excluded.version
  THEN COALESCE(relay_attention_receipts.reviewed_ms,excluded.reviewed_ms) ELSE excluded.reviewed_ms END
