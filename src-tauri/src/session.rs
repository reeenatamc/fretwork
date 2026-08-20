//! Sesión de edición: la partitura abierta y su historial.
//!
//! La partitura vive **aquí**, no en el frontend. Así hay una sola fuente de verdad, el
//! historial de deshacer viaja con ella, y la interfaz se limita a mostrar lo que Rust le
//! devuelve. El frontend manda operaciones y recibe el AlphaTex ya listo para renderizar.

// Tauri deserializa los argumentos de los comandos por valor; recibirlos por referencia
// no es una opción, así que este lint no aplica aquí.
#![allow(clippy::needless_pass_by_value)]

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tabs_core::alphatex::to_alphatex;
use tabs_core::edit::{EditCommand, EditError, EditHistory};
use tabs_core::model::{BeatAddr, Score};

/// Estado compartido de la aplicación.
#[derive(Default)]
pub struct AppState {
    session: Mutex<Option<Session>>,
    /// Raíz del repositorio donde viven las tablaturas.
    root: Mutex<Option<std::path::PathBuf>>,
}

impl AppState {
    /// Fija la carpeta del repositorio donde se guardan las canciones.
    pub fn set_root(&self, root: std::path::PathBuf) {
        if let Ok(mut guard) = self.root.lock() {
            *guard = Some(root);
        }
    }

    fn root_path(&self) -> Result<std::path::PathBuf, SessionError> {
        self.root
            .lock()
            .map_err(|error| SessionError::Poisoned(error.to_string()))?
            .clone()
            .ok_or_else(|| SessionError::Edit("no se sabe dónde guardar".to_owned()))
    }
}

/// Una partitura abierta con su historial de edición.
struct Session {
    score: Score,
    history: EditHistory,
    /// Versión adornada propuesta, todavía sin aceptar.
    pending: Option<Score>,
}

/// Lo que necesita el frontend para pintarse tras cada cambio.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SessionView {
    /// Partitura serializada, lista para alphaTab.
    pub tex: String,
    /// Título de la canción.
    pub title: String,
    /// Número de compases.
    pub bar_count: u32,
    /// Si hay algo que deshacer.
    pub can_undo: bool,
    /// Si hay algo que rehacer.
    pub can_redo: bool,
}

/// Fallos de la sesión, ya en forma de texto para mostrar en la interfaz.
#[derive(Debug, thiserror::Error, Serialize)]
pub enum SessionError {
    /// No hay ninguna partitura abierta.
    #[error("no hay ninguna partitura abierta")]
    NoSession,
    /// La operación de edición falló.
    #[error("{0}")]
    Edit(String),
    /// El estado compartido quedó envenenado por un pánico previo.
    #[error("el estado de la sesión quedó corrupto: {0}")]
    Poisoned(String),
}

impl From<EditError> for SessionError {
    fn from(error: EditError) -> Self {
        Self::Edit(error.to_string())
    }
}

impl AppState {
    /// Ejecuta una operación sobre la sesión abierta.
    fn with_session<T>(
        &self,
        action: impl FnOnce(&mut Session) -> Result<T, SessionError>,
    ) -> Result<T, SessionError> {
        let mut guard = self
            .session
            .lock()
            .map_err(|error| SessionError::Poisoned(error.to_string()))?;
        let session = guard.as_mut().ok_or(SessionError::NoSession)?;
        action(session)
    }
}

impl Session {
    fn view(&self) -> SessionView {
        SessionView {
            tex: to_alphatex(&self.score),
            title: self.score.meta.title.clone(),
            bar_count: self.score.bar_count(),
            can_undo: self.history.can_undo(),
            can_redo: self.history.can_redo(),
        }
    }
}

