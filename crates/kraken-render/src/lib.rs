//! GPU renderer for Kraken.
//!
//! The renderer knows nothing about terminals: it draws a [`Frame`], which is
//! a list of coloured rectangles (backgrounds, selection, cursor, chrome) and
//! lines of styled text. Text is shaped with cosmic-text and drawn through
//! glyphon's glyph atlas. Shaped lines are cached and only re-shaped when
//! their content changes, which keeps redraws cheap while output scrolls.

use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, Style, SwashCache, TextArea, TextAtlas, TextBounds,
    TextRenderer, Viewport, Weight, Wrap,
};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub use glyphon;

/// An axis-aligned rectangle in physical pixels, with an sRGBA colour.
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: [u8; 4],
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32, rgb: [u8; 3]) -> Self {
        Rect { x, y, w, h, color: [rgb[0], rgb[1], rgb[2], 255] }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontKind {
    /// The terminal font, snapped to the cell grid.
    Mono,
    /// The system UI font for chrome (titles, labels).
    Ui,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Span {
    pub text: String,
    pub color: [u8; 3],
    pub bold: bool,
    pub italic: bool,
}

#[derive(Debug, Clone)]
pub struct TextLine {
    pub x: f32,
    pub y: f32,
    pub font: FontKind,
    /// Font size in physical pixels.
    pub size: f32,
    /// Line box height in physical pixels.
    pub line_height: f32,
    pub spans: Vec<Span>,
    /// Clip rectangle (left, top, right, bottom); `None` = whole window.
    pub clip: Option<[i32; 4]>,
    /// For [`FontKind::Mono`]: snap glyph advances to this cell width.
    pub cell_width: Option<f32>,
}

/// An image to draw: pixels (uploaded once, cached by `key`) and where.
#[derive(Debug, Clone)]
pub struct ImageDraw {
    pub key: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: std::sync::Arc<Vec<u8>>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Scissor (left, top, right, bottom) — usually the pane.
    pub clip: [i32; 4],
}

/// Everything needed to draw one window.
#[derive(Debug, Default)]
pub struct Frame {
    pub clear: [u8; 3],
    /// Background opacity (used only when the window is transparent).
    pub clear_alpha: f32,
    /// Drawn before text (cell backgrounds, selection, block cursor).
    pub under: Vec<Rect>,
    pub lines: Vec<TextLine>,
    /// Drawn after text (underlines, beam cursor, pane dimming).
    pub over: Vec<Rect>,
    /// Inline images, drawn after text and before `over`.
    pub images: Vec<ImageDraw>,
    /// A second layer above everything (welcome screen, shortcut sheet):
    /// its rects are drawn first, then its text on top.
    pub overlay_rects: Vec<Rect>,
    pub overlay_lines: Vec<TextLine>,
}

impl Frame {
    pub fn clear_all(&mut self) {
        self.under.clear();
        self.lines.clear();
        self.over.clear();
        self.images.clear();
        self.overlay_rects.clear();
        self.overlay_lines.clear();
    }
}

/// GPU and font resources shared by every window.
pub struct Shared {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub cache: Cache,
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    rect_pipeline: Option<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
    image_pipeline: Option<(wgpu::TextureFormat, wgpu::RenderPipeline, wgpu::BindGroupLayout, wgpu::Sampler)>,
    /// The resolved monospace family name.
    pub mono_family: String,
    pub ligatures: bool,
}

impl Shared {
    pub async fn new(
        instance: wgpu::Instance,
        compatible_surface: &wgpu::Surface<'_>,
        preferred_fonts: &[String],
        ligatures: bool,
    ) -> Result<Self, String> {
        let t0 = std::time::Instant::now();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(compatible_surface),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("No usable GPU was found ({e})."))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| format!("The GPU could not be initialised ({e})."))?;
        log::info!("[startup] GPU adapter + device in {} ms", t0.elapsed().as_millis());
        let t_gpu = std::time::Instant::now();
        let cache = Cache::new(&device);
        let font_system = FontSystem::new();
        log::info!("[startup] font system loaded in {} ms", t_gpu.elapsed().as_millis());
        let mono_family = pick_family(&font_system, preferred_fonts, ligatures);
        log::info!("using font family {mono_family:?}");
        Ok(Shared {
            instance,
            adapter,
            device,
            queue,
            cache,
            font_system,
            swash_cache: SwashCache::new(),
            rect_pipeline: None,
            image_pipeline: None,
            mono_family,
            ligatures,
        })
    }

    /// Re-pick the terminal font after a settings change.
    pub fn set_font_families(&mut self, preferred: &[String], ligatures: bool) {
        self.mono_family = pick_family(&self.font_system, preferred, ligatures);
        self.ligatures = ligatures;
    }

    /// Measure the terminal cell (width, height) for a font size in pixels.
    pub fn cell_size(&mut self, size_px: f32, line_height: f32) -> (f32, f32) {
        let h = (size_px * line_height).round();
        let mut buf = Buffer::new(&mut self.font_system, Metrics::new(size_px, h));
        buf.set_size(None, None);
        let attrs = Attrs::new().family(Family::Name(&self.mono_family));
        buf.set_text("MMMMMMMMMM", &attrs, Shaping::Basic, None);
        buf.shape_until_scroll(&mut self.font_system, false);
        let w = buf.layout_runs().next().map(|r| r.line_w / 10.0).unwrap_or(size_px * 0.6);
        (w, h)
    }

    fn image_pipeline(&mut self, format: wgpu::TextureFormat) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout, wgpu::Sampler) {
        if let Some((f, p, l, smp)) = &self.image_pipeline
            && *f == format
        {
            return (p.clone(), l.clone(), smp.clone());
        }
        let (p, l, smp) = create_image_pipeline(&self.device, format);
        self.image_pipeline = Some((format, p.clone(), l.clone(), smp.clone()));
        (p, l, smp)
    }

    fn rect_pipeline(&mut self, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        if let Some((f, p)) = &self.rect_pipeline
            && *f == format
        {
            return p.clone();
        }
        let p = create_rect_pipeline(&self.device, format);
        self.rect_pipeline = Some((format, p.clone()));
        p
    }
}

