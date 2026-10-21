//! Explicit, per-user GitHub updates. Network/native work stays off egui's thread.
//! The ZIP supplies an app and data only; the installer is this application's
//! own fixed script. A checksum verifies transfer integrity, not an independent
//! publisher signature: the public GitHub repository remains the trust source.
use crate::i18n::LocalizedMessage;
use crate::version::{APP_VERSION, COMPACT_REVISION};
use eframe::egui;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const REPOSITORY: &str = "Tangram-Space/updates";
// This repository also publishes other apps. Never use its global latest
// release: this fixed channel selects Turner's release, which is then verified
// independently through GitHub's exact-tag API and its versioned asset names.
const FEED_URL: &str =
    "https://raw.githubusercontent.com/Tangram-Space/updates/main/feeds/turner/stable/release.json";
const APP_ID: &str = "turner";
const CHANNEL: &str = "stable";
const BUNDLE_ID: &str = "com.tuner.guitar.compact";
const APP_NAME: &str = "Turner.app";
const EXECUTABLE: &str = "guitar-tuner-egui";
const MANIFEST: &str = "turner-update.json";
const MAX_ZIP: u64 = 256 * 1024 * 1024;
const MAX_EXPANDED: u64 = 512 * 1024 * 1024;
const MAX_ENTRIES: usize = 8192;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReleaseInfo {
    pub(crate) version: String,
    pub(crate) revision: u64,
}

#[derive(Clone, Debug)]
pub(crate) enum UpdateStatus {
    Idle,
    Checking,
    NoRelease,
    UpToDate,
    Available(ReleaseInfo),
    Downloading { received: u64, total: Option<u64> },
    Preparing,
    ReadyToRestart,
    Cancelled,
    Error(LocalizedMessage),
}

pub(crate) struct UpdateService {
    status: UpdateStatus,
    candidate: Option<Candidate>,
    job: Option<Job>,
}

impl Default for UpdateService {
    fn default() -> Self {
        Self {
            status: UpdateStatus::Idle,
            candidate: None,
            job: None,
        }
    }
}

struct Job {
    events: mpsc::Receiver<Event>,
    commit: Option<mpsc::Sender<()>>,
    cancelled: Arc<AtomicBool>,
    handoff_authorized: bool,
}

enum Event {
    Checked(Option<Candidate>),
    NoRelease,
    Progress(u64, Option<u64>),
    Preparing,
    Prepared,
    Restart,
    Failed(Failure),
}

#[derive(Debug)]
enum Failure {
    Cancelled,
    Message(LocalizedMessage),
}
type Result<T> = std::result::Result<T, Failure>;
impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        Self::Message(LocalizedMessage::new(
            format!("No se pudo completar la operación de actualización: {error}"),
            format!("Could not complete the update operation: {error}"),
        ))
    }
}
fn fail(spanish: impl Into<Cow<'static, str>>, english: impl Into<Cow<'static, str>>) -> Failure {
    Failure::Message(LocalizedMessage::new(spanish, english))
}
fn cancelled(token: &AtomicBool) -> Result<()> {
    if token.load(Ordering::Acquire) {
        Err(Failure::Cancelled)
    } else {
        Ok(())
    }
}

impl UpdateService {
    pub(crate) fn status(&self) -> &UpdateStatus {
        &self.status
    }

    fn busy(&self) -> bool {
        matches!(
            self.status,
            UpdateStatus::Checking
                | UpdateStatus::Downloading { .. }
                | UpdateStatus::Preparing
                | UpdateStatus::ReadyToRestart
        )
    }

    pub(crate) fn check(&mut self, context: &egui::Context) {
        if self.busy() {
            return;
        }
        self.cancel_job();
        self.candidate = None;
        self.status = UpdateStatus::Checking;
        self.spawn(context, None);
    }

    pub(crate) fn install(&mut self, context: &egui::Context) {
        if !matches!(self.status, UpdateStatus::Available(_)) {
            return;
        }
        let Some(candidate) = self.candidate.clone() else {
            return;
        };
        self.cancel_job();
        self.status = UpdateStatus::Downloading {
            received: 0,
            total: Some(candidate.size),
        };
        self.spawn(context, Some(candidate));
    }

    fn spawn(&mut self, context: &egui::Context, candidate: Option<Candidate>) {
        let (events, receiver) = mpsc::channel();
        let (commit, commit_receiver) = mpsc::channel();
        let token = Arc::new(AtomicBool::new(false));
        let worker_token = token.clone();
        let context = context.clone();
        let spawn = thread::Builder::new()
            .name("turner-updates".into())
            .spawn(move || {
                let reporter = Reporter { events, context };
                let outcome = match candidate {
                    Some(candidate) => {
                        install_update(candidate, &worker_token, &reporter, commit_receiver)
                    }
                    None => check_update(&worker_token, &reporter),
                };
                if let Err(error) = outcome {
                    reporter.send(Event::Failed(error));
                }
            });
        match spawn {
            Ok(_) => {
                self.job = Some(Job {
                    events: receiver,
                    commit: Some(commit),
                    cancelled: token,
                    handoff_authorized: false,
                })
            }
            Err(error) => {
                self.status = UpdateStatus::Error(LocalizedMessage::new(
                    format!("No se pudo iniciar la actualización: {error}"),
                    format!("Could not start the update: {error}"),
                ))
            }
        }
    }

    pub(crate) fn cancel(&mut self) {
        // Prepared has already been accepted by the UI and authorized the
        // installer. No Cancel control is shown after this explicit handoff.
        if matches!(self.status, UpdateStatus::ReadyToRestart) {
            return;
        }
        if self.busy() {
            self.cancel_job();
            self.status = UpdateStatus::Cancelled;
        }
    }

    fn cancel_job(&mut self) {
        if let Some(job) = self.job.take() {
            if !job.handoff_authorized {
                job.cancelled.store(true, Ordering::Release);
            }
            // Dropping the commit sender also aborts a prepared-but-unapproved
            // helper. No child/thread is joined from the event-loop thread.
        }
    }