/// Abre una partitura nueva y la deja como sesión activa.
///
/// # Errors
///
/// Falla si el estado compartido quedó corrupto por un pánico previo.
#[tauri::command]
pub fn session_new(
    state: tauri::State<'_, AppState>,
    title: String,
    bar_count: u32,
    tempo_bpm: f32,
) -> Result<SessionView, SessionError> {
    let mut score = Score::new(title, bar_count.max(1));
    score.meta.tempo_bpm = tempo_bpm;

    let session = Session {
        score,
        history: EditHistory::new(),
        pending: None,
    };
    let view = session.view();

    let mut guard = state
        .session
        .lock()
        .map_err(|error| SessionError::Poisoned(error.to_string()))?;
    *guard = Some(session);

    Ok(view)
}

/// Devuelve el estado actual sin modificar nada.
///
/// # Errors
///
/// Falla si no hay sesión abierta.
#[tauri::command]
pub fn session_view(state: tauri::State<'_, AppState>) -> Result<SessionView, SessionError> {
    state.with_session(|session| Ok(session.view()))
}

/// Aplica una operación de edición y devuelve el estado resultante.
///
/// # Errors
///
/// Falla si no hay sesión o si la operación no es válida (cuerda o traste imposibles).
#[tauri::command]
pub fn session_apply(
    state: tauri::State<'_, AppState>,
    command: EditCommand,
) -> Result<SessionView, SessionError> {
    state.with_session(|session| {
        session.history.apply(&mut session.score, &command)?;
        Ok(session.view())
    })
}

/// Aplica varias operaciones como un solo paso de deshacer.
///
/// # Errors
///
/// Igual que [`session_apply`].
#[tauri::command]
pub fn session_apply_batch(
    state: tauri::State<'_, AppState>,
    commands: Vec<EditCommand>,
) -> Result<SessionView, SessionError> {
    state.with_session(|session| {
        session
            .history
            .apply(&mut session.score, &EditCommand::Batch { commands })?;
        Ok(session.view())
    })
}

/// Deshace la última operación.
///
/// # Errors
///
/// Falla si no hay sesión abierta.
#[tauri::command]
pub fn session_undo(state: tauri::State<'_, AppState>) -> Result<SessionView, SessionError> {
    state.with_session(|session| {
        session.history.undo(&mut session.score)?;
        Ok(session.view())
    })
}

/// Rehace la última operación deshecha.
///
/// # Errors
///
/// Falla si no hay sesión abierta.
#[tauri::command]
pub fn session_redo(state: tauri::State<'_, AppState>) -> Result<SessionView, SessionError> {
    state.with_session(|session| {
        session.history.redo(&mut session.score)?;
        Ok(session.view())
    })
}

/// Devuelve las notas que hay en un compás, para que la interfaz pinte el cursor y el
/// contenido sin tener que interpretar el AlphaTex.
///
/// # Errors
///
/// Falla si no hay sesión abierta.
#[tauri::command]
pub fn session_bar_notes(
    state: tauri::State<'_, AppState>,
    bar: u32,
) -> Result<BarView, SessionError> {
    state.with_session(|session| {
        let signature = session
            .score
            .master_bars
            .get(bar as usize)
            .map(|master| master.time_signature)
            .unwrap_or_default();

        let mut summary = Vec::new();
        for (addr, beat) in session.score.iter_beats() {
            if addr.bar != bar {
                continue;
            }
            summary.push(BeatSummary {
                addr,
                duration: beat.duration as u16,
                dots: beat.dots,
                is_rest: beat.is_rest,
                notes: beat
                    .notes
                    .iter()
                    .map(|note| NoteSummary {
                        string: note.string,
                        fret: note.fret,
                        techniques: note.techniques.bits(),
                    })
                    .collect(),
            });
        }
        // ¿Está el compás lleno? Se suma con aritmética racional exacta: con puntillos y
        // tresillos, comparar en coma flotante daría respuestas erráticas justo en el
        // borde, que es donde importa.
        let capacity = tabs_core::model::Fraction::new(
            u64::from(signature.numerator),
            u64::from(signature.denominator),
        );
        let filled = session
            .score
            .iter_beats()
            .filter(|(addr, _)| addr.bar == bar && addr.voice == 0)
            .fold(tabs_core::model::Fraction::zero(), |acc, (_, beat)| {
                acc + beat.duration_in_whole_notes()
            });

        // Qué fracción del compás está escrita. Ver por qué importa en `BarView::filled`.
        let filled_ratio = if capacity.as_f64() > 0.0 {
            filled.as_f64() / capacity.as_f64()
        } else {
            0.0
        };

        Ok(BarView {
            beats: summary,
            numerator: signature.numerator,
            denominator: signature.denominator,
            is_full: filled_ratio >= 1.0,
            filled: filled_ratio,
        })
    })
}

