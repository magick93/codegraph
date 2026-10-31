# hurl_contract: RLS isolation — ops api-suite stage 8 convention file.
# Runs with api_key_a / api_key_b only; the manifest [hurl].skip list keeps
# it out of the main hurl loop.

POST {{base_url}}/api/v1/common/event-base
Authorization: Bearer {{api_key_a}}
{
  "title": "rls-isolation-event_base",
  "family_name": "rls-isolation-event_base",
  "given_name": "rls-isolation-event_base"
}

HTTP 201
[Captures]
rls_event_base_id: jsonpath "$.data.id"

GET {{base_url}}/api/v1/common/event-base?page=0&page_size=100
Authorization: Bearer {{api_key_b}}

HTTP 200
[Asserts]
jsonpath "$.data[*].id" not contains "{{rls_event_base_id}}"

GET {{base_url}}/api/v1/common/event-base/{{rls_event_base_id}}
Authorization: Bearer {{api_key_a}}

HTTP 200
