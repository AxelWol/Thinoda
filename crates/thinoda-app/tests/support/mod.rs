use egui::accesskit::Role;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_app::app::ThinodaApp;

pub fn menu_action<'a>(h: &'a Harness<'_, ThinodaApp>, label: &'a str) -> egui_kittest::Node<'a> {
    h.query_all_by_role_and_label(Role::Button, label)
        .find(|node| {
            let mut parent = node.accesskit_node().parent();
            while let Some(node) = parent {
                if node.role() == Role::Menu {
                    return true;
                }
                parent = node.parent();
            }
            false
        })
        .unwrap_or_else(|| panic!("No menu action: {label}"))
}
