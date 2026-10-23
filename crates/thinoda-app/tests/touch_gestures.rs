use egui::{Event, Modifiers, PointerButton, TouchDeviceId, TouchId, TouchPhase, vec2};
use egui_kittest::Harness;
use thinoda_app::app::{Services, ThinodaApp};
use thinoda_platform::settings::Settings;

#[test]
fn only_single_finger_holds_open_canvas_context_actions() {
    for contacts in [1, 2, 3] {
        let mut app = ThinodaApp::new(
            Settings {
                show_format_panel: false,
                show_topic_toolbar: false,
                show_topic_add_buttons: false,
                ..Default::default()
            },
            Services::default(),
            &[],
        );
        app.new_doc(thinoda_core::Workbook::default());
        let mut h = Harness::builder()
            .with_size(vec2(1000.0, 700.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.run_steps(4);
        let doc = h.state().doc().unwrap();
        let p = doc.view.selected_topic_rect(&doc.doc).unwrap().center();
        let before = doc.doc.workbook().clone();
        let revision = doc.doc.state_id();
        h.event(Event::PointerMoved(p));
        h.event(Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        for id in 1..=contacts {
            h.event(Event::Touch {
                device_id: TouchDeviceId(1),
                id: TouchId(id),
                phase: TouchPhase::Start,
                pos: p + vec2(40.0 * (id - 1) as f32, 0.0),
                force: None,
            });
        }
        h.run_steps(60);
        assert_eq!(egui::Popup::is_any_open(&h.ctx), contacts == 1);
        for id in 1..=contacts {
            h.event(Event::Touch {
                device_id: TouchDeviceId(1),
                id: TouchId(id),
                phase: TouchPhase::End,
                pos: p + vec2(40.0 * (id - 1) as f32, 0.0),
                force: None,
            });
        }
        h.event(Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(4);
        assert_eq!(egui::Popup::is_any_open(&h.ctx), contacts == 1);
        let doc = h.state().doc().unwrap();
        assert_eq!(doc.doc.workbook(), &before);
        assert_eq!(doc.doc.state_id(), revision);
        assert_eq!(doc.view.editing_topic(), None);
    }
}
