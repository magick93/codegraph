//! A FUNCTIONAL pgmq stand-in for plain Postgres (CI's postgres:15 has no
//! pgmq extension): real queue tables with pgmq's msg_id/read_ct/vt
//! contract, so generated drain SQL behaves as it does against real pgmq.
//!
//! The OVERLOAD SET matters as much as the behaviour (issue #825). pgmq
//! resolves `delete` by parameter type — it ships BOTH `delete(text,
//! bigint)` and `delete(text, bigint[])` (pgmq 1.5.1) — so a stand-in with
//! a single `delete` accepts an untyped `pgmq.delete($1, $2)` that the real
//! extension refuses (`function pgmq.delete(unknown, unknown) is not
//! unique`). This stand-in mirrors the pair: a generated statement that
//! relies on the server inferring a parameter type fails here exactly as
//! it does in production.
//!
//! Consumers: the events runtime gate (`events_runtime_tests`, the
//! monolith drain end to end) and the dispatch pgmq pin
//! (`webhook_dispatch_pgmq_tests`, the workers statements under
//! tokio-postgres' untyped binding). One stand-in, so the two gates cannot
//! disagree about the overload set.
//!
//! Only the `pgmq` schema and its functions: no roles, no grants. A gate
//! that needs the harness roles provisions them itself.

pub const PGMQ_FUNCTIONAL_SHIM: &str = r#"
-- Functional pgmq stand-in (no extension available).
CREATE SCHEMA IF NOT EXISTS pgmq;

CREATE OR REPLACE FUNCTION pgmq.create(p_name text) RETURNS void
AS $shim$ BEGIN
  EXECUTE format('CREATE TABLE IF NOT EXISTS pgmq.q_%I (msg_id BIGSERIAL PRIMARY KEY, read_ct INT DEFAULT 0, enqueued_at TIMESTAMPTZ DEFAULT now(), vt TIMESTAMPTZ NOT NULL DEFAULT now(), message JSONB)', p_name);
END $shim$ LANGUAGE plpgsql;

-- One two-argument overload, as in pgmq 1.5.1: `pgmq.send($1, $2)` is
-- unambiguous by arity.
CREATE OR REPLACE FUNCTION pgmq.send(p_queue text, p_msg jsonb) RETURNS bigint
AS $shim$ DECLARE v_id bigint; BEGIN
  EXECUTE format('INSERT INTO pgmq.q_%I (message) VALUES ($1) RETURNING msg_id', p_queue) INTO v_id USING p_msg;
  RETURN v_id;
END $shim$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION pgmq.read(p_queue text, p_vt int, p_qty int)
RETURNS TABLE (msg_id bigint, read_ct int, message jsonb)
AS $shim$ BEGIN
  RETURN QUERY EXECUTE format(
    'UPDATE pgmq.q_%I SET vt = now() + make_interval(secs => $1), read_ct = read_ct + 1 WHERE msg_id IN (SELECT msg_id FROM pgmq.q_%I WHERE vt <= now() ORDER BY msg_id LIMIT $2) RETURNING msg_id, read_ct, message', p_queue, p_queue)
    USING p_vt, p_qty;
END $shim$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION pgmq.delete(p_queue text, p_msg bigint) RETURNS void
AS $shim$ BEGIN
  EXECUTE format('DELETE FROM pgmq.q_%I WHERE msg_id = $1', p_queue) USING p_msg;
END $shim$ LANGUAGE plpgsql;

-- pgmq ships BOTH delete overloads (`delete(text, bigint)` and
-- `delete(text, bigint[])`; pgmq 1.5.1). The stand-in mirrors the pair so
-- an untyped `pgmq.delete($1, $2)` is ambiguous here exactly as it is
-- against the real extension (issue #825).
CREATE OR REPLACE FUNCTION pgmq.delete(p_queue text, p_msgs bigint[]) RETURNS void
AS $shim$ BEGIN
  EXECUTE format('DELETE FROM pgmq.q_%I WHERE msg_id = ANY($1)', p_queue) USING p_msgs;
END $shim$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION pgmq.list_queues() RETURNS TABLE (queue_name text)
AS $shim$ SELECT substring(tablename FROM 3) FROM pg_tables WHERE schemaname = 'pgmq' AND tablename LIKE 'q\_%'; $shim$ LANGUAGE sql;
"#;
