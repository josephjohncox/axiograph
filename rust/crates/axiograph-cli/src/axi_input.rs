use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt::{Display, Write as _},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
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
const MAX_IMPORT_DIAGNOSTIC_DETAIL_BYTES: usize = 4 * 1024;

#[derive(Debug)]
struct DiagnosticImportWorkBudget {
    used: usize,
    max: usize,
    exhausted: bool,
}

impl DiagnosticImportWorkBudget {
    fn new(max: usize) -> Self {
        Self {
            used: 0,
            max,
            exhausted: false,
        }
    }

    fn charge(&mut self, amount: usize) -> bool {
        let Some(next) = self.used.checked_add(amount) else {
            self.used = self.max;
            self.exhausted = true;
            return false;
        };
        if next > self.max {
            self.used = self.max;
            self.exhausted = true;
            return false;
        }
        self.used = next;
        true
    }
}

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
    compile_canonical_axi_path_with_root_bytes_mode(
        input,
        exact_bytes,
        search_roots,
        &BTreeMap::new(),
        false,
    )
}

/// Same as compiling with an unsaved root replacement, but any import whose
/// resolved candidate path is a key of `import_overlays` is compiled from the
/// overlay's exact unsaved bytes instead of disk bytes.
/// the disk-resolution guess exactly; overlay presence bypasses the disk
/// existence check entirely for that candidate.
pub(crate) fn compile_canonical_axi_path_with_overlays_collecting(
    input: &Path,
    exact_bytes: Vec<u8>,
    search_roots: &[PathBuf],
    import_overlays: &BTreeMap<PathBuf, Vec<u8>>,
) -> Result<CanonicalAxiPackage> {
    if import_overlays.len() > MAX_AXI_IMPORT_MODULES {
        return Err(anyhow!(
            "canonical .axi import overlays exceed {MAX_AXI_IMPORT_MODULES} entries"
        ));
    }
    for bytes in import_overlays.values() {
        if bytes.len() > crate::security::MAX_AXI_MODULE_BYTES {
            return Err(anyhow!(
                "unsaved canonical .axi import overlay exceeds {} bytes",
                crate::security::MAX_AXI_MODULE_BYTES
            ));
        }
    }
    compile_canonical_axi_path_with_root_bytes_mode(
        input,
        exact_bytes,
        search_roots,
        import_overlays,
        true,
    )
}

