//! Guardado y carga de tablaturas en disco.
//!
//! Cada canción es un archivo JSON dentro de `songs/`, en el propio repositorio. Es una
//! decisión deliberada: al vivir en git se obtiene copia de seguridad, historial de cómo
//! evolucionó cada arreglo y publicación con un `push`, sin montar nada.
//!
//! El JSON va indentado a propósito. Ocupa algo más, pero un `git diff` de una tablatura
//! se puede leer, y eso vale mucho más que los bytes ahorrados.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tabs_core::model::Score;

/// Carpeta donde viven las tablaturas, relativa a la raíz del repositorio.
const SONGS_DIR: &str = "songs";

/// Fallos de disco, ya en texto legible para la interfaz.
#[derive(Debug, thiserror::Error, Serialize)]
pub enum StorageError {
    /// No se pudo leer o escribir.
    #[error("no se pudo acceder a «{path}»: {reason}")]
    Io {
        /// Archivo implicado.
        path: String,
        /// Motivo del sistema.
        reason: String,
    },
    /// El archivo no contiene una partitura válida.
    #[error("«{path}» no es una tablatura válida: {reason}")]
    Malformed {
        /// Archivo implicado.
        path: String,
        /// Motivo del análisis.
        reason: String,
    },
    /// El título no da un nombre de archivo utilizable.
    #[error("el título no sirve como nombre de archivo")]
    EmptyName,
}

/// Resumen de una canción guardada, para listarlas sin cargarlas enteras.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SongEntry {
    /// Nombre del archivo sin extensión.
    pub slug: String,
    /// Título de la canción.
    pub title: String,
    /// Intérprete, si se anotó.
    pub artist: Option<String>,
    /// Número de compases.
    pub bar_count: u32,
}

