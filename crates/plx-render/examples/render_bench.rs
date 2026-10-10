//! Measures how long the viewport takes to draw a mesh, without the GUI.
//!
//!     cargo run --release -p plx-render --example render_bench -- model.inp [width height frames]
//!
//! Draws the parts contoured by a made-up result with mesh edges, orbiting a little each
//! frame like a mouse drag, and prints the GPU time per frame.

use std::time::Instant;

use glam::DVec3;
use plx_render::contour::normalize;
use plx_render::{
    Camera, ContourPalette, DisplayOptions, RenderMesh, ViewportRenderer, part_color,
    part_render_mesh,
};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .expect("usage: render_bench model.inp [width height frames]");
    let mut number = |default: u32| args.next().map_or(default, |a| a.parse().unwrap());
    let (width, height, frames) = (number(1920), number(1080), number(60));

    let start = Instant::now();
    let mesh = plx_io::inp::read_inp(path.as_ref())
        .expect("readable .inp")
        .mesh;
    let coords = mesh.coords();
    let (min, max) = mesh.bounds().expect("nodes");
    let origin = (DVec3::from(min) + DVec3::from(max)) * 0.5;
    // A smooth made-up result along the longest axis, so every contour band shows.
    let axis = (DVec3::from(max) - DVec3::from(min)).max_element();
    let values: Vec<f32> = coords
        .iter()
        .map(|c| {
            let d = DVec3::from(*c) - DVec3::from(min);
            (d.x.max(d.y).max(d.z) / axis) as f32
        })
        .collect();
    let scalars = normalize(&values, 0.0, 1.0);
    let parts: Vec<RenderMesh> = (mesh.parts.iter().enumerate())
        .map(|(index, part)| {
            let skin = plx_mesh::extract_part_skin(&mesh, part, 30.0);
            let color = part_color(index);
            part_render_mesh(coords, &skin, origin, color, |_| None, 30.0, Some(&scalars))
        })
        .collect();
    let count = |f: fn(&RenderMesh) -> usize| parts.iter().map(f).sum::<usize>();
    println!(
        "{} nodes, {} elements: {} vertices, {} triangles, {} mesh edge and {} feature edge segments ({:.0} ms to build)",
        mesh.node_count(),
        mesh.element_count(),
        count(|m| m.vertices.len()),
        count(|m| m.triangles.len() / 3),
        count(|m| m.mesh_edges.len() / 2),
        count(|m| m.feature_edges.len() / 2),
        start.elapsed().as_secs_f64() * 1e3,
    );

    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("a graphics adapter");
    println!("adapter: {}", adapter.get_info().name);
    let (device, queue) =
        pollster::block_on(adapter.request_device(&Default::default())).expect("a device");
    let mut renderer = ViewportRenderer::new(&device);
    renderer.resize(&device, width, height);
    let start = Instant::now();
    renderer.set_parts(&device, &parts);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    println!("upload: {:.1} ms", start.elapsed().as_secs_f64() * 1e3);

    let bounds = parts.iter().filter_map(RenderMesh::bounds);
    let (lo, hi) = bounds.fold((glam::Vec3::MAX, glam::Vec3::MIN), |(lo, hi), (a, b)| {
        (lo.min(a), hi.max(b))
    });
    let mut camera = Camera::default();
    camera.fit(lo, hi);
    let palette = ContourPalette::new(&plx_render::contour::band_colors(&Default::default()));
    for mesh_edges in [true, false] {
        let options = DisplayOptions {
            mesh_edges,
            contour: Some(palette),
        };
        let mut draw = |camera: &mut Camera| {
            camera.orbit(3.0, 1.0);
            renderer.render(&device, &queue, camera, options);
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        };
        // The first frames compile shaders and warm caches.
        for _ in 0..3 {
            draw(&mut camera);
        }
        let start = Instant::now();
        for _ in 0..frames {
            draw(&mut camera);
        }
        let ms = start.elapsed().as_secs_f64() * 1e3 / frames as f64;
        println!(
            "{width}x{height}, mesh edges {}: {ms:.2} ms per frame ({:.0} fps)",
            if mesh_edges { "on " } else { "off" },
            1e3 / ms
        );
    }
}
