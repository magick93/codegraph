# Platform Studio: Configuration Screens

## Audience & Purpose

This document describes every configuration screen in the Codegraph Platform
Studio — the web UI where enterprise users model their application. These
screens replace the 5+ TOML files, CLI flags, and hardcoded code defaults that
currently configure the engine.

**Target user:** Enterprise architect, domain expert, or senior developer
configuring a generated application. Not a sysadmin, not a junior dev. They
understand entities, relationships, and business rules but should never touch
TOML.

---

## 1. Core Workflow

```
1. Sign up / Create org
2. CREATE PROJECT → Connect Git repo → Platform clones + scans schemas
3. REVIEW AUTO-DISCOVERY → Platform presents entity/VO candidates
4. CONFIGURE DOMAINS → Organise schemas into bounded contexts
5. CONFIGURE ENTITIES → Per-entity: operations, workflow, search, hierarchy
6. CONFIGURE TYPE MAPPINGS → Primitive types, composite types, codelists
7. CONFIGURE UI → Override components, wizard flows, labels
8. CONFIGURE PROFILE → Select generators, features, output targets
9. CONFIGURE INTEGRATIONS → Connect extension points
10. GENERATE → Platform runs pipeline → commits code to repo branch
11. DEPLOY → (future) Push to staging/production environments
12. ITERATE → Edit config → Regenerate → Branch updates
```

---

## 2. Screen: Project Overview (Dashboard)

**Route:** `/projects/{project_id}`

**Purpose:** Status dashboard for one application project. Entry point for all
configuration.

```
┌─────────────────────────────────────────────────────────────┐
│  ◁ Projects   │   MyApp                           [Generate] │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────────────┐  ┌──────────────────────────────┐ │
│  │ Source               │  │ Last Generation               │ │
│  │ github.com/org/schemas│  │ 2 hours ago · feat/codegen-v3 │ │
│  │ branch: main          │  │ 54 files changed · ✅ Success │ │
│  │ 128 schemas found     │  │ [View Diff] [Commit Details]   │ │
│  └──────────────────────┘  └──────────────────────────────┘ │
│                                                              │
│  ┌──────────────────────┐  ┌──────────────────────────────┐ │
│  │ Configuration        │  │ Recent Activity               │ │
│  │ ──────────────────── │  │ - Added Workflow to Leave    │ │
│  │ ✅ Domains (8)        │  │ - Changed Type: Amount      │ │
│  │ ✅ Entities (42)      │  │ - Added Integration: Xero   │ │
│  │ ⚠️  Workflow (3 warn) │  │                              │ │
│  │ ✅ UI Overrides (5)   │  │ [View All Activity]          │ │
│  └──────────────────────┘  └──────────────────────────────┘ │
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ Configuration Quick Links                                 ││
│  │ ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐ ││
│  │ │Domains │ │Entities│ │ Types  │ │  UI    │ │Profile │ ││
│  │ └────────┘ └────────┘ └────────┘ └────────┘ └────────┘ ││
│  └──────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

**Elements:**
- **Header bar:** Breadcrumb (`Projects > MyApp`), action button "Generate"
- **Source card:** Git repo URL, branch, schema count — click opens Source screen
- **Last Generation card:** Timestamp, branch name, file count, status badge
  (green/red), "View Diff" links to the generated commit
- **Configuration card:** Summary of each config section with status checks
  (✅ configured, ⚠️ warnings, 🔴 missing required)
- **Recent Activity:** Timeline of config changes (audit trail)
- **Quick Links:** Large icon tiles navigating to each config screen

---

## 3. Screen: Source (Git Repository)

**Route:** `/projects/{project_id}/source`

**Purpose:** Connect the project to a Git repo. This is the schema source AND
the code output target.

```
┌─────────────────────────────────────────────────────────────┐
│  ◁ Project Overview   │   Source Configuration              │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ Git Repository                                            ││
│  │                                                          ││
│  │  Repository URL   ┌────────────────────────────────────┐ ││
│  │                   │ https://github.com/org/schemas.git  │ ││
│  │                   └────────────────────────────────────┘ ││
│  │                                                          ││
│  │  Auth Method      ● SSH Key  ○ Personal Access Token   │ ││
│  │                                                          ││
│  │  SSH Key                     [Upload Key File]          │ ││
│  │  ┌────────────────────────────────────────────────────┐ ││
│  │  │ ssh-ed25519 AAAAC3...opencode@machine               │ ││
│  │  └────────────────────────────────────────────────────┘ ││
│  │                                                          ││
│  │  Branch           ┌────────────────────────────────────┐ ││
│  │                   │ main                                │ ││
│  │                   └────────────────────────────────────┘ ││
│  │                                                          ││
│  │  Schema Path      ┌────────────────────────────────────┐ ││
│  │                   │ schemas/hr-open-standards/          │ ││
│  │                   └────────────────────────────────────┘ ││
│  │                                                          ││
│  │  Output Branch    ┌────────────────────────────────────┐ ││
│  │                   │ codegen/{timestamp}                 │ ││
│  │                   └────────────────────────────────────┘ ││
│  │                                                          ││
│  │  [Test Connection] │ ✅ Connected — 128 schemas found   │ │
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ Discovered Schemas (128)    [Refresh] [Filter...]        ││
│  │                                                          ││
│  │ ┌──────────────────────────────────────────────────────┐ ││
│  │ │ Icon │ Schema                    │ Domain     │ Type │ ││
│  │ ├──────────────────────────────────────────────────────┤ ││
│  │ │ 📄  │ PersonType.json           │ (unassigned)│ obj │ ││
│  │ │ 📄  │ EmployeeType.json         │ (unassigned)│ obj │ ││
│  │ │ 📄  │ PayRunType.json           │ (unassigned)│ obj │ ││
│  │ │ 📄  │ LeaveRequestType.json     │ (unassigned)│ obj │ ││
│  │ │ ...  │ (128 rows, paginated)    │             │     │ ││
│  │ └──────────────────────────────────────────────────────┘ ││
│  │                                                          ││
│  │ ✅ Schemas auto-discovered — about 100 per second        ││
│  │ You can now assign schemas to domains in the next screen ││
│  └──────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

**Elements:**
- **Repository URL:** Text input with placeholder. Supports HTTPS and SSH.
- **Auth Method:** Radio group — SSH Key (file upload or paste) or Personal
  Access Token (for HTTPS). Token field masks on blur.
- **Branch:** Text input with dropdown of branches discovered from remote.
- **Schema Path:** Relative path within repo to JSON Schema root directory.
- **Output Branch:** Template string `codegen/{timestamp}`. User can customise.
  Code gets committed to this branch on each generation.
