//! Screenshot of the 3D view with its annotations, or of another part of the window such as a
//! plot, copied to the clipboard or saved as PNG.
//!
//! egui captures the whole window; the image is cropped to the view afterwards. The capture
//! is requested one frame after the click so that the closing menu is no longer painted.

use std::path::Path;

use egui::{ColorImage, Context, Rect, UserData, ViewportCommand};

/// Where a screenshot goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Clipboard,
    File,
}

/// A capture: where it goes and the part of the window it shows.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Request {
    target: Target,
    /// Part of the window in points; `None` is the 3D view.
    region: Option<Rect>,
}

#[derive(Default)]
pub struct Screenshot {
    /// Requested capture, sent with the next frame.
    pending: Option<Request>,
}

impl Screenshot {
    pub fn request(&mut self, target: Target) {
        self.pending = Some(Request {
            target,
            region: None,
        });
    }

    /// A capture of `region` of the window, in points, instead of the 3D view.
    pub fn request_region(&mut self, target: Target, region: Rect) {
        self.pending = Some(Request {
            target,
            region: Some(region),
        });
    }

    /// Sends a pending request and delivers captured images; `view` is the 3D view in points.
    pub fn update(&mut self, ctx: &Context, view: Rect, output: &mut Vec<String>) {
        let captured: Vec<_> = ctx.input(|i| {
            i.raw
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Screenshot {
                        user_data, image, ..
                    } => user_data
                        .data
                        .as_ref()
                        .and_then(|d| d.downcast_ref::<Request>())
                        .map(|request| (*request, image.clone())),
                    _ => None,
                })
                .collect()
        });
        for (request, image) in captured {
            let image = crop(
                &image,
                request.region.unwrap_or(view),
                ctx.pixels_per_point(),
            );
            let name = if request.region.is_some() {
                "plot"
            } else {
                "screenshot"
            };
            deliver(ctx, request.target, image, name, output);
        }
        if let Some(request) = self.pending.take() {
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(UserData::new(request)));
            ctx.request_repaint();
        }
    }
}

/// The part of the window image covered by `rect`, clamped to the image.
pub fn crop(image: &ColorImage, rect: Rect, pixels_per_point: f32) -> ColorImage {
    let [width, height] = image.size;
    let px = |v: f32, max: usize| ((v * pixels_per_point).round().max(0.0) as usize).min(max);
    let (x0, y0) = (px(rect.min.x, width), px(rect.min.y, height));
    let (x1, y1) = (px(rect.max.x, width), px(rect.max.y, height));
    image.region_by_pixels([x0, y0], [x1 - x0, y1 - y0])
}

/// Copies or saves the image; `name` is what it shows, lower case.
fn deliver(ctx: &Context, target: Target, image: ColorImage, name: &str, output: &mut Vec<String>) {
    let [width, height] = image.size;
    let mut chars = name.chars();
    let title = chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect::<String>())
        .unwrap_or_default();
    match target {
        Target::Clipboard => {
            ctx.copy_image(image);
            output.push(format!(
                "{title} ({width} x {height}) copied to the clipboard"
            ));
        }
        Target::File => {
            let Some(path) = rfd::FileDialog::new()
                .set_title(format!("Save {title} As"))
                .add_filter("PNG image", &["png"])
                .set_file_name(format!("{name}.png"))
                .save_file()
            else {
                return;
            };
            let path = if path.extension().is_none() {
                path.with_extension("png")
            } else {
                path
            };
            match write_png(&path, &image) {
                Ok(()) => output.push(format!("{title} saved: {}", path.display())),
                Err(err) => output.push(format!(
                    "{title} could not be saved ({}): {err}",
                    path.display()
                )),
            }
        }
    }
}

fn write_png(path: &Path, image: &ColorImage) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, image.size[0] as u32, image.size[1] as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    let rgba: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(|c| c.to_srgba_unmultiplied())
        .collect();
    writer.write_image_data(&rgba)?;
    writer.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Color32, pos2};

    #[test]
    fn crop_is_clamped_and_scaled() {
        let mut image = ColorImage::filled([10, 8], Color32::WHITE);
        image.pixels[2 * 10 + 4] = Color32::RED;
        let part = crop(
            &image,
            Rect::from_min_max(pos2(2.0, 1.0), pos2(100.0, 100.0)),
            2.0,
        );
        assert_eq!(part.size, [6, 6]);
        assert_eq!(part.pixels[0], Color32::RED);
    }

    #[test]
    fn writes_png() {
        let path = std::env::temp_dir().join(format!("plx-screenshot-{}.png", std::process::id()));
        write_png(&path, &ColorImage::filled([3, 2], Color32::RED)).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(&bytes[1..4], b"PNG");
        assert_eq!(&bytes[16..24], &[0, 0, 0, 3, 0, 0, 0, 2]);
    }
}