    /// Returns true once, only after the external helper has a committed plan.
    /// The caller closes the ROOT viewport, allowing normal eframe persistence.
    pub(crate) fn poll(&mut self, context: &egui::Context) -> bool {
        let mut messages = Vec::new();
        let mut disconnected = false;
        if let Some(job) = &self.job {
            loop {
                match job.events.try_recv() {
                    Ok(event) => messages.push(event),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        let mut finished = false;
        let mut restart = false;
        for event in messages {
            match event {
                Event::Checked(candidate) => {
                    self.status = candidate
                        .as_ref()
                        .map(|c| UpdateStatus::Available(c.info.clone()))
                        .unwrap_or(UpdateStatus::UpToDate);
                    self.candidate = candidate;
                    finished = true;
                }
                Event::NoRelease => {
                    self.status = UpdateStatus::NoRelease;
                    finished = true;
                }
                Event::Progress(received, total) => {
                    self.status = UpdateStatus::Downloading { received, total }
                }
                Event::Preparing => self.status = UpdateStatus::Preparing,
                Event::Prepared => {
                    if let Some(job) = &mut self.job {
                        if job.cancelled.load(Ordering::Acquire) {
                            continue;
                        }
                        self.status = UpdateStatus::ReadyToRestart;
                        if job
                            .commit
                            .take()
                            .is_some_and(|sender| sender.send(()).is_ok())
                        {
                            job.handoff_authorized = true;
                        } else {
                            self.status = UpdateStatus::Error(LocalizedMessage::new(
                                "El instalador dejó de responder antes del reinicio.",
                                "The installer stopped responding before restart.",
                            ));
                            finished = true;
                        }
                    }
                }
                Event::Restart => {
                    self.status = UpdateStatus::ReadyToRestart;
                    restart = true;
                    finished = true;
                }
                Event::Failed(Failure::Cancelled) => {
                    self.status = UpdateStatus::Cancelled;
                    finished = true;
                }
                Event::Failed(Failure::Message(error)) => {
                    self.status = UpdateStatus::Error(error);
                    finished = true;
                }
            }
        }
        if disconnected && !finished {
            self.status = UpdateStatus::Error(LocalizedMessage::new(
                "La tarea de actualización terminó sin un resultado.",
                "The update task ended without a result.",
            ));
            finished = true;
        }
        if finished {
            self.job = None;
        }
        if self.busy() && !restart {
            context.request_repaint_after_for(Duration::from_millis(50), egui::ViewportId::ROOT);
        }
        restart
    }
}
impl Drop for UpdateService {
    fn drop(&mut self) {
        self.cancel_job();
    }
}

struct Reporter {
    events: mpsc::Sender<Event>,
    context: egui::Context,
}
impl Reporter {
    fn send(&self, event: Event) {
        let _ = self.events.send(event);
        self.context.request_repaint_of(egui::ViewportId::ROOT);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Version(u32, u32, u32);
fn parse_version(value: &str) -> Result<Version> {
    let parts: Vec<_> = value.split('.').collect();
    if parts.len() != 3
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|c| c.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
        })
    {
        return Err(fail(
            "La versión publicada no tiene un formato válido.",
            "The published version has an invalid format.",
        ));
    }
    let number = |part: &str| {
        part.parse::<u32>()
            .map_err(|_| fail("Versión fuera de rango.", "Version out of range."))
    };
    Ok(Version(
        number(parts[0])?,
        number(parts[1])?,
        number(parts[2])?,
    ))
}

#[derive(Clone)]
struct Installed {
    bundle: PathBuf,
    version: String,
    revision: u64,
}
#[derive(Clone)]
struct Candidate {
    info: ReleaseInfo,
    installed: Installed,
    url: String,
    asset: String,
    hash: String,
    size: u64,
}

struct FeedRelease {
    info: ReleaseInfo,
    tag: String,
    asset: String,
    hash: String,
}

fn json(bytes: &[u8]) -> Result<Value> {
    serde_json::from_slice(bytes).map_err(|_| {
        fail(
            "GitHub devolvió datos de actualización inválidos.",
            "GitHub returned invalid update data.",
        )
    })
}
fn field<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    value.get(name).and_then(Value::as_str).ok_or_else(|| {
        fail(
            format!("Falta el campo de actualización {name}."),
            format!("The update field {name} is missing."),
        )
    })
}
fn number(value: &Value, name: &str) -> Result<u64> {
    value.get(name).and_then(Value::as_u64).ok_or_else(|| {
        fail(
            format!("El campo {name} debe ser un entero."),
            format!("The field {name} must be an integer."),
        )
    })
}
fn asset_url(tag: &str, asset: &str) -> String {
    format!("https://github.com/{REPOSITORY}/releases/download/{tag}/{asset}")
}
fn release_api_url(tag: &str) -> Result<String> {
    // Validate before inserting a tag into a URL path. Feed data cannot change
    // the repository, request another app's tag, or insert path/query syntax.
    parse_tag(tag)?;
    Ok(format!(
        "https://api.github.com/repos/{REPOSITORY}/releases/tags/{tag}"
    ))
}
fn parse_feed(feed: &Value) -> Result<FeedRelease> {
    let tag = field(feed, "tag")?;
    let (version, revision) = parse_tag(tag)?;
    let asset = format!("Turner-v{version}-r{revision}-macos-x86_64.app.zip");
    let url = asset_url(tag, &asset);
    let hash = validate_manifest(feed, &version, revision, tag, &asset, &url)?;
    Ok(FeedRelease {
        info: ReleaseInfo { version, revision },
        tag: tag.into(),
        asset,
        hash,
    })
}
fn validate_selected_release(release: &Value, tag: &str) -> Result<()> {
    if release.get("draft") != Some(&Value::Bool(false))
        || release.get("prerelease") != Some(&Value::Bool(false))
        || field(release, "tag_name")? != tag
    {
        return Err(fail(
            "La publicación no corresponde a la versión estable de Turner anunciada por el canal.",
            "The release does not match the stable Turner version announced by the channel.",
        ));
    }
    Ok(())
}
fn validate_manifest_pair(feed: &Value, release_manifest: &Value) -> Result<()> {
    // The publisher uploads this complete JSON as a release asset before
    // advancing the stable feed. Comparing parsed JSON permits formatting
    // differences, while keeping hashes, identity and any provenance equal.
    if feed != release_manifest {
        return Err(fail("El canal estable y el manifiesto de la publicación no coinciden. Volvé a buscar actualizaciones más tarde.", "The stable channel and release manifest do not match. Check for updates again later."));
    }
    Ok(())
}
fn release_asset(release: &Value, tag: &str, name: &str, limit: u64) -> Result<(String, u64)> {
    let assets = release
        .get("assets")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            fail(
                "La publicación no contiene archivos.",
                "The release contains no files.",
            )
        })?;
    let matches: Vec<_> = assets
        .iter()
        .filter(|a| a.get("name").and_then(Value::as_str) == Some(name))
        .collect();
    if matches.len() != 1 {
        return Err(fail(
            format!("La publicación debe contener exactamente un archivo {name}."),
            format!("The release must contain exactly one file named {name}."),
        ));
    }
    let asset = matches[0];
    let url = field(asset, "browser_download_url")?;
    let size = number(asset, "size")?;
    if url != asset_url(tag, name)
        || field(asset, "state")? != "uploaded"
        || size == 0
        || size > limit
    {
        return Err(fail(
            "El archivo publicado no cumple el origen o tamaño esperado.",
            "The published file has an unexpected source or size.",
        ));
    }
    Ok((url.to_string(), size))
}