fn compile_canonical_axi_path_with_root_bytes_mode(
    input: &Path,
    exact_bytes: Vec<u8>,
    search_roots: &[PathBuf],
    import_overlays: &BTreeMap<PathBuf, Vec<u8>>,
    collect_diagnostics: bool,
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
    let diagnostic_limits = axiograph_kernel::KernelDiagnosticLimits::default();
    let mut diagnostic_import_work =
        DiagnosticImportWorkBudget::new(diagnostic_limits.max_work / 2);
    let root_source = match CanonicalModuleSource::parse(exact_bytes.clone()) {
        Ok(source) => source,
        Err(axiograph_kernel::KernelCompileError::Parse { line, message })
            if collect_diagnostics =>
        {
            let exact_text = String::from_utf8(exact_bytes)
                .map_err(|_| axiograph_kernel::KernelCompileError::InvalidUtf8)?;
            return Err(diagnostics::parse_diagnostic_collection(
                &input,
                &exact_text,
                line,
                message,
                1,
            )
            .into());
        }
        Err(error) => return Err(error.into()),
    };
    reject_obsolete_pathdb_snapshot_module(root_source.parsed())?;
    let root_module = root_source.parsed().module_name.clone();
    let mut package_bytes = root_source.exact_text().len();
    let mut paths = BTreeMap::from([(root_module.clone(), input.clone())]);
    let mut sources = BTreeMap::from([(root_module.clone(), root_source)]);
    let mut load_failures = BTreeMap::<(String, usize, String), String>::new();
    let mut load_failure_message_bytes = 0_usize;
    let mut additional_omitted_load_failures = 0_usize;
    let mut authoritative_load_failure = None::<(String, usize, String)>;
    let mut canonical_failure_seen = false;
    let root_importer = Arc::<str>::from(root_module.as_str());
    let mut pending = sources[&root_module]
        .parsed()
        .imports
        .iter()
        .enumerate()
        .map(|(index, import)| (Arc::clone(&root_importer), index, import.clone()))
        .collect::<VecDeque<_>>();
    if pending.len() > MAX_AXI_IMPORT_MODULES * MAX_AXI_IMPORT_MODULES {
        return Err(anyhow!(
            "canonical .axi import worklist exceeds finite bound"
        ));
    }
    while let Some((importer, import_index, import)) = pending.pop_front() {
        if collect_diagnostics && canonical_failure_seen && !diagnostic_import_work.charge(1) {
            break;
        }
        if sources.contains_key(&import) {
            continue;
        }
        if sources.len() >= MAX_AXI_IMPORT_MODULES {
            if collect_diagnostics && canonical_failure_seen {
                additional_omitted_load_failures =
                    additional_omitted_load_failures.saturating_add(1);
                break;
            }
            return Err(anyhow!(
                "canonical .axi import closure exceeds {MAX_AXI_IMPORT_MODULES} modules"
            ));
        }
        let resolved = if collect_diagnostics && canonical_failure_seen {
            resolve_import_source_with_diagnostic_budget(
                &input,
                &import,
                search_roots,
                import_overlays,
                &mut diagnostic_import_work,
            )
        } else {
            resolve_import_source(&input, &import, search_roots, import_overlays)
        };
        let (path, source) = match resolved {
            Ok(resolved) => resolved,
            Err(error) if collect_diagnostics => {
                canonical_failure_seen = true;
                if diagnostic_import_work.exhausted {
                    break;
                }
                let key = (importer.to_string(), import_index, import);
                let is_authoritative = authoritative_load_failure.is_none();
                authoritative_load_failure.get_or_insert_with(|| key.clone());
                let detail = bounded_display(&error, MAX_IMPORT_DIAGNOSTIC_DETAIL_BYTES);
                let message_bytes = "module `"
                    .len()
                    .saturating_add(key.0.len())
                    .saturating_add("` cannot resolve import `".len())
                    .saturating_add(key.2.len())
                    .saturating_add("`: ".len())
                    .saturating_add(detail.len());
                let fits = load_failures.len() < axiograph_kernel::MAX_KERNEL_DIAGNOSTIC_ITEMS
                    && load_failure_message_bytes
                        .checked_add(message_bytes)
                        .is_some_and(|total| {
                            total <= axiograph_kernel::MAX_KERNEL_DIAGNOSTIC_MESSAGE_BYTES
                        });
                if !fits {
                    if !is_authoritative {
                        additional_omitted_load_failures =
                            additional_omitted_load_failures.saturating_add(1);
                    }
                    break;
                }
                load_failure_message_bytes += message_bytes;
                load_failures.insert(key, detail);
                continue;
            }
            Err(error) => return Err(error),
        };
        let Some(next_package_bytes) = package_bytes.checked_add(source.exact_text().len()) else {
            if collect_diagnostics && canonical_failure_seen {
                additional_omitted_load_failures =
                    additional_omitted_load_failures.saturating_add(1);
                break;
            }
            return Err(anyhow!("canonical .axi package byte count overflow"));
        };
        if next_package_bytes > MAX_AXI_PACKAGE_BYTES {
            if collect_diagnostics && canonical_failure_seen {
                additional_omitted_load_failures =
                    additional_omitted_load_failures.saturating_add(1);
                break;
            }
            return Err(anyhow!(
                "canonical .axi import closure exceeds {MAX_AXI_PACKAGE_BYTES} bytes"
            ));
        }
        if let Err(error) = reject_obsolete_pathdb_snapshot_module(source.parsed()) {
            if collect_diagnostics && canonical_failure_seen {
                additional_omitted_load_failures =
                    additional_omitted_load_failures.saturating_add(1);
                break;
            }
            return Err(error);
        }
        package_bytes = next_package_bytes;
        paths.insert(import.clone(), path);
        let child_imports = source.parsed().imports.len();
        if pending.len().saturating_add(child_imports)
            > MAX_AXI_IMPORT_MODULES * MAX_AXI_IMPORT_MODULES
        {
            if collect_diagnostics && canonical_failure_seen {
                additional_omitted_load_failures =
                    additional_omitted_load_failures.saturating_add(1);
                break;
            }
            return Err(anyhow!(
                "canonical .axi import worklist exceeds finite bound"
            ));
        }
        let child_importer = Arc::<str>::from(import.as_str());
        pending.extend(
            source
                .parsed()
                .imports
                .iter()
                .enumerate()
                .map(|(index, child)| (Arc::clone(&child_importer), index, child.clone())),
        );
        sources.insert(import, source);
    }

    let repository_descriptor = search_roots
        .first()
        .and_then(|root| fs::canonicalize(root).ok())
        .or_else(|| input.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| input.clone());
    let repository_id =
        RepositoryIdV2::from_descriptor_bytes(repository_descriptor.to_string_lossy().as_bytes());
    let preliminary_request = KernelCompilationRequest {
        repository_id: repository_id.clone(),
        accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(&[b"closure-order-probe"]),
        root_module: root_module.clone(),
        modules: sources.values().cloned().collect(),
    };
    if collect_diagnostics && authoritative_load_failure.is_some() {
        let remaining_work = diagnostic_limits
            .max_work
            .saturating_sub(diagnostic_import_work.used)
            .max(1);
        let remaining_message_bytes = diagnostic_limits
            .max_message_bytes
            .saturating_sub(load_failure_message_bytes)
            .max(1);
        let mut known_import_failures = load_failures.keys().cloned().collect::<BTreeSet<_>>();
        let (authoritative_module, authoritative_import_index, authoritative_import) =
            authoritative_load_failure
                .as_ref()
                .ok_or_else(|| anyhow!("diagnostic load-failure identity is missing"))?;
        known_import_failures.insert((
            authoritative_module.clone(),
            *authoritative_import_index,
            authoritative_import.clone(),
        ));
        let authoritative_unknown = axiograph_kernel::KernelCompileError::UnknownImport {
            module: authoritative_module.clone(),
            import: authoritative_import.clone(),
        };
        let mut collected = CanonicalCompiler::collect_diagnostics_for_known_import_failures_with_authoritative_error(
            &preliminary_request,
            axiograph_kernel::KernelDiagnosticLimits {
                max_message_bytes: remaining_message_bytes,
                max_work: remaining_work,
                ..diagnostic_limits
            },
            &known_import_failures,
            authoritative_load_failure
                .as_ref()
                .ok_or_else(|| anyhow!("diagnostic load-failure identity is missing"))?,
            &authoritative_unknown,
        )?;
        collected.work_units = collected
            .work_units
            .saturating_add(diagnostic_import_work.used)
            .min(diagnostic_limits.max_work);
        collected.work_exhausted |= diagnostic_import_work.exhausted;
        collected.limits = diagnostic_limits;
        collected.observed_errors = collected
            .observed_errors
            .saturating_add(additional_omitted_load_failures);
        collected.omitted_observed_errors = collected
            .omitted_observed_errors
            .saturating_add(additional_omitted_load_failures);
        for diagnostic in &mut collected.diagnostics {
            if let (
                axiograph_kernel::KernelCompileError::UnknownImport { module, import },
                axiograph_kernel::KernelDiagnosticSubjectV1::Syntactic {
                    address:
                        axiograph_dsl::schema_v1::CanonicalSyntacticAddressV1::ImportName {
                            import_index,
                        },
                    ..
                },
            ) = (&diagnostic.cause, &diagnostic.subject)
            {
                if let Some(detail) =
                    load_failures.get(&(module.clone(), *import_index, import.clone()))
                {
                    diagnostic.cause = axiograph_kernel::KernelCompileError::ImportResolution {
                        module: module.clone(),
                        import: import.clone(),
                        detail: detail.clone(),
                    };
                }
            }
        }
        let authoritative_error = load_failures
            .get(&(
                authoritative_module.clone(),
                *authoritative_import_index,
                authoritative_import.clone(),
            ))
            .map_or(authoritative_unknown, |detail| {
                axiograph_kernel::KernelCompileError::ImportResolution {
                    module: authoritative_module.clone(),
                    import: authoritative_import.clone(),
                    detail: detail.clone(),
                }
            });
        collected.message_bytes = collected
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.cause.to_string().len())
            .sum();
        diagnostics::retain_authoritative_loader_error(&mut collected, authoritative_error);
        return Err(diagnostics::collect_kernel_diagnostics(collected, &sources, &paths).into());
    }
    let preliminary = match CanonicalCompiler::compile(preliminary_request.clone()) {
        Ok(snapshot) => snapshot,
        Err(error) if collect_diagnostics => {
            let remaining_work = diagnostic_limits
                .max_work
                .saturating_sub(diagnostic_import_work.used)
                .max(1);
            let mut collected = CanonicalCompiler::collect_diagnostics_with_authoritative_error(
                &preliminary_request,
                axiograph_kernel::KernelDiagnosticLimits {
                    max_work: remaining_work,
                    ..diagnostic_limits
                },
                &error,
            )?;
            collected.work_units = collected
                .work_units
                .saturating_add(diagnostic_import_work.used)
                .min(diagnostic_limits.max_work);
            collected.work_exhausted |= diagnostic_import_work.exhausted;
            collected.limits = diagnostic_limits;
            diagnostics::retain_authoritative_compile_error(&mut collected, error);
            return Err(
                diagnostics::collect_kernel_diagnostics(collected, &sources, &paths).into(),
            );
        }
        Err(error) => return Err(diagnostics::compile_diagnostic(error, &sources, &paths)),
    };
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
    import_overlays: &BTreeMap<PathBuf, Vec<u8>>,
) -> Result<(PathBuf, CanonicalModuleSource)> {
    resolve_import_source_mode(root_input, import, search_roots, import_overlays, None)
}

