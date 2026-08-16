//! tabs-repo — capturador, repositorio e impresor de tablaturas.
//!
//! Hito M0: comandos mínimos para verificar que el puente IPC funciona y para
//! que el arnés de diagnóstico deje sus resultados en disco. Escribir el informe
//! permite comprobar la build empaquetada sin depender de mirar la ventana.

pub mod session;
pub mod storage;

use std::path::PathBuf;

use tabs_core::alphatex::to_alphatex;
use tabs_core::Score;

/// Prueba de ida y vuelta entre el webview y Rust.
#[tauri::command]
#[must_use]
fn ping(message: &str) -> String {
    format!("pong: {message}")
}

/// Crea una partitura vacía con el título y número de compases indicados.
#[tauri::command]
#[must_use]
fn new_score(title: String, bar_count: u32) -> Score {
    Score::new(title, bar_count)
}

/// Convierte una partitura a AlphaTex, que es lo que alphaTab sabe renderizar.
// Tauri deserializa los argumentos por valor; no se puede recibir por referencia.
#[tauri::command]
#[must_use]
#[allow(clippy::needless_pass_by_value)]
fn render_alphatex(score: Score) -> String {
    to_alphatex(&score)
}

/// Averigua dónde guardar las tablaturas.
///
/// El objetivo es escribir dentro del repositorio, para que las canciones queden
/// versionadas en git. Se busca en este orden:
///
/// 1. La variable `TABS_REPO_ROOT`, que permite fijarlo explícitamente.
/// 2. Subiendo desde el ejecutable hasta encontrar una carpeta con `src-tauri`, que es
///    como se ve el repositorio cuando la app corre desde `target/release`.
/// 3. Como último recurso, la carpeta de datos del usuario: es mejor guardar en un sitio
///    poco elegante que perder una transcripción.
fn resolve_repo_root() -> PathBuf {
    if let Ok(explicit) = std::env::var("TABS_REPO_ROOT") {
        let path = PathBuf::from(explicit);
        if path.is_dir() {
            return path;
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        let mut current = exe.parent();
        while let Some(dir) = current {
            if dir.join("src-tauri").is_dir() {
                return dir.to_path_buf();
            }
            current = dir.parent();
        }
    }

    std::env::temp_dir().join("tabs-repo")
}

/// Indica si el proceso se lanzó en modo autocomprobación (`TABS_SELFTEST=1`).
#[tauri::command]
#[must_use]
fn is_selftest() -> bool {
    std::env::var("TABS_SELFTEST").is_ok_and(|value| value == "1")
}

/// Ruta donde el arnés deja su informe, junto al ejecutable en curso.
fn diagnostics_path() -> PathBuf {
    std::env::temp_dir().join("tabs-repo-m0-diagnostics.json")
}

/// Guarda el informe de diagnóstico del arnés M0 y devuelve dónde lo dejó.
///
/// # Errors
///
/// Devuelve error si el archivo no se puede escribir.
#[tauri::command]
fn save_diagnostics(report: &str) -> Result<String, String> {
    let path = diagnostics_path();
    std::fs::write(&path, report).map_err(|e| format!("no se pudo escribir el informe: {e}"))?;
    Ok(path.display().to_string())
}

/// Arranca la aplicación de escritorio.
///
/// # Panics
///
/// Entra en pánico si Tauri no consigue inicializar la ventana o el contexto,
/// situación irrecuperable en la que no hay nada mejor que hacer que abortar.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
// Único `expect` permitido del proyecto: si la ventana no arranca no hay app,
// y no existe nada más útil que hacer que abortar con un mensaje claro.
#[allow(clippy::expect_used)]
pub fn run() {
    let state = session::AppState::default();
    state.set_root(resolve_repo_root());

    tauri::Builder::default()
        .manage(state)
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            save_diagnostics,
            is_selftest,
            new_score,
            render_alphatex,
            session::session_new,
            session::session_view,
            session::session_apply,
            session::session_apply_batch,
            session::session_undo,
            session::session_redo,
            session::session_bar_notes,
            session::session_save,
            session::session_open,
            session::session_list,
            session::session_set_meta,
        ])
        .run(tauri::generate_context!())
        .expect("no se pudo inicializar la aplicación Tauri");
}

#[cfg(test)]
mod tests {
    use super::{diagnostics_path, new_score, ping, render_alphatex};

    #[test]
    fn ping_devuelve_el_mensaje_recibido() {
        assert_eq!(ping("hola"), "pong: hola");
    }

    #[test]
    fn la_ruta_del_informe_es_absoluta() {
        assert!(diagnostics_path().is_absolute());
    }

    #[test]
    fn una_partitura_nueva_se_renderiza_a_alphatex() {
        let score = new_score("Prueba".to_owned(), 4);
        assert_eq!(score.bar_count(), 4);

        let tex = render_alphatex(score);
        assert!(tex.contains("\\title \"Prueba\""));
        assert_eq!(
            tex.matches('|').count(),
            4,
            "cuatro compases, cuatro barras"
        );
    }
}
