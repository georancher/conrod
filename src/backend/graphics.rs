//! Conrod's generic graphics backend.
//!
//! **Note:** Conrod currently uses Piston's generic [graphics
//! crate](https://github.com/PistonDevelopers/graphics) (and specifically the
//! [**Graphics**](http://docs.piston.rs/graphics/graphics/trait.Graphics.html)) and
//! [**CharacterCache**](http://docs.piston.rs/graphics/graphics/character/trait.CharacterCache.html)
//! traits to enable genericity over custom user backends. This dependency may change in the near
//! future in favour of a simplified conrod-specific graphics and character caching backend trait.
//!
//! This is the only module in which the piston graphics crate will be used directly.


use {Backend, Color, Point, Rect, Scalar};
use graph::{self, Container, Graph, NodeIndex};
use piston_graphics;
use std::any::Any;
use std::iter::once;
use theme::Theme;
use widget::primitive;

#[doc(inline)]
pub use piston_graphics::{Context, DrawState, Graphics, ImageSize, Transformed};
#[doc(inline)]
pub use piston_graphics::character::{Character, CharacterCache};


/// Draw the given **Graph** using the given **CharacterCache** and **Graphics** backends.
pub fn draw_from_graph<B, G>(context: Context,
                             graphics: &mut G,
                             character_cache: &mut B::CharacterCache,
                             graph: &Graph,
                             depth_order: &[NodeIndex],
                             theme: &Theme)
    where B: Backend,
          G: Graphics<Texture=B::Texture>,
{

    // A stack of contexts, one for each scroll group.
    //
    // FIXME: This allocation every time draw is called is unnecessary. We should re-use a buffer
    // (perhaps owned by the Ui) for this.
    let mut crop_stack: Vec<(NodeIndex, Context)> = Vec::new();

    // Retrieve the core window widget so that we can use it to filter visible widgets.
    let window_idx = NodeIndex::new(0);
    let window = match graph.widget(window_idx){
        Some(window) => window,
        // If we don't yet have the window widget, we won't have *any* widgets, so bail out.
        None => return,
    };

    // A function for checking whether or not a widget would be visible.
    //
    // TODO: Refactor this into a `visible_area_of_widget` graph algo. Also, consider calculating
    // the visible area during the `set_widgets` stage, as it might be more optimal than doing so
    // here.
    let is_visible = |idx: NodeIndex, container: &Container| -> bool {
        container.rect.overlap(window.rect).is_some()
        && graph::algo::cropped_area_of_widget(graph, idx).is_some()
    };

    // The depth order describes the order in which widgets should be drawn.
    for &idx in depth_order {
        if let Some(ref container) = graph.widget(idx) {

            // If we're currently using a cropped context and the current `crop_parent_idx` is
            // *not* a depth-wise parent of the widget at the current `idx`, we should pop that
            // cropped context from the stack as we are done with it.
            while let Some(&(crop_parent_idx, _)) = crop_stack.last() {
                if graph.does_recursive_depth_edge_exist(crop_parent_idx, idx) {
                    break;
                } else {
                    crop_stack.pop();
                }
            }

            // Check the stack for the current Context.
            let context = crop_stack.last().map(|&(_, ctxt)| ctxt).unwrap_or(context);

            // Draw the widget, but only if it would actually be visible on the window.
            if is_visible(idx, container) {
                draw_from_container::<B, G>(&context, graphics, character_cache, container, theme);
            }

            // If the current widget should crop its children, we need to add a context for it to
            // the top of the stack.
            if container.crop_kids {
                let context = crop_context(context, container.kid_area.rect);
                crop_stack.push((idx, context));
            }
        }
    }
        
}


