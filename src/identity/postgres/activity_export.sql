-- Activity → History → Export: the filtered requests as rows for a CSV file,
-- newest first. The same visibility and filters as the feed.
SELECT to_char(to_timestamp(j.created_ms/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD HH24:MI') AS at,ag.name AS agent,
 COALESCE((SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=j.payload->>'requester'),'') AS requester,
 CASE WHEN c.kind='direct' THEN 'direct chat' ELSE '#'||c.name END AS place,k.kind,
 CASE WHEN m.state='complete' AND j.state='finished' THEN 'done' WHEN m.state IN ('failed','interrupted') THEN 'could not complete'
  WHEN m.state='cancelled' THEN 'stopped' WHEN j.state='waiting' THEN 'waiting' WHEN j.state='finished' THEN m.state ELSE 'working' END AS status,
 COALESCE((SELECT replace(left(s.content,500),E'\n',' ') FROM (/*SOURCE*/) s),'') AS summary
/*FILTER*/
AND $12::bigint>0
ORDER BY j.sequence DESC LIMIT 5001
