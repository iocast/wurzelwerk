//! The board: an iced `canvas` rendered by wgpu.
//!
//! Performance strategy for very large boards (many clans × many people):
//! * **Static layer cached** in a `canvas::Cache` - tessellated once for an area larger
//!   than the canvas (a `Snapshot`); zooming and panning only change the transformation
//!   the GPU applies to it (`World`). It is re-tessellated when the data changes, the view
//!   leaves the snapshot, the zoom crosses a level of detail or settles after zooming.
//! * **Viewport culling** through a uniform spatial grid - only visible nodes are touched.
//! * **Batched paths** - all node bodies, all solid edges, all dashed edges, all arrows,
//!   all clan dots of one colour are each a *single* path -> very few draw calls.
//! * **Level of detail** -
//!   `zoom >= 0.55` full cards + text, `>= 0.04` coloured blocks, below that a density map.
//! * **Cached routes** - lines are routed once in world space (`routing::Routes`) and
//!   only re-routed when a card they depend on moves; drawing, culling and hit testing
//!   just read the cache.
//! * Interactive feedback (line being drawn) lives in an uncached overlay frame.

use std::borrow::Cow;
use std::collections::HashMap;

use iced::font::Weight;
use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::Tree;
use iced::advanced::{Layout, Widget, layout};
use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, LineDash, Path, Stroke, path::Builder};
use iced::{
    Color, Element, Font, Length, Pixels, Point, Rectangle, Renderer, Size, Theme,
    Transformation, Vector, alignment,
};

use crate::app::{App, Message, Tool};
use crate::camera::Camera;
use crate::i18n::t;
use crate::model::{Link, LinkLabelPosition, LinkStyle, NODE_H, NODE_W, Person, link_style};
use crate::routing::{LABEL_TEXT, Route, connection_preview, label_layout, label_width};
use crate::ui;

const LOD_DETAIL: f32 = 0.55;
const LOD_BLOCKS: f32 = 0.04;
const LOD_EDGES: f32 = 0.12;
const LOD_EDGE_LABELS: f32 = 0.7;
const LOD_GRID: f32 = 0.6;
const LOD_HANDLES: f32 = 0.3;
const LOD_CLAN_LABELS: f32 = 0.08;
/// Zoom levels at which the static layer changes beyond plain scaling.
const LOD_STEPS: [f32; 7] = [
    LOD_BLOCKS,
    LOD_CLAN_LABELS,
    LOD_EDGES,
    LOD_HANDLES,
    LOD_DETAIL,
    LOD_GRID,
    LOD_EDGE_LABELS,
];
/// Extra area tessellated around the canvas, as a fraction of its size per side.
const SNAPSHOT_MARGIN: f32 = 0.5;
/// Zoom range (relative to the snapshot) shown by scaling before re-tessellating.
const SNAPSHOT_SCALE: std::ops::RangeInclusive<f32> = 0.67..=1.5;

pub struct Board<'a> {
    pub app: &'a App,
}

#[derive(Debug, Default)]
pub enum Interaction {
    #[default]
    Idle,
    Panning {
        last: Point,
    },
    Dragging {
        id: String,
        grab: Vector,
        moved: bool,
    },
    Connecting {
        from: String,
        cursor: Point,
        port: Option<usize>,
    },
    DraggingLinkRoute {
        id: String,
        start: Point,
        points: Vec<Point>,
        segment: usize,
        custom_bends: Vec<(usize, usize)>,
        start_port: u8,
        end_port: u8,
        label_position: Option<LinkLabelPosition>,
    },
    DraggingLinkVertex {
        id: String,
        start: Point,
        point_index: usize,
        stored_vertex: Option<usize>,
        original_route: Vec<[f32; 2]>,
        custom_bends: Vec<(usize, usize)>,
        route_point: Point,
        /// Route at the start of the drag (snap targets).
        points: Vec<Point>,
        moved: bool,
        start_port: u8,
        end_port: u8,
    },
    DraggingLinkLabel {
        id: String,
        cursor_start: Point,
        label_start: Point,
        route: Vec<Point>,
        position: LinkLabelPosition,
    },
    DraggingClanLabel {
        id: String,
        start: Point,
        offset: [f32; 2],
    },
    /// Moving the box of a clan without members.
    DraggingClanBox {
        id: String,
        grab: Vector,
        moved: bool,
    },
}

impl canvas::Program<Message> for Board<'_> {
    type State = Interaction;

    fn update(
        &self,
        state: &mut Interaction,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let app = self.app;
        let cam = app.camera;
        let canvas::Event::Mouse(mouse_event) = event else {
            return None;
        };
        let local = |p: Point| Point::new(p.x - bounds.x, p.y - bounds.y);

        match mouse_event {
            mouse::Event::WheelScrolled { delta } => {
                let pos = cursor.position_in(bounds)?;
                let steps = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y,
                    mouse::ScrollDelta::Pixels { y, .. } => *y / 50.0,
                };
                Some(
                    canvas::Action::publish(Message::ZoomAt(pos, 1.15f32.powf(steps)))
                        .and_capture(),
                )
            }
            mouse::Event::ButtonPressed(button) => {
                let pos = cursor.position_in(bounds)?;
                let absolute = cursor.position()?;
                match button {
                    mouse::Button::Left => self.press_left(state, pos, absolute),
                    mouse::Button::Middle | mouse::Button::Right => {
                        *state = Interaction::Panning { last: absolute };
                        Some(canvas::Action::capture())
                    }
                    _ => None,
                }
            }
            mouse::Event::CursorMoved { .. } => match state {
                Interaction::Panning { last } => {
                    let now = cursor.position()?;
                    let delta = Vector::new(now.x - last.x, now.y - last.y);
                    *last = now;
                    Some(canvas::Action::publish(Message::PanBy(delta)).and_capture())
                }
                Interaction::Dragging { id, grab, moved } => {
                    let world = cam.to_world(local(cursor.position()?));
                    *moved = true;
                    let to = Point::new(world.x - grab.x, world.y - grab.y);
                    Some(
                        canvas::Action::publish(Message::NodeDragged(id.clone(), to)).and_capture(),
                    )
                }
                Interaction::Connecting { cursor: at, .. } => {
                    *at = local(cursor.position()?);
                    Some(canvas::Action::request_redraw().and_capture())
                }
                Interaction::DraggingLinkRoute {
                    id,
                    start,
                    points,
                    segment,
                    custom_bends,
                    start_port,
                    end_port,
                    label_position,
                } => {
                    let world = cam.to_world(local(cursor.position()?));
                    let route = dragged_route(
                        points,
                        *segment,
                        Vector::new(world.x - start.x, world.y - start.y),
                        custom_bends,
                    );
                    let label_position = label_position
                        .map(|position| remap_label_for_drag(position, *segment, points.len()));
                    Some(
                        canvas::Action::publish(Message::LinkRouteDragged(
                            id.clone(),
                            route,
                            label_position,
                            *start_port,
                            *end_port,
                        ))
                        .and_capture(),
                    )
                }
                Interaction::DraggingLinkVertex {
                    id,
                    start,
                    point_index,
                    stored_vertex,
                    original_route,
                    custom_bends,
                    route_point,
                    points,
                    moved,
                    start_port,
                    end_port,
                } => {
                    let world = cam.to_world(local(cursor.position()?));
                    let delta = Vector::new(world.x - start.x, world.y - start.y);
                    if delta.x.abs() > 0.5 || delta.y.abs() > 0.5 {
                        *moved = true;
                    }
                    if *moved {
                        let target = snap_vertex(
                            points,
                            *point_index,
                            Point::new(route_point.x + delta.x, route_point.y + delta.y),
                        );
                        let route = move_route_vertex(
                            original_route,
                            custom_bends,
                            *point_index,
                            *stored_vertex,
                            target,
                        );
                        return Some(
                            canvas::Action::publish(Message::LinkRouteDragged(
                                id.clone(),
                                route,
                                None,
                                *start_port,
                                *end_port,
                            ))
                            .and_capture(),
                        );
                    }
                    Some(canvas::Action::request_redraw().and_capture())
                }
                Interaction::DraggingLinkLabel {
                    id,
                    cursor_start,
                    label_start,
                    route,
                    position,
                } => {
                    let world = cam.to_world(local(cursor.position()?));
                    let delta = Vector::new(world.x - cursor_start.x, world.y - cursor_start.y);
                    let target = Point::new(label_start.x + delta.x, label_start.y + delta.y);
                    let position = attach_label_to_segment(route, position.segment, target);
                    Some(
                        canvas::Action::publish(Message::LinkLabelDragged(id.clone(), position))
                            .and_capture(),
                    )
                }
                Interaction::DraggingClanBox { id, grab, moved } => {
                    let world = cam.to_world(local(cursor.position()?));
                    *moved = true;
                    Some(
                        canvas::Action::publish(Message::ClanMoved(
                            id.clone(),
                            [world.x - grab.x, world.y - grab.y],
                        ))
                        .and_capture(),
                    )
                }
                Interaction::DraggingClanLabel { id, start, offset } => {
                    let world = cam.to_world(local(cursor.position()?));
                    Some(
                        canvas::Action::publish(Message::ClanLabelDragged(
                            id.clone(),
                            [offset[0] + world.x - start.x, offset[1] + world.y - start.y],
                        ))
                        .and_capture(),
                    )
                }
                Interaction::Idle => None,
            },
            mouse::Event::ButtonReleased(_) => match std::mem::take(state) {
                Interaction::Idle => None,
                Interaction::Dragging {
                    id, moved: true, ..
                } => Some(canvas::Action::publish(Message::NodeDropped(id)).and_capture()),
                Interaction::Connecting { from, .. } => {
                    let target = cursor
                        .position_in(bounds)
                        .and_then(|p| app.hit_test(cam.to_world(p)))
                        .map(|i| app.data.people[i].id.clone());
                    match target {
                        Some(to) if to != from => Some(
                            canvas::Action::publish(Message::ConnectRequested(from, to))
                                .and_capture(),
                        ),
                        _ => Some(canvas::Action::request_redraw().and_capture()),
                    }
                }
                Interaction::DraggingLinkRoute { id, .. }
                | Interaction::DraggingLinkVertex { id, .. }
                | Interaction::DraggingLinkLabel { id, .. } => {
                    Some(canvas::Action::publish(Message::LinkDropped(id)).and_capture())
                }
                Interaction::DraggingClanLabel { id, .. }
                | Interaction::DraggingClanBox {
                    id, moved: true, ..
                } => Some(canvas::Action::publish(Message::ClanLabelDropped(id)).and_capture()),
                _ => Some(canvas::Action::capture()),
            },
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Interaction,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let app = self.app;
        let mut overlay = Frame::new(renderer, bounds.size());
        if let Interaction::Connecting { from, cursor, port } = state
            && let Some(p) = app.data.person(from)
        {
            let target = app.camera.to_world(*cursor);
            let target_person = app
                .hit_test(target)
                .map(|i| &app.data.people[i])
                .filter(|person| person.id != p.id);
            let points =
                connection_preview(p, target, target_person, app.routes.obstacles(), *port);
            let screen_points: Vec<_> = points
                .iter()
                .map(|point| app.camera.to_screen(*point))
                .collect();
            let line = polyline(&screen_points);
            overlay.stroke(
                &line,
                Stroke {
                    line_dash: LineDash {
                        segments: &[6.0, 4.0],
                        offset: 0,
                    },
                    ..Stroke::default().with_width(1.6).with_color(ui::pal().selection)
                },
            );
            overlay.fill(&Path::circle(*cursor, 4.0), ui::pal().selection);
        }

        vec![overlay.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Interaction,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match state {
            Interaction::Panning { .. } | Interaction::Dragging { .. } => {
                mouse::Interaction::Grabbing
            }
            Interaction::Connecting { .. } => mouse::Interaction::Crosshair,
            Interaction::DraggingLinkRoute { .. }
            | Interaction::DraggingLinkVertex { .. }
            | Interaction::DraggingLinkLabel { .. } => mouse::Interaction::Grabbing,
            Interaction::DraggingClanLabel { .. } | Interaction::DraggingClanBox { .. } => {
                mouse::Interaction::Grabbing
            }
            Interaction::Idle => {
                let Some(pos) = cursor.position_in(bounds) else {
                    return mouse::Interaction::default();
                };
                match self.app.tool {
                    Tool::AddPerson | Tool::Connect | Tool::Clan => mouse::Interaction::Crosshair,
                    Tool::Select | Tool::Members => {
                        let world = self.app.camera.to_world(pos);
                        if self.app.hit_test(world).is_some()
                            || self.app.empty_clan_at(world).is_some()
                            || (self.app.tool == Tool::Select
                                && (link_hit(self.app, &self.app.camera, pos)
                                    || link_vertex_hit(self.app, &self.app.camera, pos).is_some()))
                        {
                            mouse::Interaction::Pointer
                        } else {
                            mouse::Interaction::Grab
                        }
                    }
                }
            }
        }
    }
}

