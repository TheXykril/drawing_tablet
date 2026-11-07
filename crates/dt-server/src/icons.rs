//! Nano Banana generated icons, embedded and uploaded to egui once per context.
use eframe::egui::{self, ColorImage, Context, Image, TextureHandle, TextureOptions, Vec2};

#[derive(Clone, Copy)]
pub enum Icon {
    Start,
    Stop,
    Copy,
    Help,
    Clear,
    Monitor,
    Tablet,
    Pen,
    Stylus,
}
impl Icon {
    fn asset(self) -> (&'static str, &'static [u8]) {
        match self {
            Self::Start => ("start", include_bytes!("../assets/icons/start.png")),
            Self::Stop => ("stop", include_bytes!("../assets/icons/stop.png")),
            Self::Copy => ("copy", include_bytes!("../assets/icons/copy.png")),
            Self::Help => ("help", include_bytes!("../assets/icons/help.png")),
            Self::Clear => ("clear", include_bytes!("../assets/icons/clear.png")),
            Self::Monitor => ("monitor", include_bytes!("../assets/icons/monitor.png")),
            Self::Tablet => ("tablet", include_bytes!("../assets/icons/tablet.png")),
            Self::Pen => ("pen", include_bytes!("../assets/icons/pen.png")),
            Self::Stylus => ("stylus", include_bytes!("../assets/icons/stylus.png")),
        }
    }
}

pub fn icon_image(ctx: &Context, icon: Icon, size: f32) -> Image<'static> {
    let (name, bytes) = icon.asset();
    let id = egui::Id::new(("generated-icon", name));
    let existing = ctx.data_mut(|data| data.get_temp::<TextureHandle>(id));
    let texture = existing.unwrap_or_else(|| {
        let image = image::load_from_memory(bytes)
            .expect("embedded icon PNG")
            .into_rgba8();
        let color_image = ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        );
        let texture = ctx.load_texture(name, color_image, TextureOptions::LINEAR);
        ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
        texture
    });
    Image::new((texture.id(), Vec2::splat(size))).fit_to_exact_size(Vec2::splat(size))
}