/// Guarda la partitura abierta en `songs/` y devuelve el nombre de archivo.
///
/// # Errors
///
/// Falla si no hay sesión, si el título no da un nombre válido o si no se puede escribir.
#[tauri::command]
pub fn session_save(state: tauri::State<'_, AppState>) -> Result<String, SessionError> {
    let root = state.root_path()?;
    state.with_session(|session| {
        crate::storage::save(&root, &mut session.score)
            .map_err(|error| SessionError::Edit(error.to_string()))
    })
}

/// Abre una canción guardada y la deja como sesión activa.
///
/// # Errors
///
/// Falla si el archivo no existe o no es una tablatura válida.
#[tauri::command]
pub fn session_open(
    state: tauri::State<'_, AppState>,
    slug: String,
) -> Result<SessionView, SessionError> {
    let root = state.root_path()?;
    let score = crate::storage::load(&root, &slug)
        .map_err(|error| SessionError::Edit(error.to_string()))?;

    let session = Session {
        score,
        history: EditHistory::new(),
        pending: None,
    };
    let view = session.view();

    let mut guard = state
        .session
        .lock()
        .map_err(|error| SessionError::Poisoned(error.to_string()))?;
    *guard = Some(session);

    Ok(view)
}

/// Lista las canciones guardadas.
///
/// # Errors
///
/// Falla si no se puede leer la carpeta de canciones.
#[tauri::command]
pub fn session_list(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<crate::storage::SongEntry>, SessionError> {
    let root = state.root_path()?;
    crate::storage::list(&root).map_err(|error| SessionError::Edit(error.to_string()))
}

/// Cambia los datos de cabecera de la canción abierta.
///
/// # Errors
///
/// Falla si no hay sesión abierta.
#[tauri::command]
pub fn session_set_meta(
    state: tauri::State<'_, AppState>,
    title: Option<String>,
    artist: Option<String>,
    source_url: Option<String>,
    tempo_bpm: Option<f32>,
) -> Result<SessionView, SessionError> {
    state.with_session(|session| {
        if let Some(title) = title {
            session.score.meta.title = title;
        }
        if let Some(artist) = artist {
            session.score.meta.artist = Some(artist);
        }
        if let Some(url) = source_url {
            session.score.meta.source_url = Some(url);
        }
        if let Some(tempo) = tempo_bpm {
            session.score.meta.tempo_bpm = tempo;
        }
        Ok(session.view())
    })
}

/// Pone las etiquetas de la canción abierta.
///
/// # Errors
///
/// Falla si no hay sesión abierta.
#[tauri::command]
pub fn session_set_tags(
    state: tauri::State<'_, AppState>,
    tags: Vec<String>,
) -> Result<SessionView, SessionError> {
    state.with_session(|session| {
        session.score.meta.tags = tabs_core::model::normalize_tags(tags);
        Ok(session.view())
    })
}

/// Devuelve el nombre de archivo que le toca a la canción abierta.
///
/// El progreso se guarda por canción y la canción se identifica por su archivo, así que la
/// interfaz necesita saber cuál es sin tener que repetir aquí las reglas del nombre.
///
/// # Errors
///
/// Falla si no hay sesión abierta.
#[tauri::command]
pub fn session_slug(state: tauri::State<'_, AppState>) -> Result<String, SessionError> {
    state.with_session(|session| Ok(crate::storage::slugify(&session.score.meta.title)))
}

/// Lee el progreso de una canción.
///
/// # Errors
///
/// Falla si no se puede acceder a la carpeta de progreso.
#[tauri::command]
pub fn practice_get(
    state: tauri::State<'_, AppState>,
    slug: String,
) -> Result<crate::practice::Practice, SessionError> {
    let root = state.root_path()?;
    crate::practice::load(&root, &slug).map_err(|error| SessionError::Edit(error.to_string()))
}

/// Cambia el estado o los tempos de una canción y devuelve el progreso resultante.
///
/// Lo que no se manda no se toca: la interfaz cambia una cosa cada vez y no tiene por qué
/// reenviar el resto para no borrarlo sin querer.
///
/// # Errors
///
/// Falla si no se puede leer o escribir el progreso.
#[tauri::command]
pub fn practice_set(
    state: tauri::State<'_, AppState>,
    slug: String,
    status: Option<crate::practice::Status>,
    tempo_bpm: Option<f32>,
    target_bpm: Option<f32>,
) -> Result<crate::practice::Practice, SessionError> {
    let root = state.root_path()?;
    let mut practice = crate::practice::load(&root, &slug)
        .map_err(|error| SessionError::Edit(error.to_string()))?;

    if let Some(status) = status {
        practice.status = status;
    }
    if let Some(tempo) = tempo_bpm {
        practice.tempo_bpm = tempo.max(0.0);
    }
    if let Some(target) = target_bpm {
        practice.target_bpm = target.max(0.0);
    }

    crate::practice::save(&root, &slug, &practice)
        .map_err(|error| SessionError::Edit(error.to_string()))?;
    Ok(practice)
}

/// Marca o desmarca un compás como atragantado.
///
/// # Errors
///
/// Falla si no se puede leer o escribir el progreso.
#[tauri::command]
pub fn practice_toggle_bar(
    state: tauri::State<'_, AppState>,
    slug: String,
    bar: u32,
) -> Result<crate::practice::Practice, SessionError> {
    let root = state.root_path()?;
    let mut practice = crate::practice::load(&root, &slug)
        .map_err(|error| SessionError::Edit(error.to_string()))?;
    practice.toggle_bar(bar);
    crate::practice::save(&root, &slug, &practice)
        .map_err(|error| SessionError::Edit(error.to_string()))?;
    Ok(practice)
}

/// Devuelve el progreso de todas las canciones que tengan alguno.
///
/// # Errors
///
/// Falla si no se puede leer la carpeta de progreso.
#[tauri::command]
pub fn practice_all(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<crate::practice::PracticeEntry>, SessionError> {
    let root = state.root_path()?;
    crate::practice::load_all(&root).map_err(|error| SessionError::Edit(error.to_string()))
}

/// Cambia el instrumento de la pista.
///
/// El número es el programa General MIDI: 24 nylon, 25 acústica metálica, 26 eléctrica
/// jazz, 27 eléctrica limpia, 29 con saturación, 30 con distorsión.
///
/// # Errors
///
/// Falla si no hay sesión abierta.
#[tauri::command]
pub fn session_set_instrument(
    state: tauri::State<'_, AppState>,
    program: u8,
) -> Result<SessionView, SessionError> {
    state.with_session(|session| {
        if let Some(track) = session.score.tracks.first_mut() {
            track.midi_program = program;
        }
        Ok(session.view())
    })
}

/// Lista los soundfonts que haya en la carpeta `soundfonts/` del repositorio.
///
/// El que trae alphaTab suena correcto pero delgado. En vez de empaquetar uno grande de
/// licencia ajena, la aplicación carga el que se deje en esa carpeta: así cada quien usa
/// el que prefiera sin que el repositorio cargue con decenas de megas.
///
/// # Errors
///
/// Falla si no se sabe dónde está el repositorio.
#[tauri::command]
pub fn list_soundfonts(state: tauri::State<'_, AppState>) -> Result<Vec<String>, SessionError> {
    let dir = state.root_path()?.join("soundfonts");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        // Que no exista la carpeta no es un error: simplemente no hay ninguno.
        return Ok(Vec::new());
    };

    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let extension = path.extension()?.to_str()?.to_lowercase();
            if extension == "sf2" || extension == "sf3" {
                path.file_name()?.to_str().map(ToOwned::to_owned)
            } else {
                None
            }
        })
        .collect();

    names.sort();
    Ok(names)
}