impl Board<'_> {
    fn press_left(
        &self,
        state: &mut Interaction,
        pos: Point,
        absolute: Point,
    ) -> Option<canvas::Action<Message>> {
        let app = self.app;
        let cam = app.camera;
        let world = cam.to_world(pos);

        // connection handles of the selected person work in every tool
        if let Some(sel) = app.selected_person()
            && cam.zoom >= LOD_HANDLES
            && let Some(port) = handle_points(sel, &cam)
                .iter()
                .position(|handle| handle.distance(pos) <= 9.0)
        {
            *state = Interaction::Connecting {
                from: sel.id.clone(),
                cursor: pos,
                port: Some(port),
            };
            return Some(canvas::Action::request_redraw().and_capture());
        }

        if app.tool == Tool::Select {
            if let Some(clan) = clan_label_hit(app, &cam, pos) {
                *state = Interaction::DraggingClanLabel {
                    id: clan.id.clone(),
                    start: world,
                    offset: clan.label_offset,
                };
                return Some(canvas::Action::capture());
            }
            if let Some((
                link,
                point_index,
                stored_vertex,
                route,
                custom_bends,
                route_point,
                start_port,
                end_port,
            )) = link_vertex_hit(app, &cam, pos)
            {
                *state = Interaction::DraggingLinkVertex {
                    id: link.id.clone(),
                    start: world,
                    point_index,
                    stored_vertex,
                    original_route: route.clone(),
                    custom_bends: custom_bends.clone(),
                    route_point,
                    points: app
                        .routes
                        .get(&link.id)
                        .map(|route| route.points.clone())
                        .unwrap_or_default(),
                    moved: false,
                    start_port,
                    end_port,
                };
                let selection = stored_vertex.map_or_else(
                    || canvas::Action::publish(Message::SelectLink(link.id.clone())),
                    |vertex| {
                        canvas::Action::publish(Message::SelectLinkVertex(
                            link.id.clone(),
                            vertex,
                            route,
                        ))
                    },
                );
                return Some(selection.and_capture());
            }
            if let Some((link, label_start, points, position)) = link_label_hit(app, &cam, pos) {
                *state = Interaction::DraggingLinkLabel {
                    id: link.id.clone(),
                    cursor_start: world,
                    label_start,
                    route: points,
                    position,
                };
                return Some(
                    canvas::Action::publish(Message::SelectLink(link.id.clone())).and_capture(),
                );
            }
            if app.hit_test(world).is_none()
                && let Some((link, points, segment, custom_bends, start_port, end_port)) =
                    link_segment_hit(app, &cam, pos)
            {
                *state = Interaction::DraggingLinkRoute {
                    id: link.id.clone(),
                    start: world,
                    label_position: link.label_position.map(|position| {
                        if position.legacy_absolute {
                            attach_label_to_route(
                                &points,
                                Point::new(position.offset[0], position.offset[1]),
                            )
                        } else {
                            position
                        }
                    }),
                    points,
                    segment,
                    custom_bends,
                    start_port,
                    end_port,
                };
                return Some(
                    canvas::Action::publish(Message::SelectLink(link.id.clone())).and_capture(),
                );
            }
        }

        let hit = app.hit_test(world).map(|i| &app.data.people[i]);
        let select =
            |id: Option<String>| canvas::Action::publish(Message::SelectPerson(id)).and_capture();

        // box of a clan without members: click opens it, drag moves it
        if hit.is_none()
            && matches!(app.tool, Tool::Select | Tool::Clan | Tool::Members)
            && let Some(id) = app.empty_clan_at(world)
            && let Some(rect) = app.data.clan(&id).and_then(|c| c.box_rect())
        {
            *state = Interaction::DraggingClanBox {
                grab: Vector::new(world.x - rect.x, world.y - rect.y),
                id: id.clone(),
                moved: false,
            };
            return Some(canvas::Action::publish(Message::ShowClan(id)).and_capture());
        }

        match (app.tool, hit) {
            (Tool::AddPerson, None) => {
                Some(canvas::Action::publish(Message::AddPersonAt(world)).and_capture())
            }
            (Tool::Clan, None) => {
                Some(canvas::Action::publish(Message::AddClanAt(world)).and_capture())
            }
            (Tool::Members, Some(p)) => {
                Some(canvas::Action::publish(Message::ToggleClan(p.id.clone())).and_capture())
            }
            (Tool::Connect, Some(p)) => {
                *state = Interaction::Connecting {
                    from: p.id.clone(),
                    cursor: pos,
                    port: None,
                };
                Some(select(Some(p.id.clone())))
            }
            (Tool::Select | Tool::AddPerson | Tool::Clan, Some(p)) => {
                *state = Interaction::Dragging {
                    id: p.id.clone(),
                    grab: Vector::new(world.x - p.x, world.y - p.y),
                    moved: false,
                };
                Some(select(Some(p.id.clone())))
            }
            (_, None) => {
                *state = Interaction::Panning { last: absolute };
                Some(select(None))
            }
        }
    }
}

