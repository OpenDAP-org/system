//! Screenshots (Main + Volume −, controls.rs): the shell draws what the display shows into an
//! offscreen buffer, without the touch indicator, and saves it as PNG where Android users look for
//! them: `~/Pictures/Screenshots/Screenshot_YYYYMMDD-HHMMSS.png` (the shell's user, UTC).

use std::{
    io::BufWriter,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            Bind, Color32F, ExportMem, Frame, Offscreen, Renderer,
            element::RenderElement,
            gles::{GlesRenderer, GlesTexture},
            utils::draw_render_elements,
        },
    },
    utils::{Physical, Rectangle, Scale, Size, Transform},
};

/// Draws `elements` (topmost first) on `clear` at `size`, and saves the picture. Returns its path.
pub fn capture<E: RenderElement<GlesRenderer>>(
    renderer: &mut GlesRenderer,
    elements: &[E],
    size: Size<i32, Physical>,
    scale: Scale<f64>,
    clear: Color32F,
) -> Result<PathBuf, String> {
    let e = |err: smithay::backend::renderer::gles::GlesError| err.to_string();
    let buffer_size = Size::from((size.w, size.h));
    let mut texture: GlesTexture = renderer
        .create_buffer(Fourcc::Abgr8888, buffer_size)
        .map_err(e)?;
    let mut target = renderer.bind(&mut texture).map_err(e)?;
    let whole = Rectangle::from_size(size);
    let drawn = {
        let mut frame = renderer
            .render(&mut target, size, Transform::Normal)
            .map_err(e)?;
        frame.clear(clear, &[whole]).map_err(e)?;
        draw_render_elements(&mut frame, scale, elements, &[whole]).map_err(e)?;
        frame.finish().map_err(e)?
    };
    // The drawing must be done before it is read back.
    renderer.wait(&drawn).map_err(e)?;
    let mapping = renderer
        .copy_framebuffer(&target, Rectangle::from_size(buffer_size), Fourcc::Abgr8888)
        .map_err(e)?;
    let pixels = renderer.map_texture(&mapping).map_err(e)?;
    save(pixels, size.w as u32, size.h as u32)
}

fn save(rgba: &[u8], width: u32, height: u32) -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("no home directory")?;
    let dir = home.join("Pictures/Screenshots");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("Screenshot_{}.png", stamp()));
    let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    Ok(path)
}

/// `YYYYMMDD-HHMMSS` in UTC, from the system clock (days to civil date: Howard Hinnant's algorithm).
fn stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}