/// Lee un soundfont de la carpeta `soundfonts/` para que alphaTab lo cargue.
///
/// # Errors
///
/// Falla si el archivo no existe o no se puede leer.
#[tauri::command]
pub fn read_soundfont(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<Vec<u8>, SessionError> {
    // Sólo el nombre del archivo: un nombre con rutas dentro podría sacar la lectura
    // fuera de la carpeta prevista.
    let file_name = std::path::Path::new(&name)
        .file_name()
        .ok_or_else(|| SessionError::Edit("nombre de archivo inválido".to_owned()))?;

    let path = state.root_path()?.join("soundfonts").join(file_name);
    std::fs::read(&path)
        .map_err(|error| SessionError::Edit(format!("no se pudo leer «{name}»: {error}")))
}

/// Propuesta de arreglo: la versión más difícil, sin aplicarla todavía.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ArrangementPreview {
    /// Detalle de lo que se hizo y cuánto subió la dificultad.
    pub arrangement: tabs_core::transform::Arrangement,
    /// La versión adornada, lista para renderizar y escuchar.
    pub tex: String,
}

/// Propone una versión más difícil sin tocar la partitura abierta.
///
/// No se aplica nada hasta que se acepta: la persona tiene que poder escucharla y
/// descartarla, porque el criterio musical no se puede dar por bueno sin oírlo.
///
/// # Errors
///
/// Falla si no hay sesión abierta.
#[tauri::command]
pub fn session_preview_harder(
    state: tauri::State<'_, AppState>,
    target_delta: f32,
) -> Result<ArrangementPreview, SessionError> {
    state.with_session(|session| {
        let options = tabs_core::transform::Options {
            target_delta,
            ..tabs_core::transform::Options::default()
        };
        let (arranged, arrangement) = tabs_core::transform::embellish(&session.score, options);
        let tex = to_alphatex(&arranged);
        // Se guarda a un lado para poder aceptarla sin recalcular.
        session.pending = Some(arranged);
        Ok(ArrangementPreview { arrangement, tex })
    })
}

