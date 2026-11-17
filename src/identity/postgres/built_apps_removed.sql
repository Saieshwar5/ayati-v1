-- Built apps removed (schema 126). Cuckoo no longer builds or runs apps of its
-- own; Apps keeps connected services and links to the company's own systems.
-- Every built app, its files, versions, records, roles, agent rules, runs,
-- watchers and waiting events goes. Work an app event already started stays as
-- ordinary conversation history. Dropping relay_built_app_events also drops its
-- work-signal trigger (schema 97).
DROP TABLE relay_built_app_events;
DROP TABLE relay_built_app_watchers;
DROP TABLE relay_built_app_preview_errors;
DROP TABLE relay_built_app_action_runs;
DROP TABLE relay_built_app_agent_rules;
DROP TABLE relay_built_app_helpers;
DROP TABLE relay_built_app_records;
DROP TABLE relay_built_app_roles;
DROP TABLE relay_built_app_versions;
DROP TABLE relay_built_app_files;
DROP TABLE relay_built_app_people;
DROP TABLE relay_built_apps;
