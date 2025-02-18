use deft::base::{EventContext, Rect};
use deft::element::{Element, ElementBackend, ElementWeak};
use deft::event_loop::create_event_loop_fn_mut;
use deft::js::JsError;
use deft::render::RenderFn;
use deft::{bind_js_event_listener, js_weak_value, ok_or_return, some_or_return, JsValue};
use deft_macros::{event, js_methods, mrc_object};
use deft_skia_safe::{AlphaType, Bitmap, ColorSpace, ColorType, FilterMode, Image, ImageInfo, MipmapMode, Paint, SamplingOptions};
use spice_client_glib::prelude::{Cast, ChannelExt};
use spice_client_glib::{glib, ChannelEvent, DisplayChannel, InputsChannel, MainChannel, MouseButton, MouseButtonMask, Session};
use std::{slice, thread};
use std::any::Any;
use std::sync::{mpsc, Arc, Mutex};
use std::sync::mpsc::Sender;
use deft::event::{KeyDownEvent, KeyUpEvent, MouseDownEvent, MouseMoveEvent, MouseUpEvent};

#[mrc_object]
pub struct SpiceBackend {
    element_weak: ElementWeak,
    image_holder: Option<Image>,
    input_sender: Option<Sender<InputEvent>>,
    pressed_button: Option<MouseButton>,
}

js_weak_value!(SpiceBackend, SpiceBackendWeak);

#[event]
pub struct DisplayOpenEvent;

#[event]
pub struct DisplayCloseEvent;

#[event]
pub struct ConnectSuccessEvent;

#[event]
pub struct ConnectFailEvent {
    reason: i32,
    message: String,
}

#[derive(Default)]
struct SpiceSessionState {
    display: Option<DisplayChannel>,
    inputs: Option<InputsChannel>,
}

unsafe impl Send for SpiceSessionState {}
unsafe impl Sync for SpiceSessionState {}

enum InputEvent {
    Position(i32, i32, Option<MouseButton>),
    ButtonPress(MouseButton),
    ButtonRelease(MouseButton),
    KeyPress(u32),
    KeyRelease(u32),
}

struct RenderData {
    img: Image,
    render_rect: Rect,
    scale: f32,
}


#[js_methods]
impl SpiceBackend {

    #[js_func]
    pub fn new() -> Result<(Element, Self), JsError> {
        let ele = Element::create(Self::create);
        let backend = ele.get_backend_as::<Self>().clone();
        Ok((ele, backend))
    }

