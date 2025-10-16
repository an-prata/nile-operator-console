use crate::stand::{StandState, ValveState};
use eframe::egui::{self, Color32};

const COLOR_OPEN: Color32 = Color32::from_rgb(0, 255, 0);
const COLOR_CLOSED: Color32 = Color32::from_rgb(255, 0, 0);
const COLOR_UNKNOWN: Color32 = Color32::from_rgb(128, 128, 128);

/// A wrapper over an [`egui::ColorImage`] and [`egui::TextureHandle`] for handling a changing image
/// and reloading its corrosponding texture.
///
/// [`egui::ColorImage`]: egui::ColorImage
/// [`egui::TextureHandle`]: egui::TextureHandle
pub struct Diagram {
    pub image: egui::ColorImage,
    pub base_image: egui::ColorImage,
    pub texture: Option<egui::TextureHandle>,
}

impl Diagram {
    /// Create a new [`Diagram`] from the given slice of bytes.
    ///
    /// [`Diagram`]: Diagram
    pub fn from_bytes(bytes: &[u8]) -> image::ImageResult<Self> {
        let image = image::load_from_memory(bytes)?;
        let image_buf = image.to_rgba8();
        let pixels = image_buf.as_flat_samples();

        let base_image = egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as _, image.height() as _],
            pixels.as_slice(),
        );

        Ok(Self {
            image: base_image.clone(),
            base_image,
            texture: None,
        })
    }

    /// Reload the texture for the given [`Diagram`] using the given [`egui::Context`].
    ///
    /// [`Diagram`]: Diagram
    /// [`egui::Context`]: egui::Context
    pub fn reload_texture(&mut self, ctx: &egui::Context) {
        self.texture = Some(ctx.load_texture(
            "diagram",
            self.image.clone(),
            egui::TextureOptions::default(),
        ));
    }

    /// Reset the [`Diagram`]'s image, undoing all performed plots.
    ///
    /// [`Diagram`]: Diagram
    pub fn reset_image(&mut self) {
        self.image = self.base_image.clone();
    }

    pub fn plot_valves(&mut self, stand_state: StandState) {
        // NP1
        self.set_region(
            405,
            405 + 40,
            475,
            475 + 40,
            match stand_state.valve_np1 {
                Some(ValveState::Open) => COLOR_OPEN,
                Some(ValveState::Closed) => COLOR_CLOSED,
                None => COLOR_UNKNOWN,
            },
        );

        // NP2
        self.set_region(
            400,
            400 + 40,
            190,
            190 + 40,
            match stand_state.valve_np2 {
                Some(ValveState::Open) => COLOR_OPEN,
                Some(ValveState::Closed) => COLOR_CLOSED,
                None => COLOR_UNKNOWN,
            },
        );

        // NP3
        self.set_region(
            365,
            365 + 40,
            240,
            240 + 40,
            match stand_state.valve_np3 {
                Some(ValveState::Open) => COLOR_OPEN,
                Some(ValveState::Closed) => COLOR_CLOSED,
                None => COLOR_UNKNOWN,
            },
        );

        // NP4
        self.set_region(
            175,
            175 + 40,
            450,
            450 + 40,
            match stand_state.valve_np4 {
                Some(ValveState::Open) => COLOR_OPEN,
                Some(ValveState::Closed) => COLOR_CLOSED,
                None => COLOR_UNKNOWN,
            },
        );

        // IP1
        self.set_region(
            665,
            665 + 40,
            475,
            475 + 40,
            match stand_state.valve_ip1 {
                Some(ValveState::Open) => COLOR_OPEN,
                Some(ValveState::Closed) => COLOR_CLOSED,
                None => COLOR_UNKNOWN,
            },
        );

        // IP2
        self.set_region(
            670,
            670 + 40,
            195,
            195 + 40,
            match stand_state.valve_ip2 {
                Some(ValveState::Open) => COLOR_OPEN,
                Some(ValveState::Closed) => COLOR_CLOSED,
                None => COLOR_UNKNOWN,
            },
        );

        // IP3
        self.set_region(
            735,
            735 + 40,
            285,
            285 + 40,
            match stand_state.valve_ip3 {
                Some(ValveState::Open) => COLOR_OPEN,
                Some(ValveState::Closed) => COLOR_CLOSED,
                None => COLOR_UNKNOWN,
            },
        );
    }

    pub fn set_region(&mut self, x0: usize, x1: usize, y0: usize, y1: usize, color: Color32) {
        for x in x0.min(x1)..x0.max(x1) {
            for y in y0.min(y1)..y0.max(y1) {
                self.set_pixel(x, y, color);
            }
        }
    }

    pub fn set_pixel(&mut self, x: usize, y: usize, color: Color32) {
        let w = self.image.width();
        self.image.pixels[y * w + x] = color;
    }
}