- **Test Connection:** Action button. On click: clone repo (shallow), count
  schemas, show ✅ or ❌ with error message.
- **Schema Browser:** Paginated table of discovered `.json` schema files.
  Columns: icon (📄/🔗 for ref-only), filename, domain assignment (initially
  "unassigned"), type (object/enum). Click row → preview schema contents in a
  slide-over panel.
- **Refresh button:** Re-scans the repo for schema changes.

**What maps to config:**
| UI Field | TOML equivalent | Notes |
|---|---|---|
| Repository URL | — | New concept. No TOML equivalent |
| Auth / SSH Key | — | New concept |
| Branch | — | New concept |
| Schema Path | — | Previously CLI arg `--schemas` |
| Output Branch | — | New concept |

---

## 4. Screen: Domains (Bounded Contexts)

**Route:** `/projects/{project_id}/domains`

**Purpose:** Organise schemas into bounded contexts (domains). Define
dependencies between domains.

```
┌─────────────────────────────────────────────────────────────┐
│  ◁ Project Overview   │   Domains & Architecture            │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ Domain Dependency Diagram                                ││
│  │                                                          ││
│  │   ┌──────────┐       ┌──────────────┐                   ││
│  │   │ Common   │───▶──│ Compensation  │───▶── Payroll ──▶││
│  │   └──────────┘       └──────────────┘       Benefits   ││
│  │        │                                                ││
│  │        ├──▶ Timecard ──▶ Payroll                        ││
│  │        │                                                ││
│  │        └──▶ Recruiting ──▶ Screening                    ││
│  │                            ├── Interviewing             ││
│  │                            └── Assessments              ││
│  │                                                          ││
│  │        Wellness (no deps)                                ││
│  │                                                          ││
│  │    [Auto-Layout]  [Zoom: 100%]  [+ Add Domain]          ││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ Domain List (8)                                          ││
│  │                                                          ││
│  │ ┌──────────────────────────────────────────────────────┐ ││
│  │ │ Common  ● 6 entities  22 schemas  2 VOs  [Edit] [×]│ ││
│  │ │ Foundational types shared across all domains         │ ││
│  │ │ Postgres schema: common    ● core tier               │ ││
│  │ ├──────────────────────────────────────────────────────┤ ││
│  │ │ Payroll ● 4 entities  8 schemas  0 VOs    [Edit] [×]│ ││
│  │ │ Pay run processing, tax calculations, IRD filing     │ ││
│  │ │ Depends on: Compensation, Timecard   ● extended tier │ ││
│  │ ├──────────────────────────────────────────────────────┤ ││
│  │ │ Recruiting ● 3 entities  12 schemas  3 VOs [Edit] [×]│ ││
│  │ │ Vacancies, applications, candidates                   │ ││
│  │ │ Depends on: Common                  ● extended tier   │ ││
│  │ └──────────────────────────────────────────────────────┘ ││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ Global Defaults (applies to all domains)    [Edit]       ││
│  │ ┌──────────────────────────────────────────────────────┐ ││
│  │ │ App Name: codegraph-app                               │ ││
│  │ │ Type Suffix: Type  (stripped from schema titles)      │ ││
│  │ │ Default Operations: Create, Read, Update, Delete, List│ ││
│  │ │ Max Bulk Size: 100                                    │ ││
│  │ │ Split OpenAPI by Domain: Off                          │ ││
│  │ └──────────────────────────────────────────────────────┘ ││
│  └──────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

**Elements:**
- **Dependency Graph:** Interactive DAG rendered with SvelteFlow (reusing
  IFML diagram engine). Nodes are domains. Edges are `depends_on`
  relationships. Drag to rearrange. Click node → highlight. Double-click →
  edit domain. Cycles flagged with red border + error tooltip.
- **Auto-Layout button:** Runs topological sort layout algorithm.
- **Add Domain:** Opens a drawer to create a new domain.
- **Domain List:** Sortable table. Each row shows: domain name, status dot
  (🟢 configured / 🟡 needs attention / 🔴 missing), entity/VO counts,
  description, tier badge, action buttons.
- **Global Defaults:** Collapsible section for defaults inherited by all
  domains.

### Domain Edit Drawer (slide-over panel)

```
┌─────────────────────────────────────────────┐
│ ✕ Edit Domain: Payroll                      │
├─────────────────────────────────────────────┤
│                                             │
│  Label        ┌──────────────────────────┐  │
│               │ Payroll                   │  │
│               └──────────────────────────┘  │
│                                             │
│  Description  ┌──────────────────────────┐  │
│               │ Pay run processing, tax   │  │
│               │ calculations, IRD filing  │  │
│               └──────────────────────────┘  │
│                                             │
│  Postgres     ┌──────────────────────────┐  │
│  Schema       │ payroll                   │  │
│               └──────────────────────────┘  │
│                                             │
│  Tier         ● Core  ○ Extended          │  │
│                                             │
│  Dependencies ☑ Common                     │  │
│               ☑ Compensation               │  │
│               ☐ Timecard                   │  │
│               ☐ ...                        │  │
│                                             │
│  Auditable    ● Yes  ○ No                  │  │
│                                             │
│  Assigned     ┌──────────────────────────┐  │
│  Schemas      │ PayRunType               │  │
│               │ PaySlipType              │  │
│               │ TaxRecordType            │  │
│               │ + Add schemas...         │  │
│               └──────────────────────────┘  │
│                                             │
│  [Delete Domain]        [Cancel]  [Save]   │
└─────────────────────────────────────────────┘
```

**What maps to config:**
| UI Field | TOML equivalent | Notes |
|---|---|---|
| Label | `[domains.{name}].label` | |
| Description | (not in TOML) | New — free text for generated docs |
| Postgres Schema | `[domains.{name}].postgres_schema` | |
| Tier | `[domains.{name}].tier` | Dropdown: core / extended |
| Dependencies | `[domains.{name}].depends_on` | Multi-select from other domain labels |
| Auditable | `[domains.{name}].auditable` | Toggle |
| Assigned Schemas | `[domains.{name}].entities` | Multi-select from discovered schemas, auto-filtered to object types |
| App Name | `[defaults].app_name` | Global defaults |
| Type Suffix | `[defaults].type_suffix` | Global defaults |
| Default Operations | `[defaults].operations` | Checkbox group |
| Max Bulk Size | `[defaults].max_bulk_size` | Number input |
| Split OpenAPI | `[defaults].split_openapi_by_domain` | Toggle |

---

## 5. Screen: Entities

**Route:** `/projects/{project_id}/entities`

**Purpose:** Configure every schema type — classify as Entity or Value Object,
define CRUD operations, workflow, search, hierarchy, DTO structure, and
parent-child relationships.

```
┌─────────────────────────────────────────────────────────────┐
│  ◁ Project Overview   │   Entities & Value Objects         │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌────────────┐ ┌──────────────────────────────────────────┐│
│  │ Domain:    │ │ Search entities...              [Filter] ││
│  │ All        │ │                                            ││
│  │ Common     │ │ ┌────────────────────────────────────────┐││
│  │ Payroll    │ │ │ Status│ Entity         │ Domain  │Op│ │││
│  │ Timecard   │ │ ├────────────────────────────────────────┤││
│  │ Recruiting │ │ │ 🟢   │ Person          │ Common  │CRUD│ ││
│  │ ...        │ │ │ 🟢   │ Employee        │ Payroll │CRUD│ ││
│  └────────────┘ │ │ 🟡   │ PayRun          │ Payroll │CR L│ ││
│                  │ │ 🟢   │ LeaveRequest    │ Timecard│CRUD│ ││
│  ┌────────────┐  │ │ 🔴   │ Candidate       │Recruit │ CR │ ││
│  │ Quick:     │  │ │ 🟢   │ JobPosting      │Recruit │CRUD│ ││
│  │ Entities   │  │ │ 🟣   │ AddressType     │ Common │ VO │ ││
│  │ VOs        │  │ │ 🟣   │ AmountType      │ Common │ VO │ ││
│  │ Unassigned │  │ └────────────────────────────────────────┘││
│  │ Excluded   │  │                                            ││
│  └────────────┘  └──────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

