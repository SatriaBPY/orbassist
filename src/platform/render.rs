//! Menggambar ke permukaan 32-bit premultiplied (GDI+) untuk jendela berlapis.
//!
//! Semua panggilan GDI+ berada di modul ini. Pemanggil hanya melihat tipe aman
//! [`Surface`] dan [`Image`].

use std::ffi::c_void;
use std::mem::size_of;
use std::path::Path;
use std::ptr::null_mut;
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use thiserror::Error;
use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, HBITMAP, HDC,
    HGDIOBJ, SelectObject,
};
use windows::Win32::Graphics::GdiPlus::{
    CompositingModeSourceOver, FlushIntentionSync, FontStyleBold, FontStyleRegular,
    GdipCreateBitmapFromFile, GdipCreateBitmapFromScan0, GdipCreateFont,
    GdipCreateFontFamilyFromName, GdipCreatePen1, GdipCreateSolidFill, GdipCreateStringFormat,
    GdipDeleteBrush, GdipDeleteFont, GdipDeleteFontFamily, GdipDeleteGraphics, GdipDeletePen,
    GdipDeleteStringFormat, GdipDisposeImage, GdipDrawEllipse, GdipDrawImageRectI, GdipDrawString,
    GdipFillEllipse, GdipFlush, GdipGetImageGraphicsContext, GdipGetImageHeight, GdipGetImageWidth,
    GdipGraphicsClear, GdipSetCompositingMode, GdipSetPixelOffsetMode, GdipSetSmoothingMode,
    GdipSetStringFormatAlign, GdipSetStringFormatFlags, GdipSetStringFormatLineAlign,
    GdipSetTextRenderingHint, GdiplusStartup, GdiplusStartupInput, GpBitmap, GpBrush, GpFont,
    GpFontFamily, GpGraphics, GpImage, GpPen, GpSolidFill, GpStringFormat,
    PixelOffsetModeHighQuality, SmoothingModeAntiAlias, Status, StringAlignment,
    StringAlignmentCenter, StringAlignmentNear, StringFormatFlagsNoWrap,
    TextRenderingHintAntiAlias, UnitPixel,
};
use windows::Win32::UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow};
use windows::core::HSTRING;

/// ARGB (0xAARRGGBB) yang dipakai GDI+.
pub type Color = u32;

