# hurl_contract: authz scope — the read-only key (api_key_limited) may read
# but every write is denied with 403 INSUFFICIENT_SCOPE. Generated once per
# run against the first tenant-scoped entity.

GET {{base_url}}/api/v1/events/public-event?page=0&page_size=10
Authorization: Bearer {{api_key_limited}}

HTTP 200

POST {{base_url}}/api/v1/events/public-event
Authorization: Bearer {{api_key_limited}}
{
  "title": "scope-denial-public_event",
  "family_name": "scope-denial-public_event",
  "given_name": "scope-denial-public_event"
}

HTTP 403
[Asserts]
jsonpath "$.error.code" == "FORBIDDEN"
jsonpath "$.error.message" contains "INSUFFICIENT_SCOPE"