**List views:**
- **Sidebar filter:** Left sidebar with domain tree + quick filters (Entities
  only / VOs only / Unassigned / Excluded)
- **Table:** Paginated. Status dot (🟢 classified, 🟡 needs review, 🔴
  unassigned, 🟣 Value Object), entity name, domain, enabled operations
  (shown as CRUD/L/etc), action buttons

**Clicking an entity row** opens the Entity Detail screen.

---

## 5a. Entity Detail Screen

**Route:** `/projects/{project_id}/entities/{entity_id}`

**Purpose:** Full configuration for a single entity or value object.

```
┌─────────────────────────────────────────────────────────────┐
│  ◁ Entities   │   Person   │  Domain: Common               │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────── Tabs ────────────────────────────────────────────┐│
│  │ [General] [Workflow] [Search] [Hierarchy] [DTO] [UI]   ││
│  └─────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌─── General Tab ─────────────────────────────────────────┐│
│  │                                                          ││
│  │  Schema Source: PersonType.json                          ││
│  │                                                          ││
│  │  Classification   ● Entity  ○ Value Object  ○ Exclude ││
│  │                                                          ││
│  │  Operations       ☑ Create  ☑ Read  ☑ Update           ││
│  │                    ☑ Delete  ☑ List                     ││
│  │                    ☐ Bulk Create                        ││
│  │                                                          ││
│  │  Role             ● Root  ○ Child                       ││
│  │                    (root = top-level API endpoint)       ││
│  │                                                          ││
│  │  API Path Segment  ┌──────────────────────────────────┐ ││
│  │                    │ people                            │ ││
│  │                    └──────────────────────────────────┘ ││
│  │                    Auto-derived: person → people        ││
│  │                                                          ││
│  │  Parent Entity    ┌──────────────────────────────────┐ ││
│  │                   │ (none — this is a root entity)    │ ││
│  │                   └──────────────────────────────────┘ ││
│  │                                                          ││
│  │  Auditable        ● Yes  ○ No                           ││
│  │                                                          ││
│  │  Max Bulk Size    ┌──────────────────────────────────┐ ││
│  │                   │ 100                               │ ││
│  │                   └──────────────────────────────────┘ ││
│  │                                                          ││
│  │  ┌── Schema Fields (from PersonType.json) ────────────┐ ││
│  │  │ Field Name          │ Type         │ Class.  │ ... │ ││
│  │  ├────────────────────────────────────────────────────┤ ││
│  │  │ givenName           │ TextType     │ String  │     │ ││
│  │  │ familyName          │ TextType     │ String  │     │ ││
│  │  │ birthDate           │ DateType     │ Date    │     │ ││
│  │  │ gender              │ GenderCode   │ Codelist│     │ ││
│  │  │ addresses           │ AddressType  │ VO:Nest │     │ ││
│  │  │ employments         │ EmploymentTy │ Entity  │     │ ││
│  │  └────────────────────────────────────────────────────┘ ││
│  └──────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

### Tab: Workflow

```
┌─── Workflow Tab ──────────────────────────────────────────┐
│                                                              │
│  Enable Workflow    ● Yes  ○ No                             │
│                                                              │
│  ┌── Workflow Designer ───────────────────────────────────┐ │
│  │                                                          │ │
│  │   [Draft] ──▶ [Submitted] ──▶ [Approved] ──▶ [Paid]   │ │
│  │      │                            │                     │ │
│  │      └──▶ [Cancelled]         [Rejected]                │ │
│  │                                                          │ │
│  │  [+ Add State]    [Auto-Layout]                         │ │
│  │                                                          │ │
│  │  Status Field: status  (PayRunStatus codelist)          │ │
│  │  Initial State: Draft                                    │ │
│  │  Terminal States: Paid, Cancelled, Rejected             │ │
│  │                                                          │ │
│  └──────────────────────────────────────────────────────────┘ │
│                                                              │
│  ┌── Data Guards ──────────────────────────────────────────┐ │
│  │  When transitioning to Approved:                        │ │
│  │  - Rule: total_amount >= 0                              │ │
│  │  - Error: "Cannot approve pay run with negative total" │ │
│  │  [+ Add Guard]                                          │ │
│  └──────────────────────────────────────────────────────────┘ │
│                                                              │
│  ┌── SLA Timers ───────────────────────────────────────────┐ │
│  │  Name         │ State      │ Type     │ Hours │ Target │ │
│  │ ├──────────────────────────────────────────────────────┤ │
│  │ │ approve_sla │ Submitted  │Escalation│ 24    │ Owner  │ │
│  │ │ ...         │            │          │       │        │ │
│  │ [+ Add Timer]                                           │ │
│  └──────────────────────────────────────────────────────────┘ │
│                                                              │
│  ┌── Approval Chains ──────────────────────────────────────┐ │
│  │  From: Submitted → To: Approved                         │ │
│  │  Step 1: Line Manager (required, timeout: 48h)          │ │
│  │  Step 2: Finance Director (required)                    │ │
│  │  [+ Add Step]  [+ Add Chain]                            │ │
│  └──────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