/// Membuat warna ARGB.
pub const fn argb(r: u8, g: u8, b: u8, a: u8) -> Color {
    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

/// Nilai `PixelFormat32bppPARGB` dari `gdipluspixelformats.h`.
const PIXEL_FORMAT_32BPP_PARGB: i32 = 0x000E_200B;

#[derive(Debug, Error)]
#[error("GDI+ gagal dengan status {0}")]
pub struct GdiError(pub i32);

fn check(status: Status) -> Result<()> {
    if status.0 == 0 {
        Ok(())
    } else {
        Err(GdiError(status.0).into())
    }
}

/// Inisialisasi GDI+ sekali untuk seluruh proses.
fn ensure_gdiplus() -> Result<()> {
    static TOKEN: OnceLock<usize> = OnceLock::new();
    if TOKEN.get().is_some() {
        return Ok(());
    }
    static INIT: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = INIT.lock().unwrap_or_else(|poison| poison.into_inner());

    if TOKEN.get().is_some() {
        return Ok(());
    }
    let input = GdiplusStartupInput {
        GdiplusVersion: 1,
        ..Default::default()
    };
    let mut token = 0usize;
    let mut output = Default::default();
    // SAFETY: input dan output menunjuk ke variabel stack yang hidup selama
    // panggilan; GDI+ menyimpan token untuk dipakai sepanjang proses.
    let status = unsafe { GdiplusStartup(&mut token, &input, &mut output) };
    check(status).context("GdiplusStartup gagal")?;
    let _ = TOKEN.set(token);
    Ok(())
}

/// Permukaan gambar 32-bit dengan alpha premultiplied, siap dipakai
/// `UpdateLayeredWindow`. Hanya dipakai dari thread UI.
pub struct Surface {
    width: i32,
    height: i32,
    hdc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    bits: *mut u8,
    image: *mut GpImage,
    graphics: *mut GpGraphics,
}

impl Surface {
    pub fn new(width: i32, height: i32) -> Result<Self> {
        if width <= 0 || height <= 0 {
            bail!("ukuran permukaan tidak valid ({width}x{height})");
        }
        ensure_gdiplus()?;

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bits: *mut c_void = null_mut();
        // SAFETY: bmi valid dan `bits` menerima pointer memori DIB.
        let bitmap = unsafe { CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) }
            .context("CreateDIBSection gagal")?;
        if bits.is_null() {
            // SAFETY: `bitmap` baru dibuat dan belum dipakai.
            unsafe {
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
            }
            bail!("CreateDIBSection tidak memberi pointer piksel");
        }

        // SAFETY: CreateCompatibleDC dengan NULL membuat DC memori.
        let hdc = unsafe { CreateCompatibleDC(None) };
        if hdc.is_invalid() {
            // SAFETY: bitmap baru dibuat dan belum dipakai.
            unsafe {
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
            }
            bail!("CreateCompatibleDC gagal");
        }
        // SAFETY: hdc valid dan bitmap baru; handle lama disimpan untuk Drop.
        let old_bitmap = unsafe { SelectObject(hdc, HGDIOBJ(bitmap.0)) };

        let mut gp_bitmap: *mut GpBitmap = null_mut();
        check(unsafe {
            GdipCreateBitmapFromScan0(
                width,
                height,
                width * 4,
                PIXEL_FORMAT_32BPP_PARGB,
                Some(bits as *const u8),
                &mut gp_bitmap,
            )
        })
        .context("GdipCreateBitmapFromScan0 gagal")?;

        let mut graphics: *mut GpGraphics = null_mut();
        // SAFETY: GpBitmap dan GpImage memiliki representasi handle yang sama.
        check(unsafe { GdipGetImageGraphicsContext(gp_bitmap.cast(), &mut graphics) })
            .context("GdipGetImageGraphicsContext gagal")?;

        let surface = Self {
            width,
            height,
            hdc,
            bitmap,
            old_bitmap,
            bits: bits.cast(),
            image: gp_bitmap.cast(),
            graphics,
        };

        check(unsafe { GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias) })?;
        check(unsafe { GdipSetTextRenderingHint(graphics, TextRenderingHintAntiAlias) })?;
        check(unsafe { GdipSetPixelOffsetMode(graphics, PixelOffsetModeHighQuality) })?;
        check(unsafe { GdipSetCompositingMode(graphics, CompositingModeSourceOver) })?;
        surface.clear()?;
        Ok(surface)
    }

    pub fn clear(&self) -> Result<()> {
        check(unsafe { GdipGraphicsClear(self.graphics, 0) })
    }

    /// Pointer mentah ke piksel BGRA premultiplied (dipakai untuk ikon tray).
    pub fn bits(&self) -> *const u8 {
        self.bits
    }

    pub fn fill_circle(&self, cx: f32, cy: f32, radius: f32, color: Color) -> Result<()> {
        let brush = Brush::new(color)?;
        check(unsafe {
            GdipFillEllipse(
                self.graphics,
                brush.0,
                cx - radius,
                cy - radius,
                radius * 2.0,
                radius * 2.0,
            )
        })
    }

    pub fn stroke_circle(
        &self,
        cx: f32,
        cy: f32,
        radius: f32,
        width: f32,
        color: Color,
    ) -> Result<()> {
        let pen = Pen::new(color, width)?;
        check(unsafe {
            GdipDrawEllipse(
                self.graphics,
                pen.0,
                cx - radius,
                cy - radius,
                radius * 2.0,
                radius * 2.0,
            )
        })
    }

    /// Menggambar teks satu baris di dalam kotak, tanpa wrapping.
    pub fn text(
        &self,
        text: &str,
        rect: (f32, f32, f32, f32),
        size: f32,
        color: Color,
        align_center: bool,
        bold: bool,
    ) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        let family = FontFamily::new("Segoe UI")?;
        let style = if bold {
            FontStyleBold.0
        } else {
            FontStyleRegular.0
        };
        let font = Font::new(family.0, size, style)?;
        let format = TextFormat::new(align_center)?;
        let brush = Brush::new(color)?;

        let layout = windows::Win32::Graphics::GdiPlus::RectF {
            X: rect.0,
            Y: rect.1,
            Width: rect.2,
            Height: rect.3,
        };
        let wide = HSTRING::from(text);
        let length = text.chars().count() as i32;
        check(unsafe {
            GdipDrawString(
                self.graphics,
                &wide,
                length,
                font.0,
                &layout,
                format.0,
                brush.0,
            )
        })
    }

    pub fn draw_image(&self, image: &Image, cx: f32, cy: f32, box_size: f32) -> Result<()> {
        let (width, height) = image.size()?;
        if width == 0 || height == 0 {
            return Ok(());
        }
        let scale = (box_size / width as f32).min(box_size / height as f32);
        let draw_width = (width as f32 * scale).round().max(1.0) as i32;
        let draw_height = (height as f32 * scale).round().max(1.0) as i32;
        let x = (cx - draw_width as f32 / 2.0).round() as i32;
        let y = (cy - draw_height as f32 / 2.0).round() as i32;
        check(unsafe { GdipDrawImageRectI(self.graphics, image.0, x, y, draw_width, draw_height) })
    }

    /// Menyelesaikan gambar ke jendela berlapis di posisi dan opacity tertentu.
    pub fn present(&self, hwnd: HWND, x: i32, y: i32, opacity: f32) -> Result<()> {
        check(unsafe { GdipFlush(self.graphics, FlushIntentionSync) })?;
        let size = SIZE {
            cx: self.width,
            cy: self.height,
        };
        let source = POINT { x: 0, y: 0 };
        let destination = POINT { x, y };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: (opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // SAFETY: hdc memegang bitmap permukaan ini, ukuran dan posisi valid, dan
        // jendela memiliki style WS_EX_LAYERED.
        unsafe {
            UpdateLayeredWindow(
                hwnd,
                None,
                Some(&destination),
                Some(&size),
                Some(self.hdc),
                Some(&source),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        }
        .context("UpdateLayeredWindow gagal")
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: semua handle dimiliki struct ini dan belum dihapus.
        unsafe {
            if !self.graphics.is_null() {
                let _ = GdipDeleteGraphics(self.graphics);
            }
            if !self.image.is_null() {
                let _ = GdipDisposeImage(self.image);
            }
            let _ = SelectObject(self.hdc, self.old_bitmap);
            let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
            let _ = DeleteDC(self.hdc);
        }
    }
}

/// Gambar (PNG/JPG/BMP) yang dimuat GDI+.
pub struct Image(*mut GpImage);

impl Image {
    pub fn load(path: &Path) -> Result<Self> {
        ensure_gdiplus()?;
        let wide = HSTRING::from(path.as_os_str().to_string_lossy().as_ref());
        let mut bitmap: *mut GpBitmap = null_mut();
        check(unsafe { GdipCreateBitmapFromFile(&wide, &mut bitmap) })
            .with_context(|| format!("gambar ikon {} tidak bisa dibuka", path.display()))?;
        Ok(Self(bitmap.cast()))
    }

    pub fn size(&self) -> Result<(u32, u32)> {
        let mut width = 0u32;
        let mut height = 0u32;
        check(unsafe { GdipGetImageWidth(self.0, &mut width) })?;
        check(unsafe { GdipGetImageHeight(self.0, &mut height) })?;
        Ok((width, height))
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: gambar dimiliki struct ini dan belum dihapus.
            unsafe {
                let _ = GdipDisposeImage(self.0);
            }
        }
    }
}

