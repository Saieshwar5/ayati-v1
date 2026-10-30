-- Audit entries gain an optional readable sentence ("Arjun Mehta removed Ravi
-- Kumar; 3 agents moved to Arjun Mehta"). Older entries keep only action/target.
ALTER TABLE relay_audit ADD COLUMN summary TEXT CHECK (summary IS NULL OR length(summary) BETWEEN 1 AND 1000);
