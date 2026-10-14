//! Two-handle year slider of the timeline (iced only has single-value sliders).
//!
//! Drag the left handle to move `from`, the right handle to move `to`, or the filled
//! part between them to shift the whole range. A press elsewhere on the rail jumps the
//! nearer handle there.

use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::{Point, Rectangle, Renderer, Size, Theme};

use crate::app::Message;
use crate::ui;

const RADIUS: f32 = 7.0;
/// Rail inset so the handles stay inside the widget at both ends.
const PAD: f32 = RADIUS + 2.0;
/// How close (px) a press has to be to grab a handle.
const REACH: f32 = 10.0;

pub struct RangeSlider {
    /// Ends of the rail.
    pub first: i32,
    pub last: i32,
    /// Selected range.
    pub from: i32,
    pub to: i32,
}

/// What the pointer is holding.
#[derive(Debug, Default)]
pub enum Grip {
    #[default]
    None,
    From,
    To,
    /// The range between the handles; `offset` = grabbed year - `from`.
    Range { offset: i32 },
}

impl RangeSlider {
    fn x_of(&self, year: i32, width: f32) -> f32 {
        let span = (self.last - self.first).max(1) as f32;
        PAD + (year - self.first) as f32 / span * (width - 2.0 * PAD).max(1.0)
    }

    fn year_at(&self, x: f32, width: f32) -> i32 {
        let t = ((x - PAD) / (width - 2.0 * PAD).max(1.0)).clamp(0.0, 1.0);
        self.first + (t * (self.last - self.first) as f32).round() as i32
    }

    /// The handle or range a press at `x` grabs.
    fn grip_at(&self, x: f32, width: f32) -> Grip {
        let (fx, tx) = (self.x_of(self.from, width), self.x_of(self.to, width));
        let (df, dt) = ((x - fx).abs(), (x - tx).abs());
        if df <= REACH || dt <= REACH {
            // handles on top of each other: take the one that can still move that way
            if df < dt || (df == dt && (x < fx || self.to == self.last)) {
                Grip::From
            } else {
                Grip::To
            }
        } else if x > fx && x < tx {
            Grip::Range {
                offset: self.year_at(x, width) - self.from,
            }
        } else if x < fx {
            Grip::From
        } else {
            Grip::To
        }
    }

    /// The range after dragging `grip` to `x`.
    fn dragged(&self, grip: &Grip, x: f32, width: f32) -> Option<(i32, i32)> {
        let year = self.year_at(x, width);
        Some(match *grip {
            Grip::None => return None,
            Grip::From => (year.min(self.to), self.to),
            Grip::To => (self.from, year.max(self.from)),
            Grip::Range { offset } => {
                let len = self.to - self.from;
                let from = (year - offset).clamp(self.first, self.last - len);
                (from, from + len)
            }
        })
    }
}

impl canvas::Program<Message> for RangeSlider {
    type State = Grip;

    fn update(
        &self,
        grip: &mut Grip,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let canvas::Event::Mouse(event) = event else {
            return None;
        };
        let publish = |(from, to)| canvas::Action::publish(Message::SetYears(from, to)).and_capture();
        match event {
            mouse::Event::ButtonPressed(mouse::Button::Left) => {
                let x = cursor.position_in(bounds)?.x;
                *grip = self.grip_at(x, bounds.width);
                // a press on the range only grabs it; elsewhere the handle jumps there
                match grip {
                    Grip::Range { .. } => Some(canvas::Action::capture()),
                    _ => self.dragged(grip, x, bounds.width).map(publish),
                }
            }
            mouse::Event::CursorMoved { .. } if !matches!(grip, Grip::None) => {
                // keeps following the pointer outside the widget while held
                let x = cursor.position()?.x - bounds.x;
                self.dragged(grip, x, bounds.width)
                    .filter(|&range| range != (self.from, self.to))
                    .map(publish)
                    .or(Some(canvas::Action::capture()))
            }
            mouse::Event::ButtonReleased(mouse::Button::Left)
                if !matches!(std::mem::take(grip), Grip::None) =>
            {
                Some(canvas::Action::capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        grip: &Grip,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let p = ui::pal();
        let mut frame = Frame::new(renderer, bounds.size());
        let (w, cy) = (bounds.width, bounds.height / 2.0);
        let (fx, tx) = (self.x_of(self.from, w), self.x_of(self.to, w));
        let accent = if matches!(grip, Grip::None) {
            p.accent
        } else {
            p.accent_hover
        };

        let rail = |x0: f32, x1: f32| {
            Path::rounded_rectangle(
                Point::new(x0, cy - 2.0),
                Size::new((x1 - x0).max(0.0), 4.0),
                2.0.into(),
            )
        };
        frame.fill(&rail(PAD, w - PAD), p.border);
        frame.fill(&rail(fx, tx), accent);
        for x in [fx, tx] {
            let handle = Path::circle(Point::new(x, cy), RADIUS);
            frame.fill(&handle, p.raised);
            frame.stroke(&handle, Stroke::default().with_width(2.0).with_color(accent));
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        grip: &Grip,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match grip {
            Grip::None if cursor.is_over(bounds) => mouse::Interaction::Pointer,
            Grip::None => mouse::Interaction::default(),
            _ => mouse::Interaction::Grabbing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f32 = 218.0; // 200 px of rail: 1 px per year

    fn slider(from: i32, to: i32) -> RangeSlider {
        RangeSlider {
            first: 1800,
            last: 2000,
            from,
            to,
        }
    }

    fn x(year: i32) -> f32 {
        PAD + (year - 1800) as f32
    }

    #[test]
    fn both_ends_can_be_dragged() {
        let s = slider(1850, 1950);
        let from = s.grip_at(x(1852), W);
        assert!(matches!(from, Grip::From));
        assert_eq!(s.dragged(&from, x(1820), W), Some((1820, 1950)));
        // the left end cannot pass the right one
        assert_eq!(s.dragged(&from, x(1990), W), Some((1950, 1950)));

        let to = s.grip_at(x(1948), W);
        assert!(matches!(to, Grip::To));
        assert_eq!(s.dragged(&to, x(1900), W), Some((1850, 1900)));
        assert_eq!(s.dragged(&to, x(1700), W), Some((1850, 1850)));
    }

    #[test]
    fn dragging_the_range_keeps_its_length_inside_the_rail() {
        let s = slider(1850, 1950);
        let range = s.grip_at(x(1900), W);
        assert!(matches!(range, Grip::Range { offset: 50 }));
        assert_eq!(s.dragged(&range, x(1920), W), Some((1870, 1970)));
        assert_eq!(s.dragged(&range, x(1990), W), Some((1900, 2000)));
        assert_eq!(s.dragged(&range, -50.0, W), Some((1800, 1900)));
    }

    #[test]
    fn handles_on_top_of_each_other_stay_movable() {
        let at_end = slider(2000, 2000);
        assert!(matches!(at_end.grip_at(x(2000), W), Grip::From));
        let at_start = slider(1800, 1800);
        assert!(matches!(at_start.grip_at(x(1800), W), Grip::To));
        // outside the handles the nearer end jumps to the press
        let s = slider(1850, 1950);
        assert!(matches!(s.grip_at(x(1810), W), Grip::From));
        assert!(matches!(s.grip_at(x(1990), W), Grip::To));
    }
}
