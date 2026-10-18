use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::camera::Camera;
use crate::contour::PALETTE_SLOTS;
use crate::mesh::{RenderMesh, Vertex};
use crate::section::ClipPlane;

pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SAMPLE_COUNT: u32 = 4;
/// How far surfaces are pushed back behind their edges, in pixels along the surface: the
/// slope-scaled depth bias moves a face by its depth change over this many pixels. A face seen
/// head on hardly moves, so the edges on the far side of a thin wall stay hidden however far
/// the view is zoomed out; a fixed offset in pixels reached through walls thinner than it.
const EDGE_OFFSET_PX: f32 = 1.5;
/// Constant part of the surface depth bias, in steps of the depth buffer: covers the rounding
/// between a line and the triangle it lies on where the face is seen head on.
const SURFACE_BIAS_CONSTANT: i32 = 2;
/// How far the surfaces and edges of parts drawn behind the others are pushed away from the
/// viewer, in pixels: where they meet the surfaces of other parts, those show in front.
const BEHIND_PX: f32 = 4.0;
/// Width of [`RenderMesh::wide_edges`] in pixels, as PrePoMax draws a selected part's outline.
pub const WIDE_EDGE_PX: f32 = 3.0;

/// PrePoMax's background gradient: Gainsboro at the top, WhiteSmoke at the bottom.
pub const BACKGROUND: [[f32; 4]; 2] = [
    [220.0 / 255.0, 220.0 / 255.0, 220.0 / 255.0, 1.0],
    [245.0 / 255.0, 245.0 / 255.0, 245.0 / 255.0, 1.0],
];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    lights: [[f32; 4]; 3],
    view_dir: [f32; 4],
    background_top: [f32; 4],
    background_bottom: [f32; 4],
    /// x, y: target size in pixels; z: width of wide edges in pixels; w: their depth offset
    /// towards the viewer in normalized depth units.
    edge: [f32; 4],
    /// x: number of contour bands, 0 when surfaces show their part colour.
    contour: [f32; 4],
    /// Section view plane: unit normal and `normal · point`.
    clip: [f32; 4],
    /// x: 1 when the section view is on, y: how far behind the plane surfaces still show.
    clip_options: [f32; 4],
    /// Colour of the wireframe of the undeformed shape in rgb.
    wireframe: [f32; 4],
    /// x: depth offset of parts drawn behind the others, in normalized depth units.
    behind: [f32; 4],
    palette: [[f32; 4]; PALETTE_SLOTS as usize],
}

/// A vertex or index buffer with its element count; `None` when there is nothing to draw.
struct GpuBuffer {
    buffer: wgpu::Buffer,
    count: u32,
}

struct GpuMesh {
    vertices: Option<GpuBuffer>,
    triangles: Option<GpuBuffer>,
    feature_edges: Option<GpuBuffer>,
    mesh_edges: Option<GpuBuffer>,
    wireframe_edges: Option<GpuBuffer>,
    wide_edges: Option<GpuBuffer>,
    visible: bool,
    /// Surfaces seen through, drawn after everything else without their mesh edges.
    transparent: bool,
    /// Drawn a little away from the viewer, so that the other parts show in front where they
    /// overlap.
    behind: bool,
}

/// The pipelines of the surfaces and lines of a part, either as they lie or pushed behind
/// the other parts.
struct PartPipelines {
    surface: wgpu::RenderPipeline,
    edge: wgpu::RenderPipeline,
    wireframe: wgpu::RenderPipeline,
    mesh_edge: wgpu::RenderPipeline,
}

/// What the viewport draws besides the shaded surfaces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayOptions {
    pub mesh_edges: bool,
    /// Colour surfaces by their normalized vertex scalars in these bands.
    pub contour: Option<ContourPalette>,
    /// Background gradient as RGBA fractions, top and bottom.
    pub background: [[f32; 4]; 2],
    /// Colour of the wireframe of the undeformed shape as RGB fractions; black on the light
    /// background.
    pub wireframe: [f32; 3],
}

/// Colours of the contour bands, minimum first, each covering an equal share of the
/// normalized range; the out-of-range colours of a manual scale are bands of their own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContourPalette {
    colors: [[f32; 3]; PALETTE_SLOTS as usize],
    count: u32,
}