/// Families to try for one configured name. Fonts often ship a no-ligature
/// twin ("JetBrainsMono" vs "JetBrainsMonoNL"); prefer the right one.
fn variants(name: &str, ligatures: bool) -> Vec<String> {
    let twin = if ligatures {
        name.replace("MonoNL", "Mono")
    } else if name.contains("MonoNL") {
        name.to_string()
    } else {
        name.replacen("Mono", "MonoNL", 1)
    };
    if twin == name { vec![name.to_string()] } else { vec![twin, name.to_string()] }
}

fn pick_family(fs: &FontSystem, preferred: &[String], ligatures: bool) -> String {
    let preferred: Vec<String> = preferred.iter().flat_map(|p| variants(p, ligatures)).collect();
    for want in &preferred {
        let found = fs.db().faces().any(|f| f.families.iter().any(|(name, _)| name.eq_ignore_ascii_case(want)));
        if found {
            return want.clone();
        }
    }
    "Menlo".into()
}

struct CachedLine {
    hash: u64,
    buffer: Buffer,
}

/// Per-window renderer: a surface plus the glyph atlas and text caches.
pub struct WindowRenderer {
    /// CPU time of the last frame before waiting for a drawable (text
    /// shaping, glyph atlas, uploads).
    pub last_prepare: std::time::Duration,
    /// Text lines shaped in the last frame (the rest came from the cache).
    pub last_shaped: usize,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    viewport: Viewport,
    atlas: TextAtlas,
    text: TextRenderer,
    overlay_text: TextRenderer,
    rects: wgpu::Buffer,
    rect_capacity: usize,
    lines: Vec<CachedLine>,
    overlay_cache: Vec<CachedLine>,
    /// GPU textures for images, by `ImageDraw::key`, with the frame they were last used.
    textures: std::collections::HashMap<u64, (wgpu::BindGroup, u64)>,
    image_quads: Option<wgpu::Buffer>,
    frame_no: u64,
    /// When set, the next presented frame is also saved here as a BMP (debug aid).
    pub capture: Option<std::path::PathBuf>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RectInstance {
    /// x0, y0, x1, y1 in normalised device coordinates.
    pos: [f32; 4],
    /// Linear RGBA.
    color: [f32; 4],
}

impl WindowRenderer {
    pub fn new(shared: &mut Shared, surface: wgpu::Surface<'static>, width: u32, height: u32) -> Self {
        let caps = surface.get_capabilities(&shared.adapter);
        let format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&shared.device, &config);
        let viewport = Viewport::new(&shared.device, &shared.cache);
        let mut atlas = TextAtlas::new(&shared.device, &shared.queue, &shared.cache, format);
        let text = TextRenderer::new(&mut atlas, &shared.device, wgpu::MultisampleState::default(), None);
        let overlay_text = TextRenderer::new(&mut atlas, &shared.device, wgpu::MultisampleState::default(), None);
        let rect_capacity = 4096;
        let rects = create_rect_buffer(&shared.device, rect_capacity);
        WindowRenderer {
            surface,
            config,
            viewport,
            atlas,
            text,
            overlay_text,
            rects,
            rect_capacity,
            lines: Vec::new(),
            overlay_cache: Vec::new(),
            textures: std::collections::HashMap::new(),
            image_quads: None,
            frame_no: 0,
            last_prepare: std::time::Duration::ZERO,
            last_shaped: 0,
            capture: None,
        }
    }

