# Eligibility-rule endpoint guard (issue #338) — hand-authored fixture file
# (survives regeneration; the generated per-entity contracts stay untouched).
#
# The modeled `eligibility rule PositiveGrant from GrantApplicationType:
# amount > 0.0` gates every guarded operation at the handler site:
#
#   - an ineligible create (amount = 0) is denied with 403 AND compensated —
#     the row is removed (the api suite proves the absence via psql);
#   - an eligible create succeeds (201) and reads back (200).
#
# Vars: base_url, api_key.

# CREATE ineligible — the guard denies the operation with 403
POST {{base_url}}/api/v1/recruiting/grant-application
Authorization: Bearer {{api_key}}
{
  "reference": "ELIGIBILITY-DENY",
  "amount": 0
}

HTTP 403

# CREATE eligible — the guard lets the operation through
POST {{base_url}}/api/v1/recruiting/grant-application
Authorization: Bearer {{api_key}}
{
  "reference": "ELIGIBILITY-ALLOW",
  "amount": 42.5
}

HTTP 201
[Captures]
grant_application_id: jsonpath "$.data.id"
[Asserts]
jsonpath "$.data.id" exists
jsonpath "$.data.amount" == 42.5

# GET — the eligible record is readable (the read guard evaluates true)
GET {{base_url}}/api/v1/recruiting/grant-application/{{grant_application_id}}
Authorization: Bearer {{api_key}}

HTTP 200
[Asserts]
jsonpath "$.data.reference" == "ELIGIBILITY-ALLOW"
jsonpath "$.data.amount" == 42.5

# GET zero-uuid — still 404 (load-or-404 before any guard evaluation)
GET {{base_url}}/api/v1/recruiting/grant-application/00000000-0000-0000-0000-000000000000
Authorization: Bearer {{api_key}}

HTTP 404