fn resolve_import_source_with_diagnostic_budget(
    root_input: &Path,
    import: &str,
    search_roots: &[PathBuf],
    import_overlays: &BTreeMap<PathBuf, Vec<u8>>,
    work: &mut DiagnosticImportWorkBudget,
) -> Result<(PathBuf, CanonicalModuleSource)> {
    resolve_import_source_mode(
        root_input,
        import,
        search_roots,
        import_overlays,
        Some(work),
    )
}

fn resolve_import_source_mode(
    root_input: &Path,
    import: &str,
    search_roots: &[PathBuf],
    import_overlays: &BTreeMap<PathBuf, Vec<u8>>,
    mut diagnostic_work: Option<&mut DiagnosticImportWorkBudget>,
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
        charge_diagnostic_import_work(&mut diagnostic_work, 1)?;
        allowed_roots.insert(fs::canonicalize(parent)?);
    }
    for root in search_roots {
        charge_diagnostic_import_work(&mut diagnostic_work, 1)?;
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
            charge_diagnostic_import_work(&mut diagnostic_work, 1)?;
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
    // An unsaved importee overlay is a candidate too, even when the buffer has
    // never been written to disk: its canonical path still matches the same
    // `<allowed_root>/<import>.axi` guess used for disk resolution, so it joins
    // the same ambiguity/duplicate-name accounting as any real file.
    for overlay_path in import_overlays.keys() {
        if allowed_roots
            .iter()
            .any(|root| overlay_path.starts_with(root))
            && overlay_path
                .file_name()
                .is_some_and(|name| name == expected_file_name_for(import).as_str())
        {
            candidate_paths.insert(overlay_path.clone());
        }
    }

    let expected_file_name = format!("{import}.axi");
    let mut match_count = 0_usize;
    let mut first_match = None;
    let mut matched_path_sample = Vec::new();
    let mut malformed_named_candidates = Vec::new();
    let mut malformed_named_omitted = 0_usize;
    for candidate in candidate_paths {
        charge_diagnostic_import_work(&mut diagnostic_work, 1)?;
        let bytes = if let Some(overlay_bytes) = import_overlays.get(&candidate) {
            if !allowed_roots.iter().any(|root| candidate.starts_with(root)) {
                return Err(anyhow!(
                    "canonical .axi import `{}` escapes configured search roots",
                    candidate.display()
                ));
            }
            if overlay_bytes.len() as u64 > crate::security::MAX_AXI_MODULE_BYTES as u64 {
                return Err(anyhow!(
                    "unsaved canonical .axi import overlay `{}` exceeds {} bytes",
                    candidate.display(),
                    crate::security::MAX_AXI_MODULE_BYTES
                ));
            }
            overlay_bytes.clone()
        } else {
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
            crate::security::read_file_bounded(
                &candidate,
                crate::security::MAX_AXI_MODULE_BYTES,
                "imported canonical .axi module",
            )?
        };
        let source = match CanonicalModuleSource::parse(bytes) {
            Ok(source) => source,
            Err(error) => {
                if candidate
                    .file_name()
                    .is_some_and(|name| name == expected_file_name.as_str())
                {
                    if malformed_named_candidates.len() < 8 {
                        malformed_named_candidates
                            .push(bounded_named_candidate_error(&candidate, &error));
                    } else {
                        malformed_named_omitted = malformed_named_omitted.saturating_add(1);
                    }
                }
                continue;
            }
        };
        if source.parsed().module_name == import {
            match_count = match_count.saturating_add(1);
            if matched_path_sample.len() < 8 {
                matched_path_sample.push(bounded_text(&candidate.display().to_string(), 384));
            }
            if first_match.is_none() {
                first_match = Some((candidate, source));
            }
        }
    }
    match match_count {
        0 if !malformed_named_candidates.is_empty() => Err(anyhow!(
            "cannot resolve imported module `{import}` from `{}`; named candidate parse failure(s): {}{}",
            root_input.display(),
            malformed_named_candidates.join("; "),
            if malformed_named_omitted == 0 {
                String::new()
            } else {
                format!("; {malformed_named_omitted} additional named candidate(s) omitted")
            }
        )),
        0 => Err(anyhow!(
            "cannot resolve imported module `{import}` from `{}`",
            root_input.display()
        )),
        1 => first_match.ok_or_else(|| anyhow!("resolved import disappeared before use")),
        _ => {
            let retained = matched_path_sample.join(", ");
            let omitted = match_count.saturating_sub(8);
            Err(anyhow!(
                "imported module `{import}` is ambiguous: {retained}{}",
                if omitted == 0 {
                    String::new()
                } else {
                    format!(", {omitted} additional candidate(s) omitted")
                }
            ))
        }
    }
}