/// Crop the given **Context** to the given **Rect**.
///
/// This is non-trivial as we must consider the view_size, viewport, the difference in
/// co-ordinate systems and the conversion from `f64` dimensions to `u16`.
fn crop_context(context: Context, rect: Rect) -> Context {
    use utils::map_range;
    let Context { draw_state, .. } = context;

    let (x, y, w, h) = rect.x_y_w_h();

    // Our view_dim is our virtual window size which is consistent no matter the display.
    let view_dim = context.get_view_size();

    // Our draw_dim is the actual window size in pixels. Our target crop area must be
    // represented in this size.
    let draw_dim = match context.viewport {
        Some(viewport) => [viewport.draw_size[0] as f64, viewport.draw_size[1] as f64],
        None => view_dim,
    };

    // Calculate the distance to the edges of the window from the center.
    let left = -view_dim[0] / 2.0;
    let right = view_dim[0] / 2.0;
    let bottom = -view_dim[1] / 2.0;
    let top = view_dim[1] / 2.0;

    // We start with the x and y in the center of our crop area, however we need it to be
    // at the top left of the crop area.
    let left_x = x - w as f64 / 2.0;
    let top_y = y - h as f64 / 2.0;

    // Map the position at the top left of the crop area in view_dim to our draw_dim.
    let x = map_range(left_x, left, right, 0, draw_dim[0] as i32);
    let y = map_range(top_y, bottom, top, 0, draw_dim[1] as i32);

    // Convert the w and h from our view_dim to the draw_dim.
    let w_scale = draw_dim[0] / view_dim[0];
    let h_scale = draw_dim[1] / view_dim[1];
    let w = w * w_scale;
    let h = h * h_scale;

    // If we ended up with negative coords for the crop area, we'll use 0 instead as we
    // can't represent the negative coords with `u32` (the target DrawState dimension type).
    // We'll hold onto the lost negative values (x_neg and y_neg) so that we can compensate
    // with the width and height.
    let x_neg = if x < 0 { x } else { 0 };
    let y_neg = if y < 0 { y } else { 0 };
    let mut x = ::std::cmp::max(0, x) as u32;
    let mut y = ::std::cmp::max(0, y) as u32;
    let mut w = ::std::cmp::max(0, (w as i32 + x_neg)) as u32;
    let mut h = ::std::cmp::max(0, (h as i32 + y_neg)) as u32;

    // If there was already some scissor set, we must check for the intersection.
    if let Some(rect) = draw_state.scissor {
        let (r_x, r_y, r_w, r_h) = (rect[0], rect[1], rect[2], rect[3]);
        if x + w < r_x || r_x + r_w < x || y + h < r_y || r_y + r_h < y {
            // If there is no intersection, we have no scissor.
            w = 0;
            h = 0;
        } else {
            // If there is some intersection, calculate the overlapping rect.
            let (a_l, a_r, a_b, a_t) = (x, x+w, y, y+h);
            let (b_l, b_r, b_b, b_t) = (r_x, r_x+r_w, r_y, r_y+r_h);
            let l = if a_l > b_l { a_l } else { b_l };
            let r = if a_r < b_r { a_r } else { b_r };
            let b = if a_b > b_b { a_b } else { b_b };
            let t = if a_t < b_t { a_t } else { b_t };
            x = l;
            y = b;
            w = r - l;
            h = t - b;
        }
    }

    Context { draw_state: draw_state.scissor([x, y, w, h]), ..context }
}



