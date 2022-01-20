// Copyright 2019 the Druid Authors
// SPDX-License-Identifier: Apache-2.0

//! macOS implementation of features at the application scope.

#![allow(non_upper_case_globals)]

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

use cocoa::appkit::{NSApp, NSApplication, NSApplicationActivationPolicyAccessory, NSApplicationActivationPolicyRegular};
use cocoa::base::{id, nil, NO, YES};
use cocoa::foundation::{NSArray, NSAutoreleasePool};
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use objc::{class, msg_send, sel, sel_impl};
use once_cell::sync::Lazy;

use crate::application::AppHandler;

use super::clipboard::Clipboard;
use super::error::Error;
use super::util;

static APP_HANDLER_IVAR: &str = "druidAppHandler";

#[derive(Clone)]
pub(crate) struct Application {
    ns_app: id,
    state: Rc<RefCell<State>>,
}

struct State {
    quitting: bool,
}

impl Application {
    pub fn new() -> Result<Application, Error> {
        // macOS demands that we run not just on one thread,
        // but specifically the first thread of the app.
        util::assert_main_thread();
        unsafe {
            let _pool = NSAutoreleasePool::new(nil);
            let ns_app = NSApp();
            let state = Rc::new(RefCell::new(State { quitting: false }));

            Ok(Application { ns_app, state })
        }
    }

    pub fn run(self, handler: Option<Box<dyn AppHandler>>) {
        unsafe {
            // Initialize the application delegate
            let delegate: id = msg_send![APP_DELEGATE.0, alloc];
            let () = msg_send![delegate, init];
            let state = DelegateState { handler };
            let state_ptr = Box::into_raw(Box::new(state));
            (*delegate).set_ivar(APP_HANDLER_IVAR, state_ptr as *mut c_void);
            let () = msg_send![self.ns_app, setDelegate: delegate];

            // Run the main app loop
            self.ns_app.run();

            // Clean up the delegate
            let () = msg_send![self.ns_app, setDelegate: nil];
            drop(Box::from_raw(state_ptr));
        }
    }

    pub fn quit(&self) {
        if let Ok(mut state) = self.state.try_borrow_mut() {
            if !state.quitting {
                state.quitting = true;
                unsafe {
                    // We want to queue up the destruction of all our windows.
                    // Failure to do so will lead to resource leaks.
                    let windows: id = msg_send![self.ns_app, windows];
                    for i in 0..windows.count() {
                        let window: id = windows.objectAtIndex(i);
                        let () = msg_send![window, performSelectorOnMainThread: sel!(close) withObject: nil waitUntilDone: NO];
                    }
                    // Stop sets a stop request flag in the OS.
                    // The run loop is stopped after dealing with events.
                    let () = msg_send![self.ns_app, stop: nil];
                }
            }
        } else {
            tracing::warn!("Application state already borrowed");
        }
    }

    pub fn clipboard(&self) -> Clipboard {
        Clipboard
    }

    pub fn get_locale() -> String {
        unsafe {
            let nslocale_class = class!(NSLocale);
            let locale: id = msg_send![nslocale_class, currentLocale];
            let ident: id = msg_send![locale, localeIdentifier];
            let mut locale = util::from_nsstring(ident);
            // This is done because the locale parsing library we use expects an unicode locale, but these vars have an ISO locale
            if let Some(idx) = locale.chars().position(|c| c == '@') {
                locale.truncate(idx);
            }
            locale
        }
    }
}

impl crate::platform::mac::ApplicationExt for crate::Application {
    fn hide(&self) {
        unsafe {
            let () = msg_send![self.backend_app.ns_app, hide: nil];
        }
    }

    fn hide_others(&self) {
        unsafe {
            let workspace = class!(NSWorkspace);
            let shared: id = msg_send![workspace, sharedWorkspace];
            let () = msg_send![shared, hideOtherApplications];
        }
    }

    fn set_menu(&self, menu: crate::Menu) {
        unsafe {
            NSApp().setMainMenu_(menu.0.menu);
        }
    }
}

struct DelegateState {
    handler: Option<Box<dyn AppHandler>>,
}

impl DelegateState {
    fn command(&mut self, command: u32) {
        if let Some(inner) = self.handler.as_mut() {
            inner.command(command)
        }
    }

    fn url_opened(&mut self, url: String) {
        if let Some(inner) = self.handler.as_mut() {
            inner.url_opened(url)
        }
    }
}

struct AppDelegate(*const Class);
unsafe impl Sync for AppDelegate {}
unsafe impl Send for AppDelegate {}

