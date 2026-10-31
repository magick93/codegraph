# hurl_contract: tenant isolation — a cross-tenant read is a silent RLS
# filter (404), never a 403. Generated once per run against the first
# tenant-scoped entity.

POST {{base_url}}/api/v1/common/event-base
Authorization: Bearer {{api_key_a}}
{
  "title": "org-a-event_base",
  "family_name": "org-a-event_base",
  "given_name": "org-a-event_base"
}

HTTP 201
[Captures]
cross_tenant_event_base_id: jsonpath "$.data.id"

GET {{base_url}}/api/v1/common/event-base/{{cross_tenant_event_base_id}}
Authorization: Bearer {{api_key_b}}

HTTP 404
