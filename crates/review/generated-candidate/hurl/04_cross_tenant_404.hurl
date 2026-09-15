# 04_cross_tenant_404.hurl
# Cross-tenant RLS isolation over HTTP: org B ({{api_key_b}}) must not see a
# row created by org A ({{api_key_a}}) — silent RLS filter, HTTP 404.

# ── 1. Org A creates a row ─────────────────────────────────────────
POST {{base_url}}/api/v1/common/code
Authorization: Bearer {{api_key_a}}
Content-Type: application/json
{}
HTTP 201
[Captures]
code_id: jsonpath "$.data.id"

# ── 2. Org B's GET is a silent 404, not a 403 ──────────────────────
GET {{base_url}}/api/v1/common/code/{{code_id}}
Authorization: Bearer {{api_key_b}}
HTTP 404

# ── 3. Org B's list does not contain org A's row ───────────────────
GET {{base_url}}/api/v1/common/code
Authorization: Bearer {{api_key_b}}
HTTP 200
[Asserts]
jsonpath "$.data[*].id" not contains {{code_id}}

# ── 4. Cleanup: org A deletes its row ──────────────────────────────
DELETE {{base_url}}/api/v1/common/code/{{code_id}}
Authorization: Bearer {{api_key_a}}
HTTP 204
