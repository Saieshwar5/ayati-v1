-- One instructions text and a short job title (D233). Responsibility, custom
-- instructions, "ask a person when", successful outcome and routine are joined
-- under headings in the order the agent read them before. A responsibility of
-- 60 characters or fewer becomes the job title; a longer one leaves it empty for
-- the owner to fill in. Nothing an agent was told is dropped.
UPDATE relay_shared_agents SET
 settings=(settings-'successful_outcome'-'routine')||jsonb_build_object('escalation','','instructions',left(concat_ws(E'\n\n',
  CASE WHEN btrim(role)<>'' THEN E'## Responsibility\n'||btrim(role) END,
  CASE WHEN btrim(COALESCE(settings->>'instructions',''))<>'' THEN E'## Instructions\n'||btrim(settings->>'instructions') END,
  CASE WHEN btrim(COALESCE(settings->>'escalation',''))<>'' THEN E'## Ask a person when\n'||btrim(settings->>'escalation') END,
  CASE WHEN btrim(COALESCE(settings->>'successful_outcome',''))<>'' THEN E'## Successful outcome\n'||btrim(settings->>'successful_outcome') END,
  CASE WHEN btrim(COALESCE(settings->>'routine',''))<>'' THEN E'## Routine\n'||btrim(settings->>'routine') END
 ),24000)),
 role=CASE WHEN char_length(btrim(role))<=60 AND position(E'\n' IN btrim(role))=0 THEN btrim(role) ELSE '' END,
 revision=CASE WHEN revision>0 THEN revision+1 ELSE revision END;
