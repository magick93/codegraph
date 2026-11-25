# hurl_contract: media surface (issue #625 PR-B) — refunds.receipt.
# PROVIDER-FREE legs only. Since #690 every media handler resolves the owning
# row under the session organization BEFORE the storage-slot check, so the
# authn leg and the missing-parent leg are deterministic with NO bucket
# configured. The happy path (upload → display → remove, and the 413/415
# rejections) needs a configured provider and lives in the runtime lane
# (issue #625 OQ8 → #695 Stage 1).
# The URL is pinned to the router's own path shape by `media_pipeline_tests`
# (issues #691/#713): /api/v1/refunds/refund-request/{id}/{field} — the entity's path segment
# plus the nesting chain `router.tera` mounts a child entity under. A NESTED
# (child) contract addresses the route the handlers actually serve: they
# extract the ancestor tuple and prove the addressed row belongs to the URL
# parent (issue #722), so the legs below are live.

# authn — no credentials, rejected by the shared middleware before the
# handler (and therefore before any extractor) runs.
PUT {{base_url}}/api/v1/refunds/refund-request/11111111-1111-4111-8111-111111111111/receipt

HTTP 401
[Asserts]
jsonpath "$.error.code" == "UNAUTHORIZED"

# ownership — a VALID key against an id no row carries must be 404, never
# 403: a foreign or missing parent is indistinguishable by design. Addressed
# through the GET display route (same path, no body extractor), so the
# answer comes from the handler and not from multipart parsing.
GET {{base_url}}/api/v1/refunds/refund-request/11111111-1111-4111-8111-111111111111/receipt
Authorization: Bearer {{api_key}}

HTTP 404
[Asserts]
jsonpath "$.error.code" == "NOT_FOUND"

