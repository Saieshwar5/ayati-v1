-- Schema 68: tokens each model request used through the gateway (core-agent CA3).
CREATE TABLE relay_model_usage (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id), id BIGSERIAL,
 assignment TEXT NOT NULL, agent TEXT NOT NULL, job TEXT,
 provider TEXT NOT NULL CHECK(char_length(provider) BETWEEN 1 AND 64),
 model TEXT NOT NULL CHECK(char_length(model) BETWEEN 1 AND 200),
 input_tokens BIGINT NOT NULL CHECK(input_tokens>=0), output_tokens BIGINT NOT NULL CHECK(output_tokens>=0),
 at_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,id)
);
CREATE INDEX relay_model_usage_job ON relay_model_usage(workspace,job) WHERE job IS NOT NULL;
CREATE INDEX relay_model_usage_agent ON relay_model_usage(workspace,agent,at_ms DESC);
