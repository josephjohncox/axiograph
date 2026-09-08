use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use walkdir::{DirEntry, WalkDir};

use axiograph_dsl::schema_v1::SchemaV1Module;
use axiograph_kernel::{
    CanonicalCompiler, CanonicalModuleSource, CompiledKernelSnapshot, KernelCompilationRequest,
    RepositoryIdV2, SnapshotIdV2,
};
use axiograph_pathdb::axi_module_import::AxiSchemaV1ImportSummary;
use axiograph_pathdb::{AxiDigest, Module, Validated};

pub(crate) mod diagnostics;

const MAX_AXI_IMPORT_MODULES: usize = 1024;
const MAX_AXI_PACKAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_AXI_SEARCH_ROOTS: usize = 32;
const MAX_AXI_SEARCH_ENTRIES: usize = 50_000;
const MAX_AXI_SEARCH_DEPTH: usize = 32;

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
pub(crate) struct CanonicalAxiPackage {
    root_module: String,
    sources: BTreeMap<String, CanonicalModuleSource>,
    snapshot: CompiledKernelSnapshot,
}

impl CanonicalAxiPackage {
    pub(crate) fn root_source(&self) -> &CanonicalModuleSource {
        &self.sources[&self.root_module]
    }

    pub(crate) fn snapshot(&self) -> &CompiledKernelSnapshot {
        &self.snapshot
    }

    pub(crate) fn ordered_sources(&self) -> Vec<CanonicalModuleSource> {
        self.snapshot
            .ir()
            .ordered_module_closure()
            .iter()
            .map(|module| self.sources[&module.module_name].clone())
            .collect()
    }
}

pub(crate) fn compile_canonical_axi_path(
    input: &Path,
    search_roots: &[PathBuf],
) -> Result<CanonicalAxiPackage> {
    let input = canonical_regular_file_path(input, "canonical .axi module")?;
    let exact_bytes = crate::security::read_file_bounded(
        &input,
        crate::security::MAX_AXI_MODULE_BYTES,
        "canonical .axi module",
    )?;
    compile_canonical_axi_path_with_root_bytes(&input, exact_bytes, search_roots)
}

/// Compile a workspace package while replacing only the root module bytes.
///
/// Editor adapters use this for unsaved root buffers. Imports are still loaded
/// from the workspace search roots and the same `CanonicalCompiler` remains the
/// sole compiler. The override is never written to disk.
pub(crate) fn compile_canonical_axi_path_with_root_bytes(
    input: &Path,
    exact_bytes: Vec<u8>,
    search_roots: &[PathBuf],
) -> Result<CanonicalAxiPackage> {
    if exact_bytes.len() > crate::security::MAX_AXI_MODULE_BYTES {
        return Err(anyhow!(
            "canonical .axi root exceeds {} bytes",
            crate::security::MAX_AXI_MODULE_BYTES
        ));
    }
    if search_roots.len() > MAX_AXI_SEARCH_ROOTS {
        return Err(anyhow!(
            "canonical .axi search roots exceed {MAX_AXI_SEARCH_ROOTS}"
        ));
    }
    let input = canonical_regular_file_path(input, "canonical .axi module")?;
    let root_source = CanonicalModuleSource::parse(exact_bytes)?;
    reject_obsolete_pathdb_snapshot_module(root_source.parsed())?;
    let root_module = root_source.parsed().module_name.clone();
    let mut package_bytes = root_source.exact_text().len();
    let mut paths = BTreeMap::from([(root_module.clone(), input.clone())]);
    let mut sources = BTreeMap::from([(root_module.clone(), root_source)]);
    let mut pending = sources[&root_module].parsed().imports.clone();
    while let Some(import) = pending.pop() {
        if sources.contains_key(&import) {
            continue;
        }
        if sources.len() >= MAX_AXI_IMPORT_MODULES {
            return Err(anyhow!(
                "canonical .axi import closure exceeds {MAX_AXI_IMPORT_MODULES} modules"
            ));
        }
        let (path, source) = resolve_import_source(&input, &import, search_roots)?;
        paths.insert(import.clone(), path);
        package_bytes = package_bytes
            .checked_add(source.exact_text().len())
            .ok_or_else(|| anyhow!("canonical .axi package byte count overflow"))?;
        if package_bytes > MAX_AXI_PACKAGE_BYTES {
            return Err(anyhow!(
                "canonical .axi import closure exceeds {MAX_AXI_PACKAGE_BYTES} bytes"
            ));
        }
        reject_obsolete_pathdb_snapshot_module(source.parsed())?;
        pending.extend(source.parsed().imports.iter().cloned());
        if pending.len() > MAX_AXI_IMPORT_MODULES * MAX_AXI_IMPORT_MODULES {
            return Err(anyhow!(
                "canonical .axi import worklist exceeds finite bound"
            ));
        }
        sources.insert(import, source);
    }

    let repository_descriptor = search_roots
        .first()
        .and_then(|root| fs::canonicalize(root).ok())
        .or_else(|| input.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| input.clone());
    let repository_id =
        RepositoryIdV2::from_descriptor_bytes(repository_descriptor.to_string_lossy().as_bytes());
    let preliminary = CanonicalCompiler::compile(KernelCompilationRequest {
        repository_id: repository_id.clone(),
        accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(&[b"closure-order-probe"]),
        root_module: root_module.clone(),
        modules: sources.values().cloned().collect(),
    })
    .map_err(|error| diagnostics::compile_diagnostic(error, &sources, &paths))?;
    let accepted_fields = preliminary
        .ir()
        .ordered_module_closure()
        .iter()
        .map(|module| sources[&module.module_name].exact_text().as_bytes())
        .collect::<Vec<_>>();
    let snapshot = CanonicalCompiler::compile(KernelCompilationRequest {
        repository_id,
        accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(&accepted_fields),
        root_module: root_module.clone(),
        modules: sources.values().cloned().collect(),
    })
    .map_err(|error| diagnostics::compile_diagnostic(error, &sources, &paths))?;
    Ok(CanonicalAxiPackage {
        root_module,
        sources,
        snapshot,
    })
}

