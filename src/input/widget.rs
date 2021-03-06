//! Contains all the logic for filtering input events and making them relative to widgets.
//!
//! The core of this module is the `Widget::for_widget` method, which creates an
//! `InputProvider` that provides input events for a specific widget.

use {Point, Rect};
use event;
use input;
use utils;
use widget;


/// Provides only events and input state that are relevant to a specific widget.
///
/// This type can be produced by calling the `UiCell::input` method with the target widget's
/// `widget::Index`. This is particularly useful
///
/// Unlike `input::Global`, `input::Widget` methods are tailored to the widget for which they are
/// produced.
#[derive(Clone)]
pub struct Widget<'a> {
    global: &'a input::Global,
    rect: Rect,
    idx: widget::Index,
}

/// A view of the `input::state::Mouse` that is specific to a single widget.
#[derive(Copy, Clone, Debug)]
pub struct Mouse<'a> {
    rect: Rect,
    mouse_abs_xy: Point,
    /// The state of each `MouseButton`.
    pub buttons: &'a input::state::mouse::ButtonMap,
}

/// An iterator yielding all events that are relevant to a specific widget.
///
/// All events provided by this Iterator will be filtered in accordance with input capturing. For
/// example: If the widget does not capture the mouse, it *will not* receive any mouse-related
/// events. If the widget captures the keyboard it *will* receive all keyboard events.
///
/// All mouse events will have their coordinates relative to the middle of the widget's `Rect`.
#[derive(Clone)]
pub struct Events<'a> {
    ui_events: input::global::UiEvents<'a>,
    capturing_keyboard: Option<widget::Index>,
    capturing_mouse: Option<widget::Index>,
    rect: Rect,
    idx: widget::Index,
}

/// An `Iterator` yielding all mouse clicks occuring within the given sequence of `widget::Event`s.
#[derive(Clone)]
pub struct Clicks<'a> {
    events: Events<'a>,
}

/// An `Iterator` yielding all mouse `button` clicks occuring within the given sequence of
/// `widget::Click`s.
#[derive(Clone)]
pub struct ButtonClicks<'a> {
    clicks: Clicks<'a>,
    button: input::MouseButton,
}

/// An iterator that yields all `event::Drag` events yielded by the `Events` iterator.
///
/// Only events that occurred while the widget was capturing the device that did the dragging will
/// be yielded.
#[derive(Clone)]
pub struct Drags<'a> {
    events: Events<'a>,
}

/// An `Iterator` yielding all mouse `button` drags occuring within the given sequence of
/// `widget::Drag`s.
#[derive(Clone)]
pub struct ButtonDrags<'a> {
    drags: Drags<'a>,
    button: input::MouseButton,
}

/// An iterator that yields all `Input::Text` events yielded by the `Events` iterator.
///
/// Only events that occurred while the widget was capturing the keyboard will be yielded.
#[derive(Clone)]
pub struct Texts<'a> {
    events: Events<'a>,
}

/// An iterator that yields all `Scroll` events yielded by the given `Events` iterator.
#[derive(Clone)]
pub struct Scrolls<'a> {
    events: Events<'a>,
}


impl<'a> Widget<'a> {

    /// Returns a `Widget` with events specifically for the given widget.
    ///
    /// Filters out only the events that directly pertain to the widget.
    ///
    /// All events will also be made relative to the widget's own (0, 0) origin.
    pub fn for_widget(idx: widget::Index, rect: Rect, global: &'a input::Global) -> Self {
        Widget {
            global: global,
            rect: rect,
            idx: idx,
        }
    }

