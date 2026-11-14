-- Schema 118: what public pages say about themselves, for link previews in
-- Chat. Keyed by the address as written; `preview` is null when the page
-- gave none. Never holds who posted or read a link.
CREATE TABLE relay_link_previews (
 url TEXT PRIMARY KEY CHECK(char_length(url) <= 2048),
 preview JSONB,
 fetched BIGINT NOT NULL
);
