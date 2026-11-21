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

# ?include=candidate — GET hydrates the relation (issue #781 probe)
GET {{base_url}}/api/v1/recruiting/applications/{{application_id}}?include=candidate
Authorization: Bearer {{api_key}}

HTTP 200
[Asserts]
jsonpath "$.included.candidate" != null

# ?include=candidate — LIST hydrates the batch map
GET {{base_url}}/api/v1/recruiting/applications?page=0&page_size=10&include=candidate
Authorization: Bearer {{api_key}}

HTTP 200
[Asserts]
jsonpath "$.included" exists
jsonpath "$.included.candidate" exists

# ?include_deleted=true — the soft-delete read toggle (issue #792). The role
# gate: JWT callers need a `manager`-or-better role; API-key sessions pass
# (their scopes enforce at the DB tier).
POST {{base_url}}/api/v1/recruiting/applications
Authorization: Bearer {{api_key}}
{
  "application_id": "contract-application"
}

HTTP 201
[Captures]
application_deleted_id: jsonpath "$.data.id"

DELETE {{base_url}}/api/v1/recruiting/applications/{{application_deleted_id}}
Authorization: Bearer {{api_key}}

HTTP 204

GET {{base_url}}/api/v1/recruiting/applications/{{application_deleted_id}}?include_deleted=true
Authorization: Bearer {{api_key}}

HTTP 200
[Asserts]
jsonpath "$.data.id" == "{{application_deleted_id}}"

GET {{base_url}}/api/v1/recruiting/applications/{{application_deleted_id}}
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
