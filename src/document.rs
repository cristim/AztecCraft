use crate::native_scene::{NativeError, NativeScene, ascii, binary};
use crate::scene::{Scene, SceneError};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    fs::Metadata,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

mod native_save;
pub use native_save::{NativeSaveError, SaveDestination, SaveOutcome, SaveReceipt, save_native};

pub const DOCUMENT_VERSION: u32 = 1;
pub const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub enum LinkStatus {
    Direct,
    Symlink,
}

#[derive(Clone)]
pub struct SourceBaseline {
    bytes: Arc<[u8]>,
    parse_bytes: Arc<[u8]>,
    metadata: Metadata,
    link: LinkStatus,
    #[cfg(any(unix, windows))]
    file: Arc<std::fs::File>,
    #[cfg(any(unix, windows))]
    destination: Option<Arc<native_save::Destination>>,
}
impl fmt::Debug for SourceBaseline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceBaseline")
            .field("bytes", &self.bytes.len())
            .field("parse_bytes", &self.parse_bytes.len())
            .field("file_length", &self.metadata.len())
            .field("link", &self.link)
            .finish()
    }
}
#[derive(Clone, Debug, Default)]
pub enum NativeOrigin {
    #[default]
    Untitled,
    Binary(SourceBaseline),
    Ascii(SourceBaseline),
    Legacy(SourceBaseline),
}
impl NativeOrigin {
    pub fn active_save_available(&self) -> bool {
        #[cfg(any(unix, windows))]
        {
            matches!(self, Self::Binary(source) if matches!(source.link, LinkStatus::Direct) && source.destination.is_some())
        }
        #[cfg(not(any(unix, windows)))]
        false
    }
    pub fn save_as_available(&self) -> bool {
        cfg!(any(unix, windows)) && matches!(self, Self::Binary(_))
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Untitled => "untitled",
            Self::Binary(_) => "maya_binary",
            Self::Ascii(_) => "maya_ascii",
            Self::Legacy(_) => "legacy_aztec",
        }
    }
    pub fn source_bytes(&self) -> Option<&Arc<[u8]>> {
        match self {
            Self::Untitled => None,
            Self::Binary(s) | Self::Ascii(s) | Self::Legacy(s) => Some(&s.bytes),
        }
    }
}

pub fn verify_native_origin(path: &Path, origin: &NativeOrigin) -> Result<(), NativeSaveError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let source = match origin {
            NativeOrigin::Binary(source)
            | NativeOrigin::Ascii(source)
            | NativeOrigin::Legacy(source) => source,
            NativeOrigin::Untitled => return Err(NativeSaveError::SourceChanged),
        };
        let (bytes, metadata, _, _) =
            read_bytes(path).map_err(|_| NativeSaveError::SourceChanged)?;
        if metadata.dev() != source.metadata.dev()
            || metadata.ino() != source.metadata.ino()
            || bytes.as_slice() != source.bytes.as_ref()
        {
            return Err(NativeSaveError::SourceChanged);
        }
        if let Some(destination) = &source.destination {
            destination.verify(&source.file, &source.bytes)?;
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        let source = match origin {
            NativeOrigin::Binary(source)
            | NativeOrigin::Ascii(source)
            | NativeOrigin::Legacy(source) => source,
            NativeOrigin::Untitled => return Err(NativeSaveError::SourceChanged),
        };
        let (bytes, _, _, file) = read_bytes(path).map_err(|_| NativeSaveError::SourceChanged)?;
        if !same_file(&file, &source.file) || bytes.as_slice() != source.bytes.as_ref() {
            return Err(NativeSaveError::SourceChanged);
        }
        if let Some(destination) = &source.destination {
            destination.verify(&source.file, &source.bytes)?;
        }
        Ok(())
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (path, origin);
        Err(NativeSaveError::Unsupported)
    }
}

/// Whether two handles name the same file (volume serial number and file index).
#[cfg(windows)]
fn same_file(a: &std::fs::File, b: &std::fs::File) -> bool {
    match (
        winapi_util::file::information(a),
        winapi_util::file::information(b),
    ) {
        (Ok(a), Ok(b)) => {
            a.volume_serial_number() == b.volume_serial_number() && a.file_index() == b.file_index()
        }
        _ => false,
    }
}
pub struct NativeDocument {
    pub scene: NativeScene,
    pub origin: NativeOrigin,
}

