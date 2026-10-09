# hurl_contract: recruiting.candidate — API contract (generated; do not edit).
# Regenerate with the codegraph pipeline. Vars: base_url, api_key.

# LIST — envelope shape
GET {{base_url}}/api/v1/recruiting/candidate?page=0&page_size=10
Authorization: Bearer {{api_key}}

HTTP 200
[Asserts]
jsonpath "$.data" exists
jsonpath "$.meta" exists

# CREATE — 201, capture the id
POST {{base_url}}/api/v1/recruiting/candidate
Authorization: Bearer {{api_key}}
{
  "family_name": "contract-candidate",
  "given_name": "contract-candidate",
  "candidate_id": "contract-candidate"
}

HTTP 201
[Captures]
candidate_id: jsonpath "$.data.id"
[Asserts]
jsonpath "$.data.id" exists

# GET by id — field echo
GET {{base_url}}/api/v1/recruiting/candidate/{{candidate_id}}
Authorization: Bearer {{api_key}}

HTTP 200
[Asserts]
jsonpath "$.data.family_name" == "contract-candidate"
jsonpath "$.data.given_name" == "contract-candidate"
jsonpath "$.data.candidate_id" == "contract-candidate"

# GET zero-uuid — 404
GET {{base_url}}/api/v1/recruiting/candidate/00000000-0000-0000-0000-000000000000
Authorization: Bearer {{api_key}}

HTTP 404

# PUT roundtrip
PUT {{base_url}}/api/v1/recruiting/candidate/{{candidate_id}}
Authorization: Bearer {{api_key}}
{
  "family_name": "contract-candidate-v2",
  "given_name": "contract-candidate-v2",
  "candidate_id": "contract-candidate-v2"
}

HTTP 200

# GET — the update landed
GET {{base_url}}/api/v1/recruiting/candidate/{{candidate_id}}
Authorization: Bearer {{api_key}}

HTTP 200
[Asserts]
jsonpath "$.data.family_name" == "contract-candidate-v2"
jsonpath "$.data.given_name" == "contract-candidate-v2"
jsonpath "$.data.candidate_id" == "contract-candidate-v2"
