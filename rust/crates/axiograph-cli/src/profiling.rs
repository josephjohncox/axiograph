//! Optional CPU profiling helpers (feature-gated).

#[cfg(feature = "profiling")]
mod enabled {
    use anyhow::{anyhow, Result};
    use clap::{Args, ValueEnum};
    use pprof::protos::Message;
    use std::collections::{BTreeMap, HashMap};
    use std::fs;
    use std::io::{BufWriter, Write};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ProfileFormat {
        Off,
        Flamegraph,
        #[value(alias = "profile")]
        Pprof,
        #[value(alias = "callstack", alias = "stacks")]
        Folded,
        All,
    }

    #[derive(Args, Debug, Clone)]
    pub struct ProfileArgs {
        /// Enable CPU profiling (feature `profiling`).
        ///
        /// Values: off|flamegraph|pprof|folded|all. `--profile` alone defaults to flamegraph.
        #[arg(
            long,
            value_enum,
            default_value_t = ProfileFormat::Off,
            default_missing_value = "flamegraph",
            num_args = 0..=1,
            global = true
        )]
        pub profile: ProfileFormat,

        /// Output path for the profile artifact (file or base path).
        ///
        /// If `--profile=all`, extensions are added per output kind.
        #[arg(long, global = true)]
        pub profile_out: Option<PathBuf>,

        /// Sampling frequency (Hz) for CPU profiling.
        #[arg(long, default_value_t = 99, global = true)]
        pub profile_hz: usize,

        /// Emit periodic snapshots every N seconds while running.
        #[arg(long, global = true)]
        pub profile_interval: Option<u64>,

        /// Emit a snapshot on SIGUSR2 (Unix only).
        #[arg(long, global = true)]
        pub profile_signal: bool,

        /// Output format for live snapshots (interval/signal). Defaults to `pprof`.
        #[arg(long, value_enum, global = true)]
        pub profile_live_format: Option<ProfileFormat>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum OutputKind {
        Flamegraph,
        Pprof,
        Folded,
    }

    impl OutputKind {
        fn ext(self) -> &'static str {
            match self {
                OutputKind::Flamegraph => "svg",
                OutputKind::Pprof => "pb",
                OutputKind::Folded => "folded",
            }
        }
    }

    impl ProfileFormat {
        fn outputs(self) -> Vec<OutputKind> {
            match self {
                ProfileFormat::Off => Vec::new(),
                ProfileFormat::Flamegraph => vec![OutputKind::Flamegraph],
                ProfileFormat::Pprof => vec![OutputKind::Pprof],
                ProfileFormat::Folded => vec![OutputKind::Folded],
                ProfileFormat::All => vec![
                    OutputKind::Flamegraph,
                    OutputKind::Pprof,
                    OutputKind::Folded,
                ],
            }
        }
    }

    pub struct Profiler {
        guard: Arc<Mutex<pprof::ProfilerGuard<'static>>>,
        format: ProfileFormat,
        live_format: ProfileFormat,
        out_base: PathBuf,
        counter: Arc<AtomicUsize>,
        stop_tx: Option<mpsc::Sender<SnapshotRequest>>,
        worker: Option<thread::JoinHandle<()>>,
    }

    impl Profiler {
        pub fn start(args: &ProfileArgs) -> Result<Option<Self>> {
            if args.profile == ProfileFormat::Off {
                return Ok(None);
            }

            let hz = i32::try_from(args.profile_hz.max(1))
                .unwrap_or(i32::MAX);
            let guard = pprof::ProfilerGuard::new(hz)
                .map_err(|e| anyhow!("failed to start profiler: {e}"))?;

            let run_id = run_id_string();
            let out_base = match args.profile_out.clone() {
                Some(path) => append_run_id(&path, &run_id),
                None => default_out_base(),
            };
            let guard = Arc::new(Mutex::new(guard));
            let counter = Arc::new(AtomicUsize::new(0));
            let live_format = args.profile_live_format.unwrap_or_else(|| {
                if args.profile_interval.is_some() || args.profile_signal {
                    ProfileFormat::Pprof
                } else {
                    args.profile
                }
            });

            let mut profiler = Self {
                guard: Arc::clone(&guard),
                format: args.profile,
                live_format,
                out_base,
                counter,
                stop_tx: None,
                worker: None,
            };

            profiler.spawn_worker(args)?;
            Ok(Some(profiler))
        }

        pub fn finish(self) -> Result<Vec<PathBuf>> {
            if let Some(tx) = self.stop_tx.as_ref() {
                let _ = tx.send(SnapshotRequest::Stop);
            }
            if let Some(worker) = self.worker {
                let _ = worker.join();
            }

            let outputs = self.format.outputs();
            if outputs.is_empty() {
                return Ok(Vec::new());
            }

            let report = {
                let guard = self
                    .guard
                    .lock()
                    .map_err(|_| anyhow!("failed to lock profiler guard"))?;
                guard
                    .report()
                    .build()
                    .map_err(|e| anyhow!("failed to build profile report: {e}"))?
            };

            write_report(&report, self.format, &self.out_base)
        }

        fn spawn_worker(&mut self, args: &ProfileArgs) -> Result<()> {
            let interval = args
                .profile_interval
                .and_then(|secs| if secs > 0 { Some(Duration::from_secs(secs)) } else { None });
            let want_signal = args.profile_signal;

            if self.live_format == ProfileFormat::Off || (interval.is_none() && !want_signal) {
                return Ok(());
            }

            let (tx, rx) = mpsc::channel::<SnapshotRequest>();
            self.stop_tx = Some(tx.clone());

            if want_signal {
                #[cfg(unix)]
                spawn_signal_listener(tx.clone())?;
                #[cfg(not(unix))]
                eprintln!("profile: --profile-signal is only supported on Unix");
            }

            let guard = Arc::clone(&self.guard);
            let out_base = self.out_base.clone();
            let format = self.live_format;
            let counter = Arc::clone(&self.counter);

            let worker = thread::spawn(move || {
                loop {
                    let recv_result = match interval {
                        Some(dur) => rx.recv_timeout(dur),
                        None => rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected),
                    };

                    match recv_result {
                        Ok(SnapshotRequest::Stop) => break,
                        Ok(SnapshotRequest::Signal) => {
                            if let Err(err) = write_snapshot(
                                &guard,
                                &out_base,
                                format,
                                &counter,
                                SnapshotKind::Signal,
                            ) {
                                eprintln!("profile: {err}");
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if interval.is_some() {
                                if let Err(err) = write_snapshot(
                                    &guard,
                                    &out_base,
                                    format,
                                    &counter,
                                    SnapshotKind::Interval,
                                ) {
                                    eprintln!("profile: {err}");
                                }
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            });

            self.worker = Some(worker);
            Ok(())
        }
    }

    fn default_out_base() -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        PathBuf::from("build/profiles").join(format!("profile-{ts}"))
    }

    fn run_id_string() -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs();
        let millis = now.subsec_millis();
        format!("{secs}-{millis:03}")
    }

    fn append_run_id(base: &Path, run_id: &str) -> PathBuf {
        let parent = base.parent().unwrap_or_else(|| Path::new("."));
        let stem = base
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("profile");
        let suffix = format!("{stem}-{run_id}");
        if let Some(ext) = base.extension().and_then(|e| e.to_str()) {
            parent.join(format!("{suffix}.{ext}"))
        } else {
            parent.join(suffix)
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum SnapshotRequest {
        Stop,
        Signal,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum SnapshotKind {
        Interval,
        Signal,
    }

    fn snapshot_base(base: &Path, kind: SnapshotKind, idx: usize) -> PathBuf {
        let parent = base.parent().unwrap_or_else(|| Path::new("."));
        let stem = base
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("profile");
        let suffix = match kind {
            SnapshotKind::Interval => "interval",
            SnapshotKind::Signal => "signal",
        };
        parent.join(format!("{stem}.{suffix}-{idx:04}"))
    }

    fn output_path(base: &Path, kind: OutputKind, force_ext: bool) -> PathBuf {
        if !force_ext {
            if base.extension().is_some() {
                return base.to_path_buf();
            }
        }
        let mut path = base.to_path_buf();
        path.set_extension(kind.ext());
        path
    }

    fn write_snapshot(
        guard: &Arc<Mutex<pprof::ProfilerGuard<'static>>>,
        out_base: &Path,
        format: ProfileFormat,
        counter: &Arc<AtomicUsize>,
        kind: SnapshotKind,
    ) -> Result<Vec<PathBuf>> {
        let report = {
            let guard = guard
                .lock()
                .map_err(|_| anyhow!("failed to lock profiler guard"))?;
            guard
                .report()
                .build()
                .map_err(|e| anyhow!("failed to build profile report: {e}"))?
        };

        let idx = counter.fetch_add(1, Ordering::SeqCst) + 1;
        let base = snapshot_base(out_base, kind, idx);
        write_report(&report, format, &base)
    }

    fn write_report(
        report: &pprof::Report,
        format: ProfileFormat,
        out_base: &Path,
    ) -> Result<Vec<PathBuf>> {
        let outputs = format.outputs();
        if outputs.is_empty() {
            return Ok(Vec::new());
        }

        let need_profile = outputs
            .iter()
            .any(|k| matches!(k, OutputKind::Pprof | OutputKind::Folded));
        let mut profile: Option<pprof::protos::Profile> = None;

        if need_profile {
            profile = Some(
                report
                    .pprof()
                    .map_err(|e| anyhow!("failed to build pprof output: {e}"))?,
            );
        }

        let force_ext = outputs.len() > 1;
        let mut written = Vec::new();
        for kind in outputs {
            let path = output_path(out_base, kind, force_ext);
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            match kind {
                OutputKind::Flamegraph => {
                    let mut file = fs::File::create(&path)
                        .map_err(|e| anyhow!("failed to create {}: {e}", path.display()))?;
                    report
                        .flamegraph(&mut file)
                        .map_err(|e| anyhow!("failed to write flamegraph: {e}"))?;
                }
                OutputKind::Pprof => {
                    let profile = profile
                        .as_ref()
                        .ok_or_else(|| anyhow!("pprof profile missing (unexpected state)"))?;
                    let file = fs::File::create(&path)
                        .map_err(|e| anyhow!("failed to create {}: {e}", path.display()))?;
                    let mut buf = Vec::new();
                    profile
                        .encode(&mut buf)
                        .map_err(|e| anyhow!("failed to encode pprof: {e}"))?;
                    let mut writer = BufWriter::new(file);
                    writer
                        .write_all(&buf)
                        .map_err(|e| anyhow!("failed to write pprof: {e}"))?;
                }
                OutputKind::Folded => {
                    let profile = profile
                        .as_ref()
                        .ok_or_else(|| anyhow!("pprof profile missing (unexpected state)"))?;
                    write_folded(profile, &path)?;
                }
            }
            written.push(path);
        }

        Ok(written)
    }

    #[cfg(unix)]
    fn spawn_signal_listener(tx: mpsc::Sender<SnapshotRequest>) -> Result<()> {
        use signal_hook::consts::SIGUSR2;
        use signal_hook::iterator::Signals;

        let mut signals =
            Signals::new([SIGUSR2]).map_err(|e| anyhow!("failed to register SIGUSR2: {e}"))?;
        thread::spawn(move || {
            for _ in signals.forever() {
                let _ = tx.send(SnapshotRequest::Signal);
            }
        });
        Ok(())
    }

    fn write_folded(profile: &pprof::protos::Profile, path: &Path) -> Result<()> {
        let mut func_names: HashMap<u64, String> = HashMap::new();
        for func in &profile.function {
            let name = profile
                .string_table
                .get(func.name as usize)
                .cloned()
                .unwrap_or_default();
            func_names.insert(func.id, name);
        }

        let mut loc_frames: HashMap<u64, Vec<String>> = HashMap::new();
        for loc in &profile.location {
            let mut frames = Vec::new();
            for line in &loc.line {
                if let Some(name) = func_names.get(&line.function_id) {
                    frames.push(name.clone());
                }
            }
            if !frames.is_empty() {
                loc_frames.insert(loc.id, frames);
            }
        }

        let mut stacks: BTreeMap<String, i64> = BTreeMap::new();
        for sample in &profile.sample {
            let value = sample.value.get(0).copied().unwrap_or(1);
            if value <= 0 {
                continue;
            }
            let mut stack: Vec<String> = Vec::new();
            for loc_id in &sample.location_id {
                if let Some(frames) = loc_frames.get(loc_id) {
                    stack.extend(frames.iter().cloned());
                }
            }
            if stack.is_empty() {
                continue;
            }
            // pprof stack order is leaf->root; folded format expects root->leaf.
            stack.reverse();
            let key = stack.join(";");
            *stacks.entry(key).or_insert(0) += value;
        }

        let file = fs::File::create(path)
            .map_err(|e| anyhow!("failed to create {}: {e}", path.display()))?;
        let mut writer = BufWriter::new(file);
        for (stack, value) in stacks {
            writeln!(writer, "{} {}", stack, value)
                .map_err(|e| anyhow!("failed to write folded stacks: {e}"))?;
        }
        Ok(())
    }
}

#[cfg(not(feature = "profiling"))]
mod disabled {
    use anyhow::Result;
    use clap::Args;
    use std::path::PathBuf;

    #[derive(Args, Debug, Clone, Default)]
    pub struct ProfileArgs {}

    pub struct Profiler;

    impl Profiler {
        pub fn start(_args: &ProfileArgs) -> Result<Option<Self>> {
            Ok(None)
        }

        pub fn finish(self) -> Result<Vec<PathBuf>> {
            Ok(Vec::new())
        }
    }
}

#[cfg(feature = "profiling")]
pub use enabled::*;
#[cfg(not(feature = "profiling"))]
pub use disabled::*;