    /// If the widget is currently capturing the mouse, this returns the state of the mouse.
    ///
    /// Returns `None` if the widget is not capturing the mouse.
    pub fn mouse(&self) -> Option<Mouse<'a>> {
        if self.global.current.widget_capturing_mouse == Some(self.idx) {
            let mouse = Mouse {
                buttons: &self.global.current.mouse.buttons,
                mouse_abs_xy: self.global.current.mouse.xy,
                rect: self.rect,
            };
            return Some(mouse);
        }
        None
    }

    /// Produces an iterator yielding all events that are relevant to a specific widget.
    ///
    /// All events provided by this Iterator will be filtered in accordance with input capturing. For
    /// example: If the widget does not capture the mouse, it *will not* receive any mouse-related
    /// events. If the widget captures the keyboard it *will* receive all keyboard events.
    ///
    /// All mouse events will have their coordinates relative to the middle of the widget's `Rect`.
    pub fn events(&self) -> Events<'a> {
        Events {
            ui_events: self.global.events().ui(),
            capturing_keyboard: self.global.start.widget_capturing_keyboard,
            capturing_mouse: self.global.start.widget_capturing_mouse,
            rect: self.rect,
            idx: self.idx,
        }
    }

    /// Filters all events yielded by `Self::events` for all `event::Click`s.
    ///
    /// A _click_ is determined to have occured if a pointing device button was both pressed and
    /// released over the widget.
    pub fn clicks(&self) -> Clicks<'a> {
        Clicks { events: self.events() }
    }

    /// Produces an iterator that yields all `event::Drag` events yielded by the `Events` iterator.
    ///
    /// Only events that occurred while the widget was capturing the device that did the dragging
    /// will be yielded.
    pub fn drags(&self) -> Drags<'a> {
        Drags { events: self.events() }
    }

    /// Produces an iterator that yields all `Input::Text` events that have occurred as `&str`s
    /// since the last time `Ui::set_widgets` was called.
    ///
    /// Only events that occurred while the widget was capturing the keyboard will be yielded.
    pub fn texts(&self) -> Texts<'a> {
        Texts { events: self.events() }
    }

    /// Produce an iterator that yields only the `Scroll` events yielded by the `Events` iterator.
    pub fn scrolls(&self) -> Scrolls<'a> {
        Scrolls { events: self.events() }
    }

}

impl<'a> Mouse<'a> {

    /// The absolute position of the mouse within the window.
    pub fn abs_xy(&self) -> Point {
        self.mouse_abs_xy
    }

    /// The position of the mouse relative to the middle of the widget's `Rect`.
    pub fn rel_xy(&self) -> Point {
        utils::vec2_sub(self.mouse_abs_xy, self.rect.xy())
    }

    /// Is the mouse currently over the widget.
    pub fn is_over(&self) -> bool {
        self.rect.is_over(self.mouse_abs_xy)
    }

}

impl<'a> Clicks<'a> {

    /// Yield only the `Click`s that occurred from the given button.
    pub fn button(self, button: input::MouseButton) -> ButtonClicks<'a> {
        ButtonClicks {
            clicks: self,
            button: button,
        }
    }

    /// Yield only left mouse button `Click`s.
    pub fn left(self) -> ButtonClicks<'a> {
        self.button(input::MouseButton::Left)
    }

    /// Yields only middle mouse button `Click`s.
    pub fn middle(self) -> ButtonClicks<'a> {
        self.button(input::MouseButton::Middle)
    }

    /// Yield only right mouse button `Click`s.
    pub fn right(self) -> ButtonClicks<'a> {
        self.button(input::MouseButton::Right)
    }

}

impl<'a> Drags<'a> {

    /// Yield only the `Drag`s that occurred from the given button.
    pub fn button(self, button: input::MouseButton) -> ButtonDrags<'a> {
        ButtonDrags {
            drags: self,
            button: button,
        }
    }

    /// Yield only left mouse button `Drag`s.
    pub fn left(self) -> ButtonDrags<'a> {
        self.button(input::MouseButton::Left)
    }
    
    /// Yields only middle mouse button `Drag`s.
    pub fn middle(self) -> ButtonDrags<'a> {
        self.button(input::MouseButton::Middle)
    }

    /// Yield only right mouse button `Drag`s.
    pub fn right(self) -> ButtonDrags<'a> {
        self.button(input::MouseButton::Right)
    }

}


impl<'a> Iterator for Events<'a> {
    type Item = event::Widget;