struct Brush(*mut GpBrush);

impl Brush {
    fn new(color: Color) -> Result<Self> {
        let mut fill: *mut GpSolidFill = null_mut();
        check(unsafe { GdipCreateSolidFill(color, &mut fill) })?;
        Ok(Self(fill as *mut GpBrush))
    }
}

impl Drop for Brush {
    fn drop(&mut self) {
        // SAFETY: brush dibuat di `new` dan hanya dihapus di sini.
        unsafe {
            let _ = GdipDeleteBrush(self.0);
        }
    }
}

struct Pen(*mut GpPen);

impl Pen {
    fn new(color: Color, width: f32) -> Result<Self> {
        let mut pen: *mut GpPen = null_mut();
        check(unsafe { GdipCreatePen1(color, width, UnitPixel, &mut pen) })?;
        Ok(Self(pen))
    }
}

impl Drop for Pen {
    fn drop(&mut self) {
        // SAFETY: pen dibuat di `new` dan hanya dihapus di sini.
        unsafe {
            let _ = GdipDeletePen(self.0);
        }
    }
}

struct FontFamily(*mut GpFontFamily);

impl FontFamily {
    fn new(name: &str) -> Result<Self> {
        let wide = HSTRING::from(name);
        let mut family: *mut GpFontFamily = null_mut();
        check(unsafe { GdipCreateFontFamilyFromName(&wide, null_mut(), &mut family) })?;
        Ok(Self(family))
    }
}