/// Acepta la propuesta y la deja como versión actual.
///
/// El cambio entra en el historial, así que se puede deshacer como cualquier otra edición.
///
/// # Errors
///
/// Falla si no hay ninguna propuesta pendiente.
#[tauri::command]
pub fn session_accept_harder(
    state: tauri::State<'_, AppState>,
) -> Result<SessionView, SessionError> {
    state.with_session(|session| {
        let arranged = session
            .pending
            .take()
            .ok_or_else(|| SessionError::Edit("no hay ninguna propuesta que aceptar".to_owned()))?;
        session.score = arranged;
        // Adornar reescribe la partitura entera, así que el historial de operaciones
        // sueltas deja de tener sentido: se empieza de cero desde esta versión.
        session.history = EditHistory::new();
        Ok(session.view())
    })
}

/// Descarta la propuesta pendiente.
///
/// # Errors
///
/// Falla si no hay sesión abierta.
#[tauri::command]
pub fn session_discard_harder(state: tauri::State<'_, AppState>) -> Result<(), SessionError> {
    state.with_session(|session| {
        session.pending = None;
        Ok(())
    })
}

/// Dificultad de la partitura abierta, de 0 a 100.
///
/// # Errors
///
/// Falla si no hay sesión abierta.
#[tauri::command]
pub fn session_difficulty(state: tauri::State<'_, AppState>) -> Result<f32, SessionError> {
    state.with_session(|session| Ok(tabs_core::difficulty::evaluate(&session.score).score))
}