fn check_update(token: &AtomicBool, reporter: &Reporter) -> Result<()> {
    let temporary = PrivateDir::new(&std::env::temp_dir())?;
    let installed = locate_installed(&temporary.path, token)?;
    let feed_path = temporary.path.join("feed.json");
    if download(
        FEED_URL,
        &feed_path,
        16 * 1024,
        token,
        None,
        &temporary.path,
    )? == 404
    {
        reporter.send(Event::NoRelease);
        return Ok(());
    }
    let feed = json(&fs::read(&feed_path)?)?;
    // Validate every app/channel/identity field before making a request whose
    // path depends on downloaded data. The feed is the only moving pointer.
    let selected = parse_feed(&feed)?;
    let response = temporary.path.join("release.json");
    require_download(
        &release_api_url(&selected.tag)?,
        &response,
        1024 * 1024,
        token,
        None,
        &temporary.path,
    )?;
    let release = json(&fs::read(&response)?)?;
    let tag = selected.tag.as_str();
    validate_selected_release(&release, tag)?;
    let (manifest_url, _) = release_asset(&release, tag, MANIFEST, 16 * 1024)?;
    let manifest_path = temporary.path.join("manifest.json");
    require_download(
        &manifest_url,
        &manifest_path,
        16 * 1024,
        token,
        None,
        &temporary.path,
    )?;
    let manifest = json(&fs::read(&manifest_path)?)?;
    validate_manifest_pair(&feed, &manifest)?;
    let (url, size) = release_asset(&release, tag, &selected.asset, MAX_ZIP)?;
    if let Some(digest) = release["assets"]
        .as_array()
        .and_then(|assets| {
            assets
                .iter()
                .find(|a| a["name"].as_str() == Some(&selected.asset))
        })
        .and_then(|a| a["digest"].as_str())
    {
        if digest != format!("sha256:{}", selected.hash) {
            return Err(fail(
                "El hash del manifiesto no coincide con el publicado por GitHub.",
                "The manifest hash does not match the hash published by GitHub.",
            ));
        }
    }
    let checksum_name = format!("{}.sha256", selected.asset);
    let (checksum_url, _) = release_asset(&release, tag, &checksum_name, 1024)?;
    let checksum = temporary.path.join("checksum.txt");
    require_download(&checksum_url, &checksum, 1024, token, None, &temporary.path)?;
    if parse_checksum(&fs::read(&checksum)?, &selected.asset)? != selected.hash {
        return Err(fail(
            "El manifiesto y el archivo SHA256 no coinciden.",
            "The manifest and SHA256 file do not match.",
        ));
    }
    cancelled(token)?;
    if selected.info.revision <= installed.revision {
        reporter.send(Event::Checked(None));
    } else {
        if parse_version(&selected.info.version)? < parse_version(&installed.version)? {
            return Err(fail(
                "La publicación intenta retroceder la versión instalada.",
                "The release would downgrade the installed version.",
            ));
        }
        reporter.send(Event::Checked(Some(Candidate {
            info: selected.info,
            installed,
            url,
            asset: selected.asset,
            hash: selected.hash,
            size,
        })));
    }
    Ok(())
}

fn parse_tag(tag: &str) -> Result<(String, u64)> {
    let value = tag.strip_prefix("turner-v").ok_or_else(|| {
        fail(
            "La etiqueta publicada no corresponde a Turner.",
            "The published tag does not belong to Turner.",
        )
    })?;
    let (version, revision) = value.rsplit_once("-r").ok_or_else(|| {
        fail(
            "Falta la revisión de Turner.",
            "The Turner revision is missing.",
        )
    })?;
    parse_version(version)?;
    if revision.is_empty()
        || !revision.bytes().all(|b| b.is_ascii_digit())
        || revision.starts_with('0')
    {
        return Err(fail("Revisión inválida.", "Invalid revision."));
    }
    let revision = revision
        .parse::<u32>()
        .map_err(|_| fail("Revisión fuera de rango.", "Revision out of range."))?;
    Ok((version.into(), revision.into()))
}
fn validate_manifest(
    manifest: &Value,
    version: &str,
    revision: u64,
    tag: &str,
    asset: &str,
    url: &str,
) -> Result<String> {
    for (key, expected) in [
        ("app_id", APP_ID),
        ("channel", CHANNEL),
        ("version", version),
        ("tag", tag),
        ("platform", "macos"),
        ("arch", "x86_64"),
        ("minimum_macos", "11.0"),
        ("asset", asset),
        ("url", url),
        ("bundle_id", BUNDLE_ID),
        ("executable", EXECUTABLE),
    ] {
        if field(manifest, key)? != expected {
            return Err(fail(
                format!("El manifiesto tiene un valor inesperado en {key}."),
                format!("The manifest has an unexpected value for {key}."),
            ));
        }
    }
    if number(manifest, "schema_version")? != 1 || number(manifest, "revision")? != revision {
        return Err(fail(
            "El esquema o la revisión del manifiesto no coinciden.",
            "The manifest schema or revision does not match.",
        ));
    }
    normalize_hash(field(manifest, "sha256")?)
}
fn normalize_hash(hash: &str) -> Result<String> {
    if hash.len() != 64 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(fail(
            "El SHA256 publicado no es válido.",
            "The published SHA256 is invalid.",
        ));
    }
    Ok(hash.to_ascii_lowercase())
}
fn parse_checksum(bytes: &[u8], asset: &str) -> Result<String> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        fail(
            "El archivo SHA256 no es texto válido.",
            "The SHA256 file is not valid text.",
        )
    })?;
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let line = lines
        .next()
        .ok_or_else(|| fail("El archivo SHA256 está vacío.", "The SHA256 file is empty."))?;
    if lines.next().is_some() {
        return Err(fail(
            "El archivo SHA256 debe describir un único ZIP.",
            "The SHA256 file must describe a single ZIP.",
        ));
    }
    let (hash, name) = line.split_once("  ").ok_or_else(|| {
        fail(
            "Formato de archivo SHA256 inesperado.",
            "Unexpected SHA256 file format.",
        )
    })?;
    if name != asset {
        return Err(fail(
            "El SHA256 corresponde a otro archivo.",
            "The SHA256 belongs to another file.",
        ));
    }
    normalize_hash(hash)
}

struct PrivateDir {
    path: PathBuf,
    keep: bool,
}
impl PrivateDir {
    fn new(parent: &Path) -> Result<Self> {
        for _ in 0..16 {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(
                ".turner-update-{}-{nonce:x}-{sequence:x}",
                std::process::id()
            ));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            builder.mode(0o700);
            match builder.create(&path) {
                Ok(()) => return Ok(Self { path, keep: false }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(fail(format!("No se puede crear la carpeta de actualización en {}: {error}. Si la app está en un volumen de sólo lectura, movela a Aplicaciones o a una carpeta propia.", parent.display()), format!("Could not create the update folder in {}: {error}. If the app is on a read-only volume, move it to Applications or a folder you own.", parent.display()))),
            }
        }
        Err(fail(
            "No se pudo crear una carpeta temporal privada.",
            "Could not create a private temporary folder.",
        ))
    }
}
impl Drop for PrivateDir {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
fn new_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    Ok(options.open(path)?)
}
struct Output {
    success: bool,
    stdout: Vec<u8>,
    stderr: String,
}

