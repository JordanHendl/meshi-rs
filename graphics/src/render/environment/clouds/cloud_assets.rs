use dashi::{
    AspectMask, Context, Format, ImageInfo, ImageView, ImageViewType, SubresourceRange,
};
use glam::UVec3;

#[derive(Clone, Copy, Debug)]
pub struct CloudNoiseSizes {
    pub base_noise_size: u32,
    pub detail_noise_size: u32,
    pub weather_map_size: u32,
    pub blue_noise_size: u32,
}

impl Default for CloudNoiseSizes {
    fn default() -> Self {
        Self {
            base_noise_size: 128,
            detail_noise_size: 32,
            weather_map_size: 256,
            blue_noise_size: 128,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CloudWeatherChannelLayout {
    pub coverage: u8,
    pub cloud_type: u8,
    pub thickness: u8,
    pub reserved: u8,
}

impl Default for CloudWeatherChannelLayout {
    fn default() -> Self {
        Self {
            coverage: 0,
            cloud_type: 1,
            thickness: 2,
            reserved: 3,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CloudAssets {
    pub base_noise: ImageView,
    pub detail_noise: ImageView,
    pub weather_map: ImageView,
    pub blue_noise: ImageView,
    pub base_noise_dims: UVec3,
    pub detail_noise_dims: UVec3,
    pub weather_map_size: u32,
    pub blue_noise_size: u32,
    pub weather_layout: CloudWeatherChannelLayout,
}

impl CloudAssets {
    pub fn new(ctx: &mut Context, sizes: CloudNoiseSizes) -> Self {
        let weather_layout = CloudWeatherChannelLayout::default();
        let base_noise_dims = UVec3::new(sizes.base_noise_size, sizes.base_noise_size, sizes.base_noise_size);
        let detail_noise_dims = UVec3::new(sizes.detail_noise_size, sizes.detail_noise_size, sizes.detail_noise_size);

        let base_noise = create_noise_atlas(
            ctx,
            "[CLOUD] Base Noise",
            base_noise_dims,
            Format::RGBA8,
            1,
            1337,
        );
        let detail_noise = create_noise_atlas(
            ctx,
            "[CLOUD] Detail Noise",
            detail_noise_dims,
            Format::RGBA8,
            2,
            7331,
        );
        let weather_map = create_weather_map(
            ctx,
            "[CLOUD] Weather Map",
            sizes.weather_map_size,
            weather_layout,
        );
        let blue_noise = create_blue_noise(
            ctx,
            "[CLOUD] Blue Noise",
            sizes.blue_noise_size,
        );

        Self {
            base_noise,
            detail_noise,
            weather_map,
            blue_noise,
            base_noise_dims,
            detail_noise_dims,
            weather_map_size: sizes.weather_map_size,
            blue_noise_size: sizes.blue_noise_size,
            weather_layout,
        }
    }

    pub fn weather_map_view(&self) -> ImageView {
        self.weather_map
    }
}

fn create_view(image: dashi::Handle<dashi::Image>, layers: u32) -> ImageView {
    ImageView {
        img: image,
        aspect: AspectMask::Color,
        view_type: ImageViewType::Type2D,
        range: SubresourceRange::new(0, 1, 0, layers),
    }
}

fn create_noise_atlas(
    ctx: &mut Context,
    name: &str,
    dims: UVec3,
    format: Format,
    seed: u32,
    layer_seed: u32,
) -> ImageView {
    let width = dims.x * dims.z;
    let height = dims.y;
    let mut data = vec![0u8; (width * height * 4) as usize];
    for z in 0..dims.z {
        for y in 0..dims.y {
            for x in 0..dims.x {
                let idx = ((y * width + (z * dims.x + x)) * 4) as usize;
                let value = hash_noise(x, y, z, seed ^ (layer_seed.wrapping_mul(z))) as u8;
                data[idx] = value;
                data[idx + 1] = value;
                data[idx + 2] = value;
                data[idx + 3] = 255;
            }
        }
    }

    let info = ImageInfo {
        debug_name: name,
        dim: [width, height, 1],
        layers: 1,
        format,
        mip_levels: 1,
        initial_data: Some(&data),
        ..Default::default()
    };

    let image = ctx.make_image(&info).expect("create noise atlas");
    create_view(image, 1)
}

fn create_weather_map(
    ctx: &mut Context,
    name: &str,
    size: u32,
    layout: CloudWeatherChannelLayout,
) -> ImageView {
    let mut data = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let idx = ((y * size + x) * 4) as usize;
            let nx = x as f32 / size as f32;
            let ny = y as f32 / size as f32;
            let coverage = fbm_2d(nx, ny, 2.0, 4, 0x1bad_cafe).powf(1.15);
            let cloud_type = fbm_2d(nx, ny, 1.3, 3, 0x55aa_22ff);
            let thickness = fbm_2d(nx, ny, 3.1, 4, 0x0ddc_4411).powf(0.9);
            let channels = [
                (coverage * 255.0) as u8,
                (cloud_type * 255.0) as u8,
                (thickness * 255.0) as u8,
                255,
            ];
            data[idx + layout.coverage as usize] = channels[0];
            data[idx + layout.cloud_type as usize] = channels[1];
            data[idx + layout.thickness as usize] = channels[2];
            data[idx + layout.reserved as usize] = channels[3];
        }
    }

    let info = ImageInfo {
        debug_name: name,
        dim: [size, size, 1],
        layers: 1,
        format: Format::RGBA8,
        mip_levels: 1,
        initial_data: Some(&data),
        ..Default::default()
    };

    let image = ctx.make_image(&info).expect("create weather map");
    create_view(image, 1)
}

fn create_blue_noise(ctx: &mut Context, name: &str, size: u32) -> ImageView {
    let mut data = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let idx = ((y * size + x) * 4) as usize;
            let v0 = hash_noise(x, y, 0, 0x1234567) as u8;
            let v1 = hash_noise(x, y, 1, 0x7654321) as u8;
            data[idx] = v0;
            data[idx + 1] = v1;
            data[idx + 2] = 0;
            data[idx + 3] = 255;
        }
    }

    let info = ImageInfo {
        debug_name: name,
        dim: [size, size, 1],
        layers: 1,
        format: Format::RGBA8,
        mip_levels: 1,
        initial_data: Some(&data),
        ..Default::default()
    };

    let image = ctx.make_image(&info).expect("create blue noise");
    create_view(image, 1)
}

fn hash_noise(x: u32, y: u32, z: u32, seed: u32) -> u32 {
    let mut v = x.wrapping_mul(374761393)
        ^ y.wrapping_mul(668265263)
        ^ z.wrapping_mul(2246822519)
        ^ seed.wrapping_mul(3266489917);
    v = (v ^ (v >> 13)).wrapping_mul(1274126177);
    v ^ (v >> 16)
}

fn hash_to_unit(v: u32) -> f32 {
    v as f32 / u32::MAX as f32
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn value_noise_2d(x: f32, y: f32, freq: f32, seed: u32) -> f32 {
    let xf = x * freq;
    let yf = y * freq;
    let x0 = xf.floor() as u32;
    let y0 = yf.floor() as u32;
    let x1 = x0.wrapping_add(1);
    let y1 = y0.wrapping_add(1);
    let fx = xf - x0 as f32;
    let fy = yf - y0 as f32;
    let u = smoothstep(fx);
    let v = smoothstep(fy);

    let h00 = hash_to_unit(hash_noise(x0, y0, 0, seed));
    let h10 = hash_to_unit(hash_noise(x1, y0, 0, seed));
    let h01 = hash_to_unit(hash_noise(x0, y1, 0, seed));
    let h11 = hash_to_unit(hash_noise(x1, y1, 0, seed));

    let hx0 = h00 + (h10 - h00) * u;
    let hx1 = h01 + (h11 - h01) * u;
    hx0 + (hx1 - hx0) * v
}

fn fbm_2d(x: f32, y: f32, base_freq: f32, octaves: u32, seed: u32) -> f32 {
    let mut total = 0.0;
    let mut amplitude = 1.0;
    let mut max = 0.0;
    let mut freq = base_freq;
    for octave in 0..octaves {
        total += value_noise_2d(x, y, freq, seed.wrapping_add(octave)) * amplitude;
        max += amplitude;
        amplitude *= 0.5;
        freq *= 2.0;
    }
    if max > 0.0 {
        total / max
    } else {
        0.0
    }
}
