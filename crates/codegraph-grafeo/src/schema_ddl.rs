/// Returns GQL DDL statements for node types and edge types.
/// These match the `codegraph-core` shared types (derived from `db/schema.hx`).
pub fn ddl_statements() -> Vec<&'static str> {
    let mut stmts = Vec::new();
    stmts.extend(node_type_ddl());
    stmts.extend(edge_type_ddl());
    stmts
}

/// Returns property names that should be indexed via `GrafeoDB::create_property_index()`.
pub fn indexed_properties() -> Vec<&'static str> {
    vec![
        "title",
        "schema_id",
        "_schema_title",
        "name",
        "_codelist_name",
    ]
}

fn node_type_ddl() -> Vec<&'static str> {
    vec![
        // SchemaNode — 21 fields from codegraph-core/src/types/schema.rs
        "CREATE NODE TYPE IF NOT EXISTS Schema (
            schema_id STRING NOT NULL,
            title STRING NOT NULL,
            description STRING,
            schema_type STRING NOT NULL,
            classification STRING NOT NULL,
            pg_type STRING NOT NULL,
            rust_type STRING NOT NULL,
            sea_orm_type STRING NOT NULL,
            domain STRING,
            rel_path STRING NOT NULL,
            rust_type_name STRING NOT NULL,
            pg_table_name STRING NOT NULL,
            api_path_segment STRING NOT NULL,
            parent_schema STRING,
            is_entity BOOLEAN NOT NULL,
            is_codelist BOOLEAN NOT NULL,
            is_primitive_wrapper BOOLEAN NOT NULL,
            has_all_of BOOLEAN NOT NULL,
            has_one_of BOOLEAN NOT NULL,
            has_any_of BOOLEAN NOT NULL,
            has_definitions BOOLEAN NOT NULL
        )",
        // PropertyNode — 16 fields + _schema_title denormalized
        "CREATE NODE TYPE IF NOT EXISTS Property (
            name STRING NOT NULL,
            prop_type STRING NOT NULL,
            description STRING,
            format STRING,
            is_required BOOLEAN NOT NULL,
            is_nullable BOOLEAN NOT NULL,
            is_array BOOLEAN NOT NULL,
            pattern STRING,
            pg_column_name STRING NOT NULL,
            pg_column_type STRING NOT NULL,
            rust_field_name STRING NOT NULL,
            rust_field_type STRING NOT NULL,
            sea_orm_type STRING NOT NULL,
            render_strategy STRING NOT NULL,
            ref_target STRING,
            classification STRING,
            _schema_title STRING NOT NULL
        )",
        // CodeList — 5 fields
        "CREATE NODE TYPE IF NOT EXISTS CodeList (
            name STRING NOT NULL,
            description STRING,
            pg_table_name STRING NOT NULL,
            render_as STRING NOT NULL,
            check_expression STRING
        )",
        // EnumValue — 3 fields + _codelist_name denormalized
        "CREATE NODE TYPE IF NOT EXISTS EnumValue (
            value STRING NOT NULL,
            display_name STRING,
            sort_order INTEGER NOT NULL,
            _codelist_name STRING NOT NULL
        )",
        // CompositeColumn — 5 fields
        "CREATE NODE TYPE IF NOT EXISTS CompositeColumn (
            suffix STRING NOT NULL,
            pg_type STRING NOT NULL,
            rust_type STRING NOT NULL,
            sea_orm_type STRING NOT NULL,
            fk_target STRING
        )",
        // CompositeRange — 6 fields
        "CREATE NODE TYPE IF NOT EXISTS CompositeRange (
            pg_column_name STRING NOT NULL,
            pg_type STRING NOT NULL,
            rust_type STRING NOT NULL,
            start_field STRING NOT NULL,
            end_field STRING NOT NULL,
            open_end BOOLEAN NOT NULL
        )",
        // Extension — 1 field
        "CREATE NODE TYPE IF NOT EXISTS Extension (
            name STRING NOT NULL
        )",
        // Domain — 1 field
        "CREATE NODE TYPE IF NOT EXISTS Domain (
            name STRING NOT NULL
        )",
        // ViewContainer — IFML
        "CREATE NODE TYPE IF NOT EXISTS ViewContainer (
            name STRING NOT NULL,
            label STRING,
            is_xor BOOLEAN NOT NULL DEFAULT false,
            is_default BOOLEAN NOT NULL DEFAULT false,
            is_landmark BOOLEAN NOT NULL DEFAULT false,
            is_modal BOOLEAN NOT NULL DEFAULT false,
            domain STRING
        )",
        // ViewComponent — IFML
        "CREATE NODE TYPE IF NOT EXISTS ViewComponent (
            name STRING NOT NULL,
            component_type STRING NOT NULL,
            mode STRING,
            entity STRING,
            fields STRING,
            filter STRING,
            domain STRING
        )",
        // Event — IFML
        "CREATE NODE TYPE IF NOT EXISTS Event (
            name STRING NOT NULL,
            event_type STRING NOT NULL,
            params STRING,
            domain STRING
        )",
        // Action — IFML
        "CREATE NODE TYPE IF NOT EXISTS ActionNode (
            name STRING NOT NULL,
            domain STRING
        )",
        // ParameterDefinition — IFML
        "CREATE NODE TYPE IF NOT EXISTS ParameterDefinition (
            name STRING NOT NULL,
            direction STRING NOT NULL,
            type_ref STRING NOT NULL,
            domain STRING
        )",
        // DataBinding — IFML
        "CREATE NODE TYPE IF NOT EXISTS DataBinding (
            conditional_expression STRING,
            expression_language STRING NOT NULL DEFAULT 'ifml',
            domain STRING
        )",
        // ModuleDefinition — IFML
        "CREATE NODE TYPE IF NOT EXISTS ModuleDefinition (
            name STRING NOT NULL,
            domain STRING
        )",
    ]
}