/// The output goes to private files rather than pipes, so an unexpectedly
/// chatty native utility cannot deadlock while the worker waits for exit.
fn run(
    program: &str,
    args: &[OsString],
    scratch: &Path,
    token: &AtomicBool,
    timeout: Duration,
    progress: Option<(&Path, u64, Option<(&Reporter, u64)>)>,
) -> Result<Output> {
    cancelled(token)?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let stdout_path = scratch.join(format!("command-{sequence}.out"));
    let stderr_path = scratch.join(format!("command-{sequence}.err"));
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(new_file(&stdout_path)?))
        .stderr(Stdio::from(new_file(&stderr_path)?))
        .spawn()
        .map_err(|error| {
            fail(
                format!("No se pudo ejecutar {program}: {error}"),
                format!("Could not run {program}: {error}"),
            )
        })?;
    let deadline = Instant::now() + timeout;
    let mut last_progress = u64::MAX;
    loop {
        let cancellation = cancelled(token);
        let oversized_output = fs::metadata(&stdout_path)
            .map(|m| m.len() > 1024 * 1024)
            .unwrap_or(false)
            || fs::metadata(&stderr_path)
                .map(|m| m.len() > 64 * 1024)
                .unwrap_or(false);
        let mut oversized_download = false;
        if let Some((path, limit, reporter)) = progress {
            let received = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            oversized_download = received > limit;
            if received != last_progress {
                last_progress = received;
                if let Some((reporter, total)) = reporter {
                    reporter.send(Event::Progress(received, Some(total)));
                }
            }
        }
        if cancellation.is_err()
            || Instant::now() >= deadline
            || oversized_output
            || oversized_download
        {
            let _ = child.kill();
            let _ = child.wait();
            cancellation?;
            return Err(fail(
                if oversized_output || oversized_download {
                    "La actualización excedió el límite de tamaño permitido."
                } else {
                    "La operación agotó su tiempo de espera. Podés volver a intentarlo."
                },
                if oversized_output || oversized_download {
                    "The update exceeded the permitted size limit."
                } else {
                    "The operation timed out. You can try again."
                },
            ));
        }
        if let Some(status) = child.try_wait()? {
            let stdout = fs::read(&stdout_path)?;
            let stderr = fs::read(&stderr_path)?;
            let _ = fs::remove_file(&stdout_path);
            let _ = fs::remove_file(&stderr_path);
            return Ok(Output {
                success: status.success(),
                stdout,
                stderr: String::from_utf8_lossy(&stderr)
                    .chars()
                    .take(2000)
                    .collect(),
            });
        }
        thread::sleep(Duration::from_millis(50));
    }
}
fn args(values: &[&OsStr]) -> Vec<OsString> {
    values.iter().map(|value| value.to_os_string()).collect()
}
fn checked_run(
    program: &str,
    arguments: &[OsString],
    scratch: &Path,
    token: &AtomicBool,
    timeout: Duration,
) -> Result<Vec<u8>> {
    let output = run(program, arguments, scratch, token, timeout, None)?;
    if !output.success {
        return Err(fail(
            format!("Falló {program}: {}", output.stderr.trim()),
            format!("{program} failed: {}", output.stderr.trim()),
        ));
    }
    Ok(output.stdout)
}
fn download(
    url: &str,
    destination: &Path,
    limit: u64,
    token: &AtomicBool,
    reporter: Option<(&Reporter, u64)>,
    scratch: &Path,
) -> Result<u16> {
    let agent = format!("Turner/{APP_VERSION}-r{COMPACT_REVISION}");
    let arguments = args(&[
        OsStr::new("-q"),
        OsStr::new("--silent"),
        OsStr::new("--show-error"),
        OsStr::new("--location"),
        OsStr::new("--proto"),
        OsStr::new("=https"),
        OsStr::new("--proto-redir"),
        OsStr::new("=https"),
        OsStr::new("--tlsv1.2"),
        OsStr::new("--max-redirs"),
        OsStr::new("5"),
        OsStr::new("--connect-timeout"),
        OsStr::new("15"),
        OsStr::new("--max-time"),
        OsStr::new("180"),
        OsStr::new("--max-filesize"),
        OsStr::new(&limit.to_string()),
        OsStr::new("--user-agent"),
        OsStr::new(&agent),
        OsStr::new("--header"),
        OsStr::new("Accept: application/vnd.github+json"),
        OsStr::new("--output"),
        destination.as_os_str(),
        OsStr::new("--write-out"),
        OsStr::new("%{http_code}"),
        OsStr::new(url),
    ]);
    let output = run(
        "/usr/bin/curl",
        &arguments,
        scratch,
        token,
        Duration::from_secs(190),
        Some((destination, limit, reporter)),
    )?;
    if !output.success {
        return Err(fail(
            format!(
                "No se pudo descargar la actualización: {}",
                output.stderr.trim()
            ),
            format!("Could not download the update: {}", output.stderr.trim()),
        ));
    }
    if fs::metadata(destination)?.len() > limit {
        return Err(fail(
            "El archivo descargado es demasiado grande.",
            "The downloaded file is too large.",
        ));
    }
    let status = std::str::from_utf8(&output.stdout)
        .unwrap_or("")
        .trim()
        .parse::<u16>()
        .map_err(|_| fail("Respuesta HTTPS inesperada.", "Unexpected HTTPS response."))?;
    match status {
        200 | 404 => Ok(status),
        403 | 429 => Err(fail(
            format!(
            "GitHub rechazó temporalmente la consulta (HTTP {status}). Probá de nuevo más tarde."
        ),
            format!("GitHub temporarily rejected the request (HTTP {status}). Try again later."),
        )),
        _ => Err(fail(
            format!("GitHub devolvió HTTP {status}; no se modificó la aplicación."),
            format!("GitHub returned HTTP {status}; the application was not modified."),
        )),
    }
}
fn require_download(
    url: &str,
    destination: &Path,
    limit: u64,
    token: &AtomicBool,
    reporter: Option<(&Reporter, u64)>,
    scratch: &Path,
) -> Result<()> {
    if download(url, destination, limit, token, reporter, scratch)? != 200 {
        return Err(fail(
            "La publicación está incompleta: falta un archivo de actualización.",
            "The release is incomplete: an update file is missing.",
        ));
    }
    Ok(())
}
fn plist_value(bundle: &Path, key: &str, scratch: &Path, token: &AtomicBool) -> Result<String> {
    let command = format!("Print :{key}");
    let plist = bundle.join("Contents/Info.plist");
    let value = checked_run(
        "/usr/libexec/PlistBuddy",
        &args(&[OsStr::new("-c"), OsStr::new(&command), plist.as_os_str()]),
        scratch,
        token,
        Duration::from_secs(10),
    )?;
    String::from_utf8(value)
        .map(|value| value.trim().to_string())
        .map_err(|_| {
            fail(
                "El Info.plist no contiene texto válido.",
                "Info.plist does not contain valid text.",
            )
        })
}
fn locate_installed(scratch: &Path, token: &AtomicBool) -> Result<Installed> {
    if !cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        return Err(fail(
            "Las actualizaciones integradas están disponibles en la app para Mac Intel.",
            "Built-in updates are available in the Intel Mac app.",
        ));
    }
    let executable = fs::canonicalize(std::env::current_exe()?)?;
    if executable
        .components()
        .any(|part| part.as_os_str() == "AppTranslocation")
    {
        return Err(fail("macOS está ejecutando una copia translocada. Cerrá Turner, mové su archivo .app a Aplicaciones (o ~/Applications) y abrilo desde allí antes de actualizar.", "macOS is running a translocated copy. Quit Turner, move its .app to Applications (or ~/Applications), then open it from there before updating."));
    }
    let bundle = bundle_from_executable(&executable)?;
    validate_bundle(&bundle, APP_VERSION, COMPACT_REVISION, scratch, token)?;
    Ok(Installed {
        bundle,
        version: APP_VERSION.into(),
        revision: COMPACT_REVISION,
    })
}
fn bundle_from_executable(executable: &Path) -> Result<PathBuf> {
    // A legacy bridge can still live in Tuner Compact.app, and a user may have
    // renamed the bundle. The executable path and bundle metadata establish
    // identity; installation continues to replace this existing path.
    let bundle = executable
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| {
            fail(
                "No se encontró el bundle de Turner.",
                "The Turner app bundle was not found.",
            )
        })?
        .to_path_buf();
    if bundle.extension() != Some(OsStr::new("app"))
        || executable != bundle.join("Contents/MacOS").join(EXECUTABLE)
    {
        return Err(fail(
            "La actualización requiere ejecutar Turner desde su archivo .app.",
            "To update, run Turner from its .app bundle.",
        ));
    }
    Ok(bundle)
}
fn validate_bundle(
    bundle: &Path,
    version: &str,
    revision: u64,
    scratch: &Path,
    token: &AtomicBool,
) -> Result<()> {
    check_tree(bundle, token)?;
    for (key, expected) in [
        ("CFBundleIdentifier", BUNDLE_ID),
        ("CFBundleExecutable", EXECUTABLE),
        ("CFBundleShortVersionString", version),
        ("CFBundleVersion", &revision.to_string()),
        ("LSMinimumSystemVersion", "11.0"),
    ] {
        if plist_value(bundle, key, scratch, token)? != expected {
            return Err(fail(
                format!("La app no coincide con la identidad o versión esperada ({key})."),
                format!("The app does not match the expected identity or version ({key})."),
            ));
        }
    }
    if plist_value(bundle, "NSMicrophoneUsageDescription", scratch, token)?.is_empty() {
        return Err(fail(
            "La app publicada no declara el uso del micrófono.",
            "The published app does not declare microphone use.",
        ));
    }
    let executable = bundle.join("Contents/MacOS").join(EXECUTABLE);
    #[cfg(unix)]
    if fs::metadata(&executable)?.permissions().mode() & 0o111 == 0 {
        return Err(fail(
            "El ejecutable publicado no tiene permiso de ejecución.",
            "The published executable does not have execute permission.",
        ));
    }
    // Only the header and load commands are needed; do not allocate the
    // complete executable merely to check its platform/deployment target.
    let mut header = Vec::new();
    File::open(executable)?
        .take(16 * 1024 * 1024 + 32)
        .read_to_end(&mut header)?;
    validate_macho(&header)
}
fn check_tree(root: &Path, token: &AtomicBool) -> Result<()> {
    let mut pending = vec![root.to_path_buf()];
    let mut size = 0u64;
    let mut entries = 0usize;
    while let Some(path) = pending.pop() {
        cancelled(token)?;
        entries += 1;
        if entries > MAX_ENTRIES {
            return Err(fail(
                "La app contiene demasiados archivos.",
                "The app contains too many files.",
            ));
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || (!metadata.is_dir() && !metadata.is_file()) {
            return Err(fail(
                "El paquete contiene enlaces o tipos de archivo no permitidos.",
                "The package contains links or file types that are not permitted.",
            ));
        }
        if metadata.is_dir() {
            for entry in fs::read_dir(path)? {
                pending.push(entry?.path());
            }
        } else {
            size = size
                .checked_add(metadata.len())
                .ok_or_else(|| fail("Tamaño inválido.", "Invalid size."))?;
        }
        if size > MAX_EXPANDED {
            return Err(fail(
                "La app descomprimida excede el límite permitido.",
                "The extracted app exceeds the permitted size limit.",
            ));
        }
    }
    Ok(())
}
fn hash_file(path: &Path, token: &AtomicBool) -> Result<String> {
    let mut input = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        cancelled(token)?;
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn bytes_at(data: &[u8], start: usize, length: usize) -> Result<&[u8]> {
    let end = start
        .checked_add(length)
        .ok_or_else(|| fail("Archivo binario mal formado.", "Malformed binary file."))?;
    data.get(start..end)
        .ok_or_else(|| fail("Archivo binario truncado.", "Truncated binary file."))
}
fn u16_at(data: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        bytes_at(data, offset, 2)?.try_into().unwrap(),
    ))
}
fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes_at(data, offset, 4)?.try_into().unwrap(),
    ))
}