fn expected_file_name_for(import: &str) -> String {
    format!("{import}.axi")
}

fn charge_diagnostic_import_work(
    work: &mut Option<&mut DiagnosticImportWorkBudget>,
    amount: usize,
) -> Result<()> {
    if work.as_deref_mut().is_some_and(|work| !work.charge(amount)) {
        return Err(anyhow!("diagnostic import-resolution work bound exhausted"));
    }
    Ok(())
}

fn bounded_named_candidate_error(
    path: &Path,
    error: &axiograph_kernel::KernelCompileError,
) -> String {
    let path = bounded_text(&path.display().to_string(), 192);
    match error {
        axiograph_kernel::KernelCompileError::Parse { line, message } => {
            let prefix = format!("{path}: parse failure at line {line}: ");
            let remaining = 384_usize.saturating_sub(prefix.len());
            format!("{prefix}{}", bounded_text(message, remaining))
        }
        _ => bounded_text(&format!("{path}: {error}"), 384),
    }
}

fn bounded_text(detail: &str, max_bytes: usize) -> String {
    if detail.len() <= max_bytes {
        return detail.to_string();
    }
    let suffix = " [truncated]";
    if max_bytes < suffix.len() {
        return String::new();
    }
    let mut end = max_bytes.saturating_sub(suffix.len()).min(detail.len());
    while !detail.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{suffix}", &detail[..end])
}

struct BoundedDisplay {
    text: String,
    max_bytes: usize,
}

impl std::fmt::Write for BoundedDisplay {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        if self.text.len().saturating_add(value.len()) > self.max_bytes {
            return Err(std::fmt::Error);
        }
        self.text.push_str(value);
        Ok(())
    }
}

fn bounded_display(value: &impl Display, max_bytes: usize) -> String {
    let mut output = BoundedDisplay {
        text: String::with_capacity(max_bytes.min(1024)),
        max_bytes,
    };
    if write!(&mut output, "{value}").is_err() {
        let suffix = " [truncated]";
        while output.text.len().saturating_add(suffix.len()) > max_bytes {
            output.text.pop();
        }
        if suffix.len() <= max_bytes {
            output.text.push_str(suffix);
        }
    }
    output.text
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
        assert!(err
            .to_string()
            .contains("module header must be the first canonical header"));

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
