# Graph artifact document conformance cases (issue #275)

Executable contract for the deterministic graph artifact document. One `##`
heading per case; fences carry the expectation:

- `json canonical` — the canonical serialization. It must be a fixed point:
  parse → normalize → re-serialize reproduces these bytes exactly. Field
  order is fixed by the document structs (`formatVersion`, `nodes`,
  `edges`; node fields `label`, `key`, `properties`; edge fields `type`,
  `from`, `to`, `properties`).
- `json accepted` — an equivalent document (different whitespace, key or
  element order) that must normalize onto the canonical bytes.
- `json rejected diagnostic=<name>` — must fail with the named diagnostic.

Document contract (formatVersion 1): `formatVersion` (u32), `nodes` sorted
by `(label, key)`, `edges` sorted by `(type, from, to, properties)`. A node
key is `<label>::<sha256-16(canonical property JSON)>`; edge endpoints
reference node keys. Property keys are sorted; null properties are omitted.

## canonical_document_is_a_fixed_point

```json canonical
{"formatVersion":1,"nodes":[{"label":"Domain","key":"Domain::7295b599c16d19e3","properties":{"name":"orders"}},{"label":"Domain","key":"Domain::c39579471ae02a3d","properties":{"name":"widgets"}}],"edges":[{"type":"InDomain","from":"Domain::7295b599c16d19e3","to":"Domain::c39579471ae02a3d","properties":{}}]}
```

## accepted_variant_normalizes_onto_canonical

```json canonical
{"formatVersion":1,"nodes":[{"label":"Domain","key":"Domain::7295b599c16d19e3","properties":{"name":"orders"}},{"label":"Domain","key":"Domain::c39579471ae02a3d","properties":{"name":"widgets"}}],"edges":[{"type":"InDomain","from":"Domain::7295b599c16d19e3","to":"Domain::c39579471ae02a3d","properties":{}}]}
```

```json accepted
{

  "formatVersion": 1,
  "nodes": [
    {
      "label": "Domain",
      "properties": {"name": "widgets"},
      "key": "Domain::c39579471ae02a3d"
    },
    {
      "label": "Domain",
      "properties": {"name": "orders"},
      "key": "Domain::7295b599c16d19e3"
    }
  ],
  "edges": [
    {
      "type": "InDomain",
      "from": "Domain::7295b599c16d19e3",
      "to": "Domain::c39579471ae02a3d",
      "properties": {}
    }
  ]
}
```

## newer_format_version_is_rejected

```json rejected diagnostic=format_version_too_new
{"edges":[],"formatVersion":2,"nodes":[]}
```