/// Estado de un compás para la interfaz.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BarView {
    /// Pulsos que ya existen.
    pub beats: Vec<BeatSummary>,
    /// Pulsos que caben en el compás según su indicación.
    ///
    /// Sin este dato el cursor no sabe cuándo cambiar de compás: un compás vacío tiene
    /// cero pulsos escritos, pero sigue teniendo sitio para cuatro negras.
    pub numerator: u8,
    /// Figura que vale un pulso.
    pub denominator: u8,
    /// Si las figuras escritas ya suman el compás entero.
    ///
    /// Es lo que decide cuándo la barra espaciadora salta al compás siguiente. Sin este
    /// dato el compás crecía sin límite y no había forma de avanzar escribiendo.
    pub is_full: bool,
    /// Qué parte del compás está escrita, de 0 a 1 o más si se pasó.
    ///
    /// Al transcribir es fácil dejar un compás a medias sin darse cuenta —falta una
    /// corchea y se sigue adelante—, y entonces lo que se escribe después acaba dentro
    /// del compás incompleto. Mostrarlo evita ese lío antes de que ocurra.
    pub filled: f64,
}

/// Resumen de un pulso para la interfaz.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BeatSummary {
    /// Dónde está.
    pub addr: BeatAddr,
    /// Divisor de la redonda: 4 es negra.
    pub duration: u16,
    /// Puntillos.
    pub dots: u8,
    /// Si es silencio.
    pub is_rest: bool,
    /// Notas que suenan.
    pub notes: Vec<NoteSummary>,
}

/// Resumen de una nota para la interfaz.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NoteSummary {
    /// Cuerda, 1 es la más aguda.
    pub string: u8,
    /// Traste relativo a la cejilla.
    pub fret: u8,
    /// Técnicas activas, como máscara de bits.
    pub techniques: u32,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{AppState, Session, SessionError};
    use tabs_core::edit::{EditCommand, EditHistory};
    use tabs_core::model::{BeatAddr, Score};

    fn state_with_score() -> AppState {
        let state = AppState::default();
        *state.session.lock().unwrap() = Some(Session {
            score: Score::new("Prueba", 4),
            history: EditHistory::new(),
            pending: None,
        });
        state
    }

    fn addr(beat: u32) -> BeatAddr {
        BeatAddr {
            track: 0,
            staff: 0,
            bar: 0,
            voice: 0,
            beat,
        }
    }

    #[test]
    fn sin_sesion_las_operaciones_avisan_en_vez_de_reventar() {
        let state = AppState::default();
        let error = state.with_session(|_| Ok(())).unwrap_err();
        assert!(matches!(error, SessionError::NoSession));
    }

    #[test]
    fn editar_actualiza_el_alphatex_devuelto() {
        let state = state_with_score();

        let before = state.with_session(|session| Ok(session.view())).unwrap();
        assert!(!before.can_undo);

        let after = state
            .with_session(|session| {
                session.history.apply(
                    &mut session.score,
                    &EditCommand::SetNote {
                        addr: addr(0),
                        string: 3,
                        fret: 5,
                    },
                )?;
                Ok(session.view())
            })
            .unwrap();

        assert!(after.can_undo, "tras editar se puede deshacer");
        assert!(after.tex.contains("5.3"), "la nota aparece en el AlphaTex");
        assert_ne!(before.tex, after.tex);
    }

    #[test]
    fn una_edicion_invalida_no_deja_el_historial_a_medias() {
        let state = state_with_score();

        let error = state
            .with_session(|session| {
                session
                    .history
                    .apply(
                        &mut session.score,
                        &EditCommand::SetNote {
                            addr: addr(0),
                            string: 9,
                            fret: 0,
                        },
                    )
                    .map_err(SessionError::from)
            })
            .unwrap_err();
        assert!(matches!(error, SessionError::Edit(_)));

        let view = state.with_session(|session| Ok(session.view())).unwrap();
        assert!(
            !view.can_undo,
            "una operación fallida no entra en el historial"
        );
    }
}
