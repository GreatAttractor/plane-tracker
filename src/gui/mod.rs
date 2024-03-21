//
// Plane Tracker
// Copyright (c) 2023-2024 Filip Szczerek <ga.software@yahoo.com>
//
// This project is licensed under the terms of the MIT license
// (see the LICENSE file for details).
//

use cgmath::{Deg, InnerSpace, Point2, Rad};
use crate::{data, data::ProgramData, data_receiver};
use gtk4 as gtk;
use gtk::cairo;
use gtk::glib;
use gtk::glib::clone;
use gtk::prelude::*;
use pointing_utils::{GeoPos, uom};
use std::{cell::RefCell, rc::Rc};
use uom::{si::f64, si::{length, velocity}};

const SPACING: i32 = 10; // control spacing in pixels
const PADDING: i32 = 10; //TODO: depend on DPI (or does it already?)

const ZOOM_FACTOR: f64 = 1.2;
mod colors {
    pub const ACTIVE: (f64, f64, f64) = (0.0, 0.6, 0.0);
    pub const INACTIVE: (f64, f64, f64) = (0.6, 0.0, 0.0);
    pub const SELECTED: (f64, f64, f64) = (1.0, 1.0, 1.0);
}
const INACTIVE_DELAY: std::time::Duration = std::time::Duration::from_secs(10);

pub struct StatusBarFields {
    server_address: gtk::Label,
    pub num_aircraft: gtk::Label,
    pub max_distance: gtk::Label
}

