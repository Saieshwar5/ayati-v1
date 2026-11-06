-- Schema 75 (Apps AP2/AP3): company systems shown as apps that open through Cuckoo,
-- who may open them, and members' access requests and app suggestions.
CREATE TABLE relay_company_apps (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id), id TEXT NOT NULL,
 name TEXT NOT NULL CHECK(char_length(name) BETWEEN 1 AND 80),
 description TEXT NOT NULL DEFAULT '' CHECK(char_length(description)<=200),
 url TEXT NOT NULL CHECK(url LIKE 'https://%' AND char_length(url)<=2000),
 audience TEXT NOT NULL CHECK(audience IN ('everyone','selected')),
 owner TEXT, revision BIGINT NOT NULL DEFAULT 1,
 created BIGINT NOT NULL, updated BIGINT NOT NULL, removed BOOLEAN NOT NULL DEFAULT false,
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,owner) REFERENCES relay_members(workspace,user_id) ON DELETE SET NULL (owner)
);
CREATE TABLE relay_company_app_people (
 workspace TEXT NOT NULL, app TEXT NOT NULL, user_id TEXT NOT NULL,
 PRIMARY KEY(workspace,app,user_id),
 FOREIGN KEY(workspace,app) REFERENCES relay_company_apps(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE TABLE relay_app_requests (
 workspace TEXT NOT NULL, id TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('access','suggestion')),
 app TEXT, name TEXT NOT NULL DEFAULT '' CHECK(char_length(name)<=80),
 url TEXT NOT NULL DEFAULT '' CHECK(char_length(url)<=2000),
 reason TEXT NOT NULL DEFAULT '' CHECK(char_length(reason)<=500),
 requester TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','approved','declined')),
 decided_by TEXT, created BIGINT NOT NULL, decided BIGINT,
 PRIMARY KEY(workspace,id),
 CHECK((kind='access') = (app IS NOT NULL)),
 FOREIGN KEY(workspace,app) REFERENCES relay_company_apps(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,requester) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX relay_app_requests_one_waiting ON relay_app_requests(workspace,app,requester)
 WHERE state='pending' AND kind='access';
