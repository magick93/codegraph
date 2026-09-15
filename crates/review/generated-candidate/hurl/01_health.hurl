# 01_health.hurl
# Liveness: the generated server answers on /health

GET {{base_url}}/health
HTTP 200
[Asserts]
jsonpath "$.status" == "ok"
