use anyhow::{anyhow, Context, Result};
use image::{imageops::FilterType, DynamicImage, ImageOutputFormat};
use screenshots::Screen;
use std::io::Cursor;

/// Whether the OS will actually hand us the screen.
///
/// macOS is the reason this exists: when Screen Recording is denied,
/// `CGDisplayCreateImage` still returns a perfectly valid image — of the
/// desktop with every window stripped out — so capture "succeeds" and the
/// proctor silently watches a wallpaper. Asking the OS directly is the only
/// reliable way to tell.
#[cfg(target_os = "macos")]
pub fn screen_capture_permitted() -> bool {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    // Preflight first; if it has never been asked, request it so the student
    // gets the system prompt instead of silently sharing nothing.
    unsafe { CGPreflightScreenCaptureAccess() || CGRequestScreenCaptureAccess() }
}

/// Windows and Linux have no equivalent gate for desktop capture.
#[cfg(not(target_os = "macos"))]
pub fn screen_capture_permitted() -> bool {
    true
}

pub fn capture_primary_screen_jpeg(quality: u8, target_width: u32) -> Result<Vec<u8>> {
    let screens = Screen::all().context("failed to enumerate screens")?;
    let screen = screens
        .first()
        .ok_or_else(|| anyhow!("no display found for screen capture"))?;

    let image = screen.capture().context("failed to capture screen")?;
    let dynamic = DynamicImage::ImageRgba8(image);
    let scaled = resize_to_width(dynamic, target_width);
    let rgb = scaled.to_rgb8();

    let mut cursor = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(rgb)
        .write_to(&mut cursor, ImageOutputFormat::Jpeg(quality))
        .context("failed to encode screenshot as JPEG")?;

    Ok(cursor.into_inner())
}

fn resize_to_width(image: DynamicImage, target_width: u32) -> DynamicImage {
    if target_width == 0 || image.width() <= target_width {
        return image;
    }

    let ratio = image.height() as f32 / image.width() as f32;
    let target_height = (target_width as f32 * ratio).round().max(1.0) as u32;
    image.resize_exact(target_width, target_height, FilterType::Triangle)
}