static APP_DELEGATE: Lazy<AppDelegate> = Lazy::new(|| unsafe {
    let mut decl = ClassDecl::new("DruidAppDelegate", class!(NSObject))
        .expect("App Delegate definition failed");
    decl.add_ivar::<*mut c_void>(APP_HANDLER_IVAR);

    // add method to the object which will be called on URL events
    // it is registered in application_will_finish_launching
    decl.add_method(
        sel!(handleURLEvent:withReplyEvent:),
        handle_url_event as extern "C" fn(&mut Object, Sel, id, id),
    );

    decl.add_method(
        sel!(application:openFile:),
        open_file as extern "C" fn(&mut Object, Sel, id, id) -> bool,
    );

    decl.add_method(
        sel!(applicationWillFinishLaunching:),
        application_will_finish_launching as extern "C" fn(&mut Object, Sel, id),
    );

    decl.add_method(
        sel!(applicationDidFinishLaunching:),
        application_did_finish_launching as extern "C" fn(&mut Object, Sel, id),
    );

    decl.add_method(
        sel!(handleMenuItem:),
        handle_menu_item as extern "C" fn(&mut Object, Sel, id),
    );
    AppDelegate(decl.register())
});

/// Parse an Apple URL event into a URL string
///
/// Takes an NSAppleEventDescriptor from an Apple URL event, unwraps
/// it, and returns the contained URL as a String.
pub fn parse_url_event(event: id) -> String {
    if event as u64 == 0u64 {
        return "".into();
    }
    unsafe {
        let class: u32 = msg_send![event, eventClass];
        let id: u32 = msg_send![event, eventID];
        if class != kInternetEventClass || id != kAEGetURL {
            return "".into();
        }
        let subevent: id = msg_send![event, paramDescriptorForKeyword: keyDirectObject];
        let nsstring: id = msg_send![subevent, stringValue];
        util::from_nsstring(nsstring)
    }
}

/// Apple kInternetEventClass constant
#[allow(non_upper_case_globals)]
pub const kInternetEventClass: u32 = 0x4755524c;

/// Apple kAEGetURL constant
#[allow(non_upper_case_globals)]
pub const kAEGetURL: u32 = 0x4755524c;

/// Apple keyDirectObject constant
#[allow(non_upper_case_globals)]
pub const keyDirectObject: u32 = 0x2d2d2d2d;

pub unsafe fn NSAppleEventManager() -> id {
    msg_send![class!(NSAppleEventManager), sharedAppleEventManager]
}

pub trait NSAppleEventManager: Sized {
    unsafe fn set_event_handler(self, event_handler: id) -> id;
}

impl NSAppleEventManager for id {
    unsafe fn set_event_handler(self, event_handler: id) -> id {
        return msg_send![self,
            setEventHandler: event_handler
            andSelector: sel!(handleURLEvent:withReplyEvent:)
            forEventClass: kInternetEventClass
            andEventID: kAEGetURL];
    }
}

extern "C" fn application_will_finish_launching(this: &mut Object, _: Sel, _notification: id) {
    unsafe {
        // register handleURLEvent:withReplyEvent: of AppDelegate class as event handler for URL events
        let ns_apple_event_manager = NSAppleEventManager();
        ns_apple_event_manager.set_event_handler(this);
    }
}

extern "C" fn application_did_finish_launching(_this: &mut Object, _: Sel, _notification: id) {
    // TODO: allow to configure is_accessory somewhere
    let is_accessory = true;
    let activation_policy = if is_accessory {
        NSApplicationActivationPolicyAccessory
    } else {
        NSApplicationActivationPolicyRegular
    };

    unsafe {
        let ns_app = NSApp();
        // We need to delay setting the activation policy and activating the app
        // until we have the main menu all set up. Otherwise the menu won't be interactable.
        ns_app.setActivationPolicy_(activation_policy);
        let () = msg_send![ns_app, activateIgnoringOtherApps: YES];
    }
}

/// This handles menu items in the case that all windows are closed.
extern "C" fn handle_menu_item(this: &mut Object, _: Sel, item: id) {
    unsafe {
        let tag: isize = msg_send![item, tag];
        let inner: *mut c_void = *this.get_ivar(APP_HANDLER_IVAR);
        let inner = &mut *(inner as *mut DelegateState);
        (*inner).command(tag as u32);
    }
}

extern "C" fn open_file(this: &mut Object, _: Sel, application: id, file: id) -> bool {
    let file_path = util::from_nsstring(file);

    unsafe {
        let inner: *mut c_void = *this.get_ivar(APP_HANDLER_IVAR);
        let inner = &mut *(inner as *mut DelegateState);
        (*inner).url_opened(file_path.to_string());
    }

    return true;
}

/// This handles url events
extern "C" fn handle_url_event(this: &mut Object, _: Sel, event: id, reply_event: id) {
    println!("got answer in event handler");
    let url = parse_url_event(event);
    println!("url is {}", url);

    unsafe {
        let inner: *mut c_void = *this.get_ivar(APP_HANDLER_IVAR);
        let inner = &mut *(inner as *mut DelegateState);
        (*inner).url_opened(url.to_string());
    }
}