impl ContourPalette {
    /// The first [`PALETTE_SLOTS`] colours; at least two are needed.
    pub fn new(colors: &[[f32; 3]]) -> Self {
        let mut palette = Self {
            colors: [[0.0; 3]; PALETTE_SLOTS as usize],
            count: colors.len().clamp(2, PALETTE_SLOTS as usize) as u32,
        };
        for (slot, color) in palette.colors.iter_mut().zip(colors) {
            *slot = *color;
        }
        palette
    }
}

impl Default for DisplayOptions {
    fn default() -> Self {
        Self {
            mesh_edges: true,
            contour: None,
            background: BACKGROUND,
            wireframe: [0.0; 3],
        }
    }
}

struct Targets {
    width: u32,
    height: u32,
    msaa_view: wgpu::TextureView,
    resolve_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
}

/// Renders the 3D scene into an offscreen texture that the GUI shows as an image.
pub struct ViewportRenderer {
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    background_pipeline: wgpu::RenderPipeline,
    front: PartPipelines,
    behind: PartPipelines,
    transparent_pipeline: wgpu::RenderPipeline,
    wide_edge_pipeline: wgpu::RenderPipeline,
    parts: Vec<GpuMesh>,
    /// Section faces of the parts, drawn while the section view is on.
    sections: Vec<GpuMesh>,
    clip: Option<ClipPlane>,
    /// Draw only the section faces of the parts and their outlines, e.g. to read results on
    /// the plane.
    sections_only: bool,
    targets: Targets,
    /// Camera and options of the image in the color texture; `None` when the scene changed
    /// since. Drawing a large mesh is expensive, and the GUI repaints for every mouse move.
    drawn: Option<(Camera, DisplayOptions)>,
}