#[derive(Debug)]
pub enum NativeLoadError {
    Read(io::ErrorKind),
    TooLarge,
    UnsupportedFormat,
    Legacy,
    Native(NativeError),
    Binary(binary::BinaryError),
    Ascii(ascii::AsciiError),
}
impl fmt::Display for NativeLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(kind) => write!(f, "cannot read regular scene file: {kind:?}"),
            Self::TooLarge => write!(f, "scene exceeds {MAX_DOCUMENT_BYTES} byte limit"),
            Self::UnsupportedFormat => {
                f.write_str("Open supports Maya .mb, static cubic .ma and legacy .aztec scenes")
            }
            Self::Legacy => f.write_str("invalid legacy scene document"),
            Self::Native(error) => error.fmt(f),
            Self::Binary(error) => error.fmt(f),
            Self::Ascii(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for NativeLoadError {}

fn read_bytes(path: &Path) -> io::Result<(Vec<u8>, Metadata, LinkStatus, std::fs::File)> {
    #[cfg(not(unix))]
    if !std::fs::metadata(path)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "document must be a regular file",
        ));
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "document must be a regular file",
        ));
    }
    let link = if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        LinkStatus::Symlink
    } else {
        LinkStatus::Direct
    };
    let mut bytes = Vec::new();
    (&file)
        .take((MAX_DOCUMENT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    Ok((bytes, metadata, link, file))
}

pub fn load_native(path: &Path) -> Result<NativeDocument, NativeLoadError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .ok_or(NativeLoadError::UnsupportedFormat)?;
    let binary_format = extension.eq_ignore_ascii_case("mb");
    let ascii_format = extension.eq_ignore_ascii_case("ma");
    if !binary_format && !ascii_format && !extension.eq_ignore_ascii_case("aztec") {
        return Err(NativeLoadError::UnsupportedFormat);
    }
    let (bytes, metadata, link, file) =
        read_bytes(path).map_err(|error| NativeLoadError::Read(error.kind()))?;
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(NativeLoadError::TooLarge);
    }
    let bytes: Arc<[u8]> = bytes.into();
    let scene = if binary_format {
        binary::decode(bytes.clone()).map_err(NativeLoadError::Binary)?
    } else if ascii_format {
        ascii::decode(bytes.clone()).map_err(NativeLoadError::Ascii)?
    } else {
        NativeScene::from_legacy(decode_legacy(&bytes).map_err(|_| NativeLoadError::Legacy)?)
            .map_err(NativeLoadError::Native)?
    };
    #[cfg(unix)]
    let destination = if matches!(link, LinkStatus::Direct) {
        native_save::Destination::capture(path)
            .ok()
            .filter(|destination| destination.matches(&metadata).unwrap_or(false))
            .map(Arc::new)
    } else {
        None
    };
    #[cfg(windows)]
    let destination = if matches!(link, LinkStatus::Direct) {
        native_save::Destination::capture(path)
            .ok()
            .filter(|destination| destination.matches(&file).unwrap_or(false))
            .map(Arc::new)
    } else {
        None
    };
    #[cfg(not(any(unix, windows)))]
    drop(file);
    let source = SourceBaseline {
        parse_bytes: bytes.clone(),
        bytes,
        metadata,
        link,
        #[cfg(any(unix, windows))]
        file: Arc::new(file),
        #[cfg(any(unix, windows))]
        destination,
    };
    Ok(NativeDocument {
        scene,
        origin: if binary_format {
            NativeOrigin::Binary(source)
        } else if ascii_format {
            NativeOrigin::Ascii(source)
        } else {
            NativeOrigin::Legacy(source)
        },
    })
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document<T> {
    version: u32,
    scene: T,
}

pub fn load(path: &Path) -> Result<Scene, DocumentError> {
    let (bytes, ..) = read_bytes(path).map_err(|source| DocumentError::Io {
        operation: "read document",
        path: path.into(),
        source,
    })?;
    decode_legacy(&bytes)
}

