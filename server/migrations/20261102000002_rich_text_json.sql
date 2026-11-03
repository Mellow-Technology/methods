-- Rich text is now stored as TipTap's JSON document tree instead of HTML.
--
-- Expand/contract: the HTML columns are kept (renamed legacy_*) so the server
-- can convert them with the same parser it uses elsewhere; it does so on
-- start-up (richtext::convert_legacy) and clears each one as it goes. A later
-- migration drops them once every deployment has converted.

ALTER TABLE work_items RENAME COLUMN description TO legacy_description_html;
ALTER TABLE work_items ALTER COLUMN legacy_description_html DROP NOT NULL;
ALTER TABLE work_items ALTER COLUMN legacy_description_html DROP DEFAULT;
UPDATE work_items SET legacy_description_html = NULL WHERE btrim(legacy_description_html) = '';
-- NULL means no description.
ALTER TABLE work_items ADD COLUMN description jsonb
    CHECK (description IS NULL OR description ->> 'type' = 'doc');

ALTER TABLE comments RENAME COLUMN body_html TO legacy_body_html;
ALTER TABLE comments ALTER COLUMN legacy_body_html DROP NOT NULL;
ALTER TABLE comments ADD COLUMN body jsonb NOT NULL DEFAULT '{"type": "doc"}'
    CHECK (body ->> 'type' = 'doc');
ALTER TABLE comments ALTER COLUMN body DROP DEFAULT;