/// Accept only a thin Intel executable whose load commands declare macOS 11.0
/// or earlier. This does not depend on Xcode tools being installed on the Mac.
fn validate_macho(data: &[u8]) -> Result<()> {
    if u32_at(data, 0)? != 0xfeedfacf || u32_at(data, 4)? != 0x01000007 || u32_at(data, 12)? != 2 {
        return Err(fail(
            "El paquete no contiene un ejecutable Mach-O Intel x86_64.",
            "The package does not contain an Intel x86_64 Mach-O executable.",
        ));
    }
    let commands = u32_at(data, 16)? as usize;
    let command_bytes = u32_at(data, 20)? as usize;
    if commands == 0 || commands > 8192 || command_bytes > 16 * 1024 * 1024 {
        return Err(fail("Cabecera Mach-O inválida.", "Invalid Mach-O header."));
    }
    bytes_at(data, 32, command_bytes)?;
    let end = 32 + command_bytes;
    let mut offset = 32;
    let mut minimum = None;
    for _ in 0..commands {
        let command = u32_at(data, offset)?;
        let length = u32_at(data, offset + 4)? as usize;
        if length < 8 || length % 4 != 0 || offset + length > end {
            return Err(fail("Comando Mach-O inválido.", "Invalid Mach-O command."));
        }
        match command {
            0x32 => {
                if length < 24 || u32_at(data, offset + 8)? != 1 {
                    return Err(fail(
                        "El ejecutable no declara la plataforma macOS.",
                        "The executable does not declare the macOS platform.",
                    ));
                }
                let value = u32_at(data, offset + 12)?;
                minimum = Some(minimum.unwrap_or(0).max(value));
            }
            0x24 => {
                if length < 16 {
                    return Err(fail(
                        "Versión mínima Mach-O inválida.",
                        "Invalid Mach-O minimum version.",
                    ));
                }
                let value = u32_at(data, offset + 8)?;
                minimum = Some(minimum.unwrap_or(0).max(value));
            }
            0x25 | 0x2f | 0x30 => {
                return Err(fail(
                    "El ejecutable declara una plataforma distinta de macOS.",
                    "The executable declares a platform other than macOS.",
                ))
            }
            _ => {}
        }
        offset += length;
    }
    if offset != end || minimum.is_none_or(|minimum| minimum == 0 || minimum > 0x000b0000) {
        return Err(fail(
            "El ejecutable no es compatible con macOS Big Sur 11.0.",
            "The executable is not compatible with macOS Big Sur 11.0.",
        ));
    }
    Ok(())
}

