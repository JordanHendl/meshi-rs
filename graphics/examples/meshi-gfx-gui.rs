use std::env::args;
use std::ffi::c_void;

use glam::{vec2, Vec2, Vec4};
use meshi_ffi_structs::event::*;
use meshi_graphics::gui::{
    GuiClipRect, GuiContext, GuiDraw, GuiLayer, GuiQuad, Menu, MenuBar, MenuBarRenderOptions,
    MenuBarState, MenuItem,
};
use meshi_graphics::{RendererSelect, TextInfo};
use meshi_utils::timer::Timer;

#[path = "common/setup.rs"]
mod common_setup;

fn quad_from_pixels(position: Vec2, size: Vec2, color: Vec4, viewport: Vec2) -> GuiQuad {
    let left = (position.x / viewport.x) * 2.0 - 1.0;
    let right = ((position.x + size.x) / viewport.x) * 2.0 - 1.0;
    let top = 1.0 - (position.y / viewport.y) * 2.0;
    let bottom = 1.0 - ((position.y + size.y) / viewport.y) * 2.0;

    GuiQuad {
        positions: [[left, top], [right, top], [right, bottom], [left, bottom]],
        uvs: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        color: color.to_array(),
    }
}

fn point_in_rect(point: Vec2, position: Vec2, size: Vec2) -> bool {
    point.x >= position.x
        && point.x <= position.x + size.x
        && point.y >= position.y
        && point.y <= position.y + size.y
}

fn update_text(
    engine: &mut meshi_graphics::RenderEngine,
    handle: dashi::Handle<meshi_graphics::TextObject>,
    text: &str,
    position: Vec2,
    color: Vec4,
    scale: f32,
    render_mode: meshi_graphics::TextRenderMode,
) {
    engine.set_text(handle, text);
    engine.set_text_info(
        handle,
        &TextInfo {
            text: text.to_string(),
            position,
            color,
            scale,
            render_mode,
        },
    );
}