    #[js_func]
    pub fn connect(&mut self, uri: String) {
        let mut element_weak = self.element_weak.clone();
        let me = self.as_weak();
        let update_img_callback = create_event_loop_fn_mut(move |img: Image| {
            if let Ok(mut me) = me.upgrade_mut() {
                me.image_holder.replace(img);
                element_weak.mark_dirty(false);
            }
        });

        let element_weak = self.element_weak.clone();
        let display_open_callback = create_event_loop_fn_mut(move |_| {
            element_weak.emit(DisplayOpenEvent);
        });

        let element_weak = self.element_weak.clone();
        let display_close_callback = create_event_loop_fn_mut(move |_| {
            element_weak.emit(DisplayCloseEvent);
        });

        let element_weak = self.element_weak.clone();
        let conn_success_callback = create_event_loop_fn_mut(move |_| {
            element_weak.emit(ConnectSuccessEvent);
        });

        let element_weak = self.element_weak.clone();
        let conn_fail_callback = create_event_loop_fn_mut(move |reason: (i32, String)| {
            element_weak.emit(ConnectFailEvent {
                reason: reason.0,
                message: reason.1,
            });
        });

        let (sender, receiver) = mpsc::channel();
        self.input_sender.replace(sender);

        thread::spawn(move || {
            let session = Session::new();
            session.set_uri(Some(&uri));
            //TODO support password
            let session_state = Arc::new(Mutex::new(SpiceSessionState::default()));
            let ss = session_state.clone();
            session.connect_channel_new(move |_, channel| {
                let channel_type = channel.channel_type();
                println!("channel type: {:?}", channel_type);
                let conn_success_callback = conn_success_callback.clone();
                let conn_fail_callback = conn_fail_callback.clone();
                if let Ok(mc) = channel.clone().downcast::<MainChannel>() {
                    mc.connect_mouse_mode_notify(|channel| {
                        let mode = channel.mouse_mode();
                        println!("mouse mode: {}", mode);
                    });
                    mc.connect_channel_event(move |channel, event| {
                        match event {
                            ChannelEvent::Opened => {
                                let mode = channel.mouse_mode();
                                println!("mouse mode: {}", mode);
                                conn_success_callback.clone().call(());
                            },
                            ChannelEvent::Closed => {},
                            ChannelEvent::None => {}
                            ChannelEvent::Switching => {}
                            ChannelEvent::ErrorConnect => {
                                println!("Error connecting");
                                conn_fail_callback.clone().call((1, "connect error".to_string()));
                            }
                            ChannelEvent::ErrorTls => {
                                println!("Error tls connecting");
                                conn_fail_callback.clone().call((2, "tls error".to_string()));
                            }
                            ChannelEvent::ErrorLink => {
                                println!("Error linking");
                                conn_fail_callback.clone().call((3, "link error".to_string()));
                            }
                            ChannelEvent::ErrorAuth => {
                                println!("Authentication failed");
                                conn_fail_callback.clone().call((4, "auth error".to_string()));
                            }
                            ChannelEvent::ErrorIo => {
                                println!("IO error");
                                conn_fail_callback.clone().call((5, "io error".to_string()));
                            }
                            ChannelEvent::__Unknown(_) => {
                                println!("unknown event");
                            }
                            _ => {},
                        }
                    });
                }
                if let Ok(ic) = channel.clone().downcast::<InputsChannel>() {
                    ChannelExt::connect(&ic);
                    let ss = session_state.clone();
                    let mut ss = ss.lock().unwrap();
                    ss.inputs = Some(ic);
                }
                if let Ok(display) = channel.clone().downcast::<DisplayChannel>() {
                    ChannelExt::connect(&display);
                    let display_open_callback = display_open_callback.clone();
                    let display_close_callback = display_close_callback.clone();
                    display.connect_channel_event(move |channel, event| {
                        match event {
                            ChannelEvent::Opened => {
                                display_open_callback.clone().call(());
                            },
                            ChannelEvent::Closed => {
                                display_close_callback.clone().call(());
                            },
                            _ => {}
                        }
                        dbg!((channel, event));
                    });
                    display.connect_gl_scanout_notify(|display| {
                        dbg!(display.gl_scanout().unwrap().fd());
                    });
                    let update_img_callback = update_img_callback.clone();
                    display.connect_display_invalidate(move |display, _x, _y, _width, _height| {
                        let data = display.primary(0).unwrap();

                        let width = data.width() as i32;
                        let height = data.height() as i32;
                        let image_info = ImageInfo::new(
                            (width, height),
                            ColorType::BGRA8888,
                            AlphaType::Unpremul,
                            ColorSpace::new_srgb(),
                        );
                        let mut bm = Bitmap::new();
                        let _ = bm.set_info(&image_info, width as usize * 4);
                        bm.alloc_pixels();
                        let src_bytes = data.data();
                        unsafe {
                            let pixels = slice::from_raw_parts_mut(bm.pixels() as *mut u8, src_bytes.len());
                            let mut offset = 0;
                            while offset < src_bytes.len() {
                                pixels[offset] =  src_bytes[offset];
                                pixels[offset + 1] = src_bytes[offset + 1];
                                pixels[offset + 2] = src_bytes[offset + 2];
                                pixels[offset + 3] = 0xFF;
                                offset += 4;
                            }
                        }
                        let img = bm.as_image();
                        let mut uic = update_img_callback.clone();
                        uic.call(img);
                    });
                    dbg!(display.monitors());
                }
            });

            session.connect();

            let main_context = glib::MainContext::default();
            let mc = main_context.clone();
            thread::spawn(move || {
                loop {
                    let e = ok_or_return!(receiver.recv());
                    let ss = ss.clone();
                    mc.invoke(move || {
                        let mut ss = ss.lock().unwrap();
                        let ic = some_or_return!(&mut ss.inputs);
                        match e {
                            InputEvent::Position(x, y, btn) => {
                                //println!("position: {:?}", (x, y));
                                let btn = btn.map(|b| get_button_mask(b));
                                ic.position(x, y, 0, btn.unwrap_or(0));
                            }
                            InputEvent::ButtonPress(b) => {
                                //println!("button pressed: {:?}", b);
                                let mask = get_button_mask(b);
                                ic.button_press(b as i32, mask);
                            }
                            InputEvent::ButtonRelease(b) => {
                                //println!("button released: {:?}", b);
                                let mask = get_button_mask(b);
                                ic.button_release(b as i32, mask);
                            }
                            InputEvent::KeyPress(scancode) => {
                                ic.key_press(scancode);
                            }
                            InputEvent::KeyRelease(scancode) => {
                                ic.key_release(scancode);
                            }
                        }
                    });
                }
            });
            let main_loop = glib::MainLoop::new(Some(&main_context), false);
            main_loop.run();
        });
    }