**Elements:**
- **Toggle:** Master on/off for workflow on this entity.
- **Visual designer:** Drag-and-drop state machine editor. Nodes are states
  (colour-coded: initial 🟢, terminal 🔴, intermediate 🟡). Edges are
  transitions. Click node → set properties. Click edge → set guard
  conditions or remove.
- **Data guards table:** Add guard with target transition, rule expression,
  error message. Rule syntax: basic expression language (DMN FEEL or simple
  comparison operators).
- **SLA timers table:** Name, trigger state, type (reminder / escalation),
  duration in hours, optional target state on expiry.
- **Approval chains:** Table of chain definitions. Each chain: from state, to
  state, ordered steps with role, required flag, timeout, auto-delegation.

### Tab: Search

```
┌─── Search Tab ─────────────────────────────────────────────┐
│                                                              │
│  Full-Text Search                                            │
│  ● Enable  ○ Disable                                        │
│                                                              │
│  Search Language: [english       ▼]                         │
│                                                              │
│  Include Columns:  [☑] givenName  [☑] familyName           │
│                    [☐] birthDate                            │
│                    [☑] email                                │
│                                                              │
│  Column Weights:                                            │
│  │ Field         │ Weight │                                 │
│  │ givenName     │ A      ▼│ ← highest                      │
│  │ familyName    │ A      ▼│                                 │
│  │ email         │ B      ▼│                                 │
│  │ notes         │ C      ▼│                                 │
│  └───────────────┴────────┘                                 │
│                                                              │
│  ── Vector Search (pgvector) ──                              │
│  ☑ Enable semantic search                                    │
│                                                              │
│  Embedding Columns: [☑] jobDescription                      │
│                    [☑] notes                                 │
│                                                              │
│  Embedding Dimensions: [1536               ]                │
│  (Matches OpenAI ada-002 default)                            │
└─────────────────────────────────────────────────────────────┘
```

### Tab: Hierarchy

```
┌─── Hierarchy Tab ──────────────────────────────────────────┐
│                                                              │
│  Tree / Hierarchy Support                                    │
│  ● Enable  ○ Disable                                        │
│                                                              │
│  Hierarchy Field: [reportsTo         ▼]                     │
│  (Self-referential FK column)                                │
│                                                              │
│  Include in Tree Response:                                   │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ Via Entity        │ Alias                │               ││
│  │ ├────────────────────────────────────────────────────────┤│
│  │ │ EmployeePosition │ assigned_positions   │  [×]         ││
│  │ │ Deployment       │ deployments          │  [×]         ││
│  │ │ [+ Add Related]  │                      │              ││
│  │ └────────────────────────────────────────────────────────┘│
│                                                              │
│  Generate Org Chart Page: ☑                                  │
└─────────────────────────────────────────────────────────────┘
```

### Tab: DTO

```
┌─── DTO Tab ────────────────────────────────────────────────┐
│                                                              │
│  Immutable Fields (excluded from Update)                    │
│  ☐ employeeId   (primary key — auto-excluded)               │
│  ☑ taxFileNumber (should not change after creation)         │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ + Add field...                                          │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                              │
│  List Response Exclusions                                   │
│  ☐ salary              (sensitive — exclude from list)      │
│  ☐ bankAccountNumber   (sensitive — exclude from list)      │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ + Add field...                                          │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                              │
│  Detail Response Expansions                                  │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ ☑ employments → Employee (expand related entity inline)│ │
│  │ ☐ department → Department                               │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                              │
│  Filter Fields (exposed as ?filter[field]=value)             │
│  ● Auto-discover  ○ Manual: [select fields...]             │
└─────────────────────────────────────────────────────────────┘
```

### Tab: UI Overrides

```
┌─── UI Tab (per-entity UI overrides) ──────────────────────┐
│                                                              │
│  Custom Components (overrides auto-generated UI)            │
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ Context    │ Schema Field    │ Component                ││
│  │ ├────────────────────────────────────────────────────────┤│
│  │ │ Detail    │ Person         │ @ui/ProfileHeader   [×] ││
│  │ │ List Cell │ Person         │ @ui/ProfileAvatar   [×] ││
│  │ │ Form      │ Address        │ @ui/AddressForm     [×] ││
│  │ │ Inline    │ Person         │ @ui/ProfileBadge    [×] ││
│  │ │ [+ Add Override]          │                          ││
│  │ └────────────────────────────────────────────────────────┘│
│                                                              │
│  Wizard Configuration                                       │
│  ● No wizard (single form)  ○ Multi-step wizard            │
│                                                              │
│  Wizard Steps:                                              │
│  ┌────────────────────────────────────────────────────────┐ ││
│  │ 1. Personal Details    ☰ [×]                           │ ││
│  │ 2. Employment Details  ☰ [×]                           │ ││
│  │ 3. Compliance & Docs   ☰ [×]                           │ ││
│  │ [+ Add Step]                                            │ ││
│  └────────────────────────────────────────────────────────┘ ││
└─────────────────────────────────────────────────────────────┘
```

**What maps to config:**
| UI Field | TOML equivalent | Notes |
|---|---|---|
| Classification | `[domains.{name}].force_entities` / `force_value_objects` / `exclude` | Three-way radio |
| Operations | `[domains.{name}.entity_config.{E}].operations` | Checkbox group |
| Role | `[domains.{name}.entity_config.{E}].role` | Root / Child |
| API Path Segment | `[domains.{name}.entity_config.{E}].path_segment` | Text, with auto-derive preview |
| Parent Entity | `[domains.{name}.entity_config.{E}].parent` | Dropdown of other entities |
| Auditable | `[domains.{name}.entity_config.{E}].auditable` | (inherits from domain) |
| Max Bulk Size | `[domains.{name}.entity_config.{E}].max_bulk_size` | Number |
| Workflow fields | `entity_config.{E}.workflow` | All workflow sub-fields |
| FTS columns/weights | `entity_config.{E}.search.fts_columns/weights` | Multi-select + weight dropdown |
| Embedding columns | `entity_config.{E}.search.embedding_columns` | Multi-select |
| Hierarchy field | `entity_config.{E}.hierarchy_field` | Dropdown of FK columns |
| Tree includes | `entity_config.{E}.tree_include` | Table |
| Immutable fields | `entity_config.{E}.dto.immutable_fields` | Multi-select |
| List exclude/include | `entity_config.{E}.dto.list_exclude/list_include` | Multi-select |
| Expand in response | `entity_config.{E}.dto.expand_in_response` | Multi-select |
| Filter fields | `entity_config.{E}.filter_fields` | Auto / Manual toggle + multi-select |
| UI overrides | `ui-overrides.toml` | Per-context component selector |
| Wizard config | `ui-domains.toml` | Toggle + step reorderable list |