    /// Switch between an opaque window and one whose background can show
    /// through (`Frame::clear_alpha`). Returns false if unsupported.
    pub fn set_transparent(&mut self, shared: &Shared, transparent: bool) -> bool {
        let caps = self.surface.get_capabilities(&shared.adapter);
        let mode = if transparent {
            [wgpu::CompositeAlphaMode::PostMultiplied, wgpu::CompositeAlphaMode::PreMultiplied]
                .into_iter()
                .find(|m| caps.alpha_modes.contains(m))
        } else {
            Some(wgpu::CompositeAlphaMode::Opaque)
        };
        let Some(mode) = mode else { return false };
        if self.config.alpha_mode != mode {
            self.config.alpha_mode = mode;
            self.surface.configure(&shared.device, &self.config);
        }
        true
    }

    pub fn resize(&mut self, shared: &Shared, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&shared.device, &self.config);
    }

    /// Draw a frame. Returns `false` if the surface was not ready and the
    /// caller should request another redraw.
    pub fn render_frame(&mut self, shared: &mut Shared, frame: &Frame) -> bool {
        let started = std::time::Instant::now();
        let (w, h) = (self.config.width, self.config.height);
        self.viewport.update(&shared.queue, Resolution { width: w, height: h });

        self.last_shaped =
            update_cache(shared, &mut self.lines, &frame.lines) + update_cache(shared, &mut self.overlay_cache, &frame.overlay_lines);
        for (renderer, lines, cache) in
            [(&mut self.text, &frame.lines, &self.lines), (&mut self.overlay_text, &frame.overlay_lines, &self.overlay_cache)]
        {
            let areas = lines.iter().zip(cache).map(|(line, cached)| text_area(line, cached, w, h));
            if let Err(e) = renderer.prepare(
                &shared.device,
                &shared.queue,
                &mut shared.font_system,
                &mut self.atlas,
                &self.viewport,
                areas,
                &mut shared.swash_cache,
            ) {
                log::error!("text prepare failed: {e:?}");
            }
        }

        // Upload rectangles: `under` first, then `over`.
        let to_instance = |r: &Rect| {
            let x0 = r.x / w as f32 * 2.0 - 1.0;
            let x1 = (r.x + r.w) / w as f32 * 2.0 - 1.0;
            let y0 = 1.0 - r.y / h as f32 * 2.0;
            let y1 = 1.0 - (r.y + r.h) / h as f32 * 2.0;
            RectInstance { pos: [x0, y0, x1, y1], color: linear_rgba(r.color) }
        };
        let instances: Vec<RectInstance> =
            frame.under.iter().chain(frame.over.iter()).chain(frame.overlay_rects.iter()).map(to_instance).collect();
        if instances.len() > self.rect_capacity {
            self.rect_capacity = instances.len().next_power_of_two();
            self.rects = create_rect_buffer(&shared.device, self.rect_capacity);
        }
        if !instances.is_empty() {
            shared.queue.write_buffer(&self.rects, 0, bytemuck::cast_slice(&instances));
        }
        let n_under = frame.under.len() as u32;
        let n_total = instances.len() as u32;

        // Images: upload any new ones, then one quad each.
        self.frame_no += 1;
        let image_draws = self.prepare_images(shared, frame, w, h);

        // Everything before this is our CPU work; acquiring the drawable can
        // block until vsync, so it is measured separately.
        self.last_prepare = started.elapsed();
        let surface_tex = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                self.surface.configure(&shared.device, &self.config);
                drop(t);
                return false;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&shared.device, &self.config);
                return false;
            }
            _ => return false,
        };
        let view = surface_tex.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let pipeline = shared.rect_pipeline(self.config.format);
        let mut encoder = shared.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("kraken frame") });
        {
            let mut c = linear_rgba([frame.clear[0], frame.clear[1], frame.clear[2], 255]);
            match self.config.alpha_mode {
                wgpu::CompositeAlphaMode::Opaque => {}
                wgpu::CompositeAlphaMode::PreMultiplied => {
                    let a = frame.clear_alpha.clamp(0.0, 1.0);
                    c = [c[0] * a, c[1] * a, c[2] * a, a];
                }
                _ => c[3] = frame.clear_alpha.clamp(0.0, 1.0),
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("kraken pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: c[0] as f64, g: c[1] as f64, b: c[2] as f64, a: c[3] as f64 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_vertex_buffer(0, self.rects.slice(..));
            if n_under > 0 {
                pass.draw(0..4, 0..n_under);
            }
            if let Err(e) = self.text.render(&self.atlas, &self.viewport, &mut pass) {
                log::error!("text render failed: {e:?}");
            }
            if let (Some((pipe, binds)), Some(quads)) = (&image_draws, &self.image_quads) {
                pass.set_pipeline(pipe);
                pass.set_vertex_buffer(0, quads.slice(..));
                for (i, (key, clip)) in binds.iter().enumerate() {
                    let Some((bg, _)) = self.textures.get(key) else { continue };
                    let l = clip[0].clamp(0, w as i32) as u32;
                    let t = clip[1].clamp(0, h as i32) as u32;
                    let r = clip[2].clamp(0, w as i32) as u32;
                    let b = clip[3].clamp(0, h as i32) as u32;
                    if r <= l || b <= t {
                        continue;
                    }
                    pass.set_scissor_rect(l, t, r - l, b - t);
                    pass.set_bind_group(0, bg, &[]);
                    pass.draw(0..4, i as u32..i as u32 + 1);
                }
                pass.set_scissor_rect(0, 0, w, h);
            }
            if n_total > n_under {
                pass.set_pipeline(&pipeline);
                pass.set_vertex_buffer(0, self.rects.slice(..));
                pass.draw(0..4, n_under..n_total);
            }
            if !frame.overlay_lines.is_empty()
                && let Err(e) = self.overlay_text.render(&self.atlas, &self.viewport, &mut pass)
            {
                log::error!("overlay text render failed: {e:?}");
            }
        }
        let readback = self.capture.is_some().then(|| {
            let bpr = (w * 4).div_ceil(256) * 256;
            let buf = shared.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("kraken capture"),
                size: (bpr * h) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &surface_tex.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buf,
                    layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: None },
                },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            (buf, bpr)
        });
        shared.queue.submit(Some(encoder.finish()));
        if let (Some((buf, bpr)), Some(path)) = (readback, self.capture.take()) {
            buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            let _ = shared.device.poll(wgpu::PollType::wait_indefinitely());
            let view = buf.slice(..).get_mapped_range();
            let Ok(data) = view else { return true };
            let bgra = !matches!(self.config.format, wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb);
            if let Err(e) = write_bmp(&path, &data, w, h, bpr, bgra) {
                log::error!("capture failed: {e}");
            }
        }
        shared.queue.present(surface_tex);
        self.atlas.trim();
        true
    }
}

