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
        Ok(BarView {
            beats: summary,
            numerator: signature.numerator,
            denominator: signature.denominator,
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
        crate::storage::save(&root, &session.score)
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
