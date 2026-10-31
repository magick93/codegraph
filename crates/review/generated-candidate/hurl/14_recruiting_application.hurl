# hurl_contract: recruiting.application — API contract (generated; do not edit).
# Regenerate with the codegraph pipeline. Vars: base_url, api_key.

# LIST — envelope shape
GET {{base_url}}/api/v1/recruiting/applications?page=0&page_size=10
Authorization: Bearer {{api_key}}

HTTP 200
[Asserts]
jsonpath "$.data" exists
jsonpath "$.meta" exists

# CREATE — 201, capture the id
POST {{base_url}}/api/v1/recruiting/applications
Authorization: Bearer {{api_key}}
{
  "application_id": "contract-application"
}

HTTP 201
[Captures]
application_id: jsonpath "$.data.id"
[Asserts]
jsonpath "$.data.id" exists

# GET by id — field echo
GET {{base_url}}/api/v1/recruiting/applications/{{application_id}}
Authorization: Bearer {{api_key}}

HTTP 200
[Asserts]
jsonpath "$.data.application_id" == "contract-application"

# GET zero-uuid — 404
GET {{base_url}}/api/v1/recruiting/applications/00000000-0000-0000-0000-000000000000
Authorization: Bearer {{api_key}}

HTTP 404

# PUT roundtrip
PUT {{base_url}}/api/v1/recruiting/applications/{{application_id}}
Authorization: Bearer {{api_key}}
{
  "application_id": "contract-application-v2"
}

HTTP 200

# GET — the update landed
GET {{base_url}}/api/v1/recruiting/applications/{{application_id}}
Authorization: Bearer {{api_key}}

HTTP 200
[Asserts]
jsonpath "$.data.application_id" == "contract-application-v2"

# DELETE — 204
DELETE {{base_url}}/api/v1/recruiting/applications/{{application_id}}
Authorization: Bearer {{api_key}}

HTTP 204

# GET — gone
GET {{base_url}}/api/v1/recruiting/applications/{{application_id}}
Authorization: Bearer {{api_key}}

HTTP 404
