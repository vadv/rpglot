//! OpenAPI documentation definition.

use rpglot_core::api::schema::{ApiSchema, DateInfo, TimelineInfo};
use rpglot_core::api::snapshot::ApiSnapshot;
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::handlers::handle_health,
        crate::handlers::handle_schema,
        crate::handlers::handle_snapshot,
        crate::handlers::handle_timeline,
        crate::handlers::handle_heatmap,
    ),
    components(schemas(
        ApiSnapshot,
        ApiSchema,
        TimelineInfo,
        DateInfo,
        rpglot_core::api::schema::ApiMode,
        rpglot_core::api::schema::SummarySchema,
        rpglot_core::api::schema::SummarySection,
        rpglot_core::api::schema::FieldSchema,
        rpglot_core::api::schema::TabsSchema,
        rpglot_core::api::schema::TabSchema,
        rpglot_core::api::schema::ColumnSchema,
        rpglot_core::api::schema::ViewSchema,
        rpglot_core::api::schema::DrillDown,
        rpglot_core::api::schema::DataType,
        rpglot_core::api::schema::Unit,
        rpglot_core::api::schema::Format,
        rpglot_core::api::snapshot::SystemSummary,
        rpglot_core::api::snapshot::CpuSummary,
        rpglot_core::api::snapshot::LoadSummary,
        rpglot_core::api::snapshot::MemorySummary,
        rpglot_core::api::snapshot::SwapSummary,
        rpglot_core::api::snapshot::DiskSummary,
        rpglot_core::api::snapshot::NetworkSummary,
        rpglot_core::api::snapshot::PsiSummary,
        rpglot_core::api::snapshot::VmstatSummary,
        rpglot_core::api::snapshot::PgSummary,
        rpglot_core::api::snapshot::BgwriterSummary,
        rpglot_core::api::snapshot::PgActivityRow,
        rpglot_core::api::snapshot::PgStatementsRow,
        rpglot_core::api::snapshot::PgTablesRow,
        rpglot_core::api::snapshot::PgIndexesRow,
        rpglot_core::api::snapshot::PgStorePlansRow,
        rpglot_core::api::snapshot::PgLocksRow,
        rpglot_core::api::snapshot::ReplicationInfo,
        rpglot_core::api::snapshot::ReplicaDetail,
    )),
    info(
        title = "rpglot API",
        version = "1.0",
        description = "PostgreSQL monitoring API — real-time and historical system/database snapshots"
    )
)]
pub(crate) struct ApiDoc;