---

## 6. Screen: Type Mappings

**Route:** `/projects/{project_id}/types`

**Purpose:** Configure how JSON Schema types map to database columns and Rust
types. This replaces `classifier.toml`.

```
┌─────────────────────────────────────────────────────────────┐
│  ◁ Project Overview   │   Type Mappings                    │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─── Tabs ────────────────────────────────────────────────┐│
│  │ [Primitives] [Composites] [Ranges] [Codelists] [Wrappers]││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌─── Primitives Tab ──────────────────────────────────────┐│
│  │                                                          ││
│  │  These types are mapped to native database columns       ││
│  │                                                          ││
│  │  ┌──────────────────────────────────────────────────────┐││
│  │  │ Schema Type      │ Postgres Type   │ Rust Type      │││
│  │  ├──────────────────────────────────────────────────────┤││
│  │  │ CodeType         │ TEXT            │ String         │││
│  │  │ TextType         │ TEXT            │ String         │││
│  │  │ IndicatorType    │ BOOLEAN         │ bool           │││
│  │  │ DateType         │ DATE            │ chrono::Naive..│││
│  │  │ DateTimeType     │ TIMESTAMPTZ     │ chrono::Date.. │││
│  │  │ NumberType       │ DOUBLE PRECISION│ f64            │││
│  │  │ IntegerType      │ BIGINT          │ i64            │││
│  │  │ PercentType      │ NUMERIC(7,4)    │ Decimal        │││
│  │  │ YearType         │ SMALLINT        │ i16            │││
│  │  │ [+ Add Mapping]  │                │                │││
│  │  └──────────────────────────────────────────────────────┘││
│  │                                                          ││
│  │  Array Types (primitives stored as Postgres arrays)      ││
│  │  ┌──────────────────────────────────────────────────────┐││
│  │  │ StringTypeArray  │ TEXT[]          │ Vec<String>    │││
│  │  └──────────────────────────────────────────────────────┘││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌─── Composites Tab ──────────────────────────────────────┐│
│  │                                                          ││
│  │  These types expand into multiple columns                ││
│  │                                                          ││
│  │  ┌──────────────────────────────────────────────────────┐││
│  │  │ Schema        │ Column               │ Type         │││
│  │  ├──────────────────────────────────────────────────────┤││
│  │  │ AmountType    │ value                │ NUMERIC(19,4)│││
│  │  │               │ currency             │ TEXT (FK)    │││
│  │  │               │                      │ codelist.cur.│││
│  │  ├──────────────────────────────────────────────────────┤││
│  │  │ GeoType       │ location             │ GEOMETRY     │││
│  │  │               │ location_name        │ TEXT         │││
│  │  │ [+ Add Composite]                                     │││
│  │  └──────────────────────────────────────────────────────┘││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌─── Ranges Tab ──────────────────────────────────────────┐│
│  │                                                          ││
│  │  Paired start/end fields merge into range columns        ││
│  │                                                          ││
│  │  ┌──────────────────────────────────────────────────────┐││
│  │  │ Schema         │ Start Field │ End Field │ Range Type│││
│  │  ├──────────────────────────────────────────────────────┤││
│  │  │ AssignmentLife.│ fromDate    │ toDate     │ DATERANGE│││
│  │  │ EffectiveTime. │ effectiveFr │ effectiveTo│TSTZRANGE │││
│  │  │ [+ Add Range]                                        │││
│  │  └──────────────────────────────────────────────────────┘││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌─── Codelists Tab ───────────────────────────────────────┐│
│  │                                                          ││
│  │  Inline Enum Threshold: [20                   ]         ││
│  │  (Values ≤ threshold use CHECK constraint instead of    ││
│  │   separate lookup table)                                 ││
│  │                                                          ││
│  │  Force CHECK Constraint (override):                     ││
│  │  ┌──────────────────────────────────────────────────────┐││
│  │  │ ☑ GenderCodeList.json                                │││
│  │  │ ☑ ConfirmationCodeList.json                          │││
│  │  │ [+ Add codelist...]                                   │││
│  │  └──────────────────────────────────────────────────────┘││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌─── Wrappers Tab ────────────────────────────────────────┐│
│  │                                                          ││
│  │  Structured Wrappers (stored as JSONB with domain type)  ││
│  │  ┌──────────────────────────────────────────────────────┐││
│  │  │ Schema           │ Postgres  │ Rust Type            │││
│  │  ├──────────────────────────────────────────────────────┤││
│  │  │ IdentifierType   │ JSONB     │ IdentifierType       │││
│  │  │ [+ Add Structured] │          │                      │││
│  │  └──────────────────────────────────────────────────────┘││
│  │                                                          ││
│  │  Media Wrappers (URL + MIME type columns)                ││
│  │  ┌──────────────────────────────────────────────────────┐││
│  │  │ Schema            │ URL Col │ MIME Col │ Allowed    │││
│  │  ├──────────────────────────────────────────────────────┤││
│  │  │ MediaReferenceType│ _url    │ _mime_typ│ image/*    │││
│  │  │                   │         │          │ app/pdf    │││
│  │  │ [+ Add Media]                                        │││
│  │  └──────────────────────────────────────────────────────┘││
│  │                                                          ││
│  │  Required PG Extensions                                  ││
│  │  ┌──────────────────────────────────────────────────────┐││
│  │  │ postgis [[+ Add Extension]]                          │││
│  │  └──────────────────────────────────────────────────────┘││
│  └──────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

**What maps to config:**
| UI Field | TOML equivalent | Notes |
|---|---|---|
| Primitive type mappings | `[primitive_wrappers]` | Table with schema→PG→Rust |
| Array type mappings | `[array_wrappers]` | Same structure |
| Composite wrappers | `[[composite_wrappers]]` | Multi-column expansion table |
| Range composites | `[[composite_ranges]]` | Start/end→single range |
| Structured wrappers | `[structured_wrappers]` | JSONB domain types |
| Media wrappers | `[media_wrappers]` | URL + MIME + accept types |
| Inline enum threshold | `inline_enum_threshold` | Number input |
| Codelist-as-check overrides | `[codelist_as_check].schemas` | Multi-select from discovered codelist schema files |
| PG extensions | `required_extensions` | Tag input |

---

## 7. Screen: UI Customization

**Route:** `/projects/{project_id}/ui`

**Purpose:** Global UI configuration — component library, layout density, brand
colours. Manages the `ui-overrides.toml` and UI generation parameters.

```
┌─────────────────────────────────────────────────────────────┐
│  ◁ Project Overview   │   UI Customization                 │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ Brand & Styling                                           ││
│  │                                                          ││
│  │  Primary Colour    ┌────────────────────────────────┐    ││
│  │                    │ #085041      [Picker]          │    ││
│  │                    └────────────────────────────────┘    ││
│  │                                                          ││
│  │  Accent Colour     ┌────────────────────────────────┐    ││
│  │                    │ #1D9E75      [Picker]          │    ││
│  │                    └────────────────────────────────┘    ││
│  │                                                          ││
│  │  Layout Density    ○ Compact  ● Comfortable  ○ Spacious ││
│  │                                                          ││
│  │  Max Content Width ┌────────────────────────────────┐    ││
│  │                    │ 1200px                          │    ││
│  │                    └────────────────────────────────┘    ││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ Component Overrides (per-schema, per-context)            ││
│  │                                                          ││
│  │ ┌──────────────────────────────────────────────────────┐ ││
│  │ │ Search overrides...                          [+Add] │ ││
│  │ ├──────────────────────────────────────────────────────┤ ││
│  │ │ Schema          │ Detail      │ List Cell │ Form   │ ││
│  │ ├──────────────────────────────────────────────────────┤ ││
│  │ │ PersonProfile   │@ui/Profile  │@ui/Profile│@ui/Pro │ ││
│  │ │ Address         │@ui/Address  │ —         │@ui/Addr│ ││
│  │ │ Phone           │ —           │ —         │@ui/Phon│ ││
│  │ └──────────────────────────────────────────────────────┘ ││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ IFML Generation (if IFML files exist in repo)            ││
│  │                                                          ││
│  │  Generate IFML routes:  ☑                                ││
│  │                                                          ││
│  │  Target Frameworks:                                      ││
│  │  ☑ SvelteKit   ☐ Next.js   ☐ Vue/Nuxt                   ││
│  │  ☐ Flutter     ☐ SwiftUI                                 ││
│  │                                                          ││
│  │  IFML files:  app.ifml, dashboard.ifml  [+Add .ifml]   ││
│  └──────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