    fn next(&mut self) -> Option<event::Widget> {
        // Loop through all events in the `ui_events` until we find one associated with our widget
        // that we can return.
        while let Some(ui_event) = self.ui_events.next() {
            match *ui_event {

                // Mouse capturing.
                event::Ui::WidgetCapturesMouse(idx) => {
                    self.capturing_mouse = Some(idx);
                    if idx == self.idx {
                        return Some(event::Widget::CapturesMouse);
                    }
                },
                event::Ui::WidgetUncapturesMouse(idx) => {
                    if Some(idx) == self.capturing_mouse {
                        self.capturing_mouse = None;
                    }
                    if idx == self.idx {
                        return Some(event::Widget::UncapturesMouse);
                    }
                },

                // Keyboard capturing.
                event::Ui::WidgetCapturesKeyboard(idx) => {
                    self.capturing_keyboard = Some(idx);
                    if idx == self.idx {
                        return Some(event::Widget::CapturesKeyboard);
                    }
                },
                event::Ui::WidgetUncapturesKeyboard(idx) => {
                    if Some(idx) == self.capturing_keyboard {
                        self.capturing_keyboard = None;
                    }
                    if idx == self.idx {
                        return Some(event::Widget::UncapturesKeyboard);
                    }
                },

                event::Ui::WindowResized(dim) =>
                    return Some(event::Widget::WindowResized(dim)),

                event::Ui::Text(idx, ref text) if idx == Some(self.idx) =>
                    return Some(text.clone().into()),

                event::Ui::Move(idx, ref move_) if idx == Some(self.idx) =>
                    return Some(move_.clone().into()),

                event::Ui::Press(idx, ref press) if idx == Some(self.idx) =>
                    return Some(press.clone().relative_to(self.rect.xy()).into()),
                
                event::Ui::Release(idx, ref release) if idx == Some(self.idx) =>
                    return Some(release.clone().relative_to(self.rect.xy()).into()),

                event::Ui::Click(idx, ref click) if idx == Some(self.idx) =>
                    return Some(click.clone().relative_to(self.rect.xy()).into()),

                event::Ui::DoubleClick(idx, ref double_click) if idx == Some(self.idx) =>
                    return Some(double_click.clone().relative_to(self.rect.xy()).into()),

                event::Ui::Drag(idx, ref drag) if idx == Some(self.idx) =>
                    return Some(drag.clone().relative_to(self.rect.xy()).into()),

                event::Ui::Scroll(idx, ref scroll) if idx == Some(self.idx) =>
                    return Some(scroll.clone().into()),

                _ => (),
                
            }
        }

        None
    }
}


impl<'a> Iterator for Clicks<'a> {
    type Item = event::Click;
    fn next(&mut self) -> Option<event::Click> {
        while let Some(event) = self.events.next() {
            if let event::Widget::Click(click) = event {
                return Some(click);
            }
        }
        None
    }
}

impl<'a> Iterator for ButtonClicks<'a> {
    type Item = event::Click;
    fn next(&mut self) -> Option<event::Click> {
        while let Some(click) = self.clicks.next() {
            if self.button == click.button {
                return Some(click);
            }
        }
        None
    }
}

impl<'a> Iterator for Drags<'a> {
    type Item = event::Drag;
    fn next(&mut self) -> Option<event::Drag> {
        while let Some(event) = self.events.next() {
            if let event::Widget::Drag(drag) = event {
                return Some(drag);
            }
        }
        None
    }
}

impl<'a> Iterator for ButtonDrags<'a> {
    type Item = event::Drag;
    fn next(&mut self) -> Option<event::Drag> {
        while let Some(drag) = self.drags.next() {
            if self.button == drag.button {
                return Some(drag);
            }
        }
        None
    }
}

impl<'a> Iterator for Texts<'a> {
    type Item = event::Text;
    fn next(&mut self) -> Option<event::Text> {
        while let Some(event) = self.events.next() {
            if let event::Widget::Text(text) = event {
                return Some(text);
            }
        }
        None
    }
}

impl<'a> Iterator for Scrolls<'a> {
    type Item = event::Scroll;
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(event) = self.events.next() {
            if let event::Widget::Scroll(scroll) = event {
                return Some(scroll);
            }
        }
        None
    }
}