/// Screen positions of the two connection handles (left / right middle).
pub fn handle_points(p: &Person, cam: &Camera) -> [Point; 2] {
    [
        cam.to_screen(Point::new(p.x, p.y + NODE_H / 2.0)),
        cam.to_screen(Point::new(p.x + NODE_W, p.y + NODE_H / 2.0)),
    ]
}

// ---------------------------------------------------------------------------
// static layer
// ---------------------------------------------------------------------------

/// Board area tessellated into `App::cache`, in its own screen space.
#[derive(Debug, Clone, Copy)]
pub struct Snapshot {
    pub cam: Camera,
    pub size: Size,
}

impl Snapshot {
    fn around(cam: Camera, canvas: Size) -> Self {
        let margin = Vector::new(canvas.width * SNAPSHOT_MARGIN, canvas.height * SNAPSHOT_MARGIN);
        Self {
            cam: Camera {
                offset: Vector::new(
                    cam.offset.x - margin.x / cam.zoom,
                    cam.offset.y - margin.y / cam.zoom,
                ),
                zoom: cam.zoom,
            },
            size: Size::new(canvas.width + 2.0 * margin.x, canvas.height + 2.0 * margin.y),
        }
    }

    /// Whether `cam` can be shown by only scaling and moving this snapshot.
    fn covers(&self, cam: Camera, canvas: Size) -> bool {
        let region = self.cam.visible_world(self.size);
        let view = cam.visible_world(canvas);
        let (low, high) = (self.cam.zoom.min(cam.zoom), self.cam.zoom.max(cam.zoom));
        SNAPSHOT_SCALE.contains(&(cam.zoom / self.cam.zoom))
            && !LOD_STEPS.iter().any(|&step| low < step && step <= high)
            && view.x >= region.x
            && view.y >= region.y
            && view.x + view.width <= region.x + region.width
            && view.y + view.height <= region.y + region.height
    }

    /// Snapshot space -> canvas space under `cam`.
    fn transformation(&self, cam: Camera) -> Transformation {
        let origin = cam.to_screen(Point::new(self.cam.offset.x, self.cam.offset.y));
        Transformation::translate(origin.x, origin.y)
            * Transformation::scale(cam.zoom / self.cam.zoom)
    }

    /// Drawn at a zoom other than the current one (sharpened once zooming settles).
    pub fn scaled(&self, cam: Camera) -> bool {
        self.cam.zoom != cam.zoom
    }
}

/// The static board layer: cached geometry placed by a GPU transformation, so zooming
/// and panning skip tessellation. Interaction and the overlay stay in the `Board` canvas.
pub struct World<'a> {
    pub app: &'a App,
}

impl Widget<Message, Theme, Renderer> for World<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        if bounds.width < 1.0 || bounds.height < 1.0 {
            return;
        }
        let app = self.app;
        let cam = app.camera;
        if !app
            .snapshot
            .get()
            .is_some_and(|snapshot| snapshot.covers(cam, bounds.size()))
        {
            app.cache.clear();
        }
        let fresh = Snapshot::around(cam, bounds.size());
        let geometry = app.cache.draw(renderer, fresh.size, |frame| {
            app.snapshot.set(Some(fresh));
            app.snapshot_text.borrow_mut().clear();
            draw_board(app, frame, fresh.cam);
        });
        let Some(snapshot) = app.snapshot.get() else {
            return;
        };
        let to_canvas = snapshot.transformation(cam);
        let labels = snapshot_labels(app, renderer, bounds.size(), to_canvas);
        renderer.with_layer(bounds, |renderer| {
            renderer.with_translation(Vector::new(bounds.x, bounds.y), |renderer| {
                renderer.with_transformation(to_canvas, |renderer| {
                    renderer.draw_geometry(geometry);
                });
                renderer.draw_geometry(labels);
            });
        });
    }
}

/// The snapshot's text placed on the canvas. Scaled text in a cached geometry would be
/// re-laid-out and re-rasterized every zoom frame, so only visible text is drawn, at
/// font sizes snapped to ~9% steps while zooming so glyphs stay in the atlas.
fn snapshot_labels(
    app: &App,
    renderer: &Renderer,
    canvas: Size,
    to_canvas: Transformation,
) -> Geometry {
    const MARGIN: f32 = 400.0;
    let scale = to_canvas.scale_factor();
    let font_scale = if scale == 1.0 {
        1.0
    } else {
        2f32.powf((scale.log2() * 8.0).round() / 8.0)
    };
    let mut frame = Frame::new(renderer, canvas);
    for label in app.snapshot_text.borrow().iter() {
        let at = label.position * to_canvas;
        if at.x < -MARGIN
            || at.y < -MARGIN
            || at.x > canvas.width + MARGIN
            || at.y > canvas.height + MARGIN
        {
            continue;
        }
        frame.fill_text(canvas::Text {
            position: at,
            size: Pixels(label.size.0 * font_scale),
            ..label.clone()
        });
    }
    frame.into_geometry()
}

impl<'a> From<World<'a>> for Element<'a, Message> {
    fn from(world: World<'a>) -> Self {
        Element::new(world)
    }
}

fn draw_board(app: &App, frame: &mut Frame, cam: Camera) {
    let size = frame.size();
    frame.fill_rectangle(Point::ORIGIN, size, ui::pal().board);

    let z = cam.zoom;
    let view = cam.visible_world(size);

    if z >= LOD_GRID {
        draw_grid(frame, &cam, view);
    }
    draw_hulls(app, frame, &cam, view);
    draw_empty_clans(app, frame, &cam, view);

    if z < LOD_BLOCKS {
        draw_density(app, frame, &cam, view);
        return;
    }
    if z >= LOD_EDGES {
        draw_edges(app, frame, &cam, view);
    }
    draw_nodes(app, frame, &cam, view);
}

/// Dots via `fill_rectangle`: lyon's rectangle fast path. As one path, the tens of
/// thousands of dots went through the general sweep-line fill (~10x the whole board).
fn draw_grid(frame: &mut Frame, cam: &Camera, view: Rectangle) {
    let step = 24.0;
    let dot = Size::new(1.5, 1.5);
    let color = ui::pal().grid_dot;
    let mut wy = (view.y / step).floor() * step;
    while wy <= view.y + view.height {
        let mut wx = (view.x / step).floor() * step;
        while wx <= view.x + view.width {
            let s = cam.to_screen(Point::new(wx, wy));
            frame.fill_rectangle(Point::new(s.x - 0.75, s.y - 0.75), dot, color);
            wx += step;
        }
        wy += step;
    }
}

fn draw_hulls(app: &App, frame: &mut Frame, cam: &Camera, view: Rectangle) {
    for clan in &app.data.clans {
        let Some(hull) = app.hulls.get(&clan.id) else {
            continue;
        };
        if hull.points.len() < 3 || !hull.bbox.intersects(&view) {
            continue;
        }
        let color = clan.color();
        let path = Path::new(|b| {
            for (i, p) in hull.points.iter().enumerate() {
                let s = cam.to_screen(*p);
                if i == 0 {
                    b.move_to(s);
                } else {
                    b.line_to(s);
                }
            }
            b.close();
        });
        frame.fill(&path, Color { a: 0.16, ..color });

        if cam.zoom >= LOD_CLAN_LABELS {
            let label = format!("{} · {}", clan.name, hull.members);
            let w = label.chars().count() as f32 * 6.6 + 18.0;
            let center = cam.to_screen(Point::new(
                hull.bbox.x + clan.label_offset[0] + w / (2.0 * cam.zoom),
                hull.bbox.y + clan.label_offset[1],
            ));
            let pill = Path::new(|b| {
                rounded_rect(
                    b,
                    Point::new(center.x - w / 2.0, center.y - 10.0),
                    Size::new(w, 20.0),
                    10.0,
                )
            });
            frame.fill(&pill, color);
            text(app, label, center, 11.5, Color::WHITE, true, true);
        }
    }
}

/// Clans without members: dashed box in the clan colour with the name pill on top.
fn draw_empty_clans(app: &App, frame: &mut Frame, cam: &Camera, view: Rectangle) {
    let z = cam.zoom;
    for clan in &app.data.clans {
        if app.clan_counts.get(&clan.id).copied().unwrap_or(0) > 0 {
            continue;
        }
        let Some(rect) = clan.box_rect() else {
            continue;
        };
        if !rect.intersects(&view) {
            continue;
        }
        let color = clan.color();
        let selected = app.show_clan && app.active_clan.as_deref() == Some(clan.id.as_str());
        let origin = cam.to_screen(Point::new(rect.x, rect.y));
        let size = Size::new(rect.width * z, rect.height * z);
        let body = Path::new(|b| rounded_rect(b, origin, size, 12.0 * z));
        frame.fill(&body, Color { a: 0.12, ..color });
        frame.stroke(
            &body,
            Stroke {
                line_dash: LineDash {
                    segments: if selected { &[] } else { &[6.0, 4.0] },
                    offset: 0,
                },
                ..Stroke::default()
                    .with_width(if selected { 2.0 } else { 1.4 })
                    .with_color(color)
            },
        );

        if z >= LOD_CLAN_LABELS {
            let label = clan.name.clone();
            let w = label.chars().count() as f32 * 6.6 + 18.0;
            let center = Point::new(origin.x + w / 2.0, origin.y);
            let pill = Path::new(|b| {
                rounded_rect(
                    b,
                    Point::new(center.x - w / 2.0, center.y - 10.0),
                    Size::new(w, 20.0),
                    10.0,
                )
            });
            frame.fill(&pill, color);
            text(app, label, center, 11.5, Color::WHITE, true, true);
        }
        if z >= LOD_DETAIL {
            text(
                app,
                t("sidebar-empty-clan").to_owned(),
                Point::new(origin.x + size.width / 2.0, origin.y + size.height * 0.58),
                11.5 * z,
                ui::alpha(color, 0.9),
                false,
                true,
            );
        }
    }
}

