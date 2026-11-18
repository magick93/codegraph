-- Consumer-owned board customizations for the LexoRank recipe.
--
-- Companion to docs/lexorank-kanban.md. This file is copied UNNUMBERED into
-- generated/migrations/ (custom.sql): clean_stale_migrations deletes only
-- files whose leading numeric prefix is >= 10, and an unnumbered name sorts
-- after the entity band (0005xx…) lexicographically, so it runs last — the
-- board.card table already exists at this point.
--
-- Idempotence note: migrations run once via the generated migrator; re-running
-- this file by hand is safe for everything except the ADD CONSTRAINT
-- (Postgres has no IF NOT EXISTS for constraints).

-- 1. Byte-wise ordering. LexoRank keys ("0|<[0-9a-z]+>") must compare by
--    ASCII codepoint; the database default collation would misorder the
--    0-9/a-z boundary under locale rules. Column-level collation applies to
--    every generated query (including the ux ?sort=rank path) with no
--    template changes.
ALTER TABLE board.card ALTER COLUMN rank TYPE text COLLATE "C";

-- 2. List-scoped neighbor lookups (move endpoint's successor/predecessor
--    queries and the ordered card reads).
CREATE INDEX IF NOT EXISTS card_list_rank_idx ON board.card (list_id, rank);

-- 3. Format guard — belt-and-braces beside the create-DTO garde pattern
--    (the update DTO does not carry the pattern attribute). Same regex as
--    the mox `pattern` constraint on CardType.rank.
ALTER TABLE board.card
    ADD CONSTRAINT card_rank_format
    CHECK (rank IS NULL OR rank ~ '^0[|][0-9a-z]*[1-9a-z]$');

-- 4. Initial rank assignment. codegraph has no column-default mechanism and
--    the before_create hook receives an immutable payload, so a BEFORE INSERT
--    trigger is the sanctioned seam (triggers run before CHECK constraints,
--    inside the same statement).

-- The LexoRank successor function (mirrors lexorank::Rank::next): walk from
-- the right; increment the first char that is not 'z' ('9' wraps to 'a',
-- otherwise +1) and truncate after it; if every rank char is 'z', append '1'.
-- The rank body starts after the fixed "0|" prefix (positions 1-2).
CREATE OR REPLACE FUNCTION board.rank_next(key text)
RETURNS text
LANGUAGE plpgsql
IMMUTABLE
AS $$
DECLARE
    i int;
    c text;
BEGIN
    i := length(key);
    WHILE i >= 3 LOOP
        c := substr(key, i, 1);
        IF c <> 'z' THEN
            IF c = '9' THEN
                RETURN substr(key, 1, i - 1) || 'a';
            ELSE
                RETURN substr(key, 1, i - 1) || chr(ascii(c) + 1);
            END IF;
        END IF;
        i := i - 1;
    END LOOP;
    RETURN key || '1';
END;
$$;

CREATE OR REPLACE FUNCTION board.assign_card_rank()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    max_rank text;
BEGIN
    IF NEW.rank IS NULL THEN
        SELECT rank INTO max_rank
          FROM board.card
         WHERE list_id = NEW.list_id
           AND rank IS NOT NULL
         ORDER BY rank COLLATE "C" DESC
         LIMIT 1;
        IF max_rank IS NULL THEN
            NEW.rank := '0|1';
        ELSE
            NEW.rank := board.rank_next(max_rank);
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS card_assign_rank ON board.card;
CREATE TRIGGER card_assign_rank
    BEFORE INSERT ON board.card
    FOR EACH ROW
    EXECUTE FUNCTION board.assign_card_rank();