/// Convierte un título en un nombre de archivo seguro y legible.
///
/// Se quitan los acentos y todo lo que no sea alfanumérico, y los espacios pasan a
/// guiones: `"Wish You Were Here"` da `wish-you-were-here`. Que el nombre siga siendo
/// reconocible importa, porque estos archivos se leen en un `git log`.
#[must_use]
pub fn slugify(title: &str) -> String {
    let mut slug = String::with_capacity(title.len());
    let mut last_was_dash = true; // evita empezar con guion

    for character in title.chars() {
        let plain = deaccent(character);
        if plain.is_ascii_alphanumeric() {
            slug.push(plain.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}

/// Sustituye una vocal acentuada por su equivalente sin acento.
fn deaccent(character: char) -> char {
    match character {
        'á' | 'à' | 'ä' | 'â' | 'Á' | 'À' | 'Ä' | 'Â' => 'a',
        'é' | 'è' | 'ë' | 'ê' | 'É' | 'È' | 'Ë' | 'Ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' | 'Í' | 'Ì' | 'Ï' | 'Î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'Ó' | 'Ò' | 'Ö' | 'Ô' => 'o',
        'ú' | 'ù' | 'ü' | 'û' | 'Ú' | 'Ù' | 'Ü' | 'Û' => 'u',
        'ñ' | 'Ñ' => 'n',
        other => other,
    }
}

/// Carpeta de canciones, creándola si hace falta.
fn songs_dir(root: &Path) -> Result<PathBuf, StorageError> {
    let dir = root.join(SONGS_DIR);
    std::fs::create_dir_all(&dir).map_err(|error| StorageError::Io {
        path: dir.display().to_string(),
        reason: error.to_string(),
    })?;
    Ok(dir)
}

/// Guarda una partitura y devuelve el nombre de archivo usado.
///
/// # Errors
///
/// Falla si el título no da un nombre válido o si no se puede escribir.
pub fn save(root: &Path, score: &Score) -> Result<String, StorageError> {
    let slug = slugify(&score.meta.title);
    if slug.is_empty() {
        return Err(StorageError::EmptyName);
    }

    let path = songs_dir(root)?.join(format!("{slug}.json"));
    let json = serde_json::to_string_pretty(score).map_err(|error| StorageError::Malformed {
        path: path.display().to_string(),
        reason: error.to_string(),
    })?;

    std::fs::write(&path, json).map_err(|error| StorageError::Io {
        path: path.display().to_string(),
        reason: error.to_string(),
    })?;

    Ok(slug)
}

/// Carga una partitura por su nombre de archivo.
///
/// # Errors
///
/// Falla si el archivo no existe o no contiene una partitura válida.
pub fn load(root: &Path, slug: &str) -> Result<Score, StorageError> {
    let path = songs_dir(root)?.join(format!("{slug}.json"));
    let json = std::fs::read_to_string(&path).map_err(|error| StorageError::Io {
        path: path.display().to_string(),
        reason: error.to_string(),
    })?;

    let mut score: Score =
        serde_json::from_str(&json).map_err(|error| StorageError::Malformed {
            path: path.display().to_string(),
            reason: error.to_string(),
        })?;

    // Un archivo editado a mano puede traer pulsos sin identificador; se reparan al abrir
    // en lugar de rechazarlo, porque estos archivos están pensados para ser legibles.
    score.assign_missing_ids();
    Ok(score)
}

/// Lista las canciones guardadas.
///
/// Los archivos ilegibles se ignoran en lugar de tumbar el listado: uno corrupto no debe
/// impedir abrir los demás.
///
/// # Errors
///
/// Falla sólo si no se puede leer la carpeta.
pub fn list(root: &Path) -> Result<Vec<SongEntry>, StorageError> {
    let dir = songs_dir(root)?;
    let entries = std::fs::read_dir(&dir).map_err(|error| StorageError::Io {
        path: dir.display().to_string(),
        reason: error.to_string(),
    })?;

    let mut songs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        // `is_none_or` sólo existe desde Rust 1.82 y el proyecto declara 1.77 como mínimo.
        if path.extension().map_or(true, |ext| ext != "json") {
            continue;
        }
        let Some(slug) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(json) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(score) = serde_json::from_str::<Score>(&json) else {
            continue;
        };

        songs.push(SongEntry {
            slug: slug.to_owned(),
            title: score.meta.title.clone(),
            artist: score.meta.artist.clone(),
            bar_count: score.bar_count(),
        });
    }

    songs.sort_by_key(|song| song.title.to_lowercase());
    Ok(songs)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{list, load, save, slugify, StorageError};
    use tabs_core::model::Score;

    /// Carpeta temporal propia de cada prueba, para que no se pisen entre ellas.
    fn temp_root(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tabs-repo-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn el_titulo_se_convierte_en_nombre_de_archivo_legible() {
        assert_eq!(slugify("Wish You Were Here"), "wish-you-were-here");
        assert_eq!(slugify("Canción Bonita"), "cancion-bonita");
        assert_eq!(slugify("  Espacios   raros  "), "espacios-raros");
        assert_eq!(slugify("AC/DC — Thunderstruck"), "ac-dc-thunderstruck");
    }

    #[test]
    fn un_titulo_sin_letras_no_da_nombre() {
        assert_eq!(slugify("¿¡...!?"), "");
        let root = temp_root("sin-nombre");
        let score = Score::new("¿¡...!?", 1);
        assert!(matches!(
            save(&root, &score).unwrap_err(),
            StorageError::EmptyName
        ));
    }

    #[test]
    fn guardar_y_volver_a_cargar_conserva_la_partitura() {
        let root = temp_root("ida-y-vuelta");
        let mut score = Score::new("Blackbird", 12);
        score.meta.artist = Some("The Beatles".to_owned());
        score.meta.tempo_bpm = 96.0;

        let slug = save(&root, &score).unwrap();
        assert_eq!(slug, "blackbird");

        let loaded = load(&root, &slug).unwrap();
        assert_eq!(loaded.meta.title, "Blackbird");
        assert_eq!(loaded.meta.artist.as_deref(), Some("The Beatles"));
        assert_eq!(loaded.bar_count(), 12);
        assert_eq!(loaded.id, score.id, "el identificador se conserva");
    }

    #[test]
    fn el_json_guardado_es_legible_para_git() {
        let root = temp_root("legible");
        let score = Score::new("Prueba", 2);
        save(&root, &score).unwrap();

        let json = std::fs::read_to_string(root.join("songs").join("prueba.json")).unwrap();
        assert!(
            json.lines().count() > 10,
            "el JSON va indentado para que el diff se pueda leer"
        );
    }

    #[test]
    fn no_se_escriben_los_campos_que_estan_en_su_valor_por_defecto() {
        // Un archivo lleno de `"repeat_start": false` entierra el cambio de verdad cuando
        // se lee un `git diff`, que es justamente para lo que existen estos archivos.
        let root = temp_root("compacto");
        save(&root, &Score::new("Prueba", 16)).unwrap();

        let json = std::fs::read_to_string(root.join("songs").join("prueba.json")).unwrap();
        for noise in [
            "\"repeat_start\"",
            "\"free_time\"",
            "\"anacrusis\"",
            "\"double_bar\"",
            "\"subtitle\"",
            "\"album\"",
        ] {
            assert!(
                !json.contains(noise),
                "{noise} sobra cuando vale lo de siempre"
            );
        }
        assert!(
            json.contains("\"title\""),
            "lo que sí importa se sigue escribiendo"
        );
    }

    #[test]
    fn lo_omitido_se_recupera_con_su_valor_por_defecto() {
        let root = temp_root("defectos");
        let mut score = Score::new("Prueba", 2);
        score.master_bars[1].repeat_start = true;
        save(&root, &score).unwrap();

        let loaded = load(&root, "prueba").unwrap();
        assert!(
            !loaded.master_bars[0].repeat_start,
            "lo omitido vuelve como falso"
        );
        assert!(
            loaded.master_bars[1].repeat_start,
            "lo que sí estaba se conserva"
        );
    }

    #[test]
    fn se_listan_las_canciones_ordenadas_por_titulo() {
        let root = temp_root("listado");
        for title in ["Zorro", "Alba", "Manzana"] {
            save(&root, &Score::new(title, 1)).unwrap();
        }

        let songs = list(&root).unwrap();
        let titles: Vec<_> = songs.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, vec!["Alba", "Manzana", "Zorro"]);
    }

    #[test]
    fn un_archivo_corrupto_no_tumba_el_listado() {
        let root = temp_root("corrupto");
        save(&root, &Score::new("Buena", 1)).unwrap();
        std::fs::write(root.join("songs").join("rota.json"), "{ esto no es json").unwrap();

        let songs = list(&root).unwrap();
        assert_eq!(songs.len(), 1, "la canción sana sigue apareciendo");
        assert_eq!(songs[0].title, "Buena");
    }

    #[test]
    fn cargar_algo_que_no_existe_avisa_en_vez_de_reventar() {
        let root = temp_root("inexistente");
        assert!(matches!(
            load(&root, "no-existe").unwrap_err(),
            StorageError::Io { .. }
        ));
    }
}
