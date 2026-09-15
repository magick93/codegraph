# 05_auth_required_401.hurl
# Missing credentials must be rejected before any data access

GET {{base_url}}/api/v1/common/code
HTTP 401

GET {{base_url}}/api/v1/common/code
Authorization: Bearer sk_invalid_key_does_not_exist
HTTP 401
