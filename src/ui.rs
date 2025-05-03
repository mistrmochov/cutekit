use dirs::home_dir;
use eyre::eyre;
use eyre::Result;
use gtk4::{
    self as gtk, gdk_pixbuf::Pixbuf, glib::object::IsA, prelude::*, AlertDialog, Builder,
    DrawingArea,
};
use serde_json::Value;
use std::fs;
use std::rc::Rc;

pub fn get_object<T>(builder: &Builder, name: &str) -> Result<T>
where
    T: IsA<gtk4::glib::Object>,
{
    builder.object(name).ok_or(eyre!(
        "Unable to get UI element {}, this likely means the XML was changed/corrupted.",
        name
    ))
}

pub fn build_fail_alert() -> AlertDialog {
    AlertDialog::builder()
        .message("Authentication failed for some reason. Check your login details and try again.")
        .buttons(vec!["Ok"])
        .build()
}

pub fn create_circular_image(path: &str, size: i32) -> DrawingArea {
    let area = DrawingArea::new();
    if let Some(home) = home_dir() {
        let conf_file = home.join(".config/cutekit/config.json");
        let conf = fs::read_to_string(conf_file.to_string_lossy().to_string())
            .as_ref()
            .unwrap()
            .to_string();

        let pixbuf = Pixbuf::from_file(path).unwrap();
        let pixbuf = Rc::new(pixbuf);

        area.set_content_width(size);
        area.set_content_height(size);

        let pb_clone = pixbuf.clone();
        area.set_draw_func(move |area, cr, _, _| {
            #[allow(deprecated)]
            let area_width = area.allocated_width() as f64;
            #[allow(deprecated)]
            let area_height = area.allocated_height() as f64;

            // Průměr obrázku bude minimální z šířky a výšky, aby byl vždy uvnitř
            let image_draw_size = area_width.min(area_height);
            let radius = image_draw_size / 2.0;
            let cx = area_width / 2.0;
            let cy = area_height / 2.0;

            let colors_vec_string = get_border_color(conf.clone());
            cr.set_source_rgb(
                string_to_u32(colors_vec_string[0].to_string()) as f64 / 255.0,
                string_to_u32(colors_vec_string[1].to_string()) as f64 / 255.0,
                string_to_u32(colors_vec_string[2].to_string()) as f64 / 255.0,
            );
            let border_width_string = get_conf_data(conf.clone(), "logo_border_width");
            let border_width = string_to_u32(border_width_string) as f64;
            cr.set_line_width(border_width);
            cr.arc(
                cx,
                cy,
                radius - (border_width / 2.0),
                0.0,
                std::f64::consts::PI * 2.0,
            );
            cr.stroke_preserve().expect("Failed to draw a border"); // vykreslí border a zachová cestu pro clip

            cr.clip(); // clipni do kruhu

            // Výpočet pozice obrázku a měřítka
            let x = cx - radius;
            let y = cy - radius;

            let scale_x = image_draw_size / pb_clone.width() as f64;
            let scale_y = image_draw_size / pb_clone.height() as f64;

            cr.save().expect("Failed to save the context");
            cr.translate(x, y);
            cr.scale(scale_x, scale_y);
            gtk::cairo::Context::set_source_pixbuf(cr, &pb_clone, 0.0, 0.0);
            cr.paint().unwrap();
            cr.restore().expect("Failed to restore the context");
        });
    }

    area
}

fn get_border_color(conf: String) -> Vec<String> {
    let mut color = Vec::new();
    let data: Value = serde_json::from_str(&conf).expect("Failed to get data from json");

    if let Some(data_array) = data.as_array() {
        for entry in data_array {
            if let Some(targets) = entry.get("logo_border_color").and_then(|s| s.as_array()) {
                for target in targets {
                    if let Some(color_cute) = target.as_str() {
                        color.push(color_cute.to_string());
                    }
                }
            }
        }
    }

    color
}

fn string_to_u32(input: String) -> u32 {
    let out;
    match input.parse::<u32>() {
        Ok(number) => {
            out = number;
        }
        Err(_) => {
            out = 0;
        }
    }
    out
}

pub fn get_conf_data(conf: String, which: &str) -> String {
    let mut out = String::new();
    let data: Value = serde_json::from_str(&conf).expect("Failed to get data from json");

    if let Some(data_array) = data.as_array() {
        for entry in data_array {
            if let Some(target) = entry.get(which).and_then(|s| s.as_str()) {
                out = target.to_string();
            }
        }
    }

    out
}
