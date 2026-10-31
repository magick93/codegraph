# hurl_contract: RLS isolation — ops api-suite stage 8 convention file.
# Runs with api_key_a / api_key_b only; the manifest [hurl].skip list keeps
# it out of the main hurl loop.

POST {{base_url}}/api/v1/events/public-event
Authorization: Bearer {{api_key_a}}
{
  "title": "rls-isolation-public_event",
  "family_name": "rls-isolation-public_event",
  "given_name": "rls-isolation-public_event"
}

HTTP 201
[Captures]
rls_public_event_id: jsonpath "$.data.id"

GET {{base_url}}/api/v1/events/public-event?page=0&page_size=100
Authorization: Bearer {{api_key_b}}

HTTP 200
[Asserts]
jsonpath "$.data[*].id" not contains "{{rls_public_event_id}}"

GET {{base_url}}/api/v1/events/public-event/{{rls_public_event_id}}
Authorization: Bearer {{api_key_a}}

HTTP 200