impl WindowRenderer {
    /// Upload new images, drop ones unused for a while, and write one quad per
    /// draw. Returns the pipeline and, per quad, (texture key, scissor).
    #[allow(clippy::type_complexity)]
    fn prepare_images(
        &mut self,
        shared: &mut Shared,
        frame: &Frame,
        w: u32,
        h: u32,
    ) -> Option<(wgpu::RenderPipeline, Vec<(u64, [i32; 4])>)> {
        // Forget textures not drawn for ~10 s (at 60 fps), so memory stays bounded.
        let now = self.frame_no;
        self.textures.retain(|_, (_, used)| now.saturating_sub(*used) < 600);
        if frame.images.is_empty() {
            return None;
        }
        let (pipe, layout, sampler) = shared.image_pipeline(self.config.format);
        let mut quads = Vec::with_capacity(frame.images.len());
        let mut binds = Vec::with_capacity(frame.images.len());
        for img in &frame.images {
            if img.width == 0 || img.height == 0 || img.rgba.len() < (img.width * img.height * 4) as usize {
                continue;
            }
            let entry = self.textures.entry(img.key).or_insert_with(|| {
                let size = wgpu::Extent3d { width: img.width, height: img.height, depth_or_array_layers: 1 };
                let tex = shared.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("kraken image"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                shared.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &img.rgba,
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(img.width * 4), rows_per_image: Some(img.height) },
                    size,
                );
                let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
                let bg = shared.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("kraken image bind"),
                    layout: &layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                    ],
                });
                (bg, now)
            });
            entry.1 = now;
            let x0 = img.x / w as f32 * 2.0 - 1.0;
            let x1 = (img.x + img.w) / w as f32 * 2.0 - 1.0;
            let y0 = 1.0 - img.y / h as f32 * 2.0;
            let y1 = 1.0 - (img.y + img.h) / h as f32 * 2.0;
            quads.push(RectInstance { pos: [x0, y0, x1, y1], color: [0.0, 0.0, 1.0, 1.0] });
            binds.push((img.key, img.clip));
        }
        let bytes = bytemuck::cast_slice::<RectInstance, u8>(&quads);
        let need = bytes.len() as u64;
        if self.image_quads.as_ref().is_none_or(|b| b.size() < need) {
            self.image_quads = Some(shared.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("kraken image quads"),
                size: need.next_power_of_two().max(256),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        if let Some(buf) = &self.image_quads {
            shared.queue.write_buffer(buf, 0, bytes);
        }
        Some((pipe, binds))
    }
}