impl ViewportRenderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("viewport.wgsl"));
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewport globals"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewport"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Float32],
        };
        let pipeline = |label: &str,
                        vs: &str,
                        fs: &str,
                        buffers: &[Option<wgpu::VertexBufferLayout>],
                        topology: wgpu::PrimitiveTopology,
                        depth: wgpu::DepthStencilState,
                        blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    ..Default::default()
                },
                depth_stencil: Some(depth),
                multisample: wgpu::MultisampleState {
                    count: SAMPLE_COUNT,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let depth_state =
            |write: bool, compare: wgpu::CompareFunction, bias: wgpu::DepthBiasState| {
                wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(write),
                    depth_compare: Some(compare),
                    stencil: Default::default(),
                    bias,
                }
            };
        let background_pipeline = pipeline(
            "viewport background",
            "vs_background",
            "fs_background",
            &[],
            wgpu::PrimitiveTopology::TriangleList,
            depth_state(false, wgpu::CompareFunction::Always, Default::default()),
            None,
        );
        // Transparent surfaces leave the depth buffer to the opaque ones behind them.
        let transparent_pipeline = pipeline(
            "viewport transparent surfaces",
            "vs_surface",
            "fs_transparent_surface",
            &[Some(vertex_layout.clone())],
            wgpu::PrimitiveTopology::TriangleList,
            depth_state(
                false,
                wgpu::CompareFunction::Less,
                wgpu::DepthBiasState {
                    constant: SURFACE_BIAS_CONSTANT,
                    slope_scale: EDGE_OFFSET_PX,
                    clamp: 0.0,
                },
            ),
            Some(wgpu::BlendState::ALPHA_BLENDING),
        );
        // `suffix` picks the vertex shaders: "" as the parts lie, "_behind" pushed back.
        let part_pipelines = |suffix: &str| {
            let line = |label: &str, vs: &str| {
                pipeline(
                    label,
                    &format!("{vs}{suffix}"),
                    "fs_edge",
                    &[Some(vertex_layout.clone())],
                    wgpu::PrimitiveTopology::LineList,
                    depth_state(false, wgpu::CompareFunction::LessEqual, Default::default()),
                    Some(wgpu::BlendState::ALPHA_BLENDING),
                )
            };
            PartPipelines {
                surface: pipeline(
                    "viewport surfaces",
                    &format!("vs_surface{suffix}"),
                    "fs_surface",
                    &[Some(vertex_layout.clone())],
                    wgpu::PrimitiveTopology::TriangleList,
                    depth_state(
                        true,
                        wgpu::CompareFunction::Less,
                        wgpu::DepthBiasState {
                            constant: SURFACE_BIAS_CONSTANT,
                            slope_scale: EDGE_OFFSET_PX,
                            clamp: 0.0,
                        },
                    ),
                    None,
                ),
                edge: line("viewport edges", "vs_edge"),
                wireframe: line("viewport wireframe", "vs_wireframe"),
                // Mesh edges index the surface vertices and take their colour from the shader.
                mesh_edge: line("viewport mesh edges", "vs_mesh_edge"),
            }
        };
        let front = part_pipelines("");
        let behind = part_pipelines("_behind");
        // Wide lines are quads: one instance per segment reads both end vertices of the line
        // list, the vertex shader spreads them across the screen.
        let vertex_size = std::mem::size_of::<Vertex>() as u64;
        let segment_layout = wgpu::VertexBufferLayout {
            array_stride: 2 * vertex_size,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: std::mem::offset_of!(Vertex, color) as u64,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: vertex_size,
                    shader_location: 2,
                },
            ],
        };
        let wide_edge_pipeline = pipeline(
            "viewport wide edges",
            "vs_wide_edge",
            "fs_edge",
            &[Some(segment_layout)],
            wgpu::PrimitiveTopology::TriangleList,
            depth_state(false, wgpu::CompareFunction::LessEqual, Default::default()),
            Some(wgpu::BlendState::ALPHA_BLENDING),
        );

        Self {
            globals,
            bind_group,
            background_pipeline,
            front,
            behind,
            transparent_pipeline,
            wide_edge_pipeline,
            parts: Vec::new(),
            sections: Vec::new(),
            clip: None,
            sections_only: false,
            targets: Targets::new(device, 1, 1),
            drawn: None,
        }
    }

    /// Replaces the scene with one mesh per part; all parts start visible.
    pub fn set_parts(&mut self, device: &wgpu::Device, parts: &[RenderMesh]) {
        self.parts = parts.iter().map(|mesh| upload(device, mesh)).collect();
        self.sections.clear();
        self.drawn = None;
    }

    /// Cuts the scene at a plane, or shows it whole again with `None`.
    pub fn set_clip_plane(&mut self, clip: Option<ClipPlane>) {
        if self.clip != clip {
            self.clip = clip;
            self.drawn = None;
        }
    }

    /// Shows only the section faces and the feature edges of the parts while a section is on.
    pub fn set_sections_only(&mut self, only: bool) {
        if self.sections_only != only {
            self.sections_only = only;
            self.drawn = None;
        }
    }

    /// Section faces of the parts, in the order of the parts.
    pub fn set_sections(&mut self, device: &wgpu::Device, sections: &[RenderMesh]) {
        self.sections = sections.iter().map(|mesh| upload(device, mesh)).collect();
        self.drawn = None;
    }

    pub fn set_part_visible(&mut self, index: usize, visible: bool) {
        if let Some(part) = self.parts.get_mut(index)
            && part.visible != visible
        {
            part.visible = visible;
            self.drawn = None;
        }
    }

    /// Draws a part behind the others where they overlap, or as it lies again.
    pub fn set_part_behind(&mut self, index: usize, behind: bool) {
        if let Some(part) = self.parts.get_mut(index)
            && part.behind != behind
        {
            part.behind = behind;
            self.drawn = None;
        }
    }

    /// Draws the surfaces of a part see-through, or opaque again.
    pub fn set_part_transparent(&mut self, index: usize, transparent: bool) {
        if let Some(part) = self.parts.get_mut(index)
            && part.transparent != transparent
        {
            part.transparent = transparent;
            self.drawn = None;
        }
    }

    /// Recreates the render targets if the size changed; returns true when it did.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) -> bool {
        let (width, height) = (width.max(1), height.max(1));
        if self.targets.width == width && self.targets.height == height {
            return false;
        }
        self.targets = Targets::new(device, width, height);
        self.drawn = None;
        true
    }

    /// The resolved color texture, to be registered with the GUI.
    pub fn color_view(&self) -> &wgpu::TextureView {
        &self.targets.resolve_view
    }

    /// Draws the scene into the color texture, unless it already shows this view of it;
    /// returns whether it drew.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: &Camera,
        options: DisplayOptions,
    ) -> bool {
        if self.drawn.as_ref() == Some(&(camera.clone(), options)) {
            return false;
        }
        self.drawn = Some((camera.clone(), options));
        let aspect = self.targets.width as f32 / self.targets.height as f32;
        let globals = Globals {
            view_proj: camera.view_proj(aspect).to_cols_array_2d(),
            lights: camera.light_directions().map(|l| l.extend(0.0).into()),
            view_dir: (-camera.forward()).extend(0.0).into(),
            background_top: options.background[0],
            background_bottom: options.background[1],
            edge: [
                self.targets.width as f32,
                self.targets.height as f32,
                WIDE_EDGE_PX,
                0.5 * WIDE_EDGE_PX
                    * camera.pixel_size(self.targets.width as f32, self.targets.height as f32)
                    / camera.depth_range(),
            ],
            contour: [
                options.contour.map_or(0.0, |p| p.count as f32),
                0.0,
                0.0,
                0.0,
            ],
            clip: self
                .clip
                .map_or([0.0; 4], |c| c.normal.extend(c.normal.dot(c.point)).into()),
            clip_options: [
                if self.clip.is_some() { 1.0 } else { 0.0 },
                // Section faces lie on the plane; a fraction of a pixel keeps them visible.
                0.25 * camera.pixel_size(self.targets.width as f32, self.targets.height as f32),
                0.0,
                0.0,
            ],
            wireframe: [
                options.wireframe[0],
                options.wireframe[1],
                options.wireframe[2],
                0.0,
            ],
            behind: [
                BEHIND_PX
                    * camera.pixel_size(self.targets.width as f32, self.targets.height as f32)
                    / camera.depth_range(),
                0.0,
                0.0,
                0.0,
            ],
            palette: palette(options.contour),
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("viewport"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("viewport"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.targets.msaa_view,
                    depth_slice: None,
                    resolve_target: Some(&self.targets.resolve_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_pipeline(&self.background_pipeline);
            pass.draw(0..3, 0..1);
            // Section faces belong to their part and share its visibility.
            let sections_only = self.sections_only && self.clip.is_some();
            // Meshes with whether they are drawn whole: with only the sections shown, the
            // parts keep just their outline for orientation.
            let sections = self
                .sections
                .iter()
                .zip(&self.parts)
                .filter(|_| self.clip.is_some())
                .map(|(section, part)| (section, part.visible, true, part.behind));
            let visible = || {
                self.parts
                    .iter()
                    .map(|p| (p, p.visible, !sections_only, p.behind))
                    .chain(sections.clone())
                    .filter(|(_, visible, _, _)| *visible)
                    .map(|(p, _, whole, behind)| (p, whole, behind))
            };
            let pipelines = |behind: bool| if behind { &self.behind } else { &self.front };
            // Opaque meshes drawn whole.
            let solid = || visible().filter(|(p, whole, _)| *whole && !p.transparent);
            for (part, _, behind) in solid() {
                if let (Some(vertices), Some(triangles)) = (&part.vertices, &part.triangles) {
                    pass.set_pipeline(&pipelines(behind).surface);
                    pass.set_vertex_buffer(0, vertices.buffer.slice(..));
                    pass.set_index_buffer(triangles.buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..triangles.count, 0, 0..1);
                }
            }
            if options.mesh_edges {
                for (part, _, behind) in solid() {
                    if let (Some(vertices), Some(edges)) = (&part.vertices, &part.mesh_edges) {
                        pass.set_pipeline(&pipelines(behind).mesh_edge);
                        pass.set_vertex_buffer(0, vertices.buffer.slice(..));
                        pass.set_index_buffer(edges.buffer.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..edges.count, 0, 0..1);
                    }
                }
            }
            for (part, _, behind) in visible().filter(|(_, whole, _)| *whole) {
                if let Some(lines) = &part.wireframe_edges {
                    pass.set_pipeline(&pipelines(behind).wireframe);
                    pass.set_vertex_buffer(0, lines.buffer.slice(..));
                    pass.draw(0..lines.count, 0..1);
                }
            }
            for (part, _, behind) in visible() {
                if let Some(lines) = &part.feature_edges {
                    pass.set_pipeline(&pipelines(behind).edge);
                    pass.set_vertex_buffer(0, lines.buffer.slice(..));
                    pass.draw(0..lines.count, 0..1);
                }
            }
            pass.set_pipeline(&self.wide_edge_pipeline);
            for lines in
                visible().filter_map(|(p, whole, _)| p.wide_edges.as_ref().filter(|_| whole))
            {
                pass.set_vertex_buffer(0, lines.buffer.slice(..));
                pass.draw(0..6, 0..lines.count / 2);
            }
            // Last, so that what lies behind them shows through.
            pass.set_pipeline(&self.transparent_pipeline);
            for (part, _, _) in visible().filter(|(p, whole, _)| *whole && p.transparent) {
                if let (Some(vertices), Some(triangles)) = (&part.vertices, &part.triangles) {
                    pass.set_vertex_buffer(0, vertices.buffer.slice(..));
                    pass.set_index_buffer(triangles.buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..triangles.count, 0, 0..1);
                }
            }
        }
        queue.submit([encoder.finish()]);
        true
    }
}

fn upload(device: &wgpu::Device, mesh: &RenderMesh) -> GpuMesh {
    let buffer = |label: &str, contents: &[u8], count: usize, usage: wgpu::BufferUsages| {
        (count > 0).then(|| GpuBuffer {
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            }),
            count: count as u32,
        })
    };
    let vertex = wgpu::BufferUsages::VERTEX;
    GpuMesh {
        vertices: buffer(
            "part vertices",
            bytemuck::cast_slice(&mesh.vertices),
            mesh.vertices.len(),
            vertex,
        ),
        triangles: buffer(
            "part triangles",
            bytemuck::cast_slice(&mesh.triangles),
            mesh.triangles.len(),
            wgpu::BufferUsages::INDEX,
        ),
        feature_edges: buffer(
            "part feature edges",
            bytemuck::cast_slice(&mesh.feature_edges),
            mesh.feature_edges.len(),
            vertex,
        ),
        mesh_edges: buffer(
            "part mesh edges",
            bytemuck::cast_slice(&mesh.mesh_edges),
            mesh.mesh_edges.len(),
            wgpu::BufferUsages::INDEX,
        ),
        wireframe_edges: buffer(
            "part wireframe edges",
            bytemuck::cast_slice(&mesh.wireframe_edges),
            mesh.wireframe_edges.len(),
            vertex,
        ),
        wide_edges: buffer(
            "part wide edges",
            bytemuck::cast_slice(&mesh.wide_edges),
            mesh.wide_edges.len(),
            vertex,
        ),
        visible: true,
        transparent: false,
        behind: false,
    }
}

