//
// Plane Tracker
// Copyright (c) 2023 Filip Szczerek <ga.software@yahoo.com>
//
// This project is licensed under the terms of the MIT license
// (see the LICENSE file for details).
//

mod config;
mod data_receiver;
mod data_sender;
mod data;
mod gui;

use data::{ProgramData, State};
use gtk4 as gtk;
use gtk::prelude::*;
use gtk::glib;
use gtk::glib::clone;
use std::{cell::RefCell, rc::Rc};

const DATA_SENDER_PORT: u16 = 45500;

fn main() -> glib::ExitCode {
    let application = gtk::Application::builder()
        .application_id("ga_software.plane_tracker")
        .build();

    let program_data_rc = Rc::new(RefCell::new(data::ProgramData::new()));

    application.connect_activate(clone!(@weak program_data_rc => @default-panic, move |app| {
        gui::init_main_window(&app, &program_data_rc);
    }));

    set_up_timer(&program_data_rc);
    set_up_data_sender(&program_data_rc);

    let exit_code = application.run();

    if program_data_rc.borrow().config.store().is_err() {
        println!("WARNING: Failed to save configuration.");
    }

    exit_code
}

fn set_up_data_sender(program_data_rc: &Rc<RefCell<ProgramData>>) {
    let (sender_worker, receiver_main) = glib::MainContext::channel(glib::Priority::DEFAULT);
    receiver_main.attach(None, clone!(@weak program_data_rc => @default-panic, move |stream| {
        program_data_rc.borrow_mut().data_senders.push(stream);
        glib::ControlFlow::Continue
    }));

    data_sender::start_listener(DATA_SENDER_PORT, sender_worker);
}

fn set_up_timer(program_data_rc: &Rc<RefCell<ProgramData>>) {
    let (sender_worker, receiver_main) = glib::MainContext::channel(glib::Priority::DEFAULT);
    receiver_main.attach(None, clone!(@weak program_data_rc => @default-panic, move |_| {
        on_timer(&program_data_rc);
        glib::ControlFlow::Continue
    }));

    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(250));
            let _ = sender_worker.send(());
        }
    });
}

fn on_timer(program_data_rc: &Rc<RefCell<ProgramData>>) {
    let pd = &mut *program_data_rc.borrow_mut();
    let interpolate = pd.config.interpolate_positions().unwrap_or(true);
    if interpolate  {
        let now = std::time::Instant::now();
        for aircraft in pd.aircraft.values_mut() {
            aircraft.update_interpolated_position(now);
            if aircraft.state == State::Selected {
                data_sender::send_data(&aircraft, &pd.observer_location, &mut pd.data_senders);
            }
        }
    }

    pd.garbage_collect();
    pd.gui.as_ref().unwrap().drawing_area.queue_draw();
}
