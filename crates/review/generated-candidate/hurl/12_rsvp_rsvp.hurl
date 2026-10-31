# hurl_contract: rsvp.rsvp — API contract (generated; do not edit).
# Regenerate with the codegraph pipeline. Vars: base_url, api_key.

# LIST — envelope shape
GET {{base_url}}/api/v1/rsvp/rsvp?page=0&page_size=10
Authorization: Bearer {{api_key}}

HTTP 200
[Asserts]
jsonpath "$.data" exists
jsonpath "$.meta" exists

# CREATE — 201, capture the id
POST {{base_url}}/api/v1/rsvp/rsvp
Authorization: Bearer {{api_key}}
{
  "status": "Confirmed",
  "timestamp": "2026-01-01T00:00:00Z"
}

HTTP 201
[Captures]
rsvp_id: jsonpath "$.data.id"
[Asserts]
jsonpath "$.data.id" exists

# GET by id — field echo
GET {{base_url}}/api/v1/rsvp/rsvp/{{rsvp_id}}
Authorization: Bearer {{api_key}}

HTTP 200
[Asserts]
jsonpath "$.data.status" == "Confirmed"
jsonpath "$.data.timestamp" == "2026-01-01T00:00:00Z"

# GET zero-uuid — 404
GET {{base_url}}/api/v1/rsvp/rsvp/00000000-0000-0000-0000-000000000000
Authorization: Bearer {{api_key}}

HTTP 404

# PUT roundtrip
PUT {{base_url}}/api/v1/rsvp/rsvp/{{rsvp_id}}
Authorization: Bearer {{api_key}}
{
  "status": "Pending",
  "timestamp": "2026-01-01T00:00:00Z"
}

HTTP 200

# GET — the update landed
GET {{base_url}}/api/v1/rsvp/rsvp/{{rsvp_id}}
Authorization: Bearer {{api_key}}

HTTP 200
[Asserts]
jsonpath "$.data.status" == "Pending"
jsonpath "$.data.timestamp" == "2026-01-01T00:00:00Z"

# DELETE — 204
DELETE {{base_url}}/api/v1/rsvp/rsvp/{{rsvp_id}}
Authorization: Bearer {{api_key}}

HTTP 204

# GET — gone
GET {{base_url}}/api/v1/rsvp/rsvp/{{rsvp_id}}
Authorization: Bearer {{api_key}}

HTTP 404