fn safe_archive_name(name: &[u8]) -> Result<String> {
    // The shipped bundle uses ASCII names. Restricting them also prevents
    // Unicode normalization and case-fold aliases on typical macOS volumes.
    if !name.is_ascii() {
        return Err(fail(
            "El ZIP contiene nombres de archivo no admitidos.",
            "The ZIP contains unsupported file names.",
        ));
    }
    let name =
        std::str::from_utf8(name).map_err(|_| fail("Nombre ZIP inválido.", "Invalid ZIP name."))?;
    let trimmed = name.strip_suffix('/').unwrap_or(name);
    if trimmed.is_empty()
        || trimmed.starts_with('/')
        || trimmed.contains('\\')
        || trimmed.bytes().any(|b| b < 32 || b == 127 || b == b':')
        || trimmed
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(fail(
            "El ZIP contiene una ruta insegura.",
            "The ZIP contains an unsafe path.",
        ));
    }
    let top = trimmed.split('/').next().unwrap_or("");
    if top != APP_NAME && top != "__MACOSX" {
        return Err(fail(
            "El ZIP contiene archivos fuera de Turner.app.",
            "The ZIP contains files outside Turner.app.",
        ));
    }
    Ok(name.to_string())
}

fn validate_zip_extra(extra: &[u8]) -> Result<()> {
    let mut offset = 0;
    while offset < extra.len() {
        let kind = u16_at(extra, offset)?;
        let length = u16_at(extra, offset + 2)? as usize;
        bytes_at(extra, offset + 4, length)?;
        // These ordinary timestamp/UID fields cannot override names, links
        // or sizes. Reject ZIP64 and alternate Unicode/link metadata too.
        let allowed = match kind {
            0x5855 => matches!(length, 8 | 12),
            0x5455 => (1..=13).contains(&length),
            0x7875 => (3..=16).contains(&length),
            _ => false,
        };
        if !allowed {
            return Err(fail(
                "El ZIP contiene metadatos no admitidos.",
                "The ZIP contains unsupported metadata.",
            ));
        }
        offset += 4 + length;
    }
    Ok(())
}

/// Inspect both the directory and local headers before invoking an extractor.
/// Reject links, overlapping/unindexed local records, traversal, encryption,
/// ZIP64 and excessive expansion. The release builder emits ordinary ZIPs.
fn validate_zip(data: &[u8]) -> Result<()> {
    if data.len() as u64 > MAX_ZIP || data.len() < 22 {
        return Err(fail("Tamaño de ZIP inválido.", "Invalid ZIP size."));
    }
    let search_start = data.len().saturating_sub(65_557);
    let end = (search_start..=data.len() - 22)
        .rev()
        .find(|&offset| {
            data.get(offset..offset + 4) == Some(&[0x50, 0x4b, 0x05, 0x06])
                && u16_at(data, offset + 20)
                    .is_ok_and(|comment| offset + 22 + comment as usize == data.len())
        })
        .ok_or_else(|| {
            fail(
                "No se encontró el directorio del ZIP.",
                "The ZIP directory was not found.",
            )
        })?;
    let count = u16_at(data, end + 10)? as usize;
    let directory_size = u32_at(data, end + 12)? as usize;
    let directory = u32_at(data, end + 16)? as usize;
    if u16_at(data, end + 4)? != 0
        || u16_at(data, end + 6)? != 0
        || u16_at(data, end + 8)? as usize != count
        || count == 0
        || count > MAX_ENTRIES
        || directory.checked_add(directory_size) != Some(end)
    {
        return Err(fail(
            "El ZIP usa un formato dividido, ZIP64 o inválido.",
            "The ZIP uses a split, ZIP64 or invalid format.",
        ));
    }
    let mut offset = directory;
    let mut names = HashSet::new();
    let mut file_names = HashSet::new();
    let mut records = Vec::new();
    let mut expanded = 0u64;
    for _ in 0..count {
        if u32_at(data, offset)? != 0x02014b50 {
            return Err(fail("Directorio ZIP inválido.", "Invalid ZIP directory."));
        }
        let flags = u16_at(data, offset + 8)?;
        let method = u16_at(data, offset + 10)?;
        let crc = u32_at(data, offset + 16)?;
        let compressed = u32_at(data, offset + 20)?;
        let unpacked = u32_at(data, offset + 24)?;
        let name_length = u16_at(data, offset + 28)? as usize;
        let extra = u16_at(data, offset + 30)? as usize;
        let comment = u16_at(data, offset + 32)? as usize;
        let attributes = u32_at(data, offset + 38)?;
        let local = u32_at(data, offset + 42)? as usize;
        let name_bytes = bytes_at(data, offset + 46, name_length)?;
        let name = safe_archive_name(name_bytes)?;
        let mode = attributes >> 16;
        let file_type = mode & 0o170000;
        validate_zip_extra(bytes_at(data, offset + 46 + name_length, extra)?)?;
        if flags & (1 | 0x40 | 0x2000) != 0
            || !matches!(method, 0 | 8)
            || !matches!(file_type, 0 | 0o100000 | 0o040000)
            || mode & 0o6000 != 0
            || (file_type == 0o040000 && !name.ends_with('/'))
            || (file_type == 0o100000 && name.ends_with('/'))
            || u16_at(data, offset + 34)? != 0
            || compressed == u32::MAX
            || unpacked == u32::MAX
        {
            return Err(fail(
                "El ZIP contiene enlaces, permisos o formatos no admitidos.",
                "The ZIP contains unsupported links, permissions or formats.",
            ));
        }
        let canonical = name.trim_end_matches('/').to_ascii_lowercase();
        if !names.insert(canonical.clone()) {
            return Err(fail(
                "El ZIP contiene rutas duplicadas o ambiguas.",
                "The ZIP contains duplicate or ambiguous paths.",
            ));
        }
        if !name.ends_with('/') {
            file_names.insert(canonical);
        }
        expanded = expanded
            .checked_add(unpacked as u64)
            .ok_or_else(|| fail("Tamaño ZIP fuera de rango.", "ZIP size out of range."))?;
        if expanded > MAX_EXPANDED {
            return Err(fail(
                "El ZIP se expandiría por encima del límite permitido.",
                "The extracted ZIP would exceed the permitted size limit.",
            ));
        }
        if u32_at(data, local)? != 0x04034b50
            || u16_at(data, local + 6)? != flags
            || u16_at(data, local + 8)? != method
        {
            return Err(fail(
                "Las cabeceras del ZIP no coinciden.",
                "The ZIP headers do not match.",
            ));
        }
        let local_name_length = u16_at(data, local + 26)? as usize;
        let local_extra = u16_at(data, local + 28)? as usize;
        validate_zip_extra(bytes_at(data, local + 30 + local_name_length, local_extra)?)?;
        if bytes_at(data, local + 30, local_name_length)? != name_bytes {
            return Err(fail(
                "El ZIP cambia una ruta entre sus cabeceras.",
                "The ZIP changes a path between its headers.",
            ));
        }
        let content = local
            .checked_add(30 + local_name_length + local_extra)
            .ok_or_else(|| fail("Offset ZIP inválido.", "Invalid ZIP offset."))?;
        let mut record_end = content
            .checked_add(compressed as usize)
            .ok_or_else(|| fail("Tamaño ZIP inválido.", "Invalid ZIP size."))?;
        if flags & 8 != 0 {
            if u32_at(data, record_end)? == 0x08074b50 {
                record_end += 4;
            }
            if u32_at(data, record_end)? != crc
                || u32_at(data, record_end + 4)? != compressed
                || u32_at(data, record_end + 8)? != unpacked
            {
                return Err(fail("Descriptor ZIP inválido.", "Invalid ZIP descriptor."));
            }
            record_end += 12;
        } else if u32_at(data, local + 14)? != crc
            || u32_at(data, local + 18)? != compressed
            || u32_at(data, local + 22)? != unpacked
        {
            return Err(fail(
                "Tamaños ZIP inconsistentes.",
                "Inconsistent ZIP sizes.",
            ));
        }
        if record_end > directory || local >= record_end {
            return Err(fail(
                "Las entradas del ZIP se superponen con su directorio.",
                "ZIP entries overlap the directory.",
            ));
        }
        records.push((local, record_end));
        offset = offset
            .checked_add(46 + name_length + extra + comment)
            .ok_or_else(|| fail("Directorio ZIP inválido.", "Invalid ZIP directory."))?;
        if offset > end {
            return Err(fail("Directorio ZIP truncado.", "Truncated ZIP directory."));
        }
    }
    if offset != end {
        return Err(fail(
            "El ZIP contiene registros no indexados.",
            "The ZIP contains unindexed records.",
        ));
    }
    records.sort_unstable();
    let mut previous = 0;
    for (start, end) in records {
        if start != previous {
            return Err(fail(
                "El ZIP contiene registros superpuestos o escondidos.",
                "The ZIP contains overlapping or hidden records.",
            ));
        }
        previous = end;
    }
    if previous != directory {
        return Err(fail(
            "El ZIP contiene datos sin indexar.",
            "The ZIP contains unindexed data.",
        ));
    }
    for name in &names {
        let mut parent = name.as_str();
        while let Some((prefix, _)) = parent.rsplit_once('/') {
            if file_names.contains(prefix) {
                return Err(fail(
                    "Una ruta ZIP usa un archivo como carpeta.",
                    "A ZIP path uses a file as a folder.",
                ));
            }
            parent = prefix;
        }
    }
    for required in [
        format!("{APP_NAME}/Contents/Info.plist"),
        format!("{APP_NAME}/Contents/MacOS/{EXECUTABLE}"),
    ] {
        if !file_names.contains(&required.to_ascii_lowercase()) {
            return Err(fail(
                "El ZIP no contiene una app completa.",
                "The ZIP does not contain a complete app.",
            ));
        }
    }
    Ok(())
}