/// Use the given **CharacterCache** and **Graphics** backends to draw the given widget.
pub fn draw_from_container<B, G>(context: &Context,
                                 graphics: &mut G,
                                 character_cache: &mut B::CharacterCache,
                                 container: &Container,
                                 theme: &Theme)
    where B: Backend,
          B::Texture: Any,
          G: Graphics<Texture=B::Texture>,
{
    use widget::primitive::shape::Style as ShapeStyle;

    match container.kind {

        primitive::shape::rectangle::KIND => {
            if let Some(rectangle) = container.unique_widget_state::<::Rectangle>() {
                match rectangle.style {
                    ShapeStyle::Fill(_) => {
                        let color = rectangle.style.get_color(theme);
                        draw_rectangle(context, graphics, container.rect, color);
                    },
                    ShapeStyle::Outline(line_style) => {
                        let (l, r, b, t) = container.rect.l_r_b_t();
                        let points = [[l, b], [l, t], [r, t], [r, b], [l, b]];
                        let points = points.iter().cloned();
                        draw_lines(context, graphics, theme, points, line_style);
                    },
                }
            }
        },

        primitive::shape::framed_rectangle::KIND => {
            if let Some(framed_rectangle) = container.unique_widget_state::<::FramedRectangle>() {
                let frame = framed_rectangle.style.frame(theme);
                if frame > 0.0 {
                    let frame_color = framed_rectangle.style.frame_color(theme);
                    let frame_rect = container.rect;
                    draw_rectangle(context, graphics, frame_rect, frame_color);
                }
                let color = framed_rectangle.style.color(theme);
                let rect = container.rect.pad(frame);
                draw_rectangle(context, graphics, rect, color);
            }
        },

        primitive::shape::oval::KIND => {
            if let Some(oval) = container.unique_widget_state::<::Oval>() {
                use std::f64::consts::PI;
                const CIRCLE_RESOLUTION: usize = 50;
                const NUM_POINTS: usize = CIRCLE_RESOLUTION + 1;
                let (x, y, w, h) = container.rect.x_y_w_h();
                let t = 2.0 * PI / CIRCLE_RESOLUTION as Scalar;
                let hw = w / 2.0;
                let hh = h / 2.0;
                let f = |i: Scalar| [x + hw * (t*i).cos(), y + hh * (t*i).sin()];
                let mut points = [[0.0, 0.0]; NUM_POINTS];
                for i in 0..NUM_POINTS {
                    points[i] = f(i as f64);
                }

                match oval.style {
                    ShapeStyle::Fill(_) => {
                        let color = oval.style.get_color(theme).to_fsa();
                        let polygon = piston_graphics::Polygon::new(color);
                        polygon.draw(&points, &context.draw_state, context.transform, graphics);
                    },
                    ShapeStyle::Outline(line_style) => {
                        let points = points.iter().cloned();
                        draw_lines(context, graphics, theme, points, line_style)
                    },
                }
            }
        },

        primitive::shape::polygon::KIND => {
            use widget::primitive::shape::Style;
            use widget::primitive::shape::polygon::State;

            if let Some(polygon) = container.state_and_style::<State, Style>() {
                match polygon.style {
                    ShapeStyle::Fill(_) => {
                        let color = polygon.style.get_color(theme).to_fsa();
                        let points = &polygon.state.points[..];
                        let polygon = piston_graphics::Polygon::new(color);
                        polygon.draw(points, &context.draw_state, context.transform, graphics);
                    },
                    ShapeStyle::Outline(line_style) => {
                        let mut points = polygon.state.points.iter().cloned();
                        let first = points.next();
                        let points = first.into_iter().chain(points).chain(first);
                        draw_lines(context, graphics, theme, points, line_style);
                    },
                }
            }
        },

        primitive::line::KIND => {
            if let Some(line) = container.unique_widget_state::<::Line>() {
                let points = once(line.state.start).chain(once(line.state.end));
                draw_lines(context, graphics, theme, points, line.style);
            }
        },

        primitive::point_path::KIND => {
            use widget::primitive::point_path::{State, Style};
            if let Some(point_path) = container.state_and_style::<State, Style>() {
                let points = point_path.state.points.iter().cloned();
                draw_lines(context, graphics, theme, points, point_path.style);
            }
        },

        primitive::text::KIND => {
            if let Some(text) = container.unique_widget_state::<::Text>() {
                use {Align, graph, text};

                let graph::UniqueWidgetState { ref state, ref style } = *text;

                let font_size = style.font_size(theme);
                let line_spacing = style.line_spacing(theme);
                let color = style.color(theme).to_fsa();
                let x_align = style.text_align(theme);
                let y_align = Align::End; // Always align text to top of Text's Rect.
                let rect = container.rect;
                let line_infos = state.line_infos.iter().cloned();
                let string = &state.string;

                let lines = line_infos.clone().map(|info| &string[info.byte_range()]);
                let line_rects =
                    text::line::rects(line_infos, font_size, rect, x_align, y_align, line_spacing);

                for (line, line_rect) in lines.zip(line_rects) {
                    let offset = [line_rect.left().round(), line_rect.bottom().round()];
                    let context = context.trans(offset[0], offset[1]).scale(1.0, -1.0);
                    let transform = context.transform;
                    let draw_state = &context.draw_state;
                    piston_graphics::text::Text::new_color(color, font_size)
                        .round()
                        .draw(line, character_cache, draw_state, transform, graphics);
                }
            }
        },

        primitive::image::KIND => {
            use widget::primitive::image::{State, Style};
            if let Some(image) = container.state_and_style::<State<B::Texture>, Style>() {
                let ::graph::UniqueWidgetState { ref state, ref style } = *image;
                if let Some(texture) = state.texture.as_ref() {
                    let mut image = piston_graphics::image::Image::new();
                    image.color = style.maybe_color.and_then(|c| c.map(|c| c.to_fsa()));
                    image.source_rectangle = Some({
                        let (x, y, w, h) = texture.src_rect.x_y_w_h();
                        [x as i32, y as i32, w as i32, h as i32]
                    });
                    let (left, top, w, h) = container.rect.l_t_w_h();
                    image.rectangle = Some([0.0, 0.0, w, h]);
                    let context = context.trans(left, top).scale(1.0, -1.0);
                    let transform = context.transform;
                    let draw_state = &context.draw_state;
                    image.draw(texture.arc.as_ref(), draw_state, transform, graphics);
                }
            }
        }

        _ => (),
    }
}


