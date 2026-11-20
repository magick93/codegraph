# Hand-authored contract for the PositiveGrant eligibility rule (issue #338,
# committed via issue #775). Canonical copy: tests/fixtures/review_gate/hurl/
# — rules_guard_api_gate seeds it into the regenerated fixture's hurl dir
# (the workers gate's post_generate 409-contract precedent; the review
# fixture regenerates in-process, so the seed is gate code, not a hook).
# Vars: base_url, api_key.
#
# Pins the modeled guard `eligibility rule PositiveGrant: amount > 0.0`
# (tests/fixtures/rosetta/grant.rosetta) over real HTTP:
#   401 — anonymous create on the guarded surface;
#   403 — amount = 0 is denied AND compensated (no amount-0 row survives;
#         rules_guard_api_gate proves the compensation in psql);
#   201 — amount > 0 passes the guard and lands;
#   200 — the eligible record reads back through the read guard.
# An eligibility denial must never be a silent 201.

# Anonymous create — the guarded surface still requires auth first
POST {{base_url}}/api/v1/recruiting/grant-application
{
  "amount": 150.0,
  "reference": "eligibility-anonymous"
}

HTTP 401
[Asserts]
jsonpath "$.error.code" == "UNAUTHORIZED"

# Ineligible create — amount = 0 fails PositiveGrant; denied with 403
POST {{base_url}}/api/v1/recruiting/grant-application
Authorization: Bearer {{api_key}}
{
  "amount": 0,
  "reference": "eligibility-zero"
}

HTTP 403
[Asserts]
jsonpath "$.error.code" == "FORBIDDEN"
jsonpath "$.error.message" contains "PositiveGrant"

# Eligible create — amount > 0 passes the guard and lands (201)
POST {{base_url}}/api/v1/recruiting/grant-application
Authorization: Bearer {{api_key}}
{
  "amount": 150.0,
  "reference": "eligibility-positive"
}

HTTP 201
[Captures]
grant_application_id: jsonpath "$.data.id"
[Asserts]
jsonpath "$.data.id" exists
jsonpath "$.data.amount" == 150.0

# GET by id — the read guard lets the eligible record through
GET {{base_url}}/api/v1/recruiting/grant-application/{{grant_application_id}}
Authorization: Bearer {{api_key}}

HTTP 200
[Asserts]
jsonpath "$.data.amount" == 150.0
jsonpath "$.data.reference" == "eligibility-positive"
