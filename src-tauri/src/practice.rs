//! Cómo va cada canción: lo que se sabe tocar y lo que todavía no.
//!
//! Una lista de canciones marcadas como «hechas» no dice nada. Lo que hace falta saber al
//! sentarse a ensayar es a qué velocidad sale hoy una pieza frente a la que debería, y qué
//! compases son los que siguen tropezando: eso es lo que decide qué se ensaya.
//!
//! Vive en `practice/`, un archivo por canción y separado de la tablatura, porque son dos
//! cosas distintas: la tablatura se publica, el progreso es de quien la toca. Sigue siendo
//! JSON legible en un `git diff` por la misma razón que las canciones.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::storage::StorageError;

/// Carpeta donde vive el progreso, relativa a la raíz del repositorio.
const PRACTICE_DIR: &str = "practice";

/// En qué punto está una canción.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// Todavía se está sacando de la grabación.
    #[default]
    Sacando,
    /// Ya está escrita entera y toca ensayarla.
    Ensayando,
    /// Sale a tempo y de memoria.
    Lista,
}

/// El progreso de una canción.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Practice {
    /// En qué punto está.
    pub status: Status,
    /// A qué velocidad sale hoy. Cero mientras no se haya medido.
    pub tempo_bpm: f32,
    /// A qué velocidad tiene que salir. Cero significa «la de la grabación».
    pub target_bpm: f32,
    /// Compases que siguen tropezando, en orden y sin repetir.
    pub tricky_bars: Vec<u32>,
}

impl Practice {
    /// Marca o desmarca un compás como atragantado.
    ///
    /// La misma tecla pone y quita: un compás deja de atragantarse de un día para otro y
    /// tener que buscar dónde se desmarca haría que nadie lo desmarcara nunca.
    pub fn toggle_bar(&mut self, bar: u32) {
        if let Some(at) = self.tricky_bars.iter().position(|&marked| marked == bar) {
            self.tricky_bars.remove(at);
        } else {
            self.tricky_bars.push(bar);
            self.tricky_bars.sort_unstable();
        }
    }
}

fn practice_dir(root: &Path) -> Result<PathBuf, StorageError> {
    let dir = root.join(PRACTICE_DIR);
    std::fs::create_dir_all(&dir).map_err(|error| StorageError::Io {
        path: dir.display().to_string(),
        reason: error.to_string(),
    })?;
    Ok(dir)
}

/// Lee el progreso de una canción.
///
/// Una canción sin archivo de progreso no es un error: es una canción que todavía no se ha
/// ensayado, y eso se representa con el progreso vacío.
///
/// # Errors
///
/// Falla si no se puede crear la carpeta de progreso.
pub fn load(root: &Path, slug: &str) -> Result<Practice, StorageError> {
    let path = practice_dir(root)?.join(format!("{slug}.json"));
    let Ok(json) = std::fs::read_to_string(&path) else {
        return Ok(Practice::default());
    };
    Ok(serde_json::from_str(&json).unwrap_or_default())
}

/// Guarda el progreso de una canción.
///
/// # Errors
///
/// Falla si no se puede escribir el archivo.
pub fn save(root: &Path, slug: &str, practice: &Practice) -> Result<(), StorageError> {
    let path = practice_dir(root)?.join(format!("{slug}.json"));
    let json = serde_json::to_string_pretty(practice).map_err(|error| StorageError::Malformed {
        path: path.display().to_string(),
        reason: error.to_string(),
    })?;
    std::fs::write(&path, json).map_err(|error| StorageError::Io {
        path: path.display().to_string(),
        reason: error.to_string(),
    })
}

/// Progreso de una canción, con la canción a la que pertenece.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PracticeEntry {
    /// Nombre de archivo de la canción, sin extensión.
    pub slug: String,
    /// Su progreso.
    pub practice: Practice,
}

/// Lee el progreso de todas las canciones que tengan alguno.
///
/// La interfaz lo cruza con el repertorio para pintar cada canción con su estado. Un
/// archivo corrupto se salta en vez de tumbar la lista: perder el progreso de una canción
/// no puede impedir ver el de las demás.
///
/// # Errors
///
/// Falla si no se puede crear o leer la carpeta de progreso.
pub fn load_all(root: &Path) -> Result<Vec<PracticeEntry>, StorageError> {
    let dir = practice_dir(root)?;
    let entries = std::fs::read_dir(&dir).map_err(|error| StorageError::Io {
        path: dir.display().to_string(),
        reason: error.to_string(),
    })?;

    let mut all = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(true, |ext| ext != "json") {
            continue;
        }
        let Some(slug) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(json) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(practice) = serde_json::from_str::<Practice>(&json) else {
            continue;
        };
        all.push(PracticeEntry {
            slug: slug.to_owned(),
            practice,
        });
    }

    all.sort_by(|left, right| left.slug.cmp(&right.slug));
    Ok(all)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{load, load_all, save, Practice, Status};

    fn temp_root(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tabs-repo-practice-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn una_cancion_sin_ensayar_no_es_un_error() {
        let root = temp_root("sin-ensayar");
        let practice = load(&root, "blackbird").unwrap();
        assert_eq!(practice.status, Status::Sacando);
        assert!(practice.tricky_bars.is_empty());
    }

    #[test]
    fn el_progreso_va_y_vuelve() {
        let root = temp_root("ida-y-vuelta");
        let mut practice = Practice {
            status: Status::Ensayando,
            tempo_bpm: 72.0,
            target_bpm: 96.0,
            tricky_bars: Vec::new(),
        };
        practice.toggle_bar(12);
        practice.toggle_bar(3);

        save(&root, "blackbird", &practice).unwrap();
        let loaded = load(&root, "blackbird").unwrap();

        assert_eq!(loaded.status, Status::Ensayando);
        assert!((loaded.tempo_bpm - 72.0).abs() < f32::EPSILON);
        assert_eq!(
            loaded.tricky_bars,
            vec![3, 12],
            "los compases quedan en orden"
        );
    }

    #[test]
    fn la_misma_tecla_marca_y_desmarca() {
        let mut practice = Practice::default();
        practice.toggle_bar(5);
        assert_eq!(practice.tricky_bars, vec![5]);
        practice.toggle_bar(5);
        assert!(
            practice.tricky_bars.is_empty(),
            "un compás deja de atragantarse"
        );
    }

    #[test]
    fn se_listan_todas_las_que_tienen_progreso() {
        let root = temp_root("listado");
        save(&root, "blackbird", &Practice::default()).unwrap();
        save(&root, "little-wing", &Practice::default()).unwrap();
        std::fs::write(root.join("practice").join("rota.json"), "{ esto no es json").unwrap();

        let all = load_all(&root).unwrap();
        let slugs: Vec<&str> = all.iter().map(|entry| entry.slug.as_str()).collect();
        assert_eq!(slugs, vec!["blackbird", "little-wing"], "la rota se salta");
    }
}