fn draw_density(app: &App, frame: &mut Frame, cam: &Camera, view: Rectangle) {
    let mut buckets: [Vec<Rectangle>; 4] = Default::default();
    app.grid.cells_in(view, |cell, nodes| {
        let count = match app.years {
            None => nodes.len(),
            Some(_) => nodes
                .iter()
                .filter(|&&i| app.shown(&app.data.people[i as usize]))
                .count(),
        };
        let bucket = match count {
            0 => return,
            1..=3 => 0,
            4..=15 => 1,
            16..=63 => 2,
            _ => 3,
        };
        buckets[bucket].push(cell);
    });
    let alphas = [0.18, 0.32, 0.5, 0.7];
    for (rects, alpha) in buckets.iter().zip(alphas) {
        if rects.is_empty() {
            continue;
        }
        let path = Path::new(|b| {
            for r in rects {
                let s = cam.to_screen(Point::new(r.x, r.y));
                let side = (r.width * cam.zoom).max(1.0);
                b.rectangle(s, Size::new(side, side));
            }
        });
        frame.fill(
            &path,
            Color {
                a: alpha,
                ..ui::pal().edge
            },
        );
    }
}

type SegmentHitCandidate<'a> = (
    &'a Link,
    Vec<Point>,
    usize,
    Vec<(usize, usize)>,
    u8,
    u8,
    f32,
);

fn link_label_hit<'a>(
    app: &'a App,
    cam: &Camera,
    pos: Point,
) -> Option<(&'a Link, Point, Vec<Point>, LinkLabelPosition)> {
    if cam.zoom < LOD_EDGE_LABELS {
        return None;
    }
    let text_size = LABEL_TEXT * cam.zoom.min(1.4);
    // runs on every hover: rule out other label rows before the costly label text
    app.data.links.iter().rev().find_map(|link| {
        if link.label.is_empty() {
            return None;
        }
        let route = app.routes.get(&link.id)?;
        let (label_at, centered) = label_layout(link, route);
        let screen_at = cam.to_screen(label_at);
        if (pos.y - screen_at.y).abs() > text_size || !app.link_shown(link) {
            return None;
        }
        let width = label_width(&app.data.link_label(&link.label), text_size);
        let left = if centered {
            screen_at.x - width / 2.0
        } else {
            screen_at.x - 4.0
        };
        let label_rect = Rectangle {
            x: left - 4.0,
            y: screen_at.y - text_size,
            width: width + 8.0,
            height: text_size * 2.0,
        };
        if !label_rect.contains(pos) {
            return None;
        }
        let center = if centered {
            label_at
        } else {
            Point::new(label_at.x + (width / cam.zoom) / 2.0, label_at.y)
        };
        let attachment = link
            .label_position
            .unwrap_or_else(|| attach_label_to_route(&route.points, label_at));
        Some((link, center, route.points.clone(), attachment))
    })
}

fn link_segment_hit<'a>(
    app: &'a App,
    cam: &Camera,
    pos: Point,
) -> Option<(&'a Link, Vec<Point>, usize, Vec<(usize, usize)>, u8, u8)> {
    if cam.zoom < LOD_EDGES {
        return None;
    }
    const REACH: f32 = 8.0;
    let world = cam.to_world(pos);
    let slack = REACH / cam.zoom;
    let mut nearest: Option<SegmentHitCandidate<'_>> = None;
    for link in &app.data.links {
        let Some(route) = app.routes.get(&link.id) else {
            continue;
        };
        let bbox = route.bbox;
        if world.x < bbox.x - slack
            || world.x > bbox.x + bbox.width + slack
            || world.y < bbox.y - slack
            || world.y > bbox.y + bbox.height + slack
            || !app.link_shown(link)
        {
            continue;
        }
        for (segment, pair) in route.points.windows(2).enumerate() {
            let distance = segment_distance(pos, cam.to_screen(pair[0]), cam.to_screen(pair[1]));
            if distance <= REACH && nearest.as_ref().is_none_or(|item| distance < item.6) {
                nearest = Some((
                    link,
                    route.points.clone(),
                    segment,
                    route.custom_bends.clone(),
                    route.start_port as u8,
                    route.end_port as u8,
                    distance,
                ));
            }
        }
    }
    nearest.map(
        |(link, points, segment, custom_bends, start_port, end_port, _)| {
            (link, points, segment, custom_bends, start_port, end_port)
        },
    )
}

fn link_hit(app: &App, cam: &Camera, pos: Point) -> bool {
    link_label_hit(app, cam, pos).is_some()
        || link_segment_hit(app, cam, pos).is_some()
        || clan_label_hit(app, cam, pos).is_some()
}

fn link_vertex_hit<'a>(
    app: &'a App,
    cam: &Camera,
    pos: Point,
) -> Option<(
    &'a Link,
    usize,
    Option<usize>,
    Vec<[f32; 2]>,
    Vec<(usize, usize)>,
    Point,
    u8,
    u8,
)> {
    if cam.zoom < LOD_EDGES {
        return None;
    }
    app.data
        .links
        .iter()
        .rev()
        .filter(|link| app.selected_link.as_deref() == Some(link.id.as_str()))
        .find_map(|link| {
            let route = app.routes.get(&link.id)?;
            let (stored_route, custom_bends) = visible_bends(&link.route, &route.custom_bends);
            route
                .points
                .iter()
                .enumerate()
                .skip(1)
                .take(route.points.len().saturating_sub(2))
                .find_map(|(point_index, point)| {
                    (cam.to_screen(*point).distance(pos) <= 9.0).then(|| {
                        let stored_vertex = custom_bends
                            .iter()
                            .find_map(|(index, stored)| (*index == point_index).then_some(*stored));
                        (
                            link,
                            point_index,
                            stored_vertex,
                            stored_route.clone(),
                            custom_bends.clone(),
                            *point,
                            route.start_port as u8,
                            route.end_port as u8,
                        )
                    })
                })
        })
}

/// The stored bends that are visible corners, in route order, with `custom_bends`
/// renumbered to match. Editing from this list forgets bends that became straight.
fn visible_bends(
    stored: &[[f32; 2]],
    custom_bends: &[(usize, usize)],
) -> (Vec<[f32; 2]>, Vec<(usize, usize)>) {
    custom_bends
        .iter()
        .filter_map(|&(point_index, stored_index)| {
            stored.get(stored_index).map(|bend| (point_index, *bend))
        })
        .enumerate()
        .map(|(new_index, (point_index, bend))| (bend, (point_index, new_index)))
        .unzip()
}

fn move_route_vertex(
    original_route: &[[f32; 2]],
    custom_bends: &[(usize, usize)],
    point_index: usize,
    stored_vertex: Option<usize>,
    target: Point,
) -> Vec<[f32; 2]> {
    if let Some(vertex) = stored_vertex {
        let mut route = original_route.to_vec();
        if let Some(point) = route.get_mut(vertex) {
            *point = [target.x, target.y];
        }
        return route;
    }
    let insertion = custom_bends
        .iter()
        .filter(|(index, _)| *index < point_index)
        .count();
    let mut route = original_route.to_vec();
    route.insert(insertion.min(route.len()), [target.x, target.y]);
    route
}

fn clan_label_hit<'a>(app: &'a App, cam: &Camera, pos: Point) -> Option<&'a crate::model::Clan> {
    if cam.zoom < LOD_CLAN_LABELS {
        return None;
    }
    app.data.clans.iter().rev().find(|clan| {
        let Some(hull) = app.hulls.get(&clan.id) else {
            return false;
        };
        if hull.points.len() < 3 {
            return false;
        }
        let label = format!("{} · {}", clan.name, hull.members);
        let width = label.chars().count() as f32 * 6.6 + 18.0;
        let center = cam.to_screen(Point::new(
            hull.bbox.x + clan.label_offset[0] + width / (2.0 * cam.zoom),
            hull.bbox.y + clan.label_offset[1],
        ));
        Rectangle {
            x: center.x - width / 2.0,
            y: center.y - 10.0,
            width,
            height: 20.0,
        }
        .contains(pos)
    })
}

