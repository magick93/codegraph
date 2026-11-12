# Hand-authored contract for the leave workflow transition machine
# (WS2 workers live gate). Runs through the gateway like every other file in
# the generated hurl dir (vars: base_url, api_key — exactly what the workers
# suite passes).
#
# Pins the branching machine seeded from domains.toml
# (leave.leave_request: draft -> submitted -> approved | rejected, approved/
# rejected terminal) through the generated workflow action endpoint:
#   POST /api/v1/leave/leave-request/{id}/actions/transition
#
# The GENUINE-status contract: 201 (create) -> 200 (draft->submitted) ->
# 409 (back-transition refused) -> 200 (submitted->approved, terminal) ->
# 409 (any transition from a terminal state). Wrong-state writes must be a
# 409 Conflict — never a silent 200.

# CREATE — capture the request id (the machine-managed status is excluded
# from the create DTO, so the body carries the required scalar only)
POST {{base_url}}/api/v1/leave/leave-request
Authorization: Bearer {{api_key}}
{
  "title": "gate-workflow-409"
}

HTTP 201
[Captures]
leave_request_id: jsonpath "$.data.id"
[Asserts]
jsonpath "$.data.id" exists

# draft -> submitted — the only transition valid from the initial state
POST {{base_url}}/api/v1/leave/leave-request/{{leave_request_id}}/actions/transition
Authorization: Bearer {{api_key}}
{
  "target_state": "submitted"
}

HTTP 200
[Asserts]
jsonpath "$.data.current_state" == "submitted"

# submitted -> draft — INVALID: the machine never goes back; must be a 409
POST {{base_url}}/api/v1/leave/leave-request/{{leave_request_id}}/actions/transition
Authorization: Bearer {{api_key}}
{
  "target_state": "draft"
}

HTTP 409

# submitted -> approved — reaches the terminal state
POST {{base_url}}/api/v1/leave/leave-request/{{leave_request_id}}/actions/transition
Authorization: Bearer {{api_key}}
{
  "target_state": "approved"
}

HTTP 200
[Asserts]
jsonpath "$.data.current_state" == "approved"
jsonpath "$.data.is_terminal" == true

# approved -> rejected — INVALID: the machine is already terminal; 409
POST {{base_url}}/api/v1/leave/leave-request/{{leave_request_id}}/actions/transition
Authorization: Bearer {{api_key}}
{
  "target_state": "rejected"
}

HTTP 409

# Unknown state — the transition validator rejects it; 409
POST {{base_url}}/api/v1/leave/leave-request/{{leave_request_id}}/actions/transition
Authorization: Bearer {{api_key}}
{
  "target_state": "archived"
}

HTTP 409