struct InstallerChild {
    child: Child,
    handed_off: bool,
}
impl Drop for InstallerChild {
    fn drop(&mut self) {
        if !self.handed_off {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
fn install_update(
    candidate: Candidate,
    token: &AtomicBool,
    reporter: &Reporter,
    commit: mpsc::Receiver<()>,
) -> Result<()> {
    let temporary = PrivateDir::new(&std::env::temp_dir())?;
    let installed = locate_installed(&temporary.path, token)?;
    if installed.bundle != candidate.installed.bundle
        || installed.version != candidate.installed.version
        || installed.revision != candidate.installed.revision
    {
        return Err(fail(
            "La app instalada cambió desde la consulta. Volvé a buscar actualizaciones.",
            "The installed app has changed since the check. Check for updates again.",
        ));
    }
    let archive = temporary.path.join(&candidate.asset);
    require_download(
        &candidate.url,
        &archive,
        MAX_ZIP,
        token,
        Some((reporter, candidate.size)),
        &temporary.path,
    )?;
    if fs::metadata(&archive)?.len() != candidate.size
        || hash_file(&archive, token)? != candidate.hash
    {
        return Err(fail(
            "El ZIP no coincide con el tamaño y SHA256 publicados; no se modificó la app.",
            "The ZIP does not match the published size and SHA256; the app was not modified.",
        ));
    }
    reporter.send(Event::Preparing);
    validate_zip(&fs::read(&archive)?)?;
    cancelled(token)?;
    let extracted = temporary.path.join("extracted");
    fs::create_dir(&extracted)?;
    checked_run(
        "/usr/bin/ditto",
        &args(&[
            OsStr::new("-x"),
            OsStr::new("-k"),
            archive.as_os_str(),
            extracted.as_os_str(),
        ]),
        &temporary.path,
        token,
        Duration::from_secs(90),
    )?;
    check_tree(&extracted, token)?;
    let app = extracted.join(APP_NAME);
    validate_bundle(
        &app,
        &candidate.info.version,
        candidate.info.revision,
        &temporary.path,
        token,
    )?;
    let source_hash = hash_file(&app.join("Contents/MacOS").join(EXECUTABLE), token)?;
    let parent = installed.bundle.parent().ok_or_else(|| {
        fail(
            "No se encontró la carpeta de instalación.",
            "The installation folder was not found.",
        )
    })?;
    // Creating the stage beside the installed app verifies parent write access
    // and keeps both replacement renames on the same filesystem.
    let mut stage = PrivateDir::new(parent)?;
    let staged_app = stage.path.join("Next.app");
    checked_run(
        "/usr/bin/ditto",
        &args(&[app.as_os_str(), staged_app.as_os_str()]),
        &temporary.path,
        token,
        Duration::from_secs(90),
    )?;
    validate_bundle(
        &staged_app,
        &candidate.info.version,
        candidate.info.revision,
        &temporary.path,
        token,
    )?;
    if hash_file(&staged_app.join("Contents/MacOS").join(EXECUTABLE), token)? != source_hash {
        return Err(fail(
            "La copia preparada no coincide con el ejecutable verificado.",
            "The staged copy does not match the verified executable.",
        ));
    }
    cancelled(token)?;
    let script = stage.path.join("install.sh");
    new_file(&script)?.write_all(INSTALLER_SCRIPT.as_bytes())?;
    let log = new_file(&stage.path.join("install.log"))?;
    let backup = stage.path.join("Previous.app");
    let child = Command::new("/usr/bin/nohup")
        .arg("/bin/bash")
        .arg(&script)
        .arg(&installed.bundle)
        .arg(&staged_app)
        .arg(&backup)
        .arg(std::process::id().to_string())
        .arg(&stage.path)
        .arg(&installed.version)
        .arg(installed.revision.to_string())
        .arg(&candidate.info.version)
        .arg(candidate.info.revision.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .spawn()
        .map_err(|error| {
            fail(
                format!("No se pudo preparar el instalador: {error}"),
                format!("Could not prepare the installer: {error}"),
            )
        })?;
    let mut helper = InstallerChild {
        child,
        handed_off: false,
    };
    let ready_deadline = Instant::now() + Duration::from_secs(10);
    while !stage.path.join("ready").is_file() {
        cancelled(token)?;
        if helper.child.try_wait()?.is_some() || Instant::now() >= ready_deadline {
            return Err(fail(
                "El instalador no confirmó que esté preparado; la app sigue intacta.",
                "The installer did not confirm it was ready; the app is unchanged.",
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
    cancelled(token)?;
    reporter.send(Event::Prepared);
    let commit_deadline = Instant::now() + Duration::from_secs(120);
    loop {
        cancelled(token)?;
        match commit.recv_timeout(Duration::from_millis(50)) {
            Ok(()) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err(Failure::Cancelled),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if helper.child.try_wait()?.is_some() || Instant::now() >= commit_deadline {
            return Err(fail(
                "No se confirmó el reinicio a tiempo; la app sigue intacta.",
                "Restart was not confirmed in time; the app is unchanged.",
            ));
        }
    }
    cancelled(token)?;
    let pending = stage.path.join("commit.pending");
    let mut confirmation = new_file(&pending)?;
    confirmation.write_all(b"install\n")?;
    confirmation.sync_all()?;
    // The UI has removed Cancel and authorized the handoff. Only this atomic
    // rename enables the external installer, which still waits for our exit.
    fs::rename(&pending, stage.path.join("commit"))?;
    let acknowledgement_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if helper.child.try_wait()?.is_some() {
            return Err(fail(
                "El instalador terminó antes de confirmar el reinicio; la app sigue abierta.",
                "The installer exited before confirming restart; the app remains open.",
            ));
        }
        if stage.path.join("committed").is_file() {
            break;
        }
        if Instant::now() >= acknowledgement_deadline {
            return Err(fail(
                "El instalador no confirmó la actualización; la app sigue abierta.",
                "The installer did not confirm the update; the app remains open.",
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
    stage.keep = true;
    helper.handed_off = true;
    reporter.send(Event::Restart);
    // Download/extraction temp files are now removed by PrivateDir::drop.
    // The adjacent stage persists with Previous.app and an installation log.
    Ok(())
}

const INSTALLER_SCRIPT: &str = r#"#!/bin/bash
# This script is shipped as source in Turner, never downloaded.
# macOS Big Sur / Bash 3.2 compatible; no elevated privileges or services.
set -u
umask 077
[[ "$#" -eq 9 ]] || exit 2
target="$1"
staged="$2"
backup="$3"
parent_pid="$4"
work="$5"
old_version="$6"
old_revision="$7"
new_version="$8"
new_revision="$9"
status() { /usr/bin/printf '%s\n' "$1" > "$work/result.txt"; }
valid_bundle() {
  [[ -d "$1" && ! -L "$1" && -f "$1/Contents/Info.plist" && ! -L "$1/Contents/Info.plist" ]] || return 1
  [[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$1/Contents/Info.plist" 2>/dev/null)" == 'com.tuner.guitar.compact' ]] || return 1
  [[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$1/Contents/Info.plist" 2>/dev/null)" == 'guitar-tuner-egui' ]] || return 1
  [[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$1/Contents/Info.plist" 2>/dev/null)" == "$2" ]] || return 1
  [[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$1/Contents/Info.plist" 2>/dev/null)" == "$3" ]] || return 1
  [[ -x "$1/Contents/MacOS/guitar-tuner-egui" && ! -L "$1/Contents/MacOS/guitar-tuner-egui" ]] || return 1
}
[[ "$parent_pid" =~ ^[1-9][0-9]*$ ]] || exit 2
[[ -d "$work" && ! -L "$work" && ! -e "$backup" && ! -L "$backup" ]] || exit 2
valid_bundle "$target" "$old_version" "$old_revision" || { status 'La app instalada cambió. No se reemplazó.'; exit 1; }
valid_bundle "$staged" "$new_version" "$new_revision" || { status 'La app preparada no es válida. No se reemplazó.'; exit 1; }
/usr/bin/printf 'ready\n' > "$work/ready" || exit 1
attempt=0
while [[ ! -f "$work/commit" ]]; do
  attempt=$((attempt + 1))
  [[ "$attempt" -le 650 ]] || { status 'El reinicio no se confirmó; instalación cancelada.'; exit 1; }
  /bin/sleep 0.2
done
[[ "$(/bin/cat "$work/commit")" == install ]] || { status 'Confirmación de instalación inválida.'; exit 1; }
/usr/bin/printf 'committed\n' > "$work/committed" || exit 1
attempt=0
while /bin/kill -0 "$parent_pid" 2>/dev/null; do
  attempt=$((attempt + 1))
  [[ "$attempt" -le 600 ]] || { status 'La aplicación no se cerró a tiempo. No se reemplazó.'; exit 1; }
  /bin/sleep 0.2
done
# Recheck immediately before the two same-filesystem renames.
valid_bundle "$target" "$old_version" "$old_revision" || { status 'La app instalada cambió. No se reemplazó.'; exit 1; }
[[ ! -e "$backup" && ! -L "$backup" ]] || { status 'Ya existe el respaldo; no se reemplazó la app.'; exit 1; }
/bin/mv "$target" "$backup" || { status 'No se pudo guardar la copia anterior. No se reemplazó la app.'; /usr/bin/open -n "$target"; exit 1; }
if ! /bin/mv "$staged" "$target"; then
  if /bin/mv "$backup" "$target"; then
    status 'Falló el reemplazo; se restauró la versión anterior.'
    /usr/bin/open -n "$target"
  else
    status "Falló el reemplazo. La copia anterior permanece en: $backup"
  fi
  exit 1
fi
if ! /usr/bin/open -n "$target"; then
  if /bin/mv "$target" "$work/Failed.app" && /bin/mv "$backup" "$target"; then
    status 'macOS rechazó la apertura; se restauró la versión anterior.'
    /usr/bin/open -n "$target"
  else
    status "macOS rechazó la apertura. La copia anterior permanece en: $backup"
  fi
  exit 1
fi
# open=0 confirms a launch request, not application health. Keep the backup.
status "Se solicitó abrir la nueva versión. Copia anterior conservada en: $backup"
exit 0
"#;

#[cfg(test)]
#[path = "updates_tests.rs"]
mod tests;
