//
// Plane Tracker
// Copyright (c) 2023 Filip Szczerek <ga.software@yahoo.com>
//
// This project is licensed under the terms of the MIT license
// (see the LICENSE file for details).
//

use cgmath::{Deg, Rad};
use crate::{data, data::ProgramData};
use gtk4 as gtk;
use gtk::cairo;
use gtk::glib;
use gtk::glib::clone;
use gtk::prelude::*;
use std::{cell::RefCell, rc::Rc};

pub struct GuiData {
    pub drawing_area: gtk::DrawingArea
}

struct RestoreTransform<'a> {
    ctx: &'a cairo::Context,
    m: cairo::Matrix
}

impl<'a> RestoreTransform<'a> {
    fn new(ctx: &cairo::Context) -> RestoreTransform {
        RestoreTransform{ m: ctx.matrix(), ctx }
    }
}

impl<'a> Drop for RestoreTransform<'a> {
    fn drop(&mut self) {
        self.ctx.set_matrix(self.m)
    }
}

/// `Ctx` uses local frame (Y points up), pixel scale.
fn draw_aircraft_icon(ctx: &cairo::Context, track: Deg<f64>) {
    const SIZE: f64 = 20.0;
    const WEDGE_ANGLE: Deg<f64> = Deg(30.0);

    let _rt = RestoreTransform::new(ctx);

    ctx.rotate(-Rad::from(track).0);

    let p0 = (-Rad::from(WEDGE_ANGLE).0.sin() * 0.5 * SIZE, 0.0);
    let p1 = (0.0, SIZE);
    let p2 = (Rad::from(WEDGE_ANGLE).0.sin() * 0.5 * SIZE, 0.0);

    ctx.move_to(p0.0, p0.1);
    ctx.line_to(p1.0, p1.1);
    ctx.line_to(p2.0, p2.1);
    ctx.line_to(p0.0, p0.1);

    ctx.set_line_width(2.0);
    ctx.stroke().unwrap();
}

/// `Ctx` uses local frame (Y points up), pixel scale.
fn draw_aircraft_info(ctx: &cairo::Context, aircraft: &data::Aircraft) {
    let _rt = RestoreTransform::new(ctx);

    // all values in pixels
    const FONT_SIZE: f64 = 20.0;
    const HORZ_OFFSET: f64 = 30.0;
    const LINE_SPACING: f64 = FONT_SIZE * 1.1;

    ctx.set_font_size(FONT_SIZE);

    ctx.move_to(HORZ_OFFSET, 0.0);
    if let Some(callsign) = &aircraft.callsign {
        ctx.show_text(&callsign).unwrap();
    }

    ctx.move_to(HORZ_OFFSET, 1.0 * LINE_SPACING);
    if let Some(track) = &aircraft.track {
        ctx.show_text(&format!("{:.0}°", track.0)).unwrap();
    }

    ctx.move_to(HORZ_OFFSET, 2.0 * LINE_SPACING);
    if let Some(altitude) = &aircraft.altitude {
        ctx.show_text(&format!("{:.0} m", altitude.0 * 0.3048)).unwrap();
    }

    ctx.move_to(HORZ_OFFSET, 3.0 * LINE_SPACING);
    if let Some(ground_speed) = &aircraft.ground_speed {
        ctx.show_text(&format!("{:.0} km/h", ground_speed.0 * 1.852)).unwrap();
    }

    ctx.move_to(HORZ_OFFSET, 4.0 * LINE_SPACING);
    ctx.show_text(&format!("{:.1} s", aircraft.t_last_update.elapsed().as_secs_f64())).unwrap();
}

fn draw_aircraft(ctx: &cairo::Context, width: i32, height: i32, program_data_rc: &Rc<RefCell<ProgramData>>) {
    let pd = program_data_rc.borrow();

    const RANGE: f64 = 150_000.0;
    let scale = width as f64 / 2.0 / RANGE;

    ctx.translate(width as f64 / 2.0, height as f64 / 2.0);
    ctx.scale(scale, -scale);

    const ACTIVE_COLOR: (f64, f64, f64) = (0.0, 0.6, 0.0);
    const INACTIVE_COLOR: (f64, f64, f64) = (0.6, 0.0, 0.0);
    const INACTIVE_DELAY: std::time::Duration = std::time::Duration::from_secs(10);

    for aircraft in pd.aircraft().values() {
        if aircraft.lat_lon.is_some() && aircraft.track.is_some() {
            let color = if aircraft.t_last_update.elapsed() > INACTIVE_DELAY {
                INACTIVE_COLOR
            } else {
                ACTIVE_COLOR
            };
            ctx.set_source_rgb(color.0, color.1, color.2);

            let projected_pos = data::project(&pd.observer_lat_lon, aircraft.lat_lon.as_ref().unwrap());
            let _rt = RestoreTransform::new(ctx);

            ctx.translate(projected_pos.x, projected_pos.y);
            ctx.scale(1.0 / scale, 1.0 / scale);
            draw_aircraft_icon(ctx, aircraft.track.unwrap());
            ctx.scale(1.0, -1.0);
            draw_aircraft_info(ctx, aircraft);
        }
    }
}

fn on_draw_main_view(ctx: &cairo::Context, width: i32, height: i32, program_data_rc: &Rc<RefCell<ProgramData>>) {
    draw_aircraft(ctx, width, height, program_data_rc);
}

pub fn init_main_window(app: &gtk::Application, program_data_rc: &Rc<RefCell<ProgramData>>) {

    let drawing_area = gtk::DrawingArea::builder().build();

    drawing_area.set_draw_func(clone!(@weak program_data_rc => @default-panic, move |_widget, ctx, width, height| {
        on_draw_main_view(ctx, width, height, &program_data_rc);
    }));

    program_data_rc.borrow_mut().gui = Some(GuiData{ drawing_area: drawing_area.clone() });

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .default_width(640)
        .default_height(480)
        .title("Plane Tracker")
        .child(&drawing_area)
        .build();

    {
        let config = &program_data_rc.borrow().config;

        if let Some(pos) = program_data_rc.borrow().config.main_window_pos() {
            //TODO: implement this
            //window.move_(pos.x, pos.y);
            //window.resize(pos.width, pos.height);
        } else {
            //window.resize(800, 600);
        }

        if let Some(is_maximized) = config.main_window_maximized() {
            if is_maximized { window.maximize(); }
        }
    }

    window.present();
}