**What maps to config:**
| UI Field | TOML equivalent | Notes |
|---|---|---|
| Component overrides | `ui-overrides.toml` | Table per entity × context |
| IFML frameworks | `profiles.toml → [profile].ifml.frameworks` | Checkbox group |
| IFML files | CLI arg `--ifml-files` | File picker from repo |

---

## 8. Screen: Generation Profile

**Route:** `/projects/{project_id}/profile`

**Purpose:** Select which code to generate, feature flags, and output targets.
Replaces `profiles.toml` and the `--profile` / `--variant` CLI flags.

```
┌─────────────────────────────────────────────────────────────┐
│  ◁ Project Overview   │   Generation Profile               │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ Profile Template                                          ││
│  │                                                          ││
│  │  ● Full Stack (API + UI + CLI)   ○ API Only              ││
│  │  ○ UI Only                        ○ CLI Only             ││
│  │  ○ Custom Profile                                        ││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌─── API Generators ──────────────────────────────────────┐│
│  │                                                          ││
│  │  ☑ Database (DDL migrations)                            ││
│  │  ☑ SeaORM Entity Models                                 ││
│  │  ☑ Codelist Tables                                      ││
│  │  ☑ DTOs (Create/Response/Update/Summary)                ││
│  │  ☑ Repository Layer                                     ││
│  │  ☑ CQRS (Commands, Queries, Events)                     ││
│  │  ☑ API Handlers (REST endpoints)                        ││
│  │  ☑ Router                                                ││
│  │  ☑ OpenAPI Specification                                ││
│  │  ☑ Integration Tests                                    ││
│  │  ☑ Webhook Dispatch + Endpoints                         ││
│  │  ☑ gRPC Service (proto + tonic)                         ││
│  │  ☑ Scaffold (main.rs, Cargo.toml, middleware)           ││
│  │  ☐ Basejump Setup (multi-tenancy)                       ││
│  │  ☐ PGMQ Setup (message queue)                           ││
│  │                                                          ││
│  │  Output Directory: ┌──────────────────────────────────┐ ││
│  │                    │ generated/api/                    │ ││
│  │                    └──────────────────────────────────┘ ││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌─── UI Generators ───────────────────────────────────────┐│
│  │                                                          ││
│  │  ☑ Pages (List, Detail, Edit, Create)                   ││
│  │  ☑ Form Components                                      ││
│  │  ☑ Svelte Stores                                        ││
│  │  ☑ API Client Modules                                   ││
│  │  ☑ E2E Playwright Tests                                 ││
│  │  ☑ UI Descriptors                                       ││
│  │  ☑ Shell/App Scaffold                                   ││
│  │  ☑ Org Chart                                            ││
│  │  ☐ IFML Routes (if IFML files present)                  ││
│  │  ☐ IFML Navigation                                      ││
│  │                                                          ││
│  │  Output Directory: ┌──────────────────────────────────┐ ││
│  │                    │ generated/ui/                     │ ││
│  │                    └──────────────────────────────────┘ ││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌─── Feature Flags ───────────────────────────────────────┐│
│  │                                                          ││
│  │  ☑ Authentication & Authorization                       ││
│  │  ☑ Pagination                                           ││
│  │  ● Strict  ○ Balanced  ○ Minimal  │ Validation          ││
│  │  ☑ Multi-tenancy  (enterprise feature)                   ││
│  │  ☑ Row-Level Security  (enterprise feature)              ││
│  │  ☑ Audit Trail  (enterprise feature)                     ││
│  │  ☐ Offline Mode                                          ││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌─── Post-Generation Scripts ─────────────────────────────┐│
│  │                                                          ││
│  │  ┌──────────────────────────────────────────────────────┐││
│  │  │ cargo fmt --all                              [×]    │││
│  │  │ echo "Generation complete"                   [×]    │││
│  │  │ [+ Add Script]                                       │││
│  │  └──────────────────────────────────────────────────────┘││
│  │  (Commands run after generation completes. Non-zero      ││
│  │   exit aborts the commit.)                               ││
│  └──────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

**What maps to config:**
| UI Field | TOML equivalent | Notes |
|---|---|---|
| Profile template | `profiles.toml → [profile].{section}.generators` | Preset selection or custom |
| Individual generators | Same | Checkbox list, grouped by section |
| Output directories | `profiles.toml → [profile].{section}.output` | Per-section path |
| Feature flags | `profiles.toml → [profile].features` | Toggle/category group |
| Post-gen scripts | `profiles.toml → [profile].{section}.scripts.post_gen` | Ordered list, with reorder handle |

---

## 9. Screen: Integrations & Extension Points

**Route:** `/projects/{project_id}/integrations`

**Purpose:** Configure integration extension points — the "plugin system" for
connecting generated apps to external services.

```
┌─────────────────────────────────────────────────────────────┐
│  ◁ Project Overview   │   Integrations                     │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─── Extension Points ────────────────────────────────────┐│
│  │                                                          ││
│  │  ┌──────────────────────────────────────────────────────┐││
│  │  │ Available Extension Points (4)                       ││
│  │  ├──────────────────────────────────────────────────────┤││
│  │  │ 🔌 PayrollFiling        │ Direction: Push           │││
│  │  │   Description: Submit pay run data to external       │││
│  │  │   payroll/tax filing service. Cardinality: Exclusive │││
│  │  │   Entities: PayRun  │  [➕ Connect Integration]     │││
│  │  ├──────────────────────────────────────────────────────┤││
│  │  │ 🔌 BankPaymentPush    │ Direction: Push             │││
│  │  │   Description: Push approved payments to bank feed   │││
│  │  │   Cardinality: Multiple                              │││
│  │  │   Entities: Payment  │  [➕ Connect Integration]     │││
│  │  ├──────────────────────────────────────────────────────┤││
│  │  │ 🔌 EmployeeSync       │ Direction: Bidirectional    │││
│  │  │   Description: Sync employee records with HRIS/      │││
│  │  │   payroll provider. Cardinality: Exclusive           │││
│  │  │   Entities: Employee  │  [➕ Connect Integration]    │││
│  │  ├──────────────────────────────────────────────────────┤││
│  │  │ 🔌 AccountingSync     │ Direction: Push             │││
│  │  │   Description: Push journal entries to accounting     │││
│  │  │   system. Cardinality: Exclusive                     │││
│  │  │   Entities: Journal   │  [➕ Connect Integration]    │││
│  │  └──────────────────────────────────────────────────────┘││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌─── Connected Integrations ──────────────────────────────┐│
│  │                                                          ││
│  │  ┌──────────────────────────────────────────────────────┐││
│  │  │ Xero Accounting Sync               ● Active    [⚙] │││
│  │  │ ├ Connected to: AccountingSync                      │││
│  │  │ │ Last sync: 5 min ago · ✅ 128 journals pushed     │││
│  │  │ │ Config: client_id, client_secret, tenant_id       │││
│  │  │ └──────────────────────────────────────────────────┘││
│  │  ├──────────────────────────────────────────────────────┤││
│  │  │ IRD Payday Filing                  ● Active    [⚙] │││
│  │  │ ├ Connected to: PayrollFiling                       │││
│  │  │ │ Last filing: 1 hour ago · ✅ filed 42 employees   │││
│  │  │ │ Config: ird_number, agency_id, gateway_url        │││
│  │  │ └──────────────────────────────────────────────────┘││
│  │  └──────────────────────────────────────────────────────┘││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ Marketplace                                   [Browse]  ││
│  │ ┌──────────────────────────────────────────────────────┐ ││
│  │ │ 🔍 Search integrations...                            │ ││
│  │ ├──────────────────────────────────────────────────────┤ ││
│  │ │ Xero Accounting Sync  │ HR + Payroll   │ Install   │ ││
│  │ │ IRD Payday Filing     │ Tax + Gov      │ Install   │ ││
│  │ │ Akahu Bank Feeds      │ Banking        │ Install   │ ││
│  │ │ Stripe Payments       │ Payments       │ Coming... │ ││
│  │ │ SendGrid Email        │ Notifications  │ Coming... │ ││
│  │ └──────────────────────────────────────────────────────┘ ││
│  └──────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