impl Drop for FontFamily {
    fn drop(&mut self) {
        // SAFETY: family dibuat di `new` dan hanya dihapus di sini.
        unsafe {
            let _ = GdipDeleteFontFamily(self.0);
        }
    }
}

struct Font(*mut GpFont);

impl Font {
    fn new(family: *mut GpFontFamily, size: f32, style: i32) -> Result<Self> {
        let mut font: *mut GpFont = null_mut();
        check(unsafe { GdipCreateFont(family, size, style, UnitPixel, &mut font) })?;
        Ok(Self(font))
    }
}

impl Drop for Font {
    fn drop(&mut self) {
        // SAFETY: font dibuat di `new` dan hanya dihapus di sini.
        unsafe {
            let _ = GdipDeleteFont(self.0);
        }
    }
}

struct TextFormat(*mut GpStringFormat);

impl TextFormat {
    fn new(center: bool) -> Result<Self> {
        let mut format: *mut GpStringFormat = null_mut();
        check(unsafe { GdipCreateStringFormat(0, 0, &mut format) })?;
        let alignment: StringAlignment = if center {
            StringAlignmentCenter
        } else {
            StringAlignmentNear
        };
        check(unsafe { GdipSetStringFormatAlign(format, alignment) })?;
        check(unsafe { GdipSetStringFormatLineAlign(format, StringAlignmentCenter) })?;
        check(unsafe { GdipSetStringFormatFlags(format, StringFormatFlagsNoWrap.0) })?;
        Ok(Self(format))
    }
}

impl Drop for TextFormat {
    fn drop(&mut self) {
        // SAFETY: format dibuat di `new` dan hanya dihapus di sini.
        unsafe {
            let _ = GdipDeleteStringFormat(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argb_packs_channels() {
        assert_eq!(argb(0x12, 0x34, 0x56, 0xFF), 0xFF12_3456);
        assert_eq!(argb(0, 0, 0, 0), 0);
    }

    #[test]
    fn invalid_surface_size_is_rejected() {
        assert!(Surface::new(0, 10).is_err());
    }

    #[test]
    fn surface_reports_size() {
        let surface = Surface::new(64, 32).expect("surface");
        assert!(!surface.bits().is_null());
    }
}
