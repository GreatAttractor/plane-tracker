//
// Plane Tracker
// Copyright (c) 2023 Filip Szczerek <ga.software@yahoo.com>
//
// This project is licensed under the terms of the MIT license
// (see the LICENSE file for details).
//

mod config;
mod data_receiver;
mod data;
mod gui;

use data::ProgramData;
use gtk4 as gtk;
use gtk::prelude::*;
use gtk::glib;
use gtk::glib::clone;
use std::{cell::RefCell, rc::Rc};

fn main() -> glib::ExitCode {
    let application = gtk::Application::builder()
        .application_id("ga_software.plane_tracker")
        .build();

    let program_data_rc = Rc::new(RefCell::new(data::ProgramData::new()));

    application.connect_activate(clone!(@weak program_data_rc => @default-panic, move |app| {
        gui::init_main_window(&app, &program_data_rc);
    }));

    let (sender_worker, receiver_main) = glib::MainContext::channel(glib::Priority::DEFAULT);
    receiver_main.attach(None, clone!(@weak program_data_rc => @default-panic, move |_| {
        on_timer(&program_data_rc);
        glib::ControlFlow::Continue
    }));

    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let _ = sender_worker.send(());
        }
    });

    let (sender_worker, receiver_main) = glib::MainContext::channel(glib::Priority::DEFAULT);
    receiver_main.attach(None, clone!(@weak program_data_rc => @default-panic, move |msg| {
        data_receiver::on_data_received(&program_data_rc, msg);
        glib::ControlFlow::Continue
    }));

    std::thread::spawn(move || {
        data_receiver::data_receiver(sender_worker);
    });

    application.run()
}

fn on_timer(program_data_rc: &Rc<RefCell<ProgramData>>) {
    let pd = program_data_rc.borrow();
    pd.gui.as_ref().unwrap().drawing_area.queue_draw();
}