fn decode_legacy(bytes: &[u8]) -> Result<Scene, DocumentError> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(DocumentError::TooLarge);
    }
    let document: Document<Scene> = serde_json::from_slice(bytes).map_err(DocumentError::Json)?;
    if document.version != DOCUMENT_VERSION {
        return Err(DocumentError::UnsupportedVersion(document.version));
    }
    Ok(document.scene)
}

pub fn save_new(path: &Path, scene: &Scene) -> Result<(), DocumentError> {
    scene.validate().map_err(DocumentError::Scene)?;
    match std::fs::symlink_metadata(path) {
        Ok(_) => {
            return Err(DocumentError::Io {
                operation: "create document",
                path: path.into(),
                source: io::Error::new(io::ErrorKind::AlreadyExists, "destination already exists"),
            });
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(DocumentError::Io {
                operation: "inspect document destination",
                path: path.into(),
                source,
            });
        }
    }
    let mut bytes = LimitedBytes(Vec::new());
    let document = Document {
        version: DOCUMENT_VERSION,
        scene,
    };
    if let Err(error) = serde_json::to_writer_pretty(&mut bytes, &document) {
        return Err(if bytes.0.len() == MAX_DOCUMENT_BYTES {
            DocumentError::TooLarge
        } else {
            DocumentError::Json(error)
        });
    }
    craft_atomic::write_new(path, &bytes.0).map_err(|source| DocumentError::Io {
        operation: "create document",
        path: path.into(),
        source,
    })
}

struct LimitedBytes(Vec<u8>);

impl Write for LimitedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let count = bytes.len().min(MAX_DOCUMENT_BYTES - self.0.len());
        if count == 0 && !bytes.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "document size limit exceeded",
            ));
        }
        self.0.extend_from_slice(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub enum DocumentError {
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    Json(serde_json::Error),
    Scene(SceneError),
    UnsupportedVersion(u32),
    TooLarge,
}

impl fmt::Display for DocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                operation,
                path,
                source,
            } => write!(f, "{operation} {}: {source}", path.display()),
            Self::Json(source) => write!(f, "invalid scene document: {source}"),
            Self::Scene(source) => write!(f, "invalid scene: {source}"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported document version {version}")
            }
            Self::TooLarge => write!(f, "document exceeds {MAX_DOCUMENT_BYTES} byte limit"),
        }
    }
}

