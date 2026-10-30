-- Invitations get a Cuckoo link (/invite/<secret>) the owner can send anywhere;
-- only its SHA-256 is stored. Sign-in remembers where to return afterwards.
ALTER TABLE relay_invitations ADD COLUMN link_hash TEXT;
CREATE UNIQUE INDEX relay_invitation_links ON relay_invitations(link_hash) WHERE link_hash IS NOT NULL;
ALTER TABLE relay_login_flows ADD COLUMN return_to TEXT;
