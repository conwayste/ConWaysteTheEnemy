/*  Copyright 2019 the Conwayste Developers.
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
use chromatica::css;

use std::collections::VecDeque;

use ggez::graphics::{self, Rect, Font, Point2, Color, DrawMode, Text, Vector2};
use ggez::{Context, GameResult};

use super::{
    widget::Widget,
    helpe::within_widget,
    UIAction, WidgetID
};

const CHAT_DISPLAY_LIMIT: f32 = 10.0;

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum TextState {
    NoInput,
    Chatting,
}

pub struct Chatbox {
    pub id: WidgetID,
    pub history_len: usize,
    pub color: Color,
    pub messages: VecDeque<Text>,
    pub dimensions: Rect,
    pub hover: bool,
    pub action: UIAction,
}

impl Chatbox {
    pub fn new(widget_id: WidgetID, len: usize) -> Self {
        let rect = Rect::new(30.0, 600.0, 300.0, 15.0*CHAT_DISPLAY_LIMIT);
        Chatbox {
            id: widget_id,
            history_len: len,
            color: Color::from(css::VIOLET),
            messages: VecDeque::with_capacity(len),
            dimensions: rect,
            hover: false,
            action: UIAction::EnterText(TextState::NoInput),
        }
    }

    pub fn add_message(&mut self, ctx: &mut Context, font: &Font, msg: &String) -> GameResult<()> {
        if self.messages.len() + 1 > self.history_len {
            self.messages.pop_front();
        }
        let text = Text::new(ctx, msg, font)?;
        self.messages.push_back(text);
        Ok(())
    }
}


impl Widget for Chatbox {
    fn id(&self) -> WidgetID {
        self.id
    }

    fn size(&self) -> Rect {
        self.dimensions
    }

    fn set_size(&mut self, new_dims: Rect) {
        self.dimensions = new_dims;
    }

    fn translate(&mut self, point: Vector2)
    {
        self.dimensions.translate(point);
    }

    fn on_hover(&mut self, point: &Point2) {
        self.hover = within_widget(point, &self.dimensions);
        //if self.hover {
        //    println!("Hovering over Chatbox, \"{:?}\"", self.label.dimensions);
        //}
    }

    fn on_click(&mut self, point: &Point2) -> Option<UIAction>
    {
        if within_widget(point, &self.dimensions) {
            println!("Clicked within Chatbox");
            self.action = UIAction::EnterText(TextState::Chatting);
            return Some(self.action);
        }
        else
        {
            // Maybe be dead code since on_click is filtered when within_widget?
            self.action = UIAction::EnterText(TextState::NoInput);
        }

        None
    }

    fn draw(&self, ctx: &mut Context, _font: &Font) -> GameResult<()> {
        let old_color = graphics::get_color(ctx);

        if self.hover {
            // Add in a teal border while hovered. Color checkbox differently to indicate  hovered state.
            let border_rect = Rect::new(self.dimensions.x-1.0, self.dimensions.y-1.0, self.dimensions.w + 4.0, self.dimensions.h + 4.0);
            graphics::set_color(ctx, Color::from(css::TEAL))?;
            graphics::rectangle(ctx, DrawMode::Line(2.0), border_rect)?;
        }
        graphics::set_color(ctx, self.color)?;
        graphics::rectangle(ctx, DrawMode::Line(4.0), self.dimensions)?;

        // TODO need to do width wrapping check
        for (i, msg) in self.messages.iter().enumerate() {
            let origin = self.dimensions.point();
            let point = Point2::new(origin.x + 5.0, origin.y + i as f32*30.0);
            graphics::draw(ctx, msg, point, 0.0)?;

            // TODO Switch to the queue draw method once ggez 0.5 lands as it is much more efficient
            //graphics::queue_text(ctx, msg, &self.dimensions.point(), Some(Point2::new(0.0, i*30.0)))?;
        }
        // graphics::draw_queued_text(ctx, DrawParam::default())?;

        graphics::set_color(ctx, old_color)
    }
}
