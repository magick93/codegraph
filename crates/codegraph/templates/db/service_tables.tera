-- Service tables for hand-written extension features (issues #43, #44).
-- Emitted by the `service_tables` generator; renumbered with the rest of the
-- generated migrations on regen. Applied transactionally by the SeaORM
-- Migrator (PL/pgSQL $$ blocks run as-is).

-- ============================================================================
-- community_graph trust transitivity (issue #43)
-- ============================================================================
-- Materialized transitive reachability over trust_connection (active edges
-- only: deleted_at IS NULL). Recompute is a full TRUNCATE + INSERT via
-- refresh_trust_paths() (WITH RECURSIVE, depth <= 3, cycle-safe seen-array),
-- triggered synchronously on trust_connection writes.
-- ============================================================================

CREATE TABLE IF NOT EXISTS community_graph.trust_path (
    id UUID NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,

    platform_organization_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000'::UUID,

    source_did TEXT NOT NULL,
    target_did TEXT NOT NULL,
    path_length INTEGER NOT NULL CHECK (path_length BETWEEN 1 AND 3),
    intermediate_dids TEXT[] NOT NULL DEFAULT '{}',
    trust_score DOUBLE PRECISION NOT NULL,
    contributing_connection_ids UUID[] NOT NULL DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Full recompute. TRUNCATE + INSERT keeps the table small (<= 10k rows at
-- depth 3) and avoids stale rows; the recursive CTE is cycle-safe via a
-- seen-array and stops at max_depth.
CREATE OR REPLACE FUNCTION community_graph.refresh_trust_paths()
RETURNS void AS $$
DECLARE
    max_depth CONSTANT INTEGER := 3;
BEGIN
    TRUNCATE community_graph.trust_path;

    INSERT INTO community_graph.trust_path (
        source_did, target_did, path_length, intermediate_dids,
        trust_score, contributing_connection_ids
    )
    WITH RECURSIVE paths AS (
        -- Base: direct edges (depth 1), score = edge weight * decay[1] = weight.
        SELECT
            tc.source_person_did                    AS source_did,
            tc.target_person_did                    AS target_did,
            1                                       AS depth,
            ARRAY[tc.target_person_did]             AS intermediate_dids,
            CASE tc.trust_level
                WHEN 'Vouched' THEN 1.0
                WHEN 'High'    THEN 0.75
                WHEN 'Medium'  THEN 0.5
                ELSE 0.25
            END                                     AS score,
            ARRAY[tc.id]                            AS connection_ids,
            ARRAY[tc.source_person_did, tc.target_person_did] AS seen
        FROM community_graph.trust_connection tc
        WHERE tc.deleted_at IS NULL
          AND tc.source_person_did IS DISTINCT FROM tc.target_person_did

        UNION ALL

        SELECT
            p.source_did,
            tc.target_person_did,
            p.depth + 1,
            p.intermediate_dids || tc.target_person_did,
            -- product of edge weights, then path-length decay
            -- (1:1.0, 2:0.8, 3:0.6)
            p.score * CASE tc.trust_level
                WHEN 'Vouched' THEN 1.0
                WHEN 'High'    THEN 0.75
                WHEN 'Medium'  THEN 0.5
                ELSE 0.25
            END
            * CASE p.depth + 1
                WHEN 2 THEN 0.8
                ELSE 0.6
              END                                   AS score,
            p.connection_ids || tc.id,
            p.seen || tc.target_person_did
        FROM paths p
        JOIN community_graph.trust_connection tc
          ON tc.source_person_did = p.target_did
         AND tc.deleted_at IS NULL
         AND tc.source_person_did IS DISTINCT FROM tc.target_person_did
        WHERE p.depth < max_depth
          AND NOT (tc.target_person_did = ANY(p.seen))
    )
    SELECT
        source_did, target_did, depth, intermediate_dids,
        score, connection_ids
    FROM paths;

    UPDATE community_graph.trust_path SET updated_at = now();
END;
$$ LANGUAGE plpgsql SECURITY DEFINER
SET search_path = community_graph, public;

-- Indexes (the API serves source-centric lookups and path scans).
CREATE INDEX IF NOT EXISTS idx_trust_path_source ON community_graph.trust_path (source_did, target_did);
CREATE INDEX IF NOT EXISTS idx_trust_path_source_only ON community_graph.trust_path (source_did);
CREATE INDEX IF NOT EXISTS idx_trust_path_target ON community_graph.trust_path (target_did);

-- Synchronous refresh on any trust_connection write.
CREATE OR REPLACE FUNCTION community_graph.trigger_refresh_trust_paths()
RETURNS trigger AS $$
BEGIN
    PERFORM community_graph.refresh_trust_paths();
    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_trust_connection_refresh_trust_paths ON community_graph.trust_connection;
CREATE TRIGGER trg_trust_connection_refresh_trust_paths
    AFTER INSERT OR UPDATE OR DELETE ON community_graph.trust_connection
    FOR EACH STATEMENT
    EXECUTE FUNCTION community_graph.trigger_refresh_trust_paths();

-- Grants (mirror the community_graph RLS migration pattern).
GRANT USAGE ON SCHEMA community_graph TO app_user, api_key;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA community_graph TO app_user, api_key;
GRANT EXECUTE ON FUNCTION community_graph.refresh_trust_paths(),
    community_graph.trigger_refresh_trust_paths() TO app_user, api_key;

-- ============================================================================
-- onboarding starter-pack join analytics (issue #44)
-- ============================================================================

CREATE SCHEMA IF NOT EXISTS onboarding;

CREATE TABLE IF NOT EXISTS onboarding.starter_pack_join (
    pack_id UUID NOT NULL,
    did TEXT NOT NULL,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (pack_id, did)
);

CREATE INDEX IF NOT EXISTS idx_starter_pack_join_did ON onboarding.starter_pack_join (did);

GRANT USAGE ON SCHEMA onboarding TO app_user, api_key;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA onboarding TO app_user, api_key;
ALTER DEFAULT PRIVILEGES IN SCHEMA onboarding
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO app_user, api_key;

-- ============================================================================
-- Permissioned data spaces — embedded space authority (issue #39, teaser)
-- ============================================================================
-- Minimal `space` + `space_member` tables backing the embedded space authority
-- in cosmos-extensions::space. `space.config` is the simplespace config jsonb
-- (policy / appAccess / ownerDid / spaceType / skey); `space_member` is the
-- member registry surfaced by `getSpace` (member *decisions* are derived from
-- the consent/delegation/support tables, not from this list). Repo-host tables
-- (writer/registration/jti) are deferred with Wave E.

CREATE SCHEMA IF NOT EXISTS space;

CREATE TABLE IF NOT EXISTS space.space (
    space_uri TEXT NOT NULL PRIMARY KEY,
    config JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS space.space_member (
    space_uri TEXT NOT NULL,
    did TEXT NOT NULL,
    added_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    removed_at TIMESTAMPTZ,
    PRIMARY KEY (space_uri, did)
);

CREATE INDEX IF NOT EXISTS idx_space_member_did ON space.space_member (did);
CREATE INDEX IF NOT EXISTS idx_space_deleted_at ON space.space (deleted_at);

GRANT USAGE ON SCHEMA space TO app_user, api_key;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA space TO app_user, api_key;
ALTER DEFAULT PRIVILEGES IN SCHEMA space
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO app_user, api_key;