fn canonical_regular_file_path(path: &Path, label: &str) -> Result<PathBuf> {
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("{label} path has no filename"))?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = fs::canonicalize(parent)
        .with_context(|| format!("canonicalize {label} parent `{}`", parent.display()))?;
    let parent_metadata = fs::symlink_metadata(&parent)?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.file_type().is_dir() {
        return Err(anyhow!("{label} parent must be a real directory"));
    }
    let canonical_parent_candidate = parent.join(file_name);
    let metadata = fs::symlink_metadata(&canonical_parent_candidate)
        .with_context(|| format!("inspect {label} `{}`", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(anyhow!(
            "{label} `{}` must be a regular file, not a symlink or special file",
            path.display()
        ));
    }
    Ok(canonical_parent_candidate)
}

fn canonical_real_directory(path: &Path, label: &str) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect {label} `{}`", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Err(anyhow!(
            "{label} `{}` must be a real directory, not a symlink or special file",
            path.display()
        ));
    }
    fs::canonicalize(path).with_context(|| format!("canonicalize {label} `{}`", path.display()))
}

fn resolve_import_source(
    root_input: &Path,
    import: &str,
    search_roots: &[PathBuf],
) -> Result<(PathBuf, CanonicalModuleSource)> {
    if import.is_empty()
        || import.len() > 256
        || import.contains('/')
        || import.contains('\\')
        || import == "."
        || import == ".."
    {
        return Err(anyhow!("invalid canonical .axi import name `{import}`"));
    }

    let mut allowed_roots = BTreeSet::new();
    if let Some(parent) = root_input.parent() {
        allowed_roots.insert(fs::canonicalize(parent)?);
    }
    for root in search_roots {
        allowed_roots.insert(canonical_real_directory(
            root,
            "canonical .axi search root",
        )?);
    }

    let mut candidate_paths = BTreeSet::new();
    let mut search_entries = 0_usize;
    for root in &allowed_roots {
        candidate_paths.insert(root.join(format!("{import}.axi")));
        for result in WalkDir::new(root)
            .follow_links(false)
            .max_depth(MAX_AXI_SEARCH_DEPTH)
            .into_iter()
            .filter_entry(is_import_search_entry)
        {
            let entry = result.with_context(|| {
                format!(
                    "failed while searching .axi imports under `{}`",
                    root.display()
                )
            })?;
            search_entries = search_entries.saturating_add(1);
            if search_entries > MAX_AXI_SEARCH_ENTRIES {
                return Err(anyhow!(
                    "canonical .axi import search exceeds {MAX_AXI_SEARCH_ENTRIES} filesystem entries"
                ));
            }
            if entry.file_type().is_file()
                && entry.path().extension().is_some_and(|ext| ext == "axi")
            {
                candidate_paths.insert(entry.into_path());
            }
        }
    }

    let mut matches = Vec::new();
    for candidate in candidate_paths {
        let metadata = match fs::symlink_metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            continue;
        }
        if !allowed_roots.iter().any(|root| candidate.starts_with(root)) {
            return Err(anyhow!(
                "canonical .axi import `{}` escapes configured search roots",
                candidate.display()
            ));
        }
        let bytes = crate::security::read_file_bounded(
            &candidate,
            crate::security::MAX_AXI_MODULE_BYTES,
            "imported canonical .axi module",
        )?;
        let Ok(source) = CanonicalModuleSource::parse(bytes) else {
            continue;
        };
        if source.parsed().module_name == import {
            matches.push((candidate, source));
        }
    }
    match matches.len() {
        0 => Err(anyhow!(
            "cannot resolve imported module `{import}` from `{}`",
            root_input.display()
        )),
        1 => {
            let Some(resolved) = matches.pop() else {
                return Err(anyhow!("resolved import disappeared before use"));
            };
            Ok(resolved)
        }
        _ => Err(anyhow!(
            "imported module `{import}` is ambiguous: {}",
            matches
                .iter()
                .map(|(path, _)| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn is_import_search_entry(entry: &DirEntry) -> bool {
    !entry.file_type().is_dir()
        || !matches!(
            entry.file_name().to_string_lossy().as_ref(),
            ".git" | "target" | "build" | "node_modules"
        )
}

pub(crate) fn reject_obsolete_pathdb_snapshot_module(module: &SchemaV1Module) -> Result<()> {
    let obsolete = module
        .schemas
        .iter()
        .any(|schema| schema.name == "PathDBExportV1")
        || module
            .instances
            .iter()
            .any(|instance| instance.schema == "PathDBExportV1");
    if obsolete {
        Err(anyhow!(
            "obsolete PathDBExportV1 `.axi` snapshots are unsupported; rebuild `.axpd` from exact accepted modules and KernelSnapshotIr"
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn require_canonical_axi_text(text: &str) -> Result<CanonicalAxiModule> {
    let digest = AxiDigest::from_axi_text(text);
    let module = axiograph_dsl::axi_v1::parse_axi_v1(text)?;
    reject_obsolete_pathdb_snapshot_module(&module)?;
    let module = axiograph_pathdb::validate_axi_v1_module(module)?;
    Ok(CanonicalAxiModule::new(digest, module))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_axi_text_is_validated() {
        let text = r#"
module Demo

schema S:
  object A
  relation R(from: A, to: A)

instance I of S:
  A = {x, y}
  R = {(from=x, to=y)}
"#;

        let module = require_canonical_axi_text(text).expect("canonical module");
        assert!(module.digest().is_revision_id_v2());
        assert_eq!(module.module().module().module_name, "Demo");
        assert_eq!(module.module().proof().schema_count, 1);
        assert_eq!(module.module().proof().instance_count, 1);
    }

    #[test]
    fn obsolete_pathdb_export_modules_reject_without_a_reader() {
        let obsolete = r#"
module PathDBExport
schema PathDBExportV1:
  object Entity
instance SnapshotV1 of PathDBExportV1:
  Entity = {Entity_0}
"#;
        let err = require_canonical_axi_text(obsolete).expect_err("obsolete snapshot must reject");
        assert!(err.to_string().contains("PathDBExportV1"));
        assert!(err.to_string().contains("unsupported"));
    }

    #[test]
    fn canonical_boundary_rejects_missing_or_multiple_module_headers() {
        let missing = r#"
schema S:
  object A
"#;
        let err = require_canonical_axi_text(missing)
            .expect_err("accepted candidate without a module header must reject");
        assert!(err.to_string().contains("exactly one explicit"));

        let multiple = r#"
module Left
schema L:
  object A
module Right
schema R:
  object B
"#;
        let err = require_canonical_axi_text(multiple)
            .expect_err("concatenated modules must reject before typechecking");
        assert!(err.to_string().contains("exactly one module header"));
    }

    #[test]
    fn compile_path_resolves_imports_and_hashes_exact_bytes_in_closure_order() {
        let temp = tempfile::tempdir().expect("tempdir");
        let base_path = temp.path().join("Base.axi");
        let root_path = temp.path().join("Root.axi");
        let base = b"module Base\n\nschema Shared:\n  object Person\n";
        let root = b"module Root\nimport Base\n\ninstance I of Shared:\n  Person = {Alice}\n";
        crate::security::write_output_bounded(&base_path, base, "CLI output").expect("write base");
        crate::security::write_output_bounded(&root_path, root, "CLI output").expect("write root");

        let package = compile_canonical_axi_path(&root_path, &[temp.path().to_path_buf()])
            .expect("compile package path");
        assert_eq!(package.root_source().parsed().module_name, "Root");
        let closure = package.snapshot().ir().ordered_module_closure();
        assert_eq!(closure[0].module_name, "Base");
        assert_eq!(closure[1].module_name, "Root");
        assert_eq!(
            package.snapshot().ir().accepted_snapshot_id(),
            &SnapshotIdV2::from_canonical_fields(&[base, root])
        );
    }

    #[test]
    fn canonical_package_rejects_oversized_root_and_search_root_fanout() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root_path = temp.path().join("Root.axi");
        crate::security::write_output_bounded(&root_path, b"module Root\n", "CLI output")
            .expect("write root");

        let oversized = vec![b'x'; crate::security::MAX_AXI_MODULE_BYTES + 1];
        let error = compile_canonical_axi_path_with_root_bytes(&root_path, oversized, &[])
            .expect_err("oversized canonical root must reject");
        assert!(error.to_string().contains("root exceeds"));

        let roots = vec![temp.path().to_path_buf(); MAX_AXI_SEARCH_ROOTS + 1];
        let error = compile_canonical_axi_path_with_root_bytes(
            &root_path,
            b"module Root\n".to_vec(),
            &roots,
        )
        .expect_err("search-root fanout must reject");
        assert!(error.to_string().contains("search roots exceed"));
    }

    #[cfg(unix)]
    #[test]
    fn canonical_paths_do_not_follow_root_import_or_search_root_symlinks() {
        let temp = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("outside tempdir");
        let root_path = temp.path().join("Root.axi");
        let outside_base = outside.path().join("Base.axi");
        crate::security::write_output_bounded(
            &root_path,
            b"module Root\nimport Base\n",
            "CLI output",
        )
        .expect("write root");
        crate::security::write_output_bounded(
            &outside_base,
            b"module Base\nschema S:\n  object A\n",
            "CLI output",
        )
        .expect("write outside import");

        let root_link = temp.path().join("RootLink.axi");
        std::os::unix::fs::symlink(&root_path, &root_link).expect("create root symlink");
        let error =
            compile_canonical_axi_path(&root_link, &[]).expect_err("symlink root must reject");
        assert!(error.to_string().contains("regular file"));

        std::os::unix::fs::symlink(&outside_base, temp.path().join("Base.axi"))
            .expect("create import symlink");
        let error = compile_canonical_axi_path(&root_path, &[temp.path().to_path_buf()])
            .expect_err("symlink import must reject");
        assert!(error.to_string().contains("cannot resolve imported module"));

        let search_root_link = temp.path().join("search-root-link");
        std::os::unix::fs::symlink(outside.path(), &search_root_link)
            .expect("create search-root symlink");
        let error = compile_canonical_axi_path(&root_path, &[search_root_link])
            .expect_err("symlink search root must reject");
        assert!(error.to_string().contains("search root"));
    }

    #[test]
    fn require_canonical_axi_text_returns_canonical_wrapper() {
        let text = r#"
module Demo

schema S:
  object A
"#;

        let module = require_canonical_axi_text(text).expect("canonical module");
        assert!(module.digest().is_revision_id_v2());
        assert_eq!(module.module().module().module_name, "Demo");
    }
}