fn edge_type_ddl() -> Vec<&'static str> {
    vec![
        "CREATE EDGE TYPE IF NOT EXISTS HasProperty (sort_order INTEGER)",
        "CREATE EDGE TYPE IF NOT EXISTS ReferencesSchema (ref_path STRING, resolved_classification STRING)",
        "CREATE EDGE TYPE IF NOT EXISTS ExtendsSchema (composition_type STRING)",
        "CREATE EDGE TYPE IF NOT EXISTS DependsOn (dependency_type STRING)",
        "CREATE EDGE TYPE IF NOT EXISTS HasEnumValue",
        "CREATE EDGE TYPE IF NOT EXISTS UsesCodeList (render_as STRING)",
        "CREATE EDGE TYPE IF NOT EXISTS ExpandsTo (sort_order INTEGER)",
        "CREATE EDGE TYPE IF NOT EXISTS CollapsesTo",
        "CREATE EDGE TYPE IF NOT EXISTS ConsumesField (role STRING)",
        "CREATE EDGE TYPE IF NOT EXISTS ItemsOf",
        "CREATE EDGE TYPE IF NOT EXISTS RequiresExtension",
        "CREATE EDGE TYPE IF NOT EXISTS InDomain",
        "CREATE EDGE TYPE IF NOT EXISTS DomainDepends (dependency_type STRING)",
        // IFML edge types
        "CREATE EDGE TYPE IF NOT EXISTS ContainsViewContainer (sort_order INTEGER)",
        "CREATE EDGE TYPE IF NOT EXISTS ContainsViewComponent (sort_order INTEGER)",
        "CREATE EDGE TYPE IF NOT EXISTS HasEvent",
        "CREATE EDGE TYPE IF NOT EXISTS NavigationFlow (target_param_binding STRING)",
        "CREATE EDGE TYPE IF NOT EXISTS DataFlow (source_param STRING, target_param STRING)",
        "CREATE EDGE TYPE IF NOT EXISTS HasParameter (direction STRING)",
        "CREATE EDGE TYPE IF NOT EXISTS ParameterBindingGroup",
        "CREATE EDGE TYPE IF NOT EXISTS ParameterBinding",
        "CREATE EDGE TYPE IF NOT EXISTS HasDataBinding",
        "CREATE EDGE TYPE IF NOT EXISTS BindsToEntity",
        "CREATE EDGE TYPE IF NOT EXISTS BindsToProperty (role STRING)",
        "CREATE EDGE TYPE IF NOT EXISTS TriggersAction",
        "CREATE EDGE TYPE IF NOT EXISTS ActionEvent (outcome STRING)",
        "CREATE EDGE TYPE IF NOT EXISTS HasModuleDefinition",
        "CREATE EDGE TYPE IF NOT EXISTS HasViewComponentPart (role STRING)",
        "CREATE EDGE TYPE IF NOT EXISTS HasConditionalExpr",
    ]
}