fn attach_label_to_route(points: &[Point], target: Point) -> LinkLabelPosition {
    let mut best: Option<(usize, f32, Point, f32)> = None;
    for (segment, pair) in points.windows(2).enumerate() {
        let start = pair[0];
        let end = pair[1];
        let delta = Vector::new(end.x - start.x, end.y - start.y);
        let length_squared = delta.x * delta.x + delta.y * delta.y;
        let along = if length_squared <= f32::EPSILON {
            0.0
        } else {
            let offset = Vector::new(target.x - start.x, target.y - start.y);
            ((offset.x * delta.x + offset.y * delta.y) / length_squared).clamp(0.0, 1.0)
        };
        let projected = Point::new(start.x + delta.x * along, start.y + delta.y * along);
        let distance = target.distance(projected);
        if best.is_none_or(|current| distance < current.3) {
            best = Some((segment, along, projected, distance));
        }
    }
    let (segment, along, projected, _) = best.unwrap_or((0, 0.5, target, 0.0));
    LinkLabelPosition {
        segment,
        along,
        offset: [target.x - projected.x, target.y - projected.y],
        legacy_absolute: false,
    }
}

fn attach_label_to_segment(points: &[Point], segment: usize, target: Point) -> LinkLabelPosition {
    let start = points[segment];
    let end = points[segment + 1];
    let delta = Vector::new(end.x - start.x, end.y - start.y);
    let length_squared = delta.x * delta.x + delta.y * delta.y;
    let along = if length_squared <= f32::EPSILON {
        0.0
    } else {
        let offset = Vector::new(target.x - start.x, target.y - start.y);
        ((offset.x * delta.x + offset.y * delta.y) / length_squared).clamp(0.0, 1.0)
    };
    let projected = Point::new(start.x + delta.x * along, start.y + delta.y * along);
    LinkLabelPosition {
        segment,
        along,
        offset: [target.x - projected.x, target.y - projected.y],
        legacy_absolute: false,
    }
}

fn remap_label_for_drag(
    mut position: LinkLabelPosition,
    dragged_segment: usize,
    original_point_count: usize,
) -> LinkLabelPosition {
    let terminal = dragged_segment == 0 || dragged_segment + 2 == original_point_count;
    if !terminal {
        return position;
    }
    if position.segment == dragged_segment {
        position.segment += 1;
    } else if dragged_segment == 0 && position.segment > dragged_segment {
        position.segment += 2;
    }
    position
}

fn segment_distance(point: Point, start: Point, end: Point) -> f32 {
    let segment = Vector::new(end.x - start.x, end.y - start.y);
    let length_squared = segment.x * segment.x + segment.y * segment.y;
    if length_squared <= f32::EPSILON {
        return point.distance(start);
    }
    let offset = Vector::new(point.x - start.x, point.y - start.y);
    let t = ((offset.x * segment.x + offset.y * segment.y) / length_squared).clamp(0.0, 1.0);
    point.distance(Point::new(start.x + t * segment.x, start.y + t * segment.y))
}

/// A dragged segment or corner closer than this to a parallel line snaps onto it,
/// so near-misses merge into one straight run instead of adding tiny jogs.
const SNAP: f32 = 6.0;

fn snap_to(value: f32, lines: impl Iterator<Item = f32>) -> f32 {
    lines
        .filter(|line| (line - value).abs() <= SNAP)
        .min_by(|a, b| (a - value).abs().total_cmp(&(b - value).abs()))
        .unwrap_or(value)
}

/// Snaps a dragged corner onto the x / y of the other route points.
fn snap_vertex(points: &[Point], point_index: usize, target: Point) -> Point {
    let others = || {
        points
            .iter()
            .enumerate()
            .filter(move |(index, _)| *index != point_index)
            .map(|(_, point)| *point)
    };
    Point::new(
        snap_to(target.x, others().map(|p| p.x)),
        snap_to(target.y, others().map(|p| p.y)),
    )
}