fn palette(contour: Option<ContourPalette>) -> [[f32; 4]; PALETTE_SLOTS as usize] {
    let mut palette = [[0.0; 4]; PALETTE_SLOTS as usize];
    if let Some(contour) = contour {
        for (slot, [r, g, b]) in palette.iter_mut().zip(contour.colors) {
            *slot = [r, g, b, 1.0];
        }
    }
    palette
}

impl Targets {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = |label: &str, format, sample_count, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        Self {
            width,
            height,
            msaa_view: texture(
                "viewport color msaa",
                COLOR_FORMAT,
                SAMPLE_COUNT,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            ),
            resolve_view: texture(
                "viewport color",
                COLOR_FORMAT,
                1,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            ),
            depth_view: texture(
                "viewport depth",
                DEPTH_FORMAT,
                SAMPLE_COUNT,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use wgpu::naga;

    /// The shader compiles only when the GUI starts; check it without a GPU.
    #[test]
    fn shader_is_valid() {
        let module = naga::front::wgsl::parse_str(include_str!("viewport.wgsl"))
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(include_str!("viewport.wgsl"))));
        let flags = naga::valid::ValidationFlags::all();
        naga::valid::Validator::new(flags, naga::valid::Capabilities::all())
            .validate(&module)
            .unwrap();
    }
}
