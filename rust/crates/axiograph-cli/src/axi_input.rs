use anyhow::{anyhow, Result};

use axiograph_dsl::schema_v1::SchemaV1Module;
use axiograph_pathdb::axi_module_import::AxiSchemaV1ImportSummary;
use axiograph_pathdb::{AxiDigest, Module, Validated};

#[derive(Debug, Clone)]
pub(crate) struct CanonicalAxiModule {
    digest: AxiDigest,
    module: Module<Validated>,
}

impl CanonicalAxiModule {
    pub(crate) fn new(digest: AxiDigest, module: Module<Validated>) -> Self {
        Self { digest, module }
    }

    pub(crate) fn digest(&self) -> &AxiDigest {
        &self.digest
    }

    pub(crate) fn module(&self) -> &Module<Validated> {
        &self.module
    }

    pub(crate) fn into_parts(self) -> (AxiDigest, Module<Validated>) {
        (self.digest, self.module)
    }

    pub(crate) fn import_into_pathdb(
        &self,
        db: &mut axiograph_pathdb::PathDB,
    ) -> Result<AxiSchemaV1ImportSummary> {
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb(
            db,
            &self.module,
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PathdbExportAxiModule {
    module: SchemaV1Module,
}

impl PathdbExportAxiModule {
    pub(crate) fn new(module: SchemaV1Module) -> Self {
        Self { module }
    }

    pub(crate) fn import_pathdb(&self) -> Result<axiograph_pathdb::PathDB> {
        axiograph_pathdb::axi_export::import_pathdb_from_axi_v1_module(&self.module)
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ClassifiedAxiModule {
    PathdbExport(PathdbExportAxiModule),
    Canonical(CanonicalAxiModule),
}

pub(crate) fn is_pathdb_export_v1_module(m: &axiograph_dsl::schema_v1::SchemaV1Module) -> bool {
    m.schemas
        .iter()
        .any(|s| s.name == axiograph_pathdb::axi_export::PATHDB_EXPORT_SCHEMA_NAME_V1)
        && m.instances.iter().any(|i| {
            i.schema == axiograph_pathdb::axi_export::PATHDB_EXPORT_SCHEMA_NAME_V1
                && i.name == axiograph_pathdb::axi_export::PATHDB_EXPORT_INSTANCE_NAME_V1
        })
}

pub(crate) fn classify_axi_text(text: &str) -> Result<ClassifiedAxiModule> {
    let digest = AxiDigest::from_axi_text(text);
    let module = axiograph_dsl::axi_v1::parse_axi_v1(text)?;
    if is_pathdb_export_v1_module(&module) {
        Ok(ClassifiedAxiModule::PathdbExport(
            PathdbExportAxiModule::new(module),
        ))
    } else {
        let module = axiograph_pathdb::validate_axi_v1_module(module)?;
        Ok(ClassifiedAxiModule::Canonical(CanonicalAxiModule::new(
            digest, module,
        )))
    }
}

pub(crate) fn require_canonical_axi_text(text: &str) -> Result<CanonicalAxiModule> {
    match classify_axi_text(text)? {
        ClassifiedAxiModule::Canonical(module) => Ok(module),
        ClassifiedAxiModule::PathdbExport(_) => Err(anyhow!(
            "expected a canonical .axi module, but input is a PathDBExportV1 snapshot"
        )),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn require_pathdb_export_axi_text(text: &str) -> Result<PathdbExportAxiModule> {
    match classify_axi_text(text)? {
        ClassifiedAxiModule::PathdbExport(module) => Ok(module),
        ClassifiedAxiModule::Canonical(_) => Err(anyhow!(
            "expected a PathDBExportV1 .axi snapshot, but input is a canonical .axi module"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_axi_text_marks_canonical_modules_as_validated() {
        let text = r#"
module Demo

schema S:
  object A
  relation R(from: A, to: A)

instance I of S:
  A = {x, y}
  R = {(from=x, to=y)}
"#;

        match classify_axi_text(text).expect("classify canonical module") {
            ClassifiedAxiModule::Canonical(module) => {
                assert!(module.digest().has_v1_prefix());
                assert_eq!(module.module().module().module_name, "Demo");
                assert_eq!(module.module().proof().schema_count, 1);
                assert_eq!(module.module().proof().instance_count, 1);
            }
            ClassifiedAxiModule::PathdbExport(_) => {
                panic!("expected canonical module classification")
            }
        }
    }

    #[test]
    fn classify_axi_text_marks_pathdb_export_snapshots() {
        let text = r#"
module Demo

schema S:
  object A
  relation R(from: A, to: A)

instance I of S:
  A = {x, y}
  R = {(from=x, to=y)}
"#;

        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, text)
            .expect("import canonical module");
        db.build_indexes();
        let export = axiograph_pathdb::axi_export::export_pathdb_to_axi_v1(&db)
            .expect("export pathdb snapshot");

        match classify_axi_text(&export).expect("classify snapshot export") {
            ClassifiedAxiModule::PathdbExport(module) => {
                assert_eq!(module.module.module_name, "PathDBExport");
                assert!(module.import_pathdb().is_ok());
            }
            ClassifiedAxiModule::Canonical(_) => {
                panic!("expected snapshot export classification")
            }
        }
    }

    #[test]
    fn require_pathdb_export_axi_text_rejects_canonical_modules() {
        let text = r#"
module Demo

schema S:
  object A
"#;

        let err = require_pathdb_export_axi_text(text).expect_err("canonical module must fail");
        assert!(
            err.to_string()
                .contains("expected a PathDBExportV1 .axi snapshot"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn require_canonical_axi_text_returns_canonical_wrapper() {
        let text = r#"
module Demo

schema S:
  object A
"#;

        let module = require_canonical_axi_text(text).expect("canonical module");
        assert!(module.digest().has_v1_prefix());
        assert_eq!(module.module().module().module_name, "Demo");
    }
}
