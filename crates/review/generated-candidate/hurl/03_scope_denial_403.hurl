# 03_scope_denial_403.hurl
# Scope enforcement over HTTP (#169): the read-only key ({{api_key_limited}})
# must be denied a write by the `scope_enforced_insert` RLS policy. Postgres
# raises P0403 INSUFFICIENT_SCOPE; the generated error mapper maps it to 403.

# ── 1. The read-only key is a valid key: reads pass ────────────────
GET {{base_url}}/api/v1/common/code
Authorization: Bearer {{api_key_limited}}
HTTP 200
[Asserts]
jsonpath "$.data" exists

# ── 2. But its write is rejected: P0403 INSUFFICIENT_SCOPE → 403 ──
POST {{base_url}}/api/v1/common/code
Authorization: Bearer {{api_key_limited}}
Content-Type: application/json
{}
HTTP 403
[Asserts]
body contains "INSUFFICIENT_SCOPE"
