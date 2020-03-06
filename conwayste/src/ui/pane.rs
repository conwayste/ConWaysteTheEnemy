/*  Copyright 2019-2020 the Conwayste Developers.
 *
 *  This file is part of conwayste.
 *
 *  conwayste is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  conwayste is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU General Public License for more details.
 *
 *  You should have received a copy of the GNU General Public License
 *  along with conwayste.  If not, see
 *  <http://www.gnu.org/licenses/>. */

use std::fmt;

use ggez::graphics::{self, Color, Rect, DrawMode, DrawParam};
use ggez::nalgebra::{Point2, Vector2};
use ggez::{Context, GameResult};

use id_tree::NodeId;

use super::{
    widget::Widget,
    common::{within_widget},
    UIError, UIResult,
};

use crate::constants::colors::*;

pub struct Pane {
    id: Option<NodeId>,
    z_index: usize,
    pub dimensions: Rect,
    pub hover: bool,
    pub floating: bool, // can the window be dragged around?
    pub previous_pos: Option<Point2<f32>>,
    pub border: f32,
    pub bg_color: Option<Color>,

    // might need something to track mouse state to see if we are still clicked within the
    // boundaries of the pane in the dragging case
}

impl fmt::Debug for Pane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Pane {{ id: {:?}, z-index: {}, Dimensions: {:?} }}", self.id, self.z_index, self.dimensions)
    }
}

/// A container of one or more widgets
impl Pane {
    /// Specify the dimensional bounds of the Pane container
    pub fn new(dimensions: Rect) -> Self {
        Pane {
            id: None,
            z_index: std::usize::MAX,
            dimensions: dimensions,
            hover: false,
            floating: true,
            previous_pos: None,
            border: 1.0,
            bg_color: None,
        }
    }

    /*
    // TODO: Currently used to reset previous position on mouse release after dragging completes.
    //      Re-evaluate design if this is the best way to do it. See issue #71 (dragging).
    pub fn update(&mut self, is_mouse_released: bool) {
        if is_mouse_released {
            self.previous_pos = None;
        }
    }
    */
}

impl Widget for Pane {
    fn id(&self) -> Option<&NodeId> {
        self.id.as_ref()
    }

    fn set_id(&mut self, new_id: NodeId) {
        self.id = Some(new_id);
    }

    fn z_index(&self) -> usize {
        self.z_index
    }

    fn set_z_index(&mut self, new_z_index: usize) {
        self.z_index = new_z_index;
    }

    fn rect(&self) -> Rect {
        self.dimensions
    }

    fn set_rect(&mut self, new_dims: Rect) -> UIResult<()> {
        if new_dims.w == 0.0 || new_dims.h == 0.0 {
            return Err(Box::new(UIError::InvalidDimensions{
                reason: format!("Cannot set the size of a Pane {:?} to a width or height of zero", self.id())
            }));
        }

        self.dimensions = new_dims;
        Ok(())
    }

    fn position(&self) -> Point2<f32> {
        self.dimensions.point().into()
    }

    fn set_position(&mut self, x: f32, y: f32) {
        self.dimensions.x = x;
        self.dimensions.y = y;
    }

    fn size(&self) -> (f32, f32) {
        (self.dimensions.w, self.dimensions.h)
    }

    fn set_size(&mut self, w: f32, h: f32) -> UIResult<()> {
        if w == 0.0 || h == 0.0 {
            return Err(Box::new(UIError::InvalidDimensions {
                reason: format!("Cannot set the width or height of Pane {:?} to zero", self.id())
            }));
        }

        self.dimensions.w = w;
        self.dimensions.h = h;

        Ok(())
    }

    fn translate(&mut self, dest: Vector2<f32>) {
        self.dimensions.translate(dest);
    }

    fn on_hover(&mut self, point: &Point2<f32>) {
        self.hover = within_widget(point, &self.dimensions);
    }

    /* TODO: fix all the drag issues
    /// original_pos is the mouse position at which the button was held before any dragging occurred
    /// current_pos is the latest mouse position after any movement
    fn on_drag(&mut self, original_pos: &Point2<f32>, current_pos: &Point2<f32>) {

        if !self.floating || !self.hover {
            return;
        }

        let mut drag_ok = true;

        // Check that the mouse down event is bounded by the pane but not by a sub-widget
        if within_widget(original_pos, &self.dimensions) {
            for widget in self.widgets.iter() {
                if within_widget(original_pos, &widget.rect()) && self.previous_pos.is_none() {
                    drag_ok = false;
                    break;
                }
            }
        } else {
            // The original mouse down event may be no longer bounded if the pane moved enough,
            // so check if we were dragging at a previous spot
            drag_ok = self.previous_pos.is_some();
        }

        if drag_ok {
            // Note where the pane was previously to calculate the delta in position
            if let None = self.previous_pos {
                self.previous_pos = Some(*current_pos);
            }

            if let Some(origin) = self.previous_pos {
                let tl_corner_offset = current_pos - origin;

                if tl_corner_offset[0] != 0.0 && tl_corner_offset[1] != 0.0 {
                    //println!("Dragging! {}, {}, {}", origin, current_pos, tl_corner_offset);
                }

                self.translate(tl_corner_offset);
                for ref mut widget in self.widgets.iter_mut() {
                    widget.translate(tl_corner_offset);
                }
            }

            self.previous_pos = Some(*current_pos);
        }
    }
    */

    fn draw(&mut self, ctx: &mut Context) -> GameResult<()> {
        if let Some(bg_color) = self.bg_color {
            let mesh = graphics::Mesh::new_rectangle(ctx, DrawMode::fill(), self.dimensions, bg_color)?;
            graphics::draw(ctx, &mesh, DrawParam::default())?;
        }

        if self.border > 0.0 {
            let mesh = graphics::Mesh::new_rectangle(ctx, DrawMode::stroke(1.0), self.dimensions, *PANE_BORDER_COLOR)?;
            graphics::draw(ctx, &mesh, DrawParam::default())?;
        }

        Ok(())
    }
}

widget_from_id!(Pane);