    #[js_func]
    pub fn bind_js_event_listener(&mut self, event_type: String, listener: JsValue) -> Result<u32, JsError> {
        let mut element = self.element_weak.upgrade_mut()?;
        let id = bind_js_event_listener!(
            element, event_type.as_str(), listener;
            "displayopen" => DisplayOpenEventListener,
            "displayclose"  => DisplayCloseEventListener,
            "connectsuccess" => ConnectSuccessEventListener,
            "connectfail" => ConnectFailEventListener,
        );
        Ok(id)
    }


    fn get_render_data(&self) -> Option<RenderData> {
        let img = self.image_holder.clone()?;
        let el = self.element_weak.upgrade_mut().ok()?;
        let (width, height) = el.get_size();
        let (img_width, img_height) = (img.width()  as f32, img.height() as f32);
        let mut w = width;
        let mut h = w / img_width * img_height;
        if h > height {
            h = height;
            w = img_width / img_height * h;
        }
        let x = (width - w) / 2.0;
        let y = (height - h) / 2.0;
        let scale = w / img_width;
        let data = RenderData {
            img,
            render_rect: Rect::new(x, y, w, h),
            scale,
        };
        Some(data)
    }

}

// #[js_methods]
impl ElementBackend for SpiceBackend {
    fn create(element: &mut Element) -> Self
    where
        Self: Sized,
    {
        SpiceBackendData {
            element_weak: element.as_weak(),
            image_holder: None,
            input_sender: None,
            pressed_button: None,
        }
        .to_ref()
    }

    fn get_name(&self) -> &str {
        "Spice"
    }

    fn render(&mut self) -> RenderFn {
        let data = some_or_return!(self.get_render_data(), RenderFn::empty());
        RenderFn::new(move |canvas| {
            let options = SamplingOptions::new(FilterMode::Linear, MipmapMode::None);
            canvas.draw_image_rect_with_sampling_options(&data.img, None, &data.render_rect.to_skia_rect(), options, &Paint::default());
        })
    }

    fn on_event(&mut self, event: Box<&mut dyn Any>, ctx: &mut EventContext<ElementWeak>) {
        let sender = some_or_return!(&self.input_sender);
        let render_data = some_or_return!(self.get_render_data());
        if let Some(e) = event.downcast_ref::<MouseDownEvent>() {
            let b = some_or_return!(map_deft_button_to_spice(e.0.button));
            sender.send(InputEvent::ButtonPress(b)).unwrap();
            self.pressed_button = Some(b);
        } else if let Some(e) = event.downcast_ref::<MouseUpEvent>() {
            let b = some_or_return!(map_deft_button_to_spice(e.0.button));
            sender.send(InputEvent::ButtonRelease(b)).unwrap();
            self.pressed_button = None;
        } else if let Some(e) = event.downcast_ref::<MouseMoveEvent>() {
            let x = (e.0.offset_x - render_data.render_rect.x) / render_data.scale;
            let y = (e.0.offset_y - render_data.render_rect.y) / render_data.scale;
            sender.send(InputEvent::Position(x as i32, y as i32, self.pressed_button)).unwrap();
        } else if let Some(e) = event.downcast_ref::<KeyDownEvent>() {
            let scancode = some_or_return!(e.0.scancode);
            sender.send(InputEvent::KeyPress(scancode)).unwrap();
        } else if let Some(e) = event.downcast_ref::<KeyUpEvent>() {
            let scancode = some_or_return!(e.0.scancode);
            sender.send(InputEvent::KeyRelease(scancode)).unwrap();
        }
    }
}

fn map_deft_button_to_spice(button: i32) -> Option<MouseButton> {
    let b = match button {
        1 => MouseButton::Left,
        2 => MouseButton::Right,
        3 => MouseButton::Middle,
        _ => return None
    };
    Some(b)
}

fn get_button_mask(b: MouseButton) -> i32 {
    match b {
        MouseButton::Left => MouseButtonMask::LEFT.bits(),
        MouseButton::Middle => MouseButtonMask::MIDDLE.bits(),
        MouseButton::Right => MouseButtonMask::RIGHT.bits(),
        _ => 0,
    }
}