fn dragged_route(
    points: &[Point],
    segment: usize,
    delta: Vector,
    custom_bends: &[(usize, usize)],
) -> Vec<[f32; 2]> {
    if points.len() < 2 || segment + 1 >= points.len() {
        return custom_bends
            .iter()
            .filter_map(|(point_index, _)| points.get(*point_index))
            .map(|point| [point.x, point.y])
            .collect();
    }
    let start = points[segment];
    let end = points[segment + 1];
    let is_horizontal = (end.x - start.x).abs() >= (end.y - start.y).abs();
    let is_terminal = segment == 0 || segment + 2 == points.len();
    // where the segment lands, snapped onto other parallel segments of the route
    let parallel = points
        .windows(2)
        .enumerate()
        .filter(|(index, pair)| {
            *index != segment
                && if is_horizontal {
                    (pair[0].y - pair[1].y).abs() < 0.01
                } else {
                    (pair[0].x - pair[1].x).abs() < 0.01
                }
        })
        .map(|(_, pair)| if is_horizontal { pair[0].y } else { pair[0].x });
    let offset = if is_horizontal {
        snap_to(start.y + delta.y, parallel) - start.y
    } else {
        snap_to(start.x + delta.x, parallel) - start.x
    };
    let mut adjusted = points.to_vec();
    if is_terminal {
        let first = if is_horizontal {
            Point::new(start.x, start.y + offset)
        } else {
            Point::new(start.x + offset, start.y)
        };
        let second = if is_horizontal {
            Point::new(end.x, start.y + offset)
        } else {
            Point::new(start.x + offset, end.y)
        };
        adjusted.splice(segment + 1..segment + 1, [first, second]);
    } else if is_horizontal {
        adjusted[segment].y += offset;
        adjusted[segment + 1].y += offset;
    } else {
        adjusted[segment].x += offset;
        adjusted[segment + 1].x += offset;
    }
    let first_moved = if is_terminal { segment + 1 } else { segment };
    let second_moved = first_moved + 1;
    let mut controls = Vec::with_capacity(custom_bends.len() + 2);
    for &(point_index, _) in custom_bends {
        if point_index == 0 || point_index + 1 == points.len() {
            continue;
        }
        let shifted_index = if is_terminal && point_index > segment {
            point_index + 2
        } else {
            point_index
        };
        if shifted_index < adjusted.len() {
            controls.push(shifted_index);
        }
    }
    controls.push(first_moved);
    controls.push(second_moved);
    controls.sort_unstable();
    controls.dedup();
    // only real corners are saved: bends that ended up on a straight run are dropped
    let corners = crate::routing::corner_indices(&adjusted, |index| {
        controls.binary_search(&index).is_ok()
    });
    let mut unique: Vec<Point> = Vec::with_capacity(controls.len());
    for index in corners {
        if controls.binary_search(&index).is_err() {
            continue;
        }
        let point = adjusted[index];
        if unique.last().is_none_or(|last| last.distance(point) > 0.01) {
            unique.push(point);
        }
    }
    unique.iter().map(|point| [point.x, point.y]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Link, Person};
    use crate::routing::{inflated, route, segment_clear};

    fn person(id: &str, x: f32, y: f32) -> Person {
        Person {
            id: id.into(),
            name: id.into(),
            born: String::new(),
            died: String::new(),
            x,
            y,
            clans: Vec::new(),
        }
    }

    fn routed_link() -> Link {
        Link {
            id: "link".into(),
            from: "from".into(),
            to: "to".into(),
            label: "related to".into(),
            route: vec![[180.0, 20.0], [180.0, 180.0]],
            start_port: None,
            end_port: None,
            label_position: None,
        }
    }

    fn is_orthogonal(points: &[Point]) -> bool {
        points.windows(2).all(|pair| {
            (pair[0].x - pair[1].x).abs() < 0.01 || (pair[0].y - pair[1].y).abs() < 0.01
        })
    }

    fn assert_on_border(point: Point, person: &Person) {
        let on_vertical_border =
            (point.x - person.x).abs() < 0.01 || (point.x - person.x - NODE_W).abs() < 0.01;
        let on_horizontal_border =
            (point.y - person.y).abs() < 0.01 || (point.y - person.y - NODE_H).abs() < 0.01;
        assert!(
            (on_vertical_border && (person.y..=person.y + NODE_H).contains(&point.y))
                || (on_horizontal_border && (person.x..=person.x + NODE_W).contains(&point.x)),
            "{point:?} is not on the border of {person:?}"
        );
    }

    /// Replays many segment / corner drags on one line like a user would (video:
    /// "mother of" between Maria and Oliver) through the real route cache.
    #[test]
    fn repeated_drags_keep_the_line_clean() {
        use crate::model::Data;
        use crate::routing::Routes;

        let people = vec![
            person("father", -260.0, 0.0),
            person("maria", 100.0, 0.0),
            person("oliver", -60.0, 260.0),
        ];
        let mut mother = routed_link();
        mother.id = "mother".into();
        mother.from = "maria".into();
        mother.to = "oliver".into();
        mother.label = "mother of".into();
        mother.route.clear();
        let mut father = mother.clone();
        father.id = "father".into();
        father.from = "father".into();
        father.label = "father of".into();
        let mut married = father.clone();
        married.id = "married".into();
        married.to = "maria".into();
        married.label = "married".into();
        let mut data = Data::from_parts(people, Vec::new(), vec![married, father, mother], Vec::new());
        let mut routes = Routes::default();
        routes.sync(&data);

        let mut seed = 0x2545_f491_u64;
        let mut next = |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        let obstacles: Vec<Rectangle> = data.people.iter().map(inflated).collect();
        for step in 0..300 {
            let route = routes.get("mother").unwrap().clone();
            let link = data.links.iter().find(|l| l.id == "mother").unwrap().clone();
            let delta = Vector::new(next(161) as f32 - 80.0, next(161) as f32 - 80.0);
            let new_route = if next(3) == 0 && route.points.len() > 2 {
                let point_index = 1 + next(route.points.len() - 2);
                let (stored_route, custom_bends) = visible_bends(&link.route, &route.custom_bends);
                let stored = custom_bends
                    .iter()
                    .find_map(|(index, stored)| (*index == point_index).then_some(*stored));
                let at = route.points[point_index];
                move_route_vertex(
                    &stored_route,
                    &custom_bends,
                    point_index,
                    stored,
                    snap_vertex(
                        &route.points,
                        point_index,
                        Point::new(at.x + delta.x, at.y + delta.y),
                    ),
                )
            } else {
                let segment = next(route.points.len() - 1);
                dragged_route(&route.points, segment, delta, &route.custom_bends)
            };
            let link = data.links.iter_mut().find(|l| l.id == "mother").unwrap();
            link.route = new_route;
            link.start_port = Some(route.start_port as u8);
            link.end_port = Some(route.end_port as u8);
            if next(10) == 0 {
                link.route.clear();
                link.start_port = None;
                link.end_port = None;
            }
            routes.sync(&data);

            let points = &routes.get("mother").unwrap().points;
            assert!(is_orthogonal(points), "step {step}: {points:?}");
            assert_on_border(points[0], &data.people[1]);
            assert_on_border(*points.last().unwrap(), &data.people[2]);
            for triple in points.windows(3) {
                let (a, b, c) = (triple[0], triple[1], triple[2]);
                let folds = (b.x - a.x) * (c.x - b.x) + (b.y - a.y) * (c.y - b.y) < -0.01;
                assert!(!folds, "step {step}: folds at {b:?} in {points:?}");
                let straight = ((a.x - b.x).abs() < 0.01 && (b.x - c.x).abs() < 0.01)
                    || ((a.y - b.y).abs() < 0.01 && (b.y - c.y).abs() < 0.01);
                assert!(!straight, "step {step}: {b:?} is no corner in {points:?}");
            }
            // no hidden bends pile up: an edit keeps only the corners that were visible
            // and adds at most the two points it moved
            let stored = data.links.iter().find(|l| l.id == "mother").unwrap();
            let visible_before = route.custom_bends.len();
            assert!(
                stored.route.len() <= visible_before + 2,
                "step {step}: {} stored bends, {visible_before} visible before: {:?}",
                stored.route.len(),
                stored.route
            );
            assert!(
                points
                    .windows(2)
                    .skip(1)
                    .take(points.len().saturating_sub(3))
                    .all(|pair| segment_clear(pair[0], pair[1], &obstacles)),
                "step {step}: crosses a card {points:?}"
            );
        }
    }

    #[test]
    fn edited_lines_show_only_real_corners_and_forget_straight_bends() {
        // video: handles next to the cards and on the long vertical were no corners
        let source = person("from", 0.0, 0.0);
        let target = person("to", -60.0, 300.0);
        let people = [source.clone(), target.clone()];
        let link = Link {
            route: vec![[200.0, 23.0], [200.0, 60.0], [200.0, 323.0]],
            ..routed_link()
        };
        let shown = route(&source, &target, &link, &people);

        assert_eq!(shown.points.len(), 4, "{:?}", shown.points);
        assert_eq!(shown.custom_bends.len(), 2);
        let segment = shown
            .points
            .windows(2)
            .position(|pair| (pair[0].x - pair[1].x).abs() < 0.01)
            .unwrap();
        let saved = dragged_route(
            &shown.points,
            segment,
            Vector::new(30.0, 0.0),
            &shown.custom_bends,
        );
        assert_eq!(saved, vec![[230.0, 23.0], [230.0, 323.0]]);
    }

    #[test]
    fn a_segment_dropped_next_to_a_parallel_one_snaps_onto_it() {
        let points = [
            Point::new(0.0, 0.0),
            Point::new(50.0, 0.0),
            Point::new(50.0, 40.0),
            Point::new(100.0, 40.0),
            Point::new(100.0, 80.0),
            Point::new(150.0, 80.0),
        ];
        // moving the first vertical to x = 96 lands 4 units from the one at x = 100
        let saved = dragged_route(&points, 1, Vector::new(46.0, 0.0), &[]);
        assert_eq!(saved, vec![[100.0, 0.0]]);
        let corner = snap_vertex(&points, 2, Point::new(53.0, 37.0));
        assert_eq!(corner, Point::new(50.0, 40.0));
    }

    #[test]
    fn dragging_terminal_segment_adds_an_orthogonal_detour() {
        let route = dragged_route(
            &[Point::new(0.0, 10.0), Point::new(100.0, 10.0)],
            0,
            Vector::new(0.0, 25.0),
            &[],
        );
        assert_eq!(route, vec![[0.0, 35.0], [100.0, 35.0]]);
    }

    #[test]
    fn dragging_inner_segment_moves_its_bends() {
        let route = dragged_route(
            &[
                Point::new(0.0, 0.0),
                Point::new(20.0, 0.0),
                Point::new(20.0, 80.0),
                Point::new(100.0, 80.0),
            ],
            1,
            Vector::new(15.0, 0.0),
            &[],
        );
        assert_eq!(route, vec![[35.0, 0.0], [35.0, 80.0]]);
    }

    #[test]
    fn dragging_an_automatic_corner_turns_only_that_corner_into_a_user_bend() {
        let moved = move_route_vertex(&[], &[], 1, None, Point::new(55.0, 30.0));

        assert_eq!(moved, vec![[55.0, 30.0]]);
        let route = route(
            &person("from", 0.0, 0.0),
            &person("to", 300.0, 100.0),
            &Link {
                route: moved,
                ..routed_link()
            },
            &[person("from", 0.0, 0.0), person("to", 300.0, 100.0)],
        );
        assert_eq!(route.custom_bends.len(), 1);
        assert!(is_orthogonal(&route.points));
    }

    #[test]
    fn dragging_an_edge_keeps_old_user_bends_without_saving_router_corners() {
        let source = person("source", 0.0, 0.0);
        let target = person("target", 320.0, 140.0);
        let link = Link {
            route: vec![[140.0, 70.0]],
            ..routed_link()
        };
        let route = route(&source, &target, &link, &[source.clone(), target.clone()]);
        let segment = route
            .points
            .windows(2)
            .position(|pair| (pair[0].y - pair[1].y).abs() < 0.01)
            .unwrap();
        let new_bends = dragged_route(
            &route.points,
            segment,
            Vector::new(0.0, 20.0),
            &route.custom_bends,
        );

        assert_eq!(new_bends.len(), 3);
        assert!(new_bends.contains(&[140.0, 70.0]));
    }

    #[test]
    fn a_label_offset_stays_attached_when_its_segment_moves() {
        let position = attach_label_to_segment(
            &[Point::new(0.0, 10.0), Point::new(100.0, 10.0)],
            0,
            Point::new(50.0, 20.0),
        );
        let moved_segment = [Point::new(0.0, 35.0), Point::new(100.0, 35.0)];
        let label = Point::new(
            moved_segment[0].x
                + (moved_segment[1].x - moved_segment[0].x) * position.along
                + position.offset[0],
            moved_segment[0].y
                + (moved_segment[1].y - moved_segment[0].y) * position.along
                + position.offset[1],
        );
        assert_eq!(label, Point::new(50.0, 45.0));
    }

    #[test]
    fn a_label_tracks_a_new_detour_segment_when_the_route_is_edited() {
        let position = LinkLabelPosition {
            segment: 0,
            along: 0.5,
            offset: [0.0, 8.0],
            legacy_absolute: false,
        };
        assert_eq!(
            remap_label_for_drag(position, 0, 2),
            LinkLabelPosition {
                segment: 1,
                ..position
            }
        );
    }

    #[test]
    fn old_absolute_label_positions_still_load() {
        let position: LinkLabelPosition = serde_json::from_str("[120.0, 80.0]").unwrap();
        assert!(position.legacy_absolute);
        assert_eq!(position.offset, [120.0, 80.0]);
    }

    #[test]
    fn dragging_a_route_segment_survives_the_persisted_route_round_trip() {
        let source = person("source", 200.0, 40.0);
        let target = person("target", -60.0, 300.0);
        let people = [source.clone(), target.clone()];
        let automatic = route(&source, &target, &routed_link(), &people);
        let segment = automatic
            .points
            .windows(2)
            .position(|pair| (pair[0].x - pair[1].x).abs() < 0.01)
            .unwrap();
        let edited = dragged_route(
            &automatic.points,
            segment,
            Vector::new(40.0, 0.0),
            &automatic.custom_bends,
        );
        let link = Link {
            route: edited.clone(),
            start_port: Some(automatic.start_port as u8),
            end_port: Some(automatic.end_port as u8),
            ..routed_link()
        };

        let rendered = route(&source, &target, &link, &people);
        let obstacles: Vec<Rectangle> = people.iter().map(inflated).collect();

        assert_on_border(rendered.points[0], &source);
        assert_on_border(*rendered.points.last().unwrap(), &target);
        assert!(is_orthogonal(&rendered.points));
        assert!(
            rendered
                .points
                .windows(2)
                .skip(1)
                .take(rendered.points.len().saturating_sub(3))
                .all(|pair| segment_clear(pair[0], pair[1], &obstacles))
        );
        // every visible bend sits exactly on its stored waypoint; stored points that
        // are no corner (e.g. on the card edge) are not shown
        assert!(rendered.custom_bends.len() >= 2);
        for &(point_index, stored_index) in &rendered.custom_bends {
            assert_eq!(
                rendered.points[point_index],
                Point::new(edited[stored_index][0], edited[stored_index][1])
            );
        }
    }

    #[test]
    fn dragging_the_start_segment_moves_its_attachment_along_the_card_edge() {
        let source = person("source", 200.0, 40.0);
        let target = person("target", -60.0, 300.0);
        let people = [source.clone(), target.clone()];
        let automatic = route(&source, &target, &routed_link(), &people);
        let first = automatic.points[0];
        let next = automatic.points[1];
        let delta = if (first.x - next.x).abs() < 0.01 {
            Vector::new(30.0, 0.0)
        } else {
            Vector::new(0.0, 30.0)
        };
        let bends = dragged_route(&automatic.points, 0, delta, &automatic.custom_bends);
        let edited = Link {
            route: bends,
            start_port: Some(automatic.start_port as u8),
            end_port: Some(automatic.end_port as u8),
            ..routed_link()
        };
        let moved = route(&source, &target, &edited, &people);
        let moved_start = moved.points[0];

        assert_on_border(moved_start, &source);
        assert!(moved_start.distance(first) > 1.0);
        assert!(is_orthogonal(&moved.points));
    }
}