impl std::error::Error for DocumentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json(source) => Some(source),
            Self::Scene(source) => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod native_open_tests {
    use super::*;
    use crate::{view_scene::ViewScene, viewport::Camera};
    use glam::Vec3;

    mod ascii_fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/maya_ascii.rs"
        ));
    }

    #[test]
    fn ascii_native_open_keeps_exact_source_origin_and_real_curve_geometry() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("curve.MA");
        let source = ascii_fixture::specimen();
        std::fs::write(&path, &source).unwrap();
        let document = load_native(&path).unwrap();
        assert!(matches!(document.origin, NativeOrigin::Ascii(_)));
        assert_eq!(document.origin.kind(), "maya_ascii");
        assert_eq!(document.origin.source_bytes().unwrap().as_ref(), source);
        assert_eq!(document.scene.retained_source_bytes(), source.len());
        assert_eq!(document.scene.retained_source_records(), 10);
        assert_eq!(document.scene.curves().len(), 1);
        assert!(document.scene.shapes().is_empty());
        let view = ViewScene::from_native(&document.scene, 256 * 1024 * 1024).unwrap();
        assert_eq!(view.curve_instances().len(), 1);
        assert!(view.curve_instances()[0].segments().len() > 1);
        assert_eq!(std::fs::read(path).unwrap(), source);
    }

    #[test]
    fn ascii_native_dispatch_never_treats_json_or_binary_as_ascii() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("wrong.ma");
        for source in [b"{}".as_slice(), b"FOR8\0\0\0\0Maya".as_slice()] {
            std::fs::write(&path, source).unwrap();
            assert!(matches!(load_native(&path), Err(NativeLoadError::Ascii(_))));
            assert_eq!(std::fs::read(&path).unwrap(), source);
        }
        let source = ascii_fixture::specimen();
        let binary_path = directory.path().join("wrong.mb");
        std::fs::write(&binary_path, &source).unwrap();
        assert!(matches!(
            load_native(&binary_path),
            Err(NativeLoadError::Binary(_))
        ));
    }

    #[test]
    #[ignore = "requires the explicitly supplied pinned genuine Maya ASCII fixture"]
    fn genuine_ascii_native_open_and_frame_keeps_retained_source() {
        let path = std::env::var_os("AZTECCRAFT_MA_CURVE_FIXTURE").unwrap();
        let path = Path::new(&path);
        let source = std::fs::read(path).unwrap();
        let document = load_native(path).unwrap();
        assert_eq!(document.origin.kind(), "maya_ascii");
        assert_eq!(document.origin.source_bytes().unwrap().as_ref(), source);
        assert_eq!(document.scene.curves().len(), 1);
        assert_eq!(document.scene.curves()[0].control_vertices().len(), 8);
        assert_eq!(document.scene.curves()[0].knots().len(), 10);
        let view = ViewScene::from_native(&document.scene, 256 * 1024 * 1024).unwrap();
        assert_eq!(view.curve_instances().len(), 1);
        for aspect in [0.5, 1.19, 2.0] {
            let mut camera = Camera::default();
            camera.frame_view(&view, None, aspect).unwrap();
            for p in view.curve_instances()[0].segments().iter().flatten() {
                let ndc = camera.project(*p, aspect).unwrap().unwrap();
                assert!(ndc.x.abs() < 1.0 && ndc.y.abs() < 1.0);
            }
        }
        assert_eq!(std::fs::read(path).unwrap(), source);
    }

    #[cfg(unix)]
    #[test]
    fn fifo_open_refuses_without_waiting_for_a_writer() {
        use std::{
            process::{Command, Stdio},
            time::{Duration, Instant},
        };
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pipe.mb");
        assert!(
            Command::new("mkfifo")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "document::native_open_tests::fifo_open_child",
                "--ignored",
            ])
            .env("AZTECCRAFT_FIFO_TEST", path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "FIFO child must reject the special file");
                break;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("opening a FIFO waited for a writer");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "bounded child of the FIFO regression"]
    fn fifo_open_child() {
        let path = std::env::var_os("AZTECCRAFT_FIFO_TEST").unwrap();
        assert!(matches!(
            load_native(Path::new(&path)),
            Err(NativeLoadError::Read(io::ErrorKind::InvalidInput))
        ));
    }

    #[test]
    #[ignore = "Requires an explicitly supplied private MB file"]
    fn private_open_frame_bounds() {
        let path = std::env::var_os("AZTECCRAFT_OPEN_TEST_FILE")
            .expect("private fixture environment variable");
        let document = load_native(Path::new(&path)).expect("private native decode");
        let view = ViewScene::from_native(&document.scene, 256 * 1024 * 1024).expect("native view");
        let mut camera = Camera::default();
        let aspect = 1.19;
        camera
            .frame_view(&view, None, aspect)
            .expect("initial frame");
        let mut bounds = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        let mut ndc = bounds;
        let mut vertices = 0;
        let mut rejected = 0;
        for point in view
            .instances()
            .iter()
            .flat_map(|instance| instance.world_vertices())
        {
            vertices += 1;
            bounds.0 = bounds.0.min(*point);
            bounds.1 = bounds.1.max(*point);
            match camera.project(*point, aspect).expect("finite projection") {
                Some(point) => {
                    ndc.0 = ndc.0.min(point);
                    ndc.1 = ndc.1.max(point);
                }
                None => rejected += 1,
            }
        }
        eprintln!(
            "instances={} vertices={vertices} rejected={rejected} world={bounds:?} ndc={ndc:?} target={:?} eye={:?} distance={}",
            view.instances().len(),
            camera.target(),
            camera.eye(),
            camera.distance()
        );
        assert_eq!(rejected, 0);
        assert!(ndc.0.x >= -1.001 && ndc.0.y >= -1.001 && ndc.1.x <= 1.001 && ndc.1.y <= 1.001);
    }
}
