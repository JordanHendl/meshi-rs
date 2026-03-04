use dashi::{AspectMask, Context, Format, ImageInfo, ImageView, ImageViewType, SubresourceRange};
use image::GenericImageView;
use meshi_utils::MeshiError;
use noren::DB;
use noren::rdb::imagery::DeviceImage;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;

pub type BindlessImageHandle = u32;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ImagePagerKey {
    Disk(PathBuf),
    Database(DatabaseImageKey),
    Inline(InlineImageKey),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DatabaseImageKey {
    pub project: Option<String>,
    pub asset_key: String,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct InlineImageKey {
    pub id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImagePagerStatus {
    Pending,
    Ready,
    Failed,
}

#[derive(Clone, Debug)]
struct ImagePagerEntry {
    handle: BindlessImageHandle,
    status: ImagePagerStatus,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ImagePagerClipmapDestination {
    pub slot: u32,
    pub handle: BindlessImageHandle,
    pub generation: u64,
}

#[derive(Clone, Debug)]
pub struct ImagePagerClipmapRing {
    handles: Vec<BindlessImageHandle>,
    next_slot: usize,
    generation: u64,
}

impl ImagePagerClipmapRing {
    pub fn initialize(slot_count: usize, backend: &mut impl ImagePagerBackend) -> Self {
        let slot_count = slot_count.max(1);
        let mut handles = Vec::with_capacity(slot_count);
        for _ in 0..slot_count {
            handles.push(backend.reserve_handle());
        }
        Self {
            handles,
            next_slot: 0,
            generation: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.handles.len()
    }

    pub fn handle_for_slot(&self, slot: u32) -> Option<BindlessImageHandle> {
        self.handles.get(slot as usize).copied()
    }

    pub fn release(&mut self, backend: &mut impl ImagePagerBackend) {
        for handle in self.handles.drain(..) {
            backend.release_image(handle);
        }
        self.next_slot = 0;
    }

    fn reserve_destination(&mut self) -> ImagePagerClipmapDestination {
        let slot = self.next_slot as u32;
        let handle = self.handles[self.next_slot];
        self.next_slot = (self.next_slot + 1) % self.handles.len();
        self.generation = self.generation.wrapping_add(1);
        ImagePagerClipmapDestination {
            slot,
            handle,
            generation: self.generation,
        }
    }
}

pub trait ImagePagerBackend {
    fn reserve_handle(&mut self) -> BindlessImageHandle;
    fn register_image(
        &mut self,
        handle: BindlessImageHandle,
        view: ImageView,
    ) -> BindlessImageHandle;
    fn register_image_immediate(&mut self, view: ImageView) -> BindlessImageHandle {
        let handle = self.reserve_handle();
        self.register_image(handle, view)
    }
    fn release_image(&mut self, handle: BindlessImageHandle);
}

pub trait ImagePagerLoader {
    fn load_from_disk(&mut self, path: &Path) -> Result<ImageView, MeshiError>;
    fn load_from_database(&mut self, key: &DatabaseImageKey) -> Result<ImageView, MeshiError>;
}

pub struct ImagePagerDefaultLoader<'a> {
    ctx: &'a mut Context,
    db: Option<NonNull<DB>>,
}

impl<'a> ImagePagerDefaultLoader<'a> {
    pub fn new(ctx: &'a mut Context) -> Self {
        Self { ctx, db: None }
    }

    pub fn with_database(ctx: &'a mut Context, db: &'a mut DB) -> Self {
        Self {
            ctx,
            db: NonNull::new(db),
        }
    }

    fn database_entry(key: &DatabaseImageKey) -> String {
        match key.project.as_deref() {
            Some(project) => format!("{project}/{}", key.asset_key),
            None => key.asset_key.clone(),
        }
    }

    fn view_from_device_image(device: DeviceImage) -> ImageView {
        ImageView {
            img: device.img,
            aspect: AspectMask::Color,
            view_type: ImageViewType::Type2D,
            range: SubresourceRange::new(0, device.info.mip_levels, 0, device.info.layers),
        }
    }

    fn view_from_image_handle(image: dashi::Handle<dashi::Image>) -> ImageView {
        ImageView {
            img: image,
            aspect: AspectMask::Color,
            view_type: ImageViewType::Type2D,
            range: SubresourceRange::new(0, 1, 0, 1),
        }
    }
}

impl ImagePagerLoader for ImagePagerDefaultLoader<'_> {
    fn load_from_disk(&mut self, path: &Path) -> Result<ImageView, MeshiError> {
        let image = image::open(path).map_err(|_| MeshiError {})?;
        let rgba = image.to_rgba8();
        let (width, height) = image.dimensions();
        let debug_name = path.to_string_lossy();

        let info = ImageInfo {
            debug_name: &debug_name,
            dim: [width, height, 1],
            layers: 1,
            format: Format::RGBA8,
            mip_levels: 1,
            initial_data: Some(rgba.as_raw()),
            ..Default::default()
        };

        let image = self.ctx.make_image(&info).map_err(|_| MeshiError {})?;
        Ok(Self::view_from_image_handle(image))
    }

    fn load_from_database(&mut self, key: &DatabaseImageKey) -> Result<ImageView, MeshiError> {
        let entry = Self::database_entry(key);
        let Some(mut db) = self.db else {
            return Err(MeshiError {});
        };
        let image = unsafe { db.as_mut() }
            .imagery_mut()
            .fetch_gpu_image(&entry)
            .map_err(|_| MeshiError {})?;
        Ok(Self::view_from_device_image(image))
    }
}

pub struct ImagePager {
    entries: HashMap<ImagePagerKey, ImagePagerEntry>,
    pending: VecDeque<ImagePagerKey>,
    pending_clipmap: VecDeque<(ImagePagerClipmapDestination, ImagePagerKey)>,
    clipmap_status: HashMap<ImagePagerClipmapDestination, ImagePagerStatus>,
    handle_to_key: HashMap<BindlessImageHandle, ImagePagerKey>,
}

impl ImagePager {
    pub fn initialize() -> Self {
        Self::new()
    }

    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            pending: VecDeque::new(),
            pending_clipmap: VecDeque::new(),
            clipmap_status: HashMap::new(),
            handle_to_key: HashMap::new(),
        }
    }

    pub fn request_image(
        &mut self,
        key: ImagePagerKey,
        backend: &mut impl ImagePagerBackend,
    ) -> BindlessImageHandle {
        if let Some(entry) = self.entries.get(&key) {
            return entry.handle;
        }

        let handle = backend.reserve_handle();
        self.entries.insert(
            key.clone(),
            ImagePagerEntry {
                handle,
                status: ImagePagerStatus::Pending,
            },
        );
        self.handle_to_key.insert(handle, key.clone());
        self.pending.push_back(key);
        handle
    }

    pub fn request_database_image_async(
        &mut self,
        key: DatabaseImageKey,
        backend: &mut impl ImagePagerBackend,
    ) -> BindlessImageHandle {
        self.request_image(ImagePagerKey::Database(key), backend)
    }

    pub fn status(&self, key: &ImagePagerKey) -> Option<ImagePagerStatus> {
        self.entries.get(key).map(|entry| entry.status)
    }

    pub fn handle(&self, key: &ImagePagerKey) -> Option<BindlessImageHandle> {
        self.entries.get(key).map(|entry| entry.handle)
    }

    pub fn register_inline_image(
        &mut self,
        key: ImagePagerKey,
        view: ImageView,
        backend: &mut impl ImagePagerBackend,
    ) -> BindlessImageHandle {
        if let Some(entry) = self.entries.get(&key) {
            return entry.handle;
        }

        let handle = backend.register_image_immediate(view);
        self.entries.insert(
            key.clone(),
            ImagePagerEntry {
                handle,
                status: ImagePagerStatus::Ready,
            },
        );
        self.handle_to_key.insert(handle, key);
        handle
    }

    pub fn push_image_to_clipmap_async(
        &mut self,
        clipmap: &mut ImagePagerClipmapRing,
        key: ImagePagerKey,
    ) -> ImagePagerClipmapDestination {
        let destination = clipmap.reserve_destination();
        self.clipmap_status
            .retain(|current, _| current.handle != destination.handle);
        self.pending_clipmap
            .retain(|(current, _)| current.handle != destination.handle);
        self.clipmap_status
            .insert(destination, ImagePagerStatus::Pending);
        self.pending_clipmap.push_back((destination, key));
        destination
    }

    pub fn push_database_to_clipmap_async(
        &mut self,
        clipmap: &mut ImagePagerClipmapRing,
        key: DatabaseImageKey,
    ) -> ImagePagerClipmapDestination {
        self.push_image_to_clipmap_async(clipmap, ImagePagerKey::Database(key))
    }

    pub fn clipmap_status(
        &self,
        destination: &ImagePagerClipmapDestination,
    ) -> Option<ImagePagerStatus> {
        self.clipmap_status.get(destination).copied()
    }

    fn load_key(
        loader: &mut impl ImagePagerLoader,
        key: &ImagePagerKey,
    ) -> Result<ImageView, MeshiError> {
        match key {
            ImagePagerKey::Disk(path) => loader.load_from_disk(path),
            ImagePagerKey::Database(db_key) => loader.load_from_database(db_key),
            ImagePagerKey::Inline(_) => Ok(ImageView::default()),
        }
    }

    pub fn process_pending(
        &mut self,
        loader: &mut impl ImagePagerLoader,
        backend: &mut impl ImagePagerBackend,
        max_per_tick: usize,
    ) {
        let mut processed = 0;
        while processed < max_per_tick {
            let Some(key) = self.pending.pop_front() else {
                break;
            };

            let Some(entry) = self.entries.get_mut(&key) else {
                continue;
            };
            if entry.status != ImagePagerStatus::Pending {
                continue;
            }

            let load_result = Self::load_key(loader, &key);

            match load_result {
                Ok(view) => {
                    let new_handle = backend.register_image(entry.handle, view);
                    if new_handle != entry.handle {
                        self.handle_to_key.remove(&entry.handle);
                        entry.handle = new_handle;
                        self.handle_to_key.insert(new_handle, key.clone());
                    }
                    entry.status = ImagePagerStatus::Ready;
                }
                Err(_) => {
                    entry.status = ImagePagerStatus::Failed;
                }
            }

            processed += 1;
        }

        while processed < max_per_tick {
            let Some((destination, key)) = self.pending_clipmap.pop_front() else {
                break;
            };

            let status = match Self::load_key(loader, &key) {
                Ok(view) => {
                    let _ = backend.register_image(destination.handle, view);
                    ImagePagerStatus::Ready
                }
                Err(_) => ImagePagerStatus::Failed,
            };
            self.clipmap_status.insert(destination, status);
            processed += 1;
        }
    }

    pub fn release_by_key(&mut self, key: &ImagePagerKey, backend: &mut impl ImagePagerBackend) {
        let Some(entry) = self.entries.remove(key) else {
            return;
        };
        backend.release_image(entry.handle);
        self.handle_to_key.remove(&entry.handle);
    }

    pub fn release_by_handle(
        &mut self,
        handle: BindlessImageHandle,
        backend: &mut impl ImagePagerBackend,
    ) {
        let Some(key) = self.handle_to_key.remove(&handle) else {
            return;
        };
        self.release_by_key(&key, backend);
    }
}

impl Default for ImagePager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dashi::Context;
    use image::{ImageBuffer, Rgba};
    use noren::rdb::imagery::{HostImage, ImageInfo as NorenImageInfo};
    use noren::{DBInfo, RDBFile};
    use std::collections::HashSet;
    use std::fs;

    struct TestBackend {
        next_handle: BindlessImageHandle,
        registered: HashSet<BindlessImageHandle>,
        released: Vec<BindlessImageHandle>,
    }

    impl TestBackend {
        fn new() -> Self {
            Self {
                next_handle: 1,
                registered: HashSet::new(),
                released: Vec::new(),
            }
        }
    }

    impl ImagePagerBackend for TestBackend {
        fn reserve_handle(&mut self) -> BindlessImageHandle {
            let handle = self.next_handle;
            self.next_handle += 1;
            handle
        }

        fn register_image(
            &mut self,
            handle: BindlessImageHandle,
            _view: ImageView,
        ) -> BindlessImageHandle {
            self.registered.insert(handle);
            handle
        }

        fn release_image(&mut self, handle: BindlessImageHandle) {
            self.released.push(handle);
        }
    }

    struct TestLoader;

    impl ImagePagerLoader for TestLoader {
        fn load_from_disk(&mut self, _path: &Path) -> Result<ImageView, MeshiError> {
            Ok(ImageView::default())
        }

        fn load_from_database(&mut self, _key: &DatabaseImageKey) -> Result<ImageView, MeshiError> {
            Ok(ImageView::default())
        }
    }

    #[test]
    fn request_image_returns_stable_handle() {
        let mut pager = ImagePager::new();
        let mut backend = TestBackend::new();
        let key = ImagePagerKey::Disk(PathBuf::from("imagery/test.png"));

        let first = pager.request_image(key.clone(), &mut backend);
        let second = pager.request_image(key.clone(), &mut backend);

        assert_eq!(first, second);
        assert_eq!(pager.status(&key), Some(ImagePagerStatus::Pending));
    }

    #[test]
    fn process_pending_registers_image() {
        let mut pager = ImagePager::new();
        let mut backend = TestBackend::new();
        let mut loader = TestLoader;
        let key = ImagePagerKey::Disk(PathBuf::from("imagery/test.png"));

        let handle = pager.request_image(key.clone(), &mut backend);
        pager.process_pending(&mut loader, &mut backend, 1);

        assert!(backend.registered.contains(&handle));
        assert_eq!(pager.status(&key), Some(ImagePagerStatus::Ready));
    }

    #[test]
    fn clipmap_push_returns_predicted_destination_and_uploads_to_reserved_handle() {
        let mut pager = ImagePager::initialize();
        let mut backend = TestBackend::new();
        let mut loader = TestLoader;
        let mut clipmap = ImagePagerClipmapRing::initialize(2, &mut backend);

        let destination = pager.push_database_to_clipmap_async(
            &mut clipmap,
            DatabaseImageKey {
                project: Some("demo".to_string()),
                asset_key: "imagery/test.png".to_string(),
            },
        );

        assert_eq!(
            pager.clipmap_status(&destination),
            Some(ImagePagerStatus::Pending)
        );

        pager.process_pending(&mut loader, &mut backend, 1);

        assert_eq!(
            pager.clipmap_status(&destination),
            Some(ImagePagerStatus::Ready)
        );
        assert!(backend.registered.contains(&destination.handle));
    }

    #[test]
    fn clipmap_ring_wraps_slots() {
        let mut backend = TestBackend::new();
        let mut clipmap = ImagePagerClipmapRing::initialize(2, &mut backend);

        let first = clipmap.reserve_destination();
        let second = clipmap.reserve_destination();
        let third = clipmap.reserve_destination();

        assert_eq!(first.slot, 0);
        assert_eq!(second.slot, 1);
        assert_eq!(third.slot, 0);
        assert_eq!(first.handle, third.handle);
        assert!(third.generation > second.generation);
    }

    #[test]
    fn disk_loader_creates_image_view() {
        let Ok(mut ctx) = Context::headless(&Default::default()) else {
            return;
        };

        let tmp_dir = std::env::temp_dir();
        let path = tmp_dir.join(format!("image_pager_test_{}.png", std::process::id()));
        let image: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(2, 2, Rgba([1, 2, 3, 4]));
        image.save(&path).expect("save test image");

        let mut loader = ImagePagerDefaultLoader::new(&mut ctx);
        let view = loader.load_from_disk(&path).expect("load image from disk");
        assert!(view.img.valid());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn database_loader_fetches_image() {
        let Ok(mut ctx) = Context::headless(&Default::default()) else {
            return;
        };

        let temp_dir = std::env::temp_dir().join(format!("image_pager_db_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        let imagery_path = temp_dir.join("imagery.rdb");

        let info = NorenImageInfo {
            name: "imagery/test_image".to_string(),
            dim: [2, 2, 1],
            layers: 1,
            format: Format::RGBA8,
            mip_levels: 1,
        };
        let data = vec![255u8; (info.dim[0] * info.dim[1] * 4) as usize];
        let host = HostImage::new(info, data);

        let mut rdb = RDBFile::new();
        rdb.add("imagery/test_image", &host)
            .expect("add imagery entry");
        rdb.save(&imagery_path).expect("save imagery rdb");

        let base_dir = temp_dir.to_string_lossy().to_string();
        let info = DBInfo {
            base_dir: base_dir.as_str(),
            layout_file: None,
            pooled_geometry_uploads: false,
        };
        let mut db = DB::new(&info).expect("create db");
        db.import_dashi_context(&mut ctx);

        let mut loader = ImagePagerDefaultLoader::with_database(&mut ctx, &mut db);
        let view = loader
            .load_from_database(&DatabaseImageKey {
                project: None,
                asset_key: "imagery/test_image".to_string(),
            })
            .expect("load image from database");

        assert!(view.img.valid());

        let _ = fs::remove_file(&imagery_path);
    }
}