fn main() {
    tracing_subscriber::fmt::init();
    let args: Vec<String> = args().collect();
    let renderer = common_setup::renderer_from_args(&args, RendererSelect::Deferred);
    let mut setup = common_setup::init(
        "meshi-gfx-gui",
        [960, 600],
        common_setup::CameraSetup::default(),
        renderer,
    );

    let text_render_mode = common_setup::text_render_mode(&setup.db);

    let title_text = setup.engine.register_text(&TextInfo {
        text: "Meshi GUI overlay".to_string(),
        position: vec2(24.0, 16.0),
        color: Vec4::ONE,
        scale: 1.6,
        render_mode: text_render_mode.clone(),
    });

    let button_a_text = setup.engine.register_text(&TextInfo {
        text: "Button A".to_string(),
        position: vec2(0.0, 0.0),
        color: Vec4::ONE,
        scale: 1.2,
        render_mode: text_render_mode.clone(),
    });

    let button_b_text = setup.engine.register_text(&TextInfo {
        text: "Button B".to_string(),
        position: vec2(0.0, 0.0),
        color: Vec4::ONE,
        scale: 1.2,
        render_mode: text_render_mode.clone(),
    });

    let status_text = setup.engine.register_text(&TextInfo {
        text: "Hover a button to animate it".to_string(),
        position: vec2(24.0, 560.0),
        color: Vec4::new(0.8, 0.85, 1.0, 1.0),
        scale: 1.1,
        render_mode: text_render_mode.clone(),
    });

    struct AppData {
        running: bool,
        cursor: Vec2,
    }

    let mut data = AppData {
        running: true,
        cursor: Vec2::ZERO,
    };

    extern "C" fn callback(event: *mut Event, data: *mut c_void) {
        unsafe {
            let e = &mut (*event);
            let r = &mut (*(data as *mut AppData));
            if e.source() == EventSource::Window && e.event_type() == EventType::Quit {
                r.running = false;
            }
            if e.source() == EventSource::Mouse && e.event_type() == EventType::CursorMoved {
                r.cursor = e.motion2d();
            }
        }
    }

    setup
        .engine
        .set_event_cb(callback, (&mut data as *mut AppData) as *mut c_void);

    let mut timer = Timer::new();
    timer.start();
    let mut last_time = timer.elapsed_seconds_f32();
    let viewport = setup.window_size;
    let mut last_status = String::new();
    let menu_bar = MenuBar {
        menus: vec![
            Menu {
                label: "File".to_string(),
                items: vec![
                    MenuItem {
                        label: "New".to_string(),
                        enabled: true,
                        shortcut: Some("Ctrl+N".to_string()),
                        checked: false,
                        action_id: Some(1),
                        is_separator: false,
                    },
                    MenuItem {
                        label: "Open".to_string(),
                        enabled: true,
                        shortcut: Some("Ctrl+O".to_string()),
                        checked: false,
                        action_id: Some(2),
                        is_separator: false,
                    },
                    MenuItem::separator(),
                    MenuItem {
                        label: "Quit".to_string(),
                        enabled: true,
                        shortcut: Some("Ctrl+Q".to_string()),
                        checked: false,
                        action_id: Some(3),
                        is_separator: false,
                    },
                ],
            },
            Menu {
                label: "View".to_string(),
                items: vec![
                    MenuItem {
                        label: "Show Grid".to_string(),
                        enabled: true,
                        shortcut: None,
                        checked: true,
                        action_id: Some(10),
                        is_separator: false,
                    },
                    MenuItem {
                        label: "Show Guides".to_string(),
                        enabled: false,
                        shortcut: None,
                        checked: false,
                        action_id: Some(11),
                        is_separator: false,
                    },
                ],
            },
        ],
    };

    while data.running {
        let now = timer.elapsed_seconds_f32();
        let dt = (now - last_time).min(1.0 / 30.0);
        last_time = now;

        let panel_position = vec2(32.0, 64.0);
        let panel_size = vec2(300.0, 220.0);
        let base_button_size = vec2(140.0, 44.0);
        let button_a_pos = panel_position + vec2(24.0, 64.0);
        let button_b_pos = panel_position + vec2(24.0, 124.0);

        let hover_a = point_in_rect(data.cursor, button_a_pos, base_button_size);
        let hover_b = point_in_rect(data.cursor, button_b_pos, base_button_size);
        let hover_scale = if hover_a || hover_b { 1.06 } else { 1.0 };

        let pulse = (now * 2.0).sin() * 0.1 + 0.9;
        let panel_color = Vec4::new(0.12, 0.14, 0.18, 0.92 * pulse);

        let button_color = |hovered: bool| {
            if hovered {
                Vec4::new(0.3, 0.7, 1.0, 0.95)
            } else {
                Vec4::new(0.2, 0.25, 0.3, 0.9)
            }
        };

        let button_size = base_button_size * hover_scale;
        let button_a_draw_pos = button_a_pos - (button_size - base_button_size) * 0.5;
        let button_b_draw_pos = button_b_pos - (button_size - base_button_size) * 0.5;

        let image_base_pos = vec2(380.0, 140.0);
        let image_motion = vec2((now * 1.4).sin() * 12.0, (now * 0.9).cos() * 6.0);
        let image_pos = image_base_pos + image_motion;
        let image_size = vec2(220.0, 160.0);
        let clip_rect = GuiClipRect::from_position_size(
            [image_base_pos.x + 16.0, image_base_pos.y + 16.0],
            [188.0, 120.0],
        );

        let mut gui = GuiContext::new();
        let menu_options = MenuBarRenderOptions {
            viewport: [viewport.x, viewport.y],
            position: [0.0, 0.0],
            layer: GuiLayer::Overlay,
            metrics: Default::default(),
            colors: Default::default(),
            state: MenuBarState { open_menu: Some(0) },
        };

        gui.submit_draw(GuiDraw::new(
            GuiLayer::Background,
            None,
            quad_from_pixels(vec2(0.0, 0.0), viewport, Vec4::new(0.05, 0.05, 0.07, 1.0), viewport),
        ));

        gui.submit_menu_bar(&menu_bar, &menu_options);

        gui.submit_draw(GuiDraw::new(
            GuiLayer::World,
            None,
            quad_from_pixels(panel_position, panel_size, panel_color, viewport),
        ));

        gui.submit_draw(GuiDraw::new(
            GuiLayer::World,
            None,
            quad_from_pixels(button_a_draw_pos, button_size, button_color(hover_a), viewport),
        ));

        gui.submit_draw(GuiDraw::new(
            GuiLayer::World,
            None,
            quad_from_pixels(button_b_draw_pos, button_size, button_color(hover_b), viewport),
        ));

        gui.submit_draw(GuiDraw::with_clip_rect(
            GuiLayer::World,
            Some(0),
            quad_from_pixels(image_pos, image_size, Vec4::ONE, viewport),
            clip_rect,
        ));

        gui.submit_draw(GuiDraw::new(
            GuiLayer::Overlay,
            None,
            quad_from_pixels(
                vec2(360.0, 100.0),
                vec2(260.0, 240.0),
                Vec4::new(0.1, 0.15, 0.2, 0.2),
                viewport,
            ),
        ));

        let frame = gui.build_frame();
        setup.engine.upload_gui_frame(frame);

        let button_text_color = if hover_a || hover_b {
            Vec4::new(0.05, 0.08, 0.12, 1.0)
        } else {
            Vec4::new(0.85, 0.9, 1.0, 1.0)
        };

        update_text(
            &mut setup.engine,
            button_a_text,
            "Button A",
            button_a_draw_pos + vec2(18.0, 12.0),
            button_text_color,
            1.2,
            text_render_mode.clone(),
        );

        update_text(
            &mut setup.engine,
            button_b_text,
            "Button B",
            button_b_draw_pos + vec2(18.0, 12.0),
            button_text_color,
            1.2,
            text_render_mode.clone(),
        );

        let status = if hover_a {
            "Hovering: Button A"
        } else if hover_b {
            "Hovering: Button B"
        } else {
            "Hover a button to animate it"
        };

        if status != last_status {
            setup.engine.set_text(status_text, status);
            last_status = status.to_string();
        }

        setup.engine.update(dt);
    }

    setup.engine.release_text(title_text);
    setup.engine.release_text(button_a_text);
    setup.engine.release_text(button_b_text);
    setup.engine.release_text(status_text);
    setup.engine.shut_down();
}