**What maps to config:**
| UI Field | TOML equivalent | Notes |
|---|---|---|
| Extension points | `extension-points.toml` | Auto-discovered or manually defined |
| Connected integrations | Integration manifests | From integration SDK |
| Marketplace | (external) | Future feature |

---

## 10. Screen: Deployment

**Route:** `/projects/{project_id}/deploy`

**Purpose:** Configure how generated code is deployed. (Future)

```
┌─────────────────────────────────────────────────────────────┐
│  ◁ Project Overview   │   Deployment                       │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ Environments                                              ││
│  │                                                          ││
│  │  ┌────────┐  ┌────────┐  ┌────────┐                     ││
│  │  │ Dev    │  │ Staging│  │ Prod   │                     ││
│  │  │ 🟢 Live│  │ 🟡 Off │  │ 🔴 Off│                     ││
│  │  └────────┘  └────────┘  └────────┘                     ││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ Auto-Deploy                                               ││
│  │                                                          ││
│  │  ● Deploy generation branch to dev automatically         ││
│  │  ○ Manual deploy                                         ││
│  │                                                          ││
│  │  Post-deploy:  ☑ Run migrations  ☑ Smoke test           ││
│  │                ☐ Notify Slack                            ││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ Custom Domain                                             ││
│  │                                                          ││
│  │  ┌──────────────────────────────────────────────────────┐││
│  │  │ app.mycompany.com                                    │││
│  │  └──────────────────────────────────────────────────────┘││
│  │  SSL: ✅ Auto-managed (Let's Encrypt)                    ││
│  └──────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

---

## 11. Naming Rules Screen

**Route:** `/projects/{project_id}/entities/naming-rules`

**Purpose:** Configure name-based classification overrides. These apply before
the auto-classifier scoring and let users force certain patterns to be treated
as Value Objects.

```
┌─────────────────────────────────────────────────────────────┐
│  ◁ Entities   │   Naming Rules                             │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Naming rules let you force schemas matching certain         │
│  naming patterns to be treated as Value Objects.            │
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ Pattern      │ Score │ Type   │ Affected Entities       ││
│  ├──────────────────────────────────────────────────────────┤│
│  │ Inclusion    │ +5    │ Hard   │ BenefitInclusion,       ││
│  │              │       │        │ DeductionInclusion      ││
│  ├──────────────────────────────────────────────────────────┤│
│  │ Report       │ +3    │ Soft   │ PayrollReport,          ││
│  │              │       │        │ ComplianceReport        ││
│  ├──────────────────────────────────────────────────────────┤│
│  │ Notification │ +3    │ Soft   │ LeaveNotification,      ││
│  │              │       │        │ PaySlipNotification     ││
│  ├──────────────────────────────────────────────────────────┤│
│  │ Vendor       │ +3    │ Soft   │ VendorType              ││
│  ├──────────────────────────────────────────────────────────┤│
│  │ Message      │ +3    │ Soft   │ PayrollMessage          ││
│  ├──────────────────────────────────────────────────────────┤│
│  │ [+ Add Rule] │       │        │                          ││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  How it works:                                                │
│  Hard rules force Value Object regardless of score.          │
│  Soft rules add their score to the VO total, then the        │
│  auto-classifier makes the final Entity/VO decision.         │
└─────────────────────────────────────────────────────────────┘
```

**What maps to config:**
| UI Field | TOML equivalent | Notes |
|---|---|---|
| Pattern | `[naming_rules].{key}` (key is the pattern) | Text input |
| Score | `[naming_rules].{key}.score` | Number input |
| Type | `[naming_rules].{key}.type` | Radio: soft / hard |

---

## 12. Classification Scoring Screen

**Route:** `/projects/{project_id}/entities/scoring`

**Purpose:** Expose the auto-classifier's scoring heuristics so advanced users
can tune them. These are currently hardcoded in `scoring.rs`.

```
┌─────────────────────────────────────────────────────────────┐
│  ◁ Entities   │   Classification Scoring                   │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  The auto-classifier evaluates each schema using signals     │
│  below. Net score ≥ threshold → Entity. < threshold → VO.   │
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ Entity Threshold                                          ││
│  │                                                          ││
│  │  Net Score ≥    [4              ]  → Entity             ││
│  │  Net Score <    [4              ]  → Value Object       ││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌─── Signal Weights ──────────────────────────────────────┐│
│  │                                                          ││
│  │  Signal                    │ Entity  │ VO     │          ││
│  │  ├───────────────────────────────────────────────────────┤│
│  │  │ Referenced by ≥3 schemas │ +3     │ —      │         ││
│  │  │ Referenced by 1-2       │ +1     │ —      │         ││
│  │  │ Referenced by 0         │ —      │ +2     │         ││
│  │  │ Has ≥8 fields           │ +2     │ —      │         ││
│  │  │ Has ≤3 fields           │ —      │ +2     │         ││
│  │  │ Extends noun type       │ +2     │ —      │         ││
│  │  │ Uses allOf              │ +2     │ —      │         ││
│  │  │ Is primitive wrapper    │ —      │ +5     │ (max)   ││
│  │  │ Is codelist/enum        │ —      │ +5     │ (max)   ││
│  │  ├──────────────────────────────────────────────────────┤│
│  │  │ [Reset to Defaults]                [Apply Changes]   ││
│  │  └──────────────────────────────────────────────────────┘│
│  └──────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