fn draw_edges(app: &App, frame: &mut Frame, cam: &Camera, view: Rectangle) {
    let z = cam.zoom;
    // routes are cached in world space; only visible ones are transformed and batched
    let mut solid: Vec<&Route> = Vec::new();
    let mut dashed: Vec<&Route> = Vec::new();
    let mut selected: Option<&Route> = None;
    let mut arrows: Vec<[Point; 3]> = Vec::new();
    let mut labels: Vec<(Point, bool, Cow<str>)> = Vec::new();
    let arrow = (8.0 * z).clamp(3.0, 9.0);
    let show_labels = z >= LOD_EDGE_LABELS;
    let label_size = LABEL_TEXT * z.min(1.4);

    for link in &app.data.links {
        let Some(r) = app.routes.get(&link.id) else {
            continue;
        };
        if !app.link_shown(link) {
            continue;
        }
        let label = (show_labels && !link.label.is_empty()).then(|| label_layout(link, r));
        // the shown label text is costly (relation names in every language), so it is
        // only resolved for labels whose row is in view
        let mut text = None;
        let visible = r.bbox.intersects(&view)
            || label.is_some_and(|(at, centered)| {
                let half = label_size / z;
                if at.y + half < view.y || at.y - half > view.y + view.height {
                    return false;
                }
                let shown = app.data.link_label(&link.label);
                let width = label_width(&shown, label_size) / z;
                let left = if centered { at.x - width / 2.0 } else { at.x };
                text = Some(shown);
                left <= view.x + view.width && left + width >= view.x
            });
        if !visible {
            continue;
        }

        let style = link_style(&link.label);
        if app.selected_link.as_deref() == Some(link.id.as_str()) {
            selected = Some(r);
        }
        if style != LinkStyle::Undirected && r.points.len() >= 2 {
            let tip = cam.to_screen(r.points[r.points.len() - 1]);
            let prev = cam.to_screen(r.points[r.points.len() - 2]);
            let (dx, dy) = (tip.x - prev.x, tip.y - prev.y);
            let len = (dx * dx + dy * dy).sqrt().max(0.001);
            let (ux, uy) = (dx / len, dy / len);
            let base = Point::new(tip.x - ux * arrow, tip.y - uy * arrow);
            let half = arrow * 0.55;
            arrows.push([
                tip,
                Point::new(base.x - uy * half, base.y + ux * half),
                Point::new(base.x + uy * half, base.y - ux * half),
            ]);
        }
        if let Some((at, centered)) = label {
            labels.push((
                cam.to_screen(at),
                centered,
                text.unwrap_or_else(|| app.data.link_label(&link.label)),
            ));
        }
        match style {
            LinkStyle::Dashed => dashed.push(r),
            _ => solid.push(r),
        }
    }

    let width = (1.4 * z).clamp(0.6, 1.6);
    if !solid.is_empty() {
        frame.stroke(
            &routes_path(&solid, cam),
            Stroke::default().with_width(width).with_color(ui::pal().edge),
        );
    }
    if !dashed.is_empty() {
        // dashed here, not via LineDash: iced carries the dash phase across subpaths
        // and joins the end of one route to the next with a phantom segment
        frame.stroke(
            &dashed_routes_path(&dashed, cam, 6.0, 4.0),
            Stroke::default().with_width(width).with_color(ui::pal().edge),
        );
    }
    if let Some(r) = selected {
        frame.stroke(
            &routes_path(&[r], cam),
            Stroke::default()
                .with_width((3.0 * z).clamp(2.0, 4.0))
                .with_color(ui::pal().selection),
        );
    }
    if !arrows.is_empty() {
        let path = Path::new(|b| {
            for [t, l, r] in &arrows {
                b.move_to(*t);
                b.line_to(*l);
                b.line_to(*r);
                b.close();
            }
        });
        frame.fill(&path, ui::pal().edge);
    }
    if let Some(r) = selected {
        for point_index in 1..r.points.len().saturating_sub(1) {
            let stored_index = r
                .custom_bends
                .iter()
                .find_map(|(index, stored)| (*index == point_index).then_some(*stored));
            let active = stored_index.is_some() && app.selected_link_vertex == stored_index;
            let handle = Path::circle(
                cam.to_screen(r.points[point_index]),
                if active { 7.0 } else { 5.0 },
            );
            frame.fill(&handle, ui::pal().node);
            frame.stroke(
                &handle,
                Stroke::default().with_width(1.5).with_color(if active {
                    ui::pal().selection
                } else {
                    ui::pal().edge
                }),
            );
        }
    }

    if labels.is_empty() {
        return;
    }
    // one batched fill for all label backgrounds, then the texts
    let backgrounds = Path::new(|b| {
        for (at, centered, label) in &labels {
            let w = label_width(label, label_size);
            let x = if *centered { at.x - w / 2.0 } else { at.x - 4.0 };
            b.rectangle(
                Point::new(x, at.y - label_size * 0.7),
                Size::new(w, label_size * 1.4),
            );
        }
    });
    frame.fill(&backgrounds, ui::pal().board);
    for (at, centered, label) in labels {
        text(
            app,
            label.into_owned(),
            at,
            label_size,
            ui::pal().muted,
            false,
            centered,
        );
    }
}

/// All routes as one path in screen space.
fn routes_path(routes: &[&Route], cam: &Camera) -> Path {
    Path::new(|b| {
        for route in routes {
            if let Some((first, rest)) = route.points.split_first() {
                b.move_to(cam.to_screen(*first));
                rest.iter().for_each(|p| b.line_to(cam.to_screen(*p)));
            }
        }
    })
}

/// All routes as one path of dashes in screen space; the pattern restarts on every route.
fn dashed_routes_path(routes: &[&Route], cam: &Camera, on: f32, off: f32) -> Path {
    Path::new(|b| {
        for route in routes {
            let mut drawing = true;
            let mut left = on;
            let mut prev: Option<Point> = None;
            for p in route.points.iter().map(|p| cam.to_screen(*p)) {
                let Some(mut from) = prev else {
                    b.move_to(p);
                    prev = Some(p);
                    continue;
                };
                let (dx, dy) = (p.x - from.x, p.y - from.y);
                let mut len = (dx * dx + dy * dy).sqrt();
                if len > 0.0 {
                    let (ux, uy) = (dx / len, dy / len);
                    while len > left {
                        from = Point::new(from.x + ux * left, from.y + uy * left);
                        len -= left;
                        if drawing {
                            b.line_to(from);
                        } else {
                            b.move_to(from);
                        }
                        drawing = !drawing;
                        left = if drawing { on } else { off };
                    }
                    left -= len;
                }
                if drawing {
                    b.line_to(p);
                } else {
                    b.move_to(p);
                }
                prev = Some(p);
            }
        }
    })
}

