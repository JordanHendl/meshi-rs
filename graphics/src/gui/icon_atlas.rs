use std::collections::HashMap;

use dashi::{
    AspectMask, Context, Format, Handle, Image, ImageInfo, ImageView, ImageViewType,
    SubresourceRange,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ToolbarIconId(u32);

impl ToolbarIconId {
    pub const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MenuGlyphId(u32);

impl MenuGlyphId {
    pub const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GuiIconRect {
    pub position: [u32; 2],
    pub size: [u32; 2],
}

impl GuiIconRect {
    pub fn new(position: [u32; 2], size: [u32; 2]) -> Self {
        Self { position, size }
    }

    fn max(&self) -> [u32; 2] {
        [
            self.position[0] + self.size[0],
            self.position[1] + self.size[1],
        ]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GuiIconUv {
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub pixel_size: [u32; 2],
}

#[derive(Debug, Clone)]
pub struct GuiIconAtlasInfo<'a> {
    pub debug_name: &'a str,
    pub size: [u32; 2],
    pub format: Format,
}

impl<'a> GuiIconAtlasInfo<'a> {
    pub fn rgba8(debug_name: &'a str, size: [u32; 2]) -> Self {
        Self {
            debug_name,
            size,
            format: Format::RGBA8,
        }
    }
}

#[derive(Debug)]
pub enum GuiIconAtlasError {
    InvalidPixelData { expected: usize, actual: usize },
    InvalidIconRect { max: [u32; 2], atlas_size: [u32; 2] },
    ImageCreationFailed(String),
}

impl std::fmt::Display for GuiIconAtlasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GuiIconAtlasError::InvalidPixelData { expected, actual } => write!(
                f,
                "Invalid icon atlas pixel data length (expected {expected}, got {actual})"
            ),
            GuiIconAtlasError::InvalidIconRect { max, atlas_size } => {
                write!(f, "Icon rect {:?} exceeds atlas size {:?}", max, atlas_size)
            }
            GuiIconAtlasError::ImageCreationFailed(message) => {
                write!(f, "Failed to create icon atlas image: {message}")
            }
        }
    }
}

impl std::error::Error for GuiIconAtlasError {}

#[derive(Debug)]
pub struct GuiIconAtlas {
    texture: ImageView,
    texture_id: Option<u32>,
    size: [u32; 2],
    toolbar_icons: HashMap<ToolbarIconId, GuiIconUv>,
    menu_glyphs: HashMap<MenuGlyphId, GuiIconUv>,
    next_toolbar_id: u32,
    next_menu_id: u32,
}

impl GuiIconAtlas {
    pub fn new(
        ctx: &mut Context,
        info: GuiIconAtlasInfo<'_>,
        pixels: &[u8],
    ) -> Result<Self, GuiIconAtlasError> {
        let expected_len = info.size[0] as usize * info.size[1] as usize * 4;
        if pixels.len() != expected_len {
            return Err(GuiIconAtlasError::InvalidPixelData {
                expected: expected_len,
                actual: pixels.len(),
            });
        }

        let image = ctx
            .make_image(&ImageInfo {
                debug_name: info.debug_name,
                dim: [info.size[0], info.size[1], 1],
                layers: 1,
                format: info.format,
                mip_levels: 1,
                initial_data: Some(pixels),
                ..Default::default()
            })
            .map_err(|err| GuiIconAtlasError::ImageCreationFailed(format!("{err:?}")))?;

        let texture = Self::create_view(image, 1);

        Ok(Self {
            texture,
            texture_id: None,
            size: info.size,
            toolbar_icons: HashMap::new(),
            menu_glyphs: HashMap::new(),
            next_toolbar_id: 0,
            next_menu_id: 0,
        })
    }

    pub fn from_view(texture: ImageView, size: [u32; 2], texture_id: Option<u32>) -> Self {
        Self {
            texture,
            texture_id,
            size,
            toolbar_icons: HashMap::new(),
            menu_glyphs: HashMap::new(),
            next_toolbar_id: 0,
            next_menu_id: 0,
        }
    }

    pub fn texture_view(&self) -> ImageView {
        self.texture
    }

    pub fn texture_id(&self) -> Option<u32> {
        self.texture_id
    }

    pub fn set_texture_id(&mut self, texture_id: u32) {
        self.texture_id = Some(texture_id);
    }

    pub fn register_toolbar_icon(
        &mut self,
        rect: GuiIconRect,
    ) -> Result<ToolbarIconId, GuiIconAtlasError> {
        let id = ToolbarIconId(self.next_toolbar_id);
        self.next_toolbar_id = self.next_toolbar_id.saturating_add(1);
        let uv = self.uv_for_rect(rect)?;
        self.toolbar_icons.insert(id, uv);
        Ok(id)
    }

    pub fn register_menu_glyph(
        &mut self,
        rect: GuiIconRect,
    ) -> Result<MenuGlyphId, GuiIconAtlasError> {
        let id = MenuGlyphId(self.next_menu_id);
        self.next_menu_id = self.next_menu_id.saturating_add(1);
        let uv = self.uv_for_rect(rect)?;
        self.menu_glyphs.insert(id, uv);
        Ok(id)
    }

    pub fn toolbar_icon(&self, id: ToolbarIconId) -> Option<GuiIconUv> {
        self.toolbar_icons.get(&id).copied()
    }

    pub fn menu_glyph(&self, id: MenuGlyphId) -> Option<GuiIconUv> {
        self.menu_glyphs.get(&id).copied()
    }

    pub fn atlas_size(&self) -> [u32; 2] {
        self.size
    }

    fn uv_for_rect(&self, rect: GuiIconRect) -> Result<GuiIconUv, GuiIconAtlasError> {
        let max = rect.max();
        if max[0] > self.size[0] || max[1] > self.size[1] {
            return Err(GuiIconAtlasError::InvalidIconRect {
                max,
                atlas_size: self.size,
            });
        }

        let size = [self.size[0] as f32, self.size[1] as f32];
        let uv_min = [
            rect.position[0] as f32 / size[0],
            rect.position[1] as f32 / size[1],
        ];
        let uv_max = [max[0] as f32 / size[0], max[1] as f32 / size[1]];

        Ok(GuiIconUv {
            uv_min,
            uv_max,
            pixel_size: rect.size,
        })
    }

    fn create_view(image: Handle<Image>, layers: u32) -> ImageView {
        ImageView {
            img: image,
            aspect: AspectMask::Color,
            view_type: ImageViewType::Type2D,
            range: SubresourceRange::new(0, 1, 0, layers),
        }
    }
}
