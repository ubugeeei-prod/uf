#![deny(missing_docs)]
//! Native type-safe ORM contracts for `@uniflowed/orm`.

use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Inline column list for schema definitions.
pub type ColumnList = SmallVec<[Column; 16]>;

/// Inline relation list for schema definitions.
pub type RelationList = SmallVec<[Relation; 8]>;

/// Inline predicate list for query planning.
pub type PredicateList = SmallVec<[Predicate; 8]>;

/// Inline migration operation list.
pub type MigrationOps = SmallVec<[MigrationOp; 8]>;

/// Type-safe ORM schema table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    /// Table name.
    pub name: CompactString,
    /// Branded Flow model type exported for rows.
    pub row_type: CompactString,
    /// Table columns.
    pub columns: ColumnList,
    /// Declared relations.
    pub relations: RelationList,
}

impl Table {
    /// Create an empty table descriptor.
    pub fn new(name: &str, row_type: &str) -> Self {
        Self {
            name: name.to_compact_string(),
            row_type: row_type.to_compact_string(),
            columns: SmallVec::new(),
            relations: SmallVec::new(),
        }
    }

    /// Add a column.
    pub fn column(mut self, column: Column) -> Self {
        self.columns.push(column);
        self
    }

    /// Add a relation.
    pub fn relation(mut self, relation: Relation) -> Self {
        self.relations.push(relation);
        self
    }
}

/// ORM column descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    /// Column name.
    pub name: CompactString,
    /// Column scalar type.
    pub ty: ColumnType,
    /// Whether the column is nullable.
    pub nullable: bool,
    /// Whether the column participates in the primary key.
    pub primary_key: bool,
}

impl Column {
    /// Create a required non-primary-key column.
    pub fn new(name: &str, ty: ColumnType) -> Self {
        Self {
            name: name.to_compact_string(),
            ty,
            nullable: false,
            primary_key: false,
        }
    }

    /// Mark the column nullable.
    pub fn nullable(mut self) -> Self {
        self.nullable = true;
        self
    }

    /// Mark the column as part of the primary key.
    pub fn primary_key(mut self) -> Self {
        self.primary_key = true;
        self
    }
}

/// ORM scalar column type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ColumnType {
    /// UTF-8 text.
    Text,
    /// Signed 64-bit integer.
    Int64,
    /// Boolean value.
    Boolean,
    /// JSON document.
    Json,
    /// Timestamp value.
    Timestamp,
    /// UUID text or native UUID type.
    Uuid,
}

/// ORM relation descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relation {
    /// Relation name.
    pub name: CompactString,
    /// Target table name.
    pub target: CompactString,
    /// Relation cardinality.
    pub cardinality: RelationCardinality,
}

impl Relation {
    /// Create a relation descriptor.
    pub fn new(name: &str, target: &str, cardinality: RelationCardinality) -> Self {
        Self {
            name: name.to_compact_string(),
            target: target.to_compact_string(),
            cardinality,
        }
    }
}

/// Relation cardinality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RelationCardinality {
    /// One source row to one target row.
    One,
    /// One source row to many target rows.
    Many,
}

/// Native query plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryPlan {
    /// Source table.
    pub table: CompactString,
    /// Predicates applied before projection.
    pub predicates: PredicateList,
    /// Projection column names.
    pub select: SmallVec<[CompactString; 8]>,
    /// Maximum row count.
    pub limit: Option<u32>,
}

impl QueryPlan {
    /// Create a query plan for a table.
    pub fn from(table: &Table) -> Self {
        Self {
            table: CompactString::from(table.name.as_str()),
            predicates: SmallVec::new(),
            select: SmallVec::new(),
            limit: None,
        }
    }

    /// Add an equality predicate.
    pub fn where_eq(mut self, column: &str, bind: &str) -> Self {
        self.predicates.push(Predicate::Eq {
            column: column.to_compact_string(),
            bind: bind.to_compact_string(),
        });
        self
    }

    /// Add a projection column.
    pub fn select(mut self, column: &str) -> Self {
        self.select.push(column.to_compact_string());
        self
    }

    /// Apply a limit.
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// Query predicate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Predicate {
    /// Equality predicate using a named bind parameter.
    Eq {
        /// Column name.
        column: CompactString,
        /// Bind parameter name.
        bind: CompactString,
    },
}

/// Database driver target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Driver {
    /// SQLite.
    Sqlite,
    /// PostgreSQL.
    Postgres,
    /// MySQL.
    Mysql,
}

/// ORM execution contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrmContract {
    /// Backing driver.
    pub driver: Driver,
    /// Whether prepared statements are required by default.
    pub prepared_by_default: bool,
    /// Whether query results use generated Flow row types.
    pub generated_flow_types: bool,
    /// Whether all query construction is parameterized.
    pub parameterized_queries_only: bool,
}

impl Default for OrmContract {
    fn default() -> Self {
        Self {
            driver: Driver::Postgres,
            prepared_by_default: true,
            generated_flow_types: true,
            parameterized_queries_only: true,
        }
    }
}

/// Migration descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Migration {
    /// Migration name.
    pub name: CompactString,
    /// Ordered operations.
    pub ops: MigrationOps,
}

impl Migration {
    /// Create an empty migration.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_compact_string(),
            ops: SmallVec::new(),
        }
    }

    /// Add an operation.
    pub fn op(mut self, op: MigrationOp) -> Self {
        self.ops.push(op);
        self
    }
}

/// Migration operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MigrationOp {
    /// Create a table.
    CreateTable {
        /// Table descriptor.
        table: Table,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn users_table() -> Table {
        Table::new("users", "UserRow")
            .column(Column::new("id", ColumnType::Uuid).primary_key())
            .column(Column::new("email", ColumnType::Text))
            .column(Column::new("created_at", ColumnType::Timestamp))
    }

    #[test]
    fn table_schema_is_compact_and_type_safe() {
        let table =
            users_table().relation(Relation::new("posts", "posts", RelationCardinality::Many));

        assert_eq!(table.name, "users");
        assert_eq!(table.row_type, "UserRow");
        assert_eq!(table.columns.len(), 3);
        assert!(table.columns[0].primary_key);
        assert_eq!(table.relations[0].cardinality, RelationCardinality::Many);
    }

    #[test]
    fn query_plan_uses_binds_and_generated_row_types() {
        let table = users_table();
        let plan = QueryPlan::from(&table)
            .select("id")
            .select("email")
            .where_eq("email", "email")
            .limit(1);
        let contract = OrmContract::default();

        assert_eq!(plan.table, "users");
        assert_eq!(plan.limit, Some(1));
        assert_eq!(plan.predicates.len(), 1);
        assert!(contract.prepared_by_default);
        assert!(contract.generated_flow_types);
        assert!(contract.parameterized_queries_only);
    }

    #[test]
    fn migration_keeps_operation_order() {
        let migration = Migration::new("create_users").op(MigrationOp::CreateTable {
            table: users_table(),
        });

        assert_eq!(migration.name, "create_users");
        assert_eq!(migration.ops.len(), 1);
    }
}
