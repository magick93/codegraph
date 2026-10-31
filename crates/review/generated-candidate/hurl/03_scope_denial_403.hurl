# hurl_contract: authz scope — the read-only key (api_key_limited) may read
# but every write is denied with 403 INSUFFICIENT_SCOPE. Generated once per
# run against the first tenant-scoped entity.

GET {{base_url}}/api/v1/common/event-base?page=0&page_size=10
Authorization: Bearer {{api_key_limited}}

HTTP 200

POST {{base_url}}/api/v1/common/event-base
Authorization: Bearer {{api_key_limited}}
{
  "title": "scope-denial-event_base",
  "family_name": "scope-denial-event_base",
  "given_name": "scope-denial-event_base"
}

HTTP 403
[Asserts]
jsonpath "$.error.code" == "FORBIDDEN"
jsonpath "$.error.message" contains "INSUFFICIENT_SCOPE"