fn polyline(points: &[Point]) -> Path {
    Path::new(|builder| {
        if let Some((first, rest)) = points.split_first() {
            builder.move_to(*first);
            rest.iter().for_each(|point| builder.line_to(*point));
        }
    })
}

fn draw_nodes(app: &App, frame: &mut Frame, cam: &Camera, view: Rectangle) {
    let z = cam.zoom;
    let people = &app.data.people;

    let mut visible: Vec<usize> = Vec::new();
    app.grid.query(view, |i| {
        let p = &people[i as usize];
        if p.x + NODE_W >= view.x
            && p.x <= view.x + view.width
            && p.y + NODE_H >= view.y
            && p.y <= view.y + view.height
            && app.shown(p)
        {
            visible.push(i as usize);
        }
    });

    let size = Size::new(NODE_W * z, NODE_H * z);

    if z >= LOD_DETAIL {
        let pal = ui::pal();
        let radius = 8.0 * z;
        // soft drop shadow, one batched path offset below the cards
        let shadows = Path::new(|b| {
            for &i in &visible {
                let p = &people[i];
                let s = cam.to_screen(Point::new(p.x, p.y));
                rounded_rect(b, Point::new(s.x, s.y + 2.0 * z), size, radius);
            }
        });
        frame.fill(&shadows, pal.node_shadow);
        let bodies = Path::new(|b| {
            for &i in &visible {
                let p = &people[i];
                rounded_rect(b, cam.to_screen(Point::new(p.x, p.y)), size, radius);
            }
        });
        frame.fill(&bodies, pal.node);
        frame.stroke(
            &bodies,
            Stroke::default()
                .with_width(1.0)
                .with_color(pal.node_border),
        );

        // accent bar in the colour of the first clan, one path per clan colour
        let mut bars: HashMap<usize, Vec<Point>> = HashMap::new();
        for &i in &visible {
            let p = &people[i];
            if let Some(&ci) = p.clans.first().and_then(|c| app.data.clan_idx.get(c)) {
                bars.entry(ci)
                    .or_default()
                    .push(cam.to_screen(Point::new(p.x, p.y)));
            }
        }
        let bar = Size::new(size.width - 2.0 * radius, (3.0 * z).max(1.5));
        for (ci, points) in bars {
            let path = Path::new(|b| {
                for s in &points {
                    rounded_rect(b, Point::new(s.x + radius, s.y), bar, bar.height / 2.0);
                }
            });
            frame.fill(&path, app.data.clans[ci].color());
        }

        let mut dots: HashMap<usize, Vec<Point>> = HashMap::new();
        let name_size = 13.0 * z;
        let year_size = 11.5 * z;
        for &i in &visible {
            let p = &people[i];
            let s = cam.to_screen(Point::new(p.x, p.y));
            let cx = s.x + size.width / 2.0;
            text(
                app,
                ellipsize(&p.name, 17),
                Point::new(cx, s.y + size.height * 0.35),
                name_size,
                ui::pal().text,
                true,
                true,
            );

            // notes / attachment markers, top-right corner of the card
            let mut mx = s.x + size.width - 8.0 * z;
            for (present, letter, color) in [
                (app.with_files.contains(&p.id), "F", ui::pal().file_mark),
                (app.with_notes.contains(&p.id), "N", ui::pal().note_mark),
            ] {
                if present {
                    text(
                        app,
                        letter.to_owned(),
                        Point::new(mx, s.y + 8.0 * z),
                        9.0 * z,
                        color,
                        true,
                        true,
                    );
                    mx -= 9.0 * z;
                }
            }

            let years = p.years();
            let dot_count = p.clans.len() as f32;
            let years_w = years.chars().count() as f32 * year_size * 0.55;
            let dots_w = if dot_count > 0.0 {
                (dot_count * 8.0 + 2.0) * z
            } else {
                0.0
            };
            let x0 = cx - (years_w + dots_w) / 2.0;
            let y = s.y + size.height * 0.72;
            if !years.is_empty() {
                text(
                    app,
                    years,
                    Point::new(x0, y),
                    year_size,
                    ui::pal().muted,
                    false,
                    false,
                );
            }
            for (k, clan_id) in p.clans.iter().enumerate() {
                if let Some(&ci) = app.data.clan_idx.get(clan_id) {
                    dots.entry(ci)
                        .or_default()
                        .push(Point::new(x0 + years_w + (6.0 + k as f32 * 8.0) * z, y));
                }
            }
        }
        for (ci, points) in dots {
            let path = Path::new(|b| points.iter().for_each(|p| b.circle(*p, 3.0 * z)));
            frame.fill(&path, app.data.clans[ci].color());
        }
    } else {
        // blocks, one path per clan colour
        let none = usize::MAX;
        let mut groups: HashMap<usize, Vec<Point>> = HashMap::new();
        for &i in &visible {
            let p = &people[i];
            let g = p
                .clans
                .first()
                .and_then(|c| app.data.clan_idx.get(c).copied())
                .unwrap_or(none);
            groups
                .entry(g)
                .or_default()
                .push(cam.to_screen(Point::new(p.x, p.y)));
        }
        let block = Size::new(size.width.max(1.5), size.height.max(1.0));
        for (g, points) in groups {
            let color = if g == none {
                ui::pal().muted
            } else {
                app.data.clans[g].color()
            };
            let path = Path::new(|b| points.iter().for_each(|p| b.rectangle(*p, block)));
            frame.fill(&path, Color { a: 0.75, ..color });
        }
    }

    // timeline: people who died within the chosen range fade into the board
    if app.years.is_some() {
        let radius = if z >= LOD_DETAIL { 8.0 * z } else { 0.0 };
        let veil = Path::new(|b| {
            for &i in visible.iter().filter(|&&i| app.faded(&people[i])) {
                let s = cam.to_screen(Point::new(people[i].x, people[i].y));
                rounded_rect(b, s, size, radius);
            }
        });
        frame.fill(&veil, ui::alpha(ui::pal().board, 0.62));
    }

    // selection: soft halo + crisp accent outline
    if let Some(p) = app.selected_person() {
        let accent = ui::pal().selection;
        let s = cam.to_screen(Point::new(p.x, p.y));
        let ring = |pad: f32| {
            Path::new(|b| {
                rounded_rect(
                    b,
                    Point::new(s.x - pad, s.y - pad),
                    Size::new(size.width + pad * 2.0, size.height + pad * 2.0),
                    8.0 * z + pad,
                )
            })
        };
        frame.stroke(
            &ring(3.5),
            Stroke::default()
                .with_width(5.0)
                .with_color(ui::alpha(accent, 0.22)),
        );
        frame.stroke(
            &ring(1.5),
            Stroke::default().with_width(2.0).with_color(accent),
        );
        if z >= LOD_HANDLES {
            for h in handle_points(p, cam) {
                let c = Path::circle(h, 5.0);
                frame.fill(&c, ui::pal().node);
                frame.stroke(&c, Stroke::default().with_width(2.0).with_color(accent));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn rounded_rect(b: &mut Builder, p: Point, s: Size, r: f32) {
    let r = r.min(s.width / 2.0).min(s.height / 2.0);
    if r <= 0.5 {
        b.rectangle(p, s);
        return;
    }
    let (x, y, w, h) = (p.x, p.y, s.width, s.height);
    b.move_to(Point::new(x + r, y));
    b.line_to(Point::new(x + w - r, y));
    b.arc_to(Point::new(x + w, y), Point::new(x + w, y + r), r);
    b.line_to(Point::new(x + w, y + h - r));
    b.arc_to(Point::new(x + w, y + h), Point::new(x + w - r, y + h), r);
    b.line_to(Point::new(x + r, y + h));
    b.arc_to(Point::new(x, y + h), Point::new(x, y + h - r), r);
    b.line_to(Point::new(x, y + r));
    b.arc_to(Point::new(x, y), Point::new(x + r, y), r);
    b.close();
}

/// Queues board text; it is drawn per frame by `World`, outside the scaled mesh cache.
fn text(
    app: &App,
    content: String,
    at: Point,
    size: f32,
    color: Color,
    bold: bool,
    centered: bool,
) {
    let horizontal = if centered {
        alignment::Horizontal::Center
    } else {
        alignment::Horizontal::Left
    };
    app.snapshot_text.borrow_mut().push(canvas::Text {
        content,
        position: at,
        color,
        size: Pixels(size),
        font: if bold {
            Font {
                weight: Weight::Bold,
                ..Font::DEFAULT
            }
        } else {
            Font::DEFAULT
        },
        align_x: horizontal.into(),
        align_y: alignment::Vertical::Center.into(),
        ..canvas::Text::default()
    });
}

fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}