pub struct GuiData {
    pub drawing_area: gtk::DrawingArea,
    pub plot_range: f64::Length, // corresponds to draw area width
    pub status_bar_fields: StatusBarFields
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

fn meters(value: f64) -> f64::Length {
    f64::Length::new::<length::meter>(value)
}

fn kilometers(value: f64) -> f64::Length {
    f64::Length::new::<length::kilometer>(value)
}

fn choose_closest(value: f64, sorted_values: &[f64]) -> f64 {
    match sorted_values.binary_search_by(|x| x.partial_cmp(&value).unwrap()) {
        Ok(idx) | Err(idx) => {
            if idx < sorted_values.len() {
                sorted_values[idx]
            } else {
                *sorted_values.last().unwrap()
            }
        }
    }
}

/// Current transform of `ctx`: Y points up, observer at (0, 0), global scale (meters).
fn draw_range_circles(ctx: &cairo::Context, scale: f64, width: i32, height: i32, program_data_rc: &Rc<RefCell<ProgramData>>) {
    let pd = program_data_rc.borrow();
    let gui = pd.gui.as_ref().unwrap();
    ctx.set_source_rgb(0.3, 0.3, 0.3);

    let text_scale = pd.config.text_scale().unwrap_or(1.0);

    const FONT_SIZE: f64 = 20.0; // pixels
    const LABEL_OFFSET: f64 = 0.2 * FONT_SIZE;
    const CROSS_SIZE: f64 = 40.0; // pixels
    const MAX_NUM_CIRCLES_SHOWN: usize = 15;
    const ALLOWED_R_STEPS_KM: [f64; 5] = [10.0, 20.0, 50.0, 100.0, 200.0];

    let cs = CROSS_SIZE / scale;
    ctx.set_line_width(2.0 / scale);
    ctx.move_to(-cs / 2.0, 0.0);
    ctx.line_to(cs / 2.0, 0.0);
    ctx.stroke().unwrap();
    ctx.move_to(0.0, -cs / 2.0);
    ctx.line_to(0.0, cs / 2.0);
    ctx.stroke().unwrap();

    ctx.set_line_width(1.0 / scale);

    ctx.set_font_size(FONT_SIZE / scale * text_scale);

    let radius_step_to_show_n_circles = |num_pixels, n| { meters(num_pixels as f64 / (scale * n as f64)) };

    let desired_radius_step = radius_step_to_show_n_circles(width, MAX_NUM_CIRCLES_SHOWN)
        .min(radius_step_to_show_n_circles(height, MAX_NUM_CIRCLES_SHOWN));

    let radius_step = kilometers(choose_closest(desired_radius_step.get::<length::kilometer>(), &ALLOWED_R_STEPS_KM));

    let mut radius = radius_step;
    let max_radius = if width > height {
        gui.plot_range
    } else if width > 0 {
        gui.plot_range * height as f64 / width as f64
    } else {
        meters(0.0)
    };

    while radius < max_radius {
        ctx.arc(
            0.0, 0.0,
            data::project_distance_on_earth(radius).get::<length::meter>(),
            0.0, 2.0 * std::f64::consts::PI
        );
        ctx.stroke().unwrap();

        {
            let _rt = RestoreTransform::new(ctx);
            ctx.scale(1.0, -1.0);

            let r = radius.get::<length::meter>();
            let text = format!("{:.0}", r / 1000.0);

            let lofs = LABEL_OFFSET / scale * text_scale;

            ctx.move_to(r + lofs, 0.0);
            ctx.show_text(&text).unwrap();
            ctx.stroke().unwrap(); // unknown why these are needed, but without them there are some invalid lines

            ctx.move_to(-r + lofs, 0.0);
            ctx.show_text(&text).unwrap();
            ctx.stroke().unwrap();

            ctx.move_to(0.0, r + FONT_SIZE / scale + lofs);
            ctx.show_text(&text).unwrap();
            ctx.stroke().unwrap();

            ctx.move_to(0.0, -r - lofs);
            ctx.show_text(&text).unwrap();
            ctx.stroke().unwrap();
        }
        radius += radius_step;
    }
}

/// Current transform of `ctx`: Y points up, aircraft at (0, 0), pixel scale.
fn draw_aircraft_icon(ctx: &cairo::Context, track: Deg<f64>, text_scale: f64) {
    const SIZE: f64 = 20.0; // pixels
    const WEDGE_ANGLE: Deg<f64> = Deg(30.0);

    let _rt = RestoreTransform::new(ctx);

    ctx.rotate(-Rad::from(track).0);

    let s = SIZE * text_scale;

    let p0 = (-Rad::from(WEDGE_ANGLE).0.sin() * 0.5 * s, -s / 2.0);
    let p1 = (0.0, s / 2.0);
    let p2 = (Rad::from(WEDGE_ANGLE).0.sin() * 0.5 * s, -s / 2.0);

    ctx.move_to(p0.0, p0.1);
    ctx.line_to(p1.0, p1.1);
    ctx.line_to(p2.0, p2.1);
    ctx.line_to(p0.0, p0.1);

    ctx.set_line_width(2.0 * text_scale);
    ctx.stroke().unwrap();
}

/// Current transform of `ctx`: Y points down, aircraft at (0, 0), pixel scale.
fn draw_aircraft_info(
    ctx: &cairo::Context,
    aircraft: &data::Aircraft,
    observer: &GeoPos,
    interpolate: bool,
    text_scale: f64
) {
    let _rt = RestoreTransform::new(ctx);

    // all values in pixels
    const FONT_SIZE: f64 = 20.0;
    const HORZ_OFFSET: f64 = 30.0;
    const LINE_SPACING: f64 = FONT_SIZE * 1.1;

    ctx.set_font_size(FONT_SIZE * text_scale);
    let h_offs = HORZ_OFFSET * text_scale;
    let l_spc = LINE_SPACING * text_scale;

    ctx.move_to(h_offs, 0.0);
    if let Some(callsign) = &aircraft.callsign {
        ctx.show_text(&callsign).unwrap();
    }

    ctx.move_to(h_offs, 1.0 * l_spc);
    if let Some(track) = &aircraft.track {
        ctx.show_text(&format!("{:.0}°", track.0)).unwrap();
    }

    ctx.move_to(h_offs, 2.0 * l_spc);
    if let Some(altitude) = &aircraft.altitude {
        ctx.show_text(&format!("{:.0} m", altitude.get::<length::meter>())).unwrap();
    }

    ctx.move_to(h_offs, 3.0 * l_spc);
    if let Some(ground_speed) = &aircraft.ground_speed {
        ctx.show_text(&format!("{:.0} km/h", ground_speed.get::<velocity::kilometer_per_hour>())).unwrap();
    }

    ctx.move_to(h_offs, 4.0 * l_spc);
    if aircraft.altitude.is_some() && aircraft.lat_lon.is_some() {
        let distance = data::get_distance(observer, aircraft, interpolate);
        ctx.show_text(&format!("{:.1} km", distance.get::<length::kilometer>())).unwrap();
    }

    ctx.move_to(h_offs, 5.0 * l_spc);
    ctx.show_text(&format!("{:.1} s", aircraft.t_last_update.elapsed().as_secs_f64())).unwrap();
}

fn draw_single_aircraft(ctx: &cairo::Context, aircraft: &data::Aircraft, scale: f64, text_scale: f64, pd: &ProgramData) {
    let lat_lon = if let Some((lat_lon, _)) = &aircraft.lat_lon { lat_lon } else { return; };
    let est_lat_lon = aircraft.estimated_lat_lon();

    let track = if let Some(track) = aircraft.track { track } else { return; };

    let projected_pos = data::project(&pd.observer_location.lat_lon, lat_lon);

    let projected_displayed_pos = data::project(
        &pd.observer_location.lat_lon,
        if est_lat_lon.is_some() { est_lat_lon.unwrap() } else { lat_lon }
    );

    let interpolate = pd.config.interpolate_positions().unwrap_or(true);

    if interpolate {
        let _rt = RestoreTransform::new(ctx);
        ctx.set_line_width(1.0 / scale);
        ctx.set_source_rgb(0.5, 0.5, 0.5);
        ctx.move_to(projected_pos.x, projected_pos.y);
        ctx.line_to(projected_displayed_pos.x, projected_displayed_pos.y);
        ctx.stroke().unwrap();
    }

    let _rt = RestoreTransform::new(ctx);
    ctx.translate(projected_displayed_pos.x, projected_displayed_pos.y);
    ctx.scale(1.0 / scale, 1.0 / scale);
    let color = if aircraft.state == data::State::Selected {
        colors::SELECTED
    } else if aircraft.t_last_update.elapsed() > INACTIVE_DELAY {
        colors::INACTIVE
    } else {
        colors::ACTIVE
    };
    ctx.set_source_rgb(color.0, color.1, color.2);
    draw_aircraft_icon(ctx, track, text_scale);
    ctx.scale(1.0, -1.0);
    draw_aircraft_info(ctx, aircraft, &pd.observer_location, interpolate, text_scale);
}

/// Current transform of `ctx`: Y points up, observer at (0, 0), global scale (meters).
fn draw_all_aircraft(ctx: &cairo::Context, width: i32, height: i32, program_data_rc: &Rc<RefCell<ProgramData>>) {
    let pd = program_data_rc.borrow();
    let text_scale = pd.config.text_scale().unwrap_or(1.0);

    let scale = width as f64 / 2.0 / pd.gui.as_ref().unwrap().plot_range.get::<length::meter>();

    let mut selected: Option<data::ModeSTransponderCode> = None;

    for aircraft in pd.aircraft.values() {
        if aircraft.state == data::State::Selected {
            selected = Some(aircraft.id);
            continue; // the selected aircraft will be drawn as last
        }

        draw_single_aircraft(ctx, aircraft, scale, text_scale, &pd);
    }

    if let Some(id) = selected {
        draw_single_aircraft(ctx, pd.aircraft.get(&id).unwrap(), scale, text_scale, &pd);
    }
}

fn on_draw_main_view(ctx: &cairo::Context, width: i32, height: i32, program_data_rc: &Rc<RefCell<ProgramData>>) {
    let scale = width as f64 / 2.0 / program_data_rc.borrow().gui.as_ref().unwrap().plot_range.get::<length::meter>();
    ctx.translate(width as f64 / 2.0, height as f64 / 2.0);
    ctx.scale(scale, -scale);

    draw_range_circles(ctx, scale, width, height, program_data_rc);
    draw_all_aircraft(ctx, width, height, program_data_rc);
}

fn on_connect(server_address: String, program_data_rc: &Rc<RefCell<ProgramData>>) {
    data_receiver::stop(program_data_rc);

    data_receiver::start(
        server_address.clone(),
        if program_data_rc.borrow().recording {
            Some(std::fs::File::create(get_recording_file_name()).unwrap())
        } else {
            None
        },
        program_data_rc
    );

    let pd = program_data_rc.borrow_mut();
    let gui = pd.gui.as_ref().unwrap();
    gui.status_bar_fields.server_address.set_text(&format!("Connected to {}", server_address));
    gui.status_bar_fields.num_aircraft.set_text("Aircraft: 0 (max: 0)");
}

fn on_connect_btn(main_wnd: &gtk::ApplicationWindow, program_data_rc: &Rc<RefCell<ProgramData>>) {
    let dialog = gtk::MessageDialog::new(
        Some(main_wnd),
        gtk::DialogFlags::MODAL,
        gtk::MessageType::Question,
        gtk::ButtonsType::OkCancel,
        "Server address:"
    );
    dialog.set_title(Some("Connect to SBS server"));
    let server_address = gtk::Text::new();
    server_address.set_text(&program_data_rc.borrow().config.server_address().unwrap_or("".into()));
    set_all_margins(&dialog.content_area(), PADDING);
    dialog.content_area().append(&server_address);
    dialog.connect_response(clone!(@weak server_address, @weak program_data_rc => @default-panic, move |dlg, response| {
        if response == gtk::ResponseType::Ok {
            program_data_rc.borrow().config.set_server_address(server_address.text().as_str());
            on_connect(server_address.text().into(), &program_data_rc);
        }
        dlg.close();
    }));
    dialog.show();
}



fn on_disconnect(program_data_rc: &Rc<RefCell<ProgramData>>) {
    data_receiver::stop(program_data_rc);

    let mut pd = program_data_rc.borrow_mut();
    pd.aircraft.clear();
    pd.max_num_aircraft = 0;

    let gui = pd.gui.as_ref().unwrap();
    gui.status_bar_fields.server_address.set_text("");
    gui.status_bar_fields.num_aircraft.set_text("Aircraft: 0 (max: 0)");
}

fn create_toolbar(
    main_wnd: &gtk::ApplicationWindow,
    program_data_rc: &Rc<RefCell<ProgramData>>
) -> gtk::Box {

    let toolbar = gtk::Box::new(gtk::Orientation::Vertical, SPACING);
    toolbar.add_css_class("toolbar");

    let connect = gtk::Button::builder().label("connect").build();
    connect.connect_clicked(clone!(@weak main_wnd, @weak program_data_rc => @default-panic, move |_| {
        on_connect_btn(&main_wnd, &program_data_rc);
    }));
    toolbar.append(&connect);

    let disconnect = gtk::Button::builder().label("disconnect").build();
    disconnect.connect_clicked(clone!(@weak program_data_rc => @default-panic, move |_| {
        on_disconnect(&program_data_rc);
    }));
    toolbar.append(&disconnect);

    let filter = gtk::CheckButton::builder()
        .label("filter")
        .tooltip_text("Filter out-of-order location messages")
        .active(program_data_rc.borrow().config.filter_ooo_messages().unwrap_or(true))
        .build();
    filter.connect_toggled(clone!(@weak program_data_rc => @default-panic, move |checkbox| {
        program_data_rc.borrow().config.set_filter_ooo_messages(checkbox.is_active());
    }));
    toolbar.append(&filter);

    let interpolate = gtk::CheckButton::builder()
        .label("intr")
        .tooltip_text("Interpolate positions between updates")
        .active(program_data_rc.borrow().config.interpolate_positions().unwrap_or(true))
        .build();
    interpolate.connect_toggled(clone!(@weak program_data_rc => @default-panic, move |checkbox| {
        program_data_rc.borrow().config.set_interpolate_positions(checkbox.is_active());
    }));
    toolbar.append(&interpolate);

    let zoom_in = gtk::Button::builder().label("zoom+").build();
    zoom_in.connect_clicked(clone!(@weak program_data_rc => @default-panic, move |_| {
        on_zoom(-1, &program_data_rc);
    }));
    toolbar.append(&zoom_in);

    let zoom_out = gtk::Button::builder().label("zoom−").build();
    zoom_out.connect_clicked(clone!(@weak program_data_rc => @default-panic, move |_| {
        on_zoom(1, &program_data_rc);
    }));
    toolbar.append(&zoom_out);

    let text_enlarge = gtk::Button::builder().label("text+").build();
    text_enlarge.connect_clicked(clone!(@weak program_data_rc => @default-panic, move |_| {
        let pd = program_data_rc.borrow();
        pd.config.set_text_scale(pd.config.text_scale().unwrap_or(1.0) * ZOOM_FACTOR);
        pd.gui.as_ref().unwrap().drawing_area.queue_draw();
    }));
    toolbar.append(&text_enlarge);

    let text_shrink = gtk::Button::builder().label("text-").build();
    text_shrink.connect_clicked(clone!(@weak program_data_rc => @default-panic, move |_| {
        let pd = program_data_rc.borrow();
        pd.config.set_text_scale(pd.config.text_scale().unwrap_or(1.0) / ZOOM_FACTOR);
        pd.gui.as_ref().unwrap().drawing_area.queue_draw();
    }));
    toolbar.append(&text_shrink);

    let toggle_recording = gtk::ToggleButton::builder().label("rec").build();
    toggle_recording.connect_clicked(clone!(@weak program_data_rc => @default-panic, move |btn| {
        on_toggle_recording(btn.is_active(), &program_data_rc);
    }));
    toolbar.append(&toggle_recording);

    toolbar
}

fn get_recording_file_name() -> String {
    format!("rec-{}.csv", chrono::Local::now().format("%Y-%m-%d_%H%M%S"))
}

fn on_toggle_recording(enabled: bool, program_data_rc: &Rc<RefCell<ProgramData>>) {
    program_data_rc.borrow_mut().recording = enabled;

    if let Some(prev_address) = data_receiver::stop(program_data_rc) {
        let rec_output = if enabled {
            Some(std::fs::File::create(get_recording_file_name()).unwrap())
        } else {
            None
        };

        data_receiver::start(prev_address, rec_output, program_data_rc);
    }
}

fn set_all_margins(widget: &impl gtk::traits::WidgetExt, margin: i32) {
    widget.set_margin_start(margin);
    widget.set_margin_end(margin);
    widget.set_margin_bottom(margin);
    widget.set_margin_top(margin);
}

fn set_start_end_margins(widget: &impl gtk::traits::WidgetExt, margin: i32) {
    widget.set_margin_start(margin);
    widget.set_margin_end(margin);
}

fn create_status_bar(program_data_rc: &Rc<RefCell<ProgramData>>) -> (gtk::Frame, StatusBarFields) {
    let status_bar_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    set_all_margins(&status_bar_box, PADDING);

    let server_address = gtk::Label::new(None);
    set_start_end_margins(&server_address, PADDING);

    let num_aircraft = gtk::Label::new(None);
    set_start_end_margins(&num_aircraft, PADDING);

    let max_distance = gtk::Label::new(None);
    set_start_end_margins(&max_distance, PADDING);

    status_bar_box.append(&server_address);
    status_bar_box.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    status_bar_box.append(&num_aircraft);
    status_bar_box.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    status_bar_box.append(&max_distance);

    let status_bar_frame = gtk::Frame::builder().child(&status_bar_box).build();
    //status_bar_frame.set_shadow_type(gtk::ShadowType::In);
    //TODO: set shadowed inset border

    (status_bar_frame, StatusBarFields{ server_address, num_aircraft, max_distance })
}

fn on_zoom(steps: i32, program_data_rc: &Rc<RefCell<ProgramData>>) {
    let mut pd = program_data_rc.borrow_mut();
    let gui = pd.gui.as_mut().unwrap();

    let new_range = gui.plot_range * ZOOM_FACTOR.powi(steps);
    if new_range >= kilometers(20.0) && new_range <= kilometers(2000.0) {
        gui.plot_range = new_range;
        gui.drawing_area.queue_draw();
    }
}

pub fn init_main_window(app: &gtk::Application, program_data_rc: &Rc<RefCell<ProgramData>>) {
    let contents = gtk::Box::new(gtk::Orientation::Vertical, SPACING);

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .default_width(640)
        .default_height(480)
        .title("Plane Tracker")
        .child(&contents)
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

    let sub_contents = gtk::Box::new(gtk::Orientation::Horizontal, SPACING);
    sub_contents.set_hexpand(true);
    sub_contents.set_vexpand(true);

    let toolbar = create_toolbar(&window, program_data_rc);
    sub_contents.append(&toolbar);

    let drawing_area = gtk::DrawingArea::builder().build();
    drawing_area.set_hexpand(true);
    drawing_area.set_draw_func(clone!(@weak program_data_rc => @default-panic, move |_widget, ctx, width, height| {
        on_draw_main_view(ctx, width, height, &program_data_rc);
    }));

    let evt_ctrl_scroll = gtk::EventControllerScroll::builder().flags(gtk::EventControllerScrollFlags::BOTH_AXES).build();
    evt_ctrl_scroll.connect_scroll(clone!(@weak program_data_rc => @default-panic, move |_, _, y| {
        on_zoom(y as i32, &program_data_rc);
        glib::signal::Propagation::Stop
    }));
    drawing_area.add_controller(evt_ctrl_scroll);

    let g_click = gtk::GestureClick::builder().build();
    g_click.connect_pressed(clone!(@weak program_data_rc => @default-panic, move |_, button, x, y| {
        on_main_view_button_pressed(button, x, y, &program_data_rc);
    }));
    drawing_area.add_controller(g_click);

    sub_contents.append(&drawing_area);

    contents.append(&sub_contents);

    let (status_bar, status_bar_fields) = create_status_bar(program_data_rc);
    contents.append(&status_bar);

    program_data_rc.borrow_mut().gui = Some(GuiData{
        drawing_area: drawing_area.clone(),
        plot_range: f64::Length::new::<length::kilometer>(200.0),
        status_bar_fields
    });

    window.present();
}

fn on_main_view_button_pressed(button: i32, x: f64, y: f64, program_data_rc: &Rc<RefCell<ProgramData>>) {
    if button != 1 { return; } //TODO: use symbolic constant

    let mut pd = program_data_rc.borrow_mut();
    let observer_ll = pd.observer_location.lat_lon.clone();
    let global;
    let scale;
    let range;
    {
        let gui = pd.gui.as_ref().unwrap();
        let dw = gui.drawing_area.width();
        let dh = gui.drawing_area.height();
        range = gui.plot_range.get::<length::meter>();
        scale = 2.0 * range / dw as f64;
        global = Point2{ x: scale * (x - dw as f64 / 2.0), y: scale * (dh as f64 / 2.0 - y) };
    };

    // At the moment we track no more than ~100 aircraft at a time, so just check all of them. If it changes,
    // switch to e.g. a kd-tree.

    let mut min_dist2 = std::f64::MAX;
    let mut closest = None;
    let mut prev_selected = None;
    for aircraft in pd.aircraft.values_mut() {
        if aircraft.state == data::State::Selected { prev_selected = Some(aircraft.id); }

        let lat_lon = if let Some(lat_lon) = &aircraft.estimated_lat_lon {
            lat_lon.0.clone()
        } else if let Some(lat_lon) = &aircraft.lat_lon {
            lat_lon.0.clone()
        } else {
            continue;
        };

        let proj = data::project(&observer_ll, &lat_lon);

        let dist2 = (proj - global).magnitude2();
        if dist2 < min_dist2 {
            min_dist2 = dist2;
            closest = Some(aircraft.id);
        }
    }

    if let Some(closest_id) = closest {
        if min_dist2.sqrt() <= range / 10.0 {
            if let Some(id) = prev_selected { pd.aircraft.get_mut(&id).unwrap().state = data::State::Normal; }
            pd.aircraft.get_mut(&closest_id).unwrap().state = data::State::Selected;
            pd.gui.as_ref().unwrap().drawing_area.queue_draw();
        }
    }
}
