# hurl_contract: authn — missing and garbage API keys are rejected (401).
# Generated once per run against the first tenant-scoped entity.

GET {{base_url}}/api/v1/common/event-base

HTTP 401
[Asserts]
jsonpath "$.error.code" == "UNAUTHORIZED"

GET {{base_url}}/api/v1/common/event-base
Authorization: Bearer not-a-real-key

HTTP 401
[Asserts]
jsonpath "$.error.code" == "UNAUTHORIZED"
