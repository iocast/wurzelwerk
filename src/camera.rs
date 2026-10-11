//! World <-> screen transform for the board.

use iced::{Point, Rectangle, Size, Vector};

pub const MIN_ZOOM: f32 = 0.001;
pub const MAX_ZOOM: f32 = 4.0;

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    /// World coordinate shown at the top-left corner of the canvas.
    pub offset: Vector,
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            offset: Vector::new(-60.0, -60.0),
            zoom: 1.0,
        }
    }
}

impl Camera {
    pub fn to_screen(&self, p: Point) -> Point {
        Point::new(
            (p.x - self.offset.x) * self.zoom,
            (p.y - self.offset.y) * self.zoom,
        )
    }

    pub fn to_world(&self, p: Point) -> Point {
        Point::new(
            p.x / self.zoom + self.offset.x,
            p.y / self.zoom + self.offset.y,
        )
    }

    pub fn visible_world(&self, size: Size) -> Rectangle {
        Rectangle {
            x: self.offset.x,
            y: self.offset.y,
            width: size.width / self.zoom,
            height: size.height / self.zoom,
        }
    }

    /// Zoom keeping the world point under `anchor` (screen) fixed.
    pub fn zoom_at(&mut self, anchor: Point, factor: f32) {
        let before = self.to_world(anchor);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        self.offset = Vector::new(
            before.x - anchor.x / self.zoom,
            before.y - anchor.y / self.zoom,
        );
    }

    pub fn pan_screen(&mut self, delta: Vector) {
        self.offset = Vector::new(
            self.offset.x - delta.x / self.zoom,
            self.offset.y - delta.y / self.zoom,
        );
    }

    pub fn center_on(&mut self, world: Point, size: Size) {
        self.offset = Vector::new(
            world.x - size.width / 2.0 / self.zoom,
            world.y - size.height / 2.0 / self.zoom,
        );
    }

    pub fn fit(&mut self, world: Rectangle, size: Size) {
        let margin = 60.0;
        let avail_w = (size.width - margin * 2.0).max(50.0);
        let avail_h = (size.height - margin * 2.0).max(50.0);
        let zoom = (avail_w / world.width.max(1.0))
            .min(avail_h / world.height.max(1.0))
            .clamp(MIN_ZOOM, 1.5);
        self.zoom = zoom;
        self.center_on(
            Point::new(world.x + world.width / 2.0, world.y + world.height / 2.0),
            size,
        );
    }
}
