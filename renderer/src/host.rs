//! Browser host glue: WebGPU (wasm + WebGPU) integration for the renderer.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! This module is only compiled when the `web` feature is enabled and the
//! target is `wasm32`. It wires a [`Renderer`] to an HTML canvas, performs
//! WebGPU feature-detection, and drives the render loop with
//! `requestAnimationFrame`.
//!
//! Note: WebGPU types in `web-sys` are gated behind the unstable
//! `web_sys_unstable_apis` cfg. We deliberately avoid those typed bindings and
//! let `wgpu` manage adapter/device acquisition through the browser's GPU
//! interface, detecting support via a feature test instead.

#![cfg(all(feature = "web", target_arch = "wasm32"))]

use std::cell::RefCell;
use std::rc::Rc;

use vertex_kernel::assembly::Assembly;
use wgpu::SurfaceTarget;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::camera::Camera;
use crate::renderer::Renderer;

/// Errors surfaced while initializing the browser renderer.
#[derive(Debug, thiserror::Error)]
pub enum WebRendererError {
    #[error("WebGPU is not supported by this browser")]
    Unsupported,
    #[error("failed to acquire a GPU adapter")]
    NoAdapter,
    #[error("wgpu surface configuration failed: {0}")]
    Surface(String),
    #[error("renderer error: {0}")]
    Renderer(#[from] crate::renderer::RendererError),
}

/// Returns `true` if the current browser exposes the WebGPU API.
///
/// Feature-detection is done by probing `navigator.gpu` without relying on the
/// unstable `web_sys` GPU type bindings.
pub fn webgpu_supported() -> bool {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return false,
    };
    let navigator = window.navigator();
    js_sys::Reflect::has(&navigator, &JsValue::from_str("gpu")).unwrap_or(false)
}

/// A browser-hosted renderer bound to a canvas. Owns the [`Renderer`] and the
/// camera used for the render loop.
pub struct WebRenderer {
    renderer: Renderer,
    camera: Camera,
    canvas: HtmlCanvasElement,
}

impl WebRenderer {
    /// Detect WebGPU support and, if available, create a renderer targeting
    /// `canvas`. Returns [`WebRendererError::Unsupported`] when the browser
    /// lacks WebGPU so callers can show a fallback message.
    pub async fn new(canvas: HtmlCanvasElement) -> Result<Self, WebRendererError> {
        if !webgpu_supported() {
            return Err(WebRendererError::Unsupported);
        }

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            flags: wgpu::InstanceFlags::default(),
            dx12_shader_compiler: wgpu::Dx12Compiler::default(),
            gles_minor_version: wgpu::Gles3MinorVersion::default(),
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .ok_or(WebRendererError::NoAdapter)?;

        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let surface = instance
            .create_surface(SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|e| WebRendererError::Surface(e.to_string()))?;

        let format = surface
            .get_default_config(&adapter, width, height)
            .ok_or(WebRendererError::NoAdapter)?
            .format;

        let renderer = Renderer::new(&instance, surface, &adapter, width, height, format).await?;

        let mut camera = Camera::default();
        camera.set_aspect(width as f32 / height as f32);

        Ok(WebRenderer {
            renderer,
            camera,
            canvas,
        })
    }

    /// Upload an assembly to render.
    pub fn set_assembly(&mut self, asm: &Assembly) {
        self.renderer.upload_assembly(asm);
    }

    /// Resize the swap chain to match the canvas's current pixel size.
    pub fn resize_to_canvas(&mut self) {
        let w = self.canvas.width().max(1);
        let h = self.canvas.height().max(1);
        self.renderer.resize(w, h);
        self.camera.set_aspect(w as f32 / h as f32);
    }

    /// Orbit the camera by the given deltas (radians).
    pub fn orbit(&mut self, dyaw: f32, dpitch: f32) {
        self.camera.orbit(dyaw, dpitch);
    }

    /// Dolly-zoom the camera by the given factor.
    pub fn zoom(&mut self, factor: f32) {
        self.camera.zoom(factor);
    }

    /// Set the highlighted node (hover/selection feedback).
    pub fn set_highlighted(&mut self, node_id: Option<u64>) {
        self.renderer.set_highlighted(node_id);
    }

    /// Pick the scene node under normalized device coordinates.
    pub fn pick(&self, ndc_x: f32, ndc_y: f32) -> Option<u64> {
        self.renderer.pick(&self.camera, ndc_x, ndc_y)
    }

    /// Render a single frame.
    pub fn render(&mut self) -> Result<(), WebRendererError> {
        self.renderer.render_frame(&self.camera)?;
        Ok(())
    }

    /// Consume `self` and start the `requestAnimationFrame` render loop. The
    /// `on_frame` callback runs once per frame before rendering (e.g. to apply
    /// pending camera input), keeping input plumbing decoupled from the host.
    pub fn run(self, on_frame: impl Fn(&mut Camera) + 'static) {
        let shared = Rc::new(RefCell::new(Some(self)));
        let on_frame = Rc::new(on_frame);

        let closure: Rc<RefCell<Option<Closure<dyn FnMut()>>>> =
            Rc::new(RefCell::new(None));
        let closure_cb = closure.clone();

        *closure.borrow_mut() = Some(Closure::new(move || {
            // Schedule the next frame first so a panic mid-frame doesn't kill
            // the loop.
            {
                let cb = closure_cb.borrow();
                if let Some(c) = cb.as_ref() {
                    let window = web_sys::window().expect("no window");
                    let _ = window.request_animation_frame(c.as_ref().unchecked_ref());
                }
            }
            let mut guard = shared.borrow_mut();
            if let Some(host) = guard.as_mut() {
                on_frame(&mut host.camera);
                let _ = host.render();
            }
        }));

        {
            let cb = closure.borrow();
            if let Some(c) = cb.as_ref() {
                let window = web_sys::window().expect("no window");
                let _ = window.request_animation_frame(c.as_ref().unchecked_ref());
            }
        }
    }
}
