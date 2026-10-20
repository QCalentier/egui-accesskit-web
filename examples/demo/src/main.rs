//! A page of egui's own widgets, for trying the adapter with a screen
//! reader: `cargo xtask web`, then serve `dist/` and open it.

/// The demo's state: what its widgets change.
struct Demo {
    clicks: u32,
    sound: bool,
    volume: f32,
    size: Size,
    name: String,
    said: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Size {
    Small,
    Medium,
    Large,
}

impl Default for Demo {
    fn default() -> Self {
        Self {
            clicks: 0,
            sound: true,
            volume: 60.0,
            size: Size::Medium,
            name: String::new(),
            said: vec!["Welcome.".to_owned()],
        }
    }
}

impl Demo {
    fn ui(&mut self, ui: &mut egui::Ui) {
        // egui draws a heading as big text; this says it is one, so a
        // screen reader can jump to it (H in browse mode).
        let title = ui.heading("egui, read aloud");
        ui.ctx().accesskit_node_builder(title.id, |node| {
            node.set_role(egui::accesskit::Role::Heading);
            node.set_level(1);
            node.set_label("egui, read aloud");
        });
        ui.label(
            "Every widget here is egui's own; a screen reader reads it as a page \
             (Tab moves between controls, Space and Enter press them).",
        );
        ui.separator();

        ui.horizontal(|ui| {
            if ui.button("Count").clicked() {
                self.clicks += 1;
                self.said.push(format!("Counted {}.", self.clicks));
            }
            ui.label(format!("Pressed {} times", self.clicks));
        });
        ui.checkbox(&mut self.sound, "Sound");
        ui.add(egui::Slider::new(&mut self.volume, 0.0..=100.0).text("Volume"));
        ui.horizontal(|ui| {
            ui.label("Size:");
            ui.radio_value(&mut self.size, Size::Small, "Small");
            ui.radio_value(&mut self.size, Size::Medium, "Medium");
            ui.radio_value(&mut self.size, Size::Large, "Large");
        });
        egui::ComboBox::from_label("Size, as a list")
            .selected_text(format!("{:?}", self.size))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.size, Size::Small, "Small");
                ui.selectable_value(&mut self.size, Size::Medium, "Medium");
                ui.selectable_value(&mut self.size, Size::Large, "Large");
            });
        ui.horizontal(|ui| {
            let label = ui.label("Name:");
            ui.text_edit_singleline(&mut self.name)
                .labelled_by(label.id);
        });
        ui.collapsing("More", |ui| {
            ui.label("Folded away until opened.");
            ui.hyperlink_to("egui", "https://github.com/emilk/egui");
        });
        ui.separator();
        ui.label("What happened:");
        for line in self.said.iter().rev().take(5) {
            ui.label(line);
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl eframe::App for Demo {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| Demo::ui(self, ui));
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast;

    wasm_bindgen_futures::spawn_local(async {
        let canvas: web_sys::HtmlCanvasElement = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("demo"))
            .and_then(|e| e.dyn_into().ok())
            .expect("the page's <canvas id=\"demo\">");
        let plugin = egui_accesskit_web::AccessKitWeb::new(&canvas);
        let started = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(move |cc| {
                    cc.egui_ctx.add_plugin(plugin);
                    Ok(Box::new(Demo::default()))
                }),
            )
            .await;
        if let Err(e) = started {
            web_sys::console::error_1(&e);
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    // The demo's widgets still build here, for the workspace's checks.
    let _ = Demo::ui;
    eprintln!("The demo runs in a browser: `cargo xtask web`, then serve dist/.");
}
