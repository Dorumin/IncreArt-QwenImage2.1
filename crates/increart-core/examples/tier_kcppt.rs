//! Writes the VRAM-tier KoboldCpp templates (docs/vram-tiers.md) from the
//! official Qwen-Image 2.1 one (koboldcpp/kcppt on Hugging Face): its
//! settings, in its order, with each tier's model, memory and prompt-enhance
//! ones on top.
//!
//!     cargo run -p increart-core --example tier_kcppt -- OFFICIAL.kcppt [out_dir]
//!
//! (out_dir: crates/increart-core/kcppt, where the app reads them from)

use std::fmt;
use std::path::{Path, PathBuf};

use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json::{Value, json};

const HF: &str = "https://huggingface.co";
const CPU: i64 = -2;
const MAIN: i64 = -1;

/// A JSON object that keeps its keys' order.
struct Ordered(Vec<(String, Value)>);

impl Ordered {
    /// Like Python's `dict.update`: a known key keeps its place.
    fn set(&mut self, key: &str, value: Value) {
        match self.0.iter_mut().find(|(k, _)| k == key) {
            Some((_, v)) => *v = value,
            None => self.0.push((key.into(), value)),
        }
    }
}

impl<'de> Deserialize<'de> for Ordered {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Ordered;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Ordered, A::Error> {
                let mut out = vec![];
                while let Some(entry) = map.next_entry()? {
                    out.push(entry);
                }
                Ok(Ordered(out))
            }
        }
        d.deserialize_map(V)
    }
}

impl Serialize for Ordered {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let official = args.first().expect("usage: tier_kcppt OFFICIAL.kcppt [out_dir]");
    let out = args.get(1).map_or(Path::new(env!("CARGO_MANIFEST_DIR")).join("kcppt"), PathBuf::from);
    let official: Ordered = serde_json::from_str(&std::fs::read_to_string(official).unwrap()).unwrap();
    std::fs::create_dir_all(&out).unwrap();

    let image =
        |quant: &str| json!(format!("{HF}/leejet/Qwen-Image-2.1-GGUF/resolve/main/qwen_image_2.1-{quant}.gguf"));
    let chat = format!("{HF}/Qwen/Qwen3-VL-8B-Instruct-GGUF/resolve/main/Qwen3VL-8B-Instruct-Q4_K_M.gguf");
    let mmproj = format!("{HF}/Qwen/Qwen3-VL-8B-Instruct-GGUF/resolve/main/mmproj-Qwen3VL-8B-Instruct-Q8_0.gguf");
    // chat beside the image model (resident) or swapped with it (autoswap)
    let tiers = [
        ("8GB", true, image("Q4_K"), CPU, 4096, 4096),
        ("12GB", true, image("Q4_K"), MAIN, 8192, 8192),
        ("16GB", true, image("Q8_0"), MAIN, 12288, 8192),
        ("24GB", false, image("Q8_0"), MAIN, 10240, 8192),
        ("32GB", false, image("Q8_0"), MAIN, 0, 8192),
    ];
    for (tier, swap, sdmodel, clip, vram, context) in tiers {
        let mut t = Ordered(official.0.clone());
        // a server: no browser on launch (or on every autoswap reload)
        t.set("launch", json!(false));
        // prompt enhance: the image model's text encoder, as a chat model
        t.set("model_param", json!(chat));
        t.set("model", json!([]));
        t.set("mmproj", json!(mmproj));
        t.set("jinja", json!(true));
        t.set("gpulayers", json!(999));
        t.set("sdoffloadcpu", json!(true));
        t.set("sdflashattention", json!(true));
        t.set("sdtiledvae", json!(512));
        t.set("sdvaedevice", json!(MAIN));
        t.set("sdmaingpu", json!(MAIN));
        if swap {
            for k in ["admin", "routermode", "autoswapmode"] {
                t.set(k, json!(true));
            }
        }
        t.set("sdmodel", sdmodel);
        t.set("sdclipdevice", json!(clip));
        t.set("sdvramlimit", json!(vram));
        t.set("contextsize", json!(context));

        let path = out.join(format!("Qwen-Image-2.1-{tier}.kcppt"));
        let mut text = Vec::new();
        let mut ser =
            serde_json::Serializer::with_formatter(&mut text, serde_json::ser::PrettyFormatter::with_indent(b" "));
        t.serialize(&mut ser).unwrap();
        text.push(b'\n');
        std::fs::write(&path, text).unwrap();
        println!("{}", path.display());
    }
}
