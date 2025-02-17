use deft::base::Rect;
use deft::element::{Element, ElementBackend, ElementWeak};
use deft::event_loop::create_event_loop_fn_mut;
use deft::js::JsError;
use deft::render::RenderFn;
use deft::{bind_js_event_listener, js_weak_value, ok_or_return, some_or_return, JsValue};
use deft_macros::{event, js_methods, mrc_object};
use deft_skia_safe::{AlphaType, Bitmap, ColorSpace, ColorType, FilterMode, Image, ImageInfo, MipmapMode, Paint, SamplingOptions};
use spice_client_glib::prelude::{Cast, ChannelExt};
use spice_client_glib::{glib, ChannelEvent, DisplayChannel, MainChannel, Session};
use std::{slice, thread};

#[mrc_object]
pub struct SpiceBackend {
    element_weak: ElementWeak,
    image_holder: Option<Image>,
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

        thread::spawn(move || {
            let session = Session::new();
            session.set_uri(Some(&uri));
            //TODO support password
            session.connect_channel_new(move |_, channel| {
                let channel_type = channel.channel_type();
                println!("channel type: {:?}", channel_type);
                let conn_success_callback = conn_success_callback.clone();
                let conn_fail_callback = conn_fail_callback.clone();
                if let Ok(mc) = channel.clone().downcast::<MainChannel>() {
                    mc.connect_channel_event(move |channel, event| {
                        match event {
                            ChannelEvent::Opened => {
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
                            _ => {

                            }
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
        }
        .to_ref()
    }

    fn get_name(&self) -> &str {
        "Spice"
    }

    fn render(&mut self) -> RenderFn {
        let img = some_or_return!(self.image_holder.clone(), RenderFn::empty());
        let el = ok_or_return!(self.element_weak.upgrade_mut(), RenderFn::empty());
        let (width, height) = el.get_size();
        let (img_width, img_height) = (img.width()  as f32, img.height() as f32);
        RenderFn::new(move |canvas| {
            let mut w = width;
            let mut h = w / img_width * img_height;
            if h > height {
                h = height;
                w = img_width / img_height * h;
            }
            let x = (width - w) / 2.0;
            let y = (height - h) / 2.0;
            let dst_rect = Rect::new(x, y, w, h);
            let options = SamplingOptions::new(FilterMode::Linear, MipmapMode::None);
            canvas.draw_image_rect_with_sampling_options(&img, None, &dst_rect.to_skia_rect(), options, &Paint::default());
        })
    }
}