/// Converts a conrod `Rect` to a `piston_graphics::types::Rectangle` expected by the Graphics
/// backend.
pub fn conrod_rect_to_graphics_rect(rect: Rect) -> piston_graphics::types::Rectangle<Scalar> {
    let (l, b, w, h) = rect.l_b_w_h();
    [l, b, w, h]
}


/// Draw a rectangle at the given Rect.
pub fn draw_rectangle<G>(context: &Context,
                         graphics: &mut G,
                         rect: Rect,
                         color: Color)
    where G: Graphics,
{
    let (l, b, w, h) = rect.l_b_w_h();
    let lbwh = [l, b, w, h];
    let rectangle = piston_graphics::Rectangle::new(color.to_fsa());
    rectangle.draw(lbwh, &context.draw_state, context.transform, graphics);
}


/// Draw a series of lines between the given **Point**s using the given style.
pub fn draw_lines<G, I>(context: &Context,
                        graphics: &mut G,
                        theme: &Theme,
                        mut points: I,
                        style: primitive::line::Style)
    where G: Graphics,
          I: Iterator<Item=Point>,
{
    use widget::primitive::line::{Cap, Pattern};

    if let Some(first) = points.next() {
        let pattern = style.get_pattern(theme);
        let color = style.get_color(theme).to_fsa();
        let thickness = style.get_thickness(theme);
        let cap = style.get_cap(theme);
        match pattern {
            Pattern::Solid => {
                let line = match cap {
                    Cap::Flat => piston_graphics::Line::new(color, thickness / 2.0),
                    Cap::Round => piston_graphics::Line::new_round(color, thickness / 2.0),
                };
                let mut start = first;
                for end in points {
                    let coords = [start[0], start[1], end[0], end[1]];
                    line.draw(coords, &context.draw_state, context.transform, graphics);
                    start = end;
                }
            },
            Pattern::Dashed => unimplemented!(),
            Pattern::Dotted => unimplemented!(),
        }
    }
}
