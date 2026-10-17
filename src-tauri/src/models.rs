use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone)]
pub struct ModelLocation {
    pub path: String,
    pub label: String,
    pub engine: String,
}
pub fn roots() -> Vec<PathBuf> {
    let base = dirs::data_dir().unwrap_or_default();
    vec![
        base.join("Meetily/models"),
        base.join("com.meetily.ai/models"),
        base.join("com.dnalevity.livecoach/models"),
    ]
}
pub fn discover() -> Vec<ModelLocation> {
    fn visit(dir: &Path, depth: usize, out: &mut Vec<ModelLocation>) {
        if depth > 4 {
            return;
        }
        if dir.join("vocab.txt").is_file()
            && dir.join("nemo128.onnx").is_file()
            && (dir.join("encoder-model.int8.onnx").is_file()
                || dir.join("encoder-model.onnx").is_file())
            && (dir.join("decoder_joint-model.int8.onnx").is_file()
                || dir.join("decoder_joint-model.onnx").is_file())
        {
            out.push(ModelLocation {
                path: dir.to_string_lossy().into(),
                label: format!(
                    "Parakeet · {}",
                    dir.file_name().unwrap_or_default().to_string_lossy()
                ),
                engine: "parakeet".into(),
            });
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    visit(&p, depth + 1, out);
                } else if p.extension().is_some_and(|e| e == "bin")
                    && p.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .starts_with("ggml-")
                {
                    out.push(ModelLocation {
                        path: p.to_string_lossy().into(),
                        label: format!(
                            "Whisper · {}",
                            p.file_name().unwrap_or_default().to_string_lossy()
                        ),
                        engine: "whisper".into(),
                    });
                }
            }
        }
    }
    let mut out = vec![];
    for root in roots() {
        visit(&root, 0, &mut out);
    }
    out
}
pub fn validate(path: &str, engine: &str) -> Result<(), String> {
    if discover()
        .iter()
        .any(|m| m.path == path && m.engine == engine)
    {
        Ok(())
    } else {
        Err("Select a compatible installed model. Live Coach never downloads models; place compatible Whisper ggml or Parakeet ONNX files in its own models folder.".into())
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn isolated_roots() {
        assert!(super::roots().iter().all(|p| p.ends_with("models")));
        assert_eq!(super::roots().len(), 3);
    }
}
