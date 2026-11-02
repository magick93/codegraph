# hurl_contract: tenant isolation — a cross-tenant read is a silent RLS
# filter (404), never a 403. Generated once per run against the first
# tenant-scoped entity.

POST {{base_url}}/api/v1/events/public-event
Authorization: Bearer {{api_key_a}}
{
  "title": "org-a-public_event",
  "family_name": "org-a-public_event",
  "given_name": "org-a-public_event"
}

HTTP 201
[Captures]
cross_tenant_public_event_id: jsonpath "$.data.id"

GET {{base_url}}/api/v1/events/public-event/{{cross_tenant_public_event_id}}
Authorization: Bearer {{api_key_b}}

HTTP 404
