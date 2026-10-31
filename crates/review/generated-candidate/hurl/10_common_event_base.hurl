# hurl_contract: common.event_base — API contract (generated; do not edit).
# Regenerate with the codegraph pipeline. Vars: base_url, api_key.

# LIST — envelope shape
GET {{base_url}}/api/v1/common/event-base?page=0&page_size=10
Authorization: Bearer {{api_key}}

HTTP 200
[Asserts]
jsonpath "$.data" exists
jsonpath "$.meta" exists

# CREATE — 201, capture the id
POST {{base_url}}/api/v1/common/event-base
Authorization: Bearer {{api_key}}
{
  "title": "contract-event_base",
  "family_name": "contract-event_base",
  "given_name": "contract-event_base"
}

HTTP 201
[Captures]
event_base_id: jsonpath "$.data.id"
[Asserts]
jsonpath "$.data.id" exists

# GET by id — field echo
GET {{base_url}}/api/v1/common/event-base/{{event_base_id}}
Authorization: Bearer {{api_key}}

HTTP 200
[Asserts]
jsonpath "$.data.title" == "contract-event_base"
jsonpath "$.data.family_name" == "contract-event_base"
jsonpath "$.data.given_name" == "contract-event_base"

# GET zero-uuid — 404
GET {{base_url}}/api/v1/common/event-base/00000000-0000-0000-0000-000000000000
Authorization: Bearer {{api_key}}

HTTP 404

# PUT roundtrip
PUT {{base_url}}/api/v1/common/event-base/{{event_base_id}}
Authorization: Bearer {{api_key}}
{
  "title": "contract-event_base-v2",
  "family_name": "contract-event_base-v2",
  "given_name": "contract-event_base-v2"
}

HTTP 200

# GET — the update landed
GET {{base_url}}/api/v1/common/event-base/{{event_base_id}}
Authorization: Bearer {{api_key}}

HTTP 200
[Asserts]
jsonpath "$.data.title" == "contract-event_base-v2"
jsonpath "$.data.family_name" == "contract-event_base-v2"
jsonpath "$.data.given_name" == "contract-event_base-v2"

# DELETE — 204
DELETE {{base_url}}/api/v1/common/event-base/{{event_base_id}}
Authorization: Bearer {{api_key}}

HTTP 204

# GET — gone
GET {{base_url}}/api/v1/common/event-base/{{event_base_id}}
Authorization: Bearer {{api_key}}

HTTP 404