const IMAGE_SHADER: &str = r#"
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32,
           @location(0) rect: vec4<f32>,
           @location(1) _unused: vec4<f32>) -> Out {
    let right = (vi & 1u) == 1u;
    let down = (vi & 2u) == 2u;
    var o: Out;
    o.pos = vec4<f32>(select(rect.x, rect.z, right), select(rect.y, rect.w, down), 0.0, 1.0);
    o.uv = vec2<f32>(select(0.0, 1.0, right), select(0.0, 1.0, down));
    return o;
}

@fragment
fn fs_main(i: Out) -> @location(0) vec4<f32> {
    return textureSample(tex, smp, i.uv);
}
"#;

fn create_image_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout, wgpu::Sampler) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("kraken image shader"),
        source: wgpu::ShaderSource::Wgsl(IMAGE_SHADER.into()),
    });
    let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("kraken image layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("kraken image pipeline layout"),
        bind_group_layouts: &[Some(&bind_layout)],
        ..Default::default()
    });
    let attributes = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4];
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("kraken image pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<RectInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &attributes,
            })],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, ..Default::default() },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        cache: None,
        multiview_mask: None,
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("kraken image sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    (pipeline, bind_layout, sampler)
}

/// Re-shape only the lines whose content changed since last frame.
/// Shape only what's new. The cache is keyed by *content* (the hash leaves
/// out position, which is applied when drawing), so when output scrolls, each
/// line's already-shaped buffer moves with it instead of being re-shaped.
/// Returns how many lines had to be shaped.
fn update_cache(shared: &mut Shared, cache: &mut Vec<CachedLine>, lines: &[TextLine]) -> usize {
    let hashes: Vec<u64> = lines.iter().map(|l| hash_line(l, &shared.mono_family, shared.ligatures)).collect();
    // Fast path: nothing moved or changed.
    if cache.len() == lines.len() && cache.iter().zip(&hashes).all(|(c, h)| c.hash == *h) {
        return 0;
    }
    let mut old: std::collections::HashMap<u64, Vec<Buffer>> = std::collections::HashMap::with_capacity(cache.len());
    for c in cache.drain(..) {
        old.entry(c.hash).or_default().push(c.buffer);
    }
    let mut shaped = 0;
    for (line, hash) in lines.iter().zip(hashes) {
        let buffer = match old.get_mut(&hash).and_then(Vec::pop) {
            Some(b) => b,
            None => {
                shaped += 1;
                shape_line(shared, line)
            }
        };
        cache.push(CachedLine { hash, buffer });
    }
    shaped
}

