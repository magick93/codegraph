# 02_code_crud.hurl
# Full CRUD round-trip on the smoke entity (common/code) with the
# full-wildcard org-A key ({{api_key}})

# ── 1. Create ──────────────────────────────────────────────────────
POST {{base_url}}/api/v1/common/code
Authorization: Bearer {{api_key}}
Content-Type: application/json
{}
HTTP 201
[Captures]
code_id: jsonpath "$.data.id"
[Asserts]
jsonpath "$.data.id" exists
jsonpath "$.data.created_at" exists
jsonpath "$.meta.correlation_id" exists

# ── 2. Read back ───────────────────────────────────────────────────
GET {{base_url}}/api/v1/common/code/{{code_id}}
Authorization: Bearer {{api_key}}
HTTP 200
[Asserts]
jsonpath "$.data.id" == {{code_id}}

# ── 3. Update ──────────────────────────────────────────────────────
PUT {{base_url}}/api/v1/common/code/{{code_id}}
Authorization: Bearer {{api_key}}
Content-Type: application/json
{}
HTTP 200

# ── 4. List shows the row ──────────────────────────────────────────
GET {{base_url}}/api/v1/common/code?page=0&page_size=10
Authorization: Bearer {{api_key}}
HTTP 200
[Asserts]
jsonpath "$.data" count >= 1
jsonpath "$.meta.correlation_id" exists

# ── 5. Delete (cleanup) ────────────────────────────────────────────
DELETE {{base_url}}/api/v1/common/code/{{code_id}}
Authorization: Bearer {{api_key}}
HTTP 204