**Note:** These values are currently hardcoded in Rust. The engine must be
modified to accept them from config before this screen can work.

---

## 13. Cross-Cutting: Global Header & Navigation

Every config screen shares this persistent shell:

```
┌─────────────────────────────────────────────────────────────┐
│  ◁ Projects  [Project Name ▼]    [Generate ▼]  [⚙️] [👤]  │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌────────┐ ┌─────────────────────────────────────────────┐ │
│  │  📊    │ │   Screen Title                               │ │
│  │ Overview││                                             │ │
│  │        ││   (content area)                              │ │
│  │  📂    ││                                             │ │
│  │ Source ││                                             │ │
│  │        ││                                             │ │
│  │  🏛️    ││                                             │ │
│  │ Domains││                                             │ │
│  │        ││                                             │ │
│  │  📇    ││                                             │ │
│  │ Entities││                                             │ │
│  │        ││                                             │ │
│  │  🔤    ││                                             │ │
│  │ Types  ││                                             │ │
│  │        ││                                             │ │
│  │  🎨    ││                                             │ │
│  │ UI     ││                                             │ │
│  │        ││                                             │ │
│  │  ⚡     ││                                             │ │
│  │ Profile││                                             │ │
│  │        ││                                             │ │
│  │  🔌    ││                                             │ │
│  │ Integr.││                                             │ │
│  │        ││                                             │ │
│  │  🚀    ││                                             │ │
│  │ Deploy ││                                             │ │
│  └────────┘ └─────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

**Left sidebar:** Persistent navigation. Active screen highlighted. Unseen
changes shown as a dot badge.

**Top bar:**
- **Breadcrumb:** Projects → Project Name
- **Project selector:** Dropdown to switch projects
- **Generate button:** Primary action. Dropdown: "Generate (commit to branch)",
  "Generate (preview only)", "Regenerate all"
- **Settings gear:** Project settings (name, description, delete)
- **User menu:** Profile, org settings, billing, sign out

---

## 14. Configuration State Model

Every screen should track its config state and display status:

```
State          │ Badge   │ Meaning
───────────────┼─────────┼────────────────────────────────────
Not configured │ 🔴     │ Required — user must take action
Auto-detected  │ 🟡     │ Platform inferred a value, needs review
Configured     │ 🟢     │ User has reviewed and accepted
Modified       │ 🟠     │ Unsaved changes pending
Error          │ ❌     │ Validation error (cycle, missing dep)
```

Each screen's save button persists config. The platform stores config as the
equivalent TOML/JSON in its database. When generation runs, it materialises
these into the `domains.toml`, `classifier.toml`, `profiles.toml`, etc. before
invoking the engine.

---

## 15. Configuration Inheritance Model

```
Global Defaults
  └── Domain Defaults (override global)
       └── Entity Config (override domain)
            └── Field/Property (not yet configurable per-field)
```

At each level, any field left as "inherit" uses the parent's value. The UI
shows the resolved effective value with a link to the source.

---

## 16. Architecture: How Config Becomes TOML

The Studio stores configuration as structured JSON in its database (one
document per project). When the user clicks "Generate":

```
Studio DB (JSON config)
       │
       ▼
Config Materialiser (Rust service)
  ├── Renders domains.toml from project config
  ├── Renders classifier.toml from type mappings
  ├── Renders profiles.toml from profile settings
  ├── Renders ui-overrides.toml from UI overrides
  └── Writes to working directory
       │
       ▼
codegraph engine (Rust library)
  ├── Reads TOML files as before
  ├── Runs pipeline: ingest → classify → generate
  └── Writes generated code to output directory
       │
       ▼
Git Writer
  ├── Clones repo (if not already)
  ├── Copies generated output to target paths
  ├── git add, git commit
  └── git push to output branch
```

The TOML files are an implementation detail that users never touch. They exist
because the engine consumes them. The platform generates them from the visual
config.

---

## 17. Future Screens (Not in MVP)

- **API Playground:** Interactive OpenAPI explorer for the generated API
- **Data Browser:** Browse data in generated app's DB (platform-hosted only)
- **Audit Log:** Full change history across all config operations
- **Team Management:** Invite users, assign roles to config screens
- **Billing & Usage:** Plan limits, API call counts, storage
- **AI Assistant:** Natural language → schema / IFML / config changes
- **Schema Diff:** Visual diff between schema versions with impact analysis
- **Migration Preview:** See what DB migration will be generated before running