fn text_area<'a>(line: &TextLine, cached: &'a CachedLine, w: u32, h: u32) -> TextArea<'a> {
    let full = TextBounds { left: 0, top: 0, right: w as i32, bottom: h as i32 };
    TextArea {
        buffer: &cached.buffer,
        left: line.x,
        top: line.y,
        scale: 1.0,
        bounds: line.clip.map(|[l, t, r, b]| TextBounds { left: l, top: t, right: r, bottom: b }).unwrap_or(full),
        default_color: Color::rgb(255, 255, 255),
        custom_glyphs: &[],
    }
}

fn write_bmp(path: &std::path::Path, data: &[u8], w: u32, h: u32, bpr: u32, bgra: bool) -> std::io::Result<()> {
    let mut out = Vec::with_capacity(54 + (w * h * 4) as usize);
    let size = 54 + w * h * 4;
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(-(h as i32)).to_le_bytes()); // top-down
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    for y in 0..h {
        let row = &data[(y * bpr) as usize..(y * bpr + w * 4) as usize];
        for px in row.as_chunks::<4>().0 {
            if bgra {
                out.extend_from_slice(&[px[0], px[1], px[2], px[3]]);
            } else {
                out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
            }
        }
    }
    std::fs::write(path, out)
}

fn hash_line(line: &TextLine, family: &str, ligatures: bool) -> u64 {
    let mut h = DefaultHasher::new();
    family.hash(&mut h);
    ligatures.hash(&mut h);
    line.font.hash(&mut h);
    line.size.to_bits().hash(&mut h);
    line.line_height.to_bits().hash(&mut h);
    line.cell_width.map(f32::to_bits).hash(&mut h);
    line.spans.hash(&mut h);
    h.finish()
}

fn shape_line(shared: &mut Shared, line: &TextLine) -> Buffer {
    let fs = &mut shared.font_system;
    let mut buffer = Buffer::new(fs, Metrics::new(line.size, line.line_height));
    buffer.set_wrap(Wrap::None);
    buffer.set_size(None, Some(line.line_height));
    let family = match line.font {
        FontKind::Mono => Family::Name(&shared.mono_family),
        FontKind::Ui => Family::SansSerif,
    };
    let mut base = Attrs::new().family(family);
    if line.font == FontKind::Mono && !shared.ligatures {
        let mut f = glyphon::cosmic_text::FontFeatures::new();
        f.disable(glyphon::cosmic_text::FeatureTag::STANDARD_LIGATURES)
            .disable(glyphon::cosmic_text::FeatureTag::CONTEXTUAL_LIGATURES)
            .disable(glyphon::cosmic_text::FeatureTag::CONTEXTUAL_ALTERNATES);
        base = base.font_features(f);
    }
    let spans = line.spans.iter().map(|s| {
        let mut a = base.clone().color(Color::rgb(s.color[0], s.color[1], s.color[2]));
        if s.bold {
            a = a.weight(Weight::BOLD);
        }
        if s.italic {
            a = a.style(Style::Italic);
        }
        (s.text.as_str(), a)
    });
    buffer.set_rich_text(spans, &base, Shaping::Advanced, None);
    // Snap every glyph (including emoji / CJK fallbacks) to the cell grid.
    buffer.set_monospace_width(line.cell_width);
    buffer.shape_until_scroll(fs, false);
    buffer
}

fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_rgba(c: [u8; 4]) -> [f32; 4] {
    [srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]), c[3] as f32 / 255.0]
}

fn create_rect_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("kraken rects"),
        size: (capacity * std::mem::size_of::<RectInstance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

const RECT_SHADER: &str = r#"
struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32,
           @location(0) rect: vec4<f32>,
           @location(1) color: vec4<f32>) -> Out {
    let x = select(rect.x, rect.z, (vi & 1u) == 1u);
    let y = select(rect.y, rect.w, (vi & 2u) == 2u);
    var o: Out;
    o.pos = vec4<f32>(x, y, 0.0, 1.0);
    o.color = color;
    return o;
}

@fragment
fn fs_main(i: Out) -> @location(0) vec4<f32> {
    return i.color;
}
"#;

fn create_rect_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("kraken rect shader"),
        source: wgpu::ShaderSource::Wgsl(RECT_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("kraken rect layout"), ..Default::default() });
    let attributes = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("kraken rect pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<RectInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &attributes,
            })],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, ..Default::default() },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        cache: None,
        multiview_mask: None,
    })
}
