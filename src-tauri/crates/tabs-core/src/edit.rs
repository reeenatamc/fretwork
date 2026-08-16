//! Operaciones de edición sobre una partitura, con deshacer y rehacer.
//!
//! Cada operación devuelve **su inversa** al aplicarse. Deshacer es entonces aplicar la
//! inversa, y no hay que guardar copias enteras de la partitura: con una transcripción de
//! varios miles de pulsos, guardar una copia por cada tecla pulsada sería insostenible.
//!
//! Las direcciones son [`BeatAddr`], no índices sueltos, y los pulsos que aún no existen
//! se crean al vuelo: al transcribir se escribe hacia delante, y frenar a la persona para
//! que "cree un pulso" antes de poner una nota sería absurdo.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::{Beat, BeatAddr, Duration, Note, NoteTechniques, Score};

/// Fallos posibles al editar.
#[derive(Debug, Error, Serialize, Deserialize, PartialEq, Eq)]
pub enum EditError {
    /// La pista, pentagrama, compás o voz no existe.
    #[error("la posición {0:?} no existe en la partitura")]
    InvalidAddress(BeatAddr),
    /// La cuerda no existe en la afinación de la pista.
    #[error("la cuerda {string} no existe: el instrumento tiene {available}")]
    InvalidString {
        /// Cuerda pedida.
        string: u8,
        /// Cuerdas disponibles.
        available: u8,
    },
    /// El traste supera los del instrumento.
    #[error("el traste {fret} supera los {max} del instrumento")]
    FretOutOfRange {
        /// Traste pedido.
        fret: u8,
        /// Traste máximo.
        max: u8,
    },
    /// Se intentó quitar algo que no estaba.
    #[error("no hay nada que quitar en esa posición")]
    NothingToRemove,
}

/// Una operación de edición.
///
/// Se serializa porque el frontend las envía por IPC y porque el historial se puede
/// guardar para depurar una sesión de transcripción.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EditCommand {
    /// Pone una nota en una cuerda. Si ya había otra en esa cuerda, la sustituye.
    SetNote {
        /// Dónde.
        addr: BeatAddr,
        /// Cuerda, donde 1 es la más aguda.
        string: u8,
        /// Traste, relativo a la cejilla.
        fret: u8,
    },
    /// Quita la nota que haya en una cuerda.
    ClearString {
        /// Dónde.
        addr: BeatAddr,
        /// Cuerda a limpiar.
        string: u8,
    },
    /// Cambia la figura rítmica de un pulso.
    SetDuration {
        /// Dónde.
        addr: BeatAddr,
        /// Figura nueva.
        duration: Duration,
        /// Puntillos.
        dots: u8,
    },
    /// Convierte el pulso en silencio, o lo devuelve a nota.
    SetRest {
        /// Dónde.
        addr: BeatAddr,
        /// Si debe quedar como silencio.
        is_rest: bool,
    },
    /// Activa o desactiva una técnica sobre una nota.
    ///
    /// La técnica viaja como máscara de bits en lugar de como [`NoteTechniques`]: los
    /// `bitflags` se serializan a JSON como texto con los nombres de las banderas, y la
    /// interfaz sólo tiene números.
    SetTechnique {
        /// Dónde.
        addr: BeatAddr,
        /// Cuerda de la nota.
        string: u8,
        /// Técnica a modificar, como máscara de bits.
        technique: u32,
        /// Si se activa o se apaga.
        on: bool,
    },
    /// Inserta un pulso vacío en una posición, desplazando los siguientes.
    InsertBeat {
        /// Dónde.
        addr: BeatAddr,
        /// Figura del pulso nuevo.
        duration: Duration,
    },
    /// Quita un pulso entero.
    RemoveBeat {
        /// Dónde.
        addr: BeatAddr,
    },
    /// Varias operaciones que se deshacen juntas.
    Batch {
        /// Operaciones en orden de aplicación.
        commands: Vec<EditCommand>,
    },
}

/// Aplica una operación y devuelve la inversa, que es lo que deshace el cambio.
///
/// # Errors
///
/// Devuelve error si la posición no existe o si la nota no cabe en el instrumento.
pub fn apply(score: &mut Score, command: &EditCommand) -> Result<EditCommand, EditError> {
    match command {
        EditCommand::SetNote { addr, string, fret } => set_note(score, *addr, *string, *fret),
        EditCommand::ClearString { addr, string } => clear_string(score, *addr, *string),
        EditCommand::SetDuration {
            addr,
            duration,
            dots,
        } => set_duration(score, *addr, *duration, *dots),
        EditCommand::SetRest { addr, is_rest } => set_rest(score, *addr, *is_rest),
        EditCommand::SetTechnique {
            addr,
            string,
            technique,
            on,
        } => set_technique(score, *addr, *string, *technique, *on),
        EditCommand::InsertBeat { addr, duration } => insert_beat(score, *addr, *duration),
        EditCommand::RemoveBeat { addr } => remove_beat(score, *addr),
        EditCommand::Batch { commands } => {
            let mut inverses = Vec::with_capacity(commands.len());
            for command in commands {
                inverses.push(apply(score, command)?);
            }
            // Para deshacer un lote hay que revertir en orden inverso.
            inverses.reverse();
            Ok(EditCommand::Batch { commands: inverses })
        }
    }
}

/// Comprueba que la cuerda y el traste existen en el instrumento.
fn validate_position(score: &Score, addr: BeatAddr, string: u8, fret: u8) -> Result<(), EditError> {
    let track = score
        .tracks
        .get(addr.track as usize)
        .ok_or(EditError::InvalidAddress(addr))?;

    let available = track.tuning.string_count();
    if string == 0 || string > available {
        return Err(EditError::InvalidString { string, available });
    }

    // El traste disponible se reduce por la cejilla: con cejilla en 3 quedan menos arriba.
    let max = track.fret_count.saturating_sub(track.capo);
    if fret > max {
        return Err(EditError::FretOutOfRange { fret, max });
    }

    Ok(())
}

/// Asegura que existe el pulso en la dirección dada, creando los que falten.
///
/// Escribir hacia delante es el gesto natural al transcribir, así que poner una nota en un
/// pulso que todavía no existe lo crea, junto con los intermedios como silencios.
fn ensure_beat(
    score: &mut Score,
    addr: BeatAddr,
    duration: Duration,
) -> Result<&mut Beat, EditError> {
    // Los identificadores se reservan antes de tomar el préstamo mutable de la voz.
    let needed = {
        let voice = voice_at(score, addr).ok_or(EditError::InvalidAddress(addr))?;
        (addr.beat as usize + 1).saturating_sub(voice.beats.len())
    };

    let mut new_ids = Vec::with_capacity(needed);
    for _ in 0..needed {
        new_ids.push(score.next_beat_id());
    }

    let voice = voice_at_mut(score, addr).ok_or(EditError::InvalidAddress(addr))?;
    for id in new_ids {
        voice.beats.push(Beat::rest(id, duration));
    }

    voice
        .beats
        .get_mut(addr.beat as usize)
        .ok_or(EditError::InvalidAddress(addr))
}

fn voice_at(score: &Score, addr: BeatAddr) -> Option<&crate::model::Voice> {
    score
        .tracks
        .get(addr.track as usize)?
        .staves
        .get(addr.staff as usize)?
        .bars
        .get(addr.bar as usize)?
        .voices
        .get(addr.voice as usize)
}

fn voice_at_mut(score: &mut Score, addr: BeatAddr) -> Option<&mut crate::model::Voice> {
    score
        .tracks
        .get_mut(addr.track as usize)?
        .staves
        .get_mut(addr.staff as usize)?
        .bars
        .get_mut(addr.bar as usize)?
        .voices
        .get_mut(addr.voice as usize)
}

fn set_note(
    score: &mut Score,
    addr: BeatAddr,
    string: u8,
    fret: u8,
) -> Result<EditCommand, EditError> {
    validate_position(score, addr, string, fret)?;

    // La figura por defecto de un pulso creado al vuelo es la negra.
    let note_id = score.next_note_id();
    let beat = ensure_beat(score, addr, Duration::Quarter)?;

    // Poner una nota deja de ser silencio automáticamente.
    beat.is_rest = false;

    let previous = beat
        .notes
        .iter()
        .find(|note| note.string == string)
        .map(|note| note.fret);

    match beat.notes.iter_mut().find(|note| note.string == string) {
        Some(existing) => existing.fret = fret,
        None => beat.notes.push(Note::new(note_id, string, fret)),
    }

    Ok(match previous {
        Some(fret) => EditCommand::SetNote { addr, string, fret },
        None => EditCommand::ClearString { addr, string },
    })
}

fn clear_string(score: &mut Score, addr: BeatAddr, string: u8) -> Result<EditCommand, EditError> {
    let beat = score
        .beat_at_mut(addr)
        .ok_or(EditError::InvalidAddress(addr))?;

    let position = beat
        .notes
        .iter()
        .position(|note| note.string == string)
        .ok_or(EditError::NothingToRemove)?;

    let removed = beat.notes.remove(position);

    // Un pulso sin notas es un silencio: si no, el serializador tendría que adivinarlo.
    if beat.notes.is_empty() {
        beat.is_rest = true;
    }

    Ok(EditCommand::SetNote {
        addr,
        string,
        fret: removed.fret,
    })
}

fn set_duration(
    score: &mut Score,
    addr: BeatAddr,
    duration: Duration,
    dots: u8,
) -> Result<EditCommand, EditError> {
    let beat = ensure_beat(score, addr, duration)?;
    let inverse = EditCommand::SetDuration {
        addr,
        duration: beat.duration,
        dots: beat.dots,
    };
    beat.duration = duration;
    beat.dots = dots;
    Ok(inverse)
}

fn set_rest(score: &mut Score, addr: BeatAddr, is_rest: bool) -> Result<EditCommand, EditError> {
    let beat = ensure_beat(score, addr, Duration::Quarter)?;
    let inverse = EditCommand::SetRest {
        addr,
        is_rest: beat.is_rest,
    };
    beat.is_rest = is_rest;
    if is_rest {
        beat.notes.clear();
    }
    Ok(inverse)
}

fn set_technique(
    score: &mut Score,
    addr: BeatAddr,
    string: u8,
    technique: u32,
    on: bool,
) -> Result<EditCommand, EditError> {
    let flags = NoteTechniques::from_bits_truncate(technique);
    let beat = score
        .beat_at_mut(addr)
        .ok_or(EditError::InvalidAddress(addr))?;
    let note = beat
        .notes
        .iter_mut()
        .find(|note| note.string == string)
        .ok_or(EditError::NothingToRemove)?;

    let was_on = note.techniques.contains(flags);
    note.techniques.set(flags, on);

    Ok(EditCommand::SetTechnique {
        addr,
        string,
        technique,
        on: was_on,
    })
}

fn insert_beat(
    score: &mut Score,
    addr: BeatAddr,
    duration: Duration,
) -> Result<EditCommand, EditError> {
    let id = score.next_beat_id();
    let voice = voice_at_mut(score, addr).ok_or(EditError::InvalidAddress(addr))?;

    let position = (addr.beat as usize).min(voice.beats.len());
    voice.beats.insert(position, Beat::rest(id, duration));

    Ok(EditCommand::RemoveBeat { addr })
}

fn remove_beat(score: &mut Score, addr: BeatAddr) -> Result<EditCommand, EditError> {
    let voice = voice_at_mut(score, addr).ok_or(EditError::InvalidAddress(addr))?;

    let position = addr.beat as usize;
    if position >= voice.beats.len() {
        return Err(EditError::NothingToRemove);
    }

    let removed = voice.beats.remove(position);

    // Reponer el pulso exige devolverlo con sus notas, así que la inversa es un lote:
    // primero se inserta el hueco y luego se rellena.
    let mut commands = vec![EditCommand::InsertBeat {
        addr,
        duration: removed.duration,
    }];
    if removed.duration != Duration::Quarter || removed.dots > 0 {
        commands.push(EditCommand::SetDuration {
            addr,
            duration: removed.duration,
            dots: removed.dots,
        });
    }
    for note in &removed.notes {
        commands.push(EditCommand::SetNote {
            addr,
            string: note.string,
            fret: note.fret,
        });
        if !note.techniques.is_empty() {
            commands.push(EditCommand::SetTechnique {
                addr,
                string: note.string,
                technique: note.techniques.bits(),
                on: true,
            });
        }
    }
    if removed.is_rest {
        commands.push(EditCommand::SetRest {
            addr,
            is_rest: true,
        });
    }

    Ok(EditCommand::Batch { commands })
}

// ─────────────────────────────────────────────────────────── Historial

/// Pila de deshacer y rehacer.
#[derive(Default, Debug)]
pub struct EditHistory {
    undo_stack: Vec<EditCommand>,
    redo_stack: Vec<EditCommand>,
}

impl EditHistory {
    /// Crea un historial vacío.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Aplica una operación y la registra para poder deshacerla.
    ///
    /// # Errors
    ///
    /// Propaga el error de la operación sin tocar el historial.
    pub fn apply(&mut self, score: &mut Score, command: &EditCommand) -> Result<(), EditError> {
        let inverse = apply(score, command)?;
        self.undo_stack.push(inverse);
        // Una edición nueva invalida lo que hubiera para rehacer.
        self.redo_stack.clear();
        Ok(())
    }

    /// Deshace la última operación. Devuelve `false` si no había nada que deshacer.
    ///
    /// # Errors
    ///
    /// Devuelve error si la inversa no se puede aplicar, señal de que el historial y la
    /// partitura se desincronizaron.
    pub fn undo(&mut self, score: &mut Score) -> Result<bool, EditError> {
        let Some(command) = self.undo_stack.pop() else {
            return Ok(false);
        };
        let inverse = apply(score, &command)?;
        self.redo_stack.push(inverse);
        Ok(true)
    }

    /// Rehace la última operación deshecha. Devuelve `false` si no había nada.
    ///
    /// # Errors
    ///
    /// Igual que [`EditHistory::undo`].
    pub fn redo(&mut self, score: &mut Score) -> Result<bool, EditError> {
        let Some(command) = self.redo_stack.pop() else {
            return Ok(false);
        };
        let inverse = apply(score, &command)?;
        self.undo_stack.push(inverse);
        Ok(true)
    }

    /// ¿Hay algo que deshacer?
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// ¿Hay algo que rehacer?
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{apply, EditCommand, EditError, EditHistory};
    use crate::model::{BeatAddr, Duration, NoteTechniques, Score};

    fn addr(bar: u32, beat: u32) -> BeatAddr {
        BeatAddr {
            track: 0,
            staff: 0,
            bar,
            voice: 0,
            beat,
        }
    }

    fn score() -> Score {
        Score::new("Prueba", 4)
    }

    #[test]
    fn poner_una_nota_crea_el_pulso_que_falte() {
        let mut score = score();
        // El pulso 2 no existe todavía: escribir hacia delante debe crearlo.
        apply(
            &mut score,
            &EditCommand::SetNote {
                addr: addr(0, 2),
                string: 3,
                fret: 5,
            },
        )
        .unwrap();

        let beat = score.beat_at(addr(0, 2)).expect("el pulso debería existir");
        assert_eq!(beat.notes.len(), 1);
        assert_eq!(beat.notes[0].fret, 5);
        assert!(!beat.is_rest, "poner una nota deja de ser silencio");

        // Los pulsos intermedios se crean como silencios.
        assert!(score.beat_at(addr(0, 0)).unwrap().is_rest);
        assert!(score.beat_at(addr(0, 1)).unwrap().is_rest);
    }

    #[test]
    fn poner_dos_notas_en_la_misma_cuerda_sustituye() {
        let mut score = score();
        let at = addr(0, 0);
        apply(
            &mut score,
            &EditCommand::SetNote {
                addr: at,
                string: 3,
                fret: 5,
            },
        )
        .unwrap();
        apply(
            &mut score,
            &EditCommand::SetNote {
                addr: at,
                string: 3,
                fret: 7,
            },
        )
        .unwrap();

        let beat = score.beat_at(at).unwrap();
        assert_eq!(
            beat.notes.len(),
            1,
            "una cuerda no puede sonar dos veces a la vez"
        );
        assert_eq!(beat.notes[0].fret, 7);
    }

    #[test]
    fn deshacer_devuelve_la_nota_anterior() {
        let mut score = score();
        let mut history = EditHistory::new();
        let at = addr(0, 0);

        history
            .apply(
                &mut score,
                &EditCommand::SetNote {
                    addr: at,
                    string: 3,
                    fret: 5,
                },
            )
            .unwrap();
        history
            .apply(
                &mut score,
                &EditCommand::SetNote {
                    addr: at,
                    string: 3,
                    fret: 7,
                },
            )
            .unwrap();
        assert_eq!(score.beat_at(at).unwrap().notes[0].fret, 7);

        assert!(history.undo(&mut score).unwrap());
        assert_eq!(
            score.beat_at(at).unwrap().notes[0].fret,
            5,
            "vuelve al traste anterior"
        );

        assert!(history.undo(&mut score).unwrap());
        assert!(
            score.beat_at(at).unwrap().notes.is_empty(),
            "vuelve a no haber nota"
        );
    }

    #[test]
    fn rehacer_reaplica_lo_deshecho() {
        let mut score = score();
        let mut history = EditHistory::new();
        let at = addr(0, 0);

        history
            .apply(
                &mut score,
                &EditCommand::SetNote {
                    addr: at,
                    string: 2,
                    fret: 3,
                },
            )
            .unwrap();
        history.undo(&mut score).unwrap();
        assert!(score.beat_at(at).unwrap().notes.is_empty());

        assert!(history.redo(&mut score).unwrap());
        assert_eq!(score.beat_at(at).unwrap().notes[0].fret, 3);
    }

    #[test]
    fn una_edicion_nueva_borra_lo_que_habia_para_rehacer() {
        let mut score = score();
        let mut history = EditHistory::new();

        history
            .apply(
                &mut score,
                &EditCommand::SetNote {
                    addr: addr(0, 0),
                    string: 1,
                    fret: 1,
                },
            )
            .unwrap();
        history.undo(&mut score).unwrap();
        assert!(history.can_redo());

        history
            .apply(
                &mut score,
                &EditCommand::SetNote {
                    addr: addr(0, 0),
                    string: 1,
                    fret: 9,
                },
            )
            .unwrap();
        assert!(!history.can_redo(), "la rama deshecha ya no tiene sentido");
    }

    #[test]
    fn deshacer_sin_historial_no_falla() {
        let mut score = score();
        let mut history = EditHistory::new();
        assert!(
            !history.undo(&mut score).unwrap(),
            "devuelve false, no error"
        );
        assert!(!history.can_undo());
    }

    #[test]
    fn quitar_la_ultima_nota_deja_un_silencio() {
        let mut score = score();
        let at = addr(0, 0);
        apply(
            &mut score,
            &EditCommand::SetNote {
                addr: at,
                string: 4,
                fret: 2,
            },
        )
        .unwrap();
        apply(
            &mut score,
            &EditCommand::ClearString {
                addr: at,
                string: 4,
            },
        )
        .unwrap();

        let beat = score.beat_at(at).unwrap();
        assert!(beat.notes.is_empty());
        assert!(beat.is_rest, "un pulso sin notas es un silencio");
    }

    #[test]
    fn no_se_aceptan_cuerdas_inexistentes() {
        let mut score = score();
        let error = apply(
            &mut score,
            &EditCommand::SetNote {
                addr: addr(0, 0),
                string: 7,
                fret: 0,
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            EditError::InvalidString {
                string: 7,
                available: 6
            }
        );
    }

    #[test]
    fn no_se_aceptan_trastes_fuera_del_mastil() {
        let mut score = score();
        let error = apply(
            &mut score,
            &EditCommand::SetNote {
                addr: addr(0, 0),
                string: 1,
                fret: 30,
            },
        )
        .unwrap_err();
        assert_eq!(error, EditError::FretOutOfRange { fret: 30, max: 22 });
    }

    #[test]
    fn la_cejilla_reduce_los_trastes_disponibles() {
        let mut score = score();
        score.tracks[0].capo = 5;
        // Con cejilla en 5, quedan 17 trastes por encima en un mástil de 22.
        apply(
            &mut score,
            &EditCommand::SetNote {
                addr: addr(0, 0),
                string: 1,
                fret: 17,
            },
        )
        .unwrap();
        let error = apply(
            &mut score,
            &EditCommand::SetNote {
                addr: addr(0, 0),
                string: 1,
                fret: 18,
            },
        )
        .unwrap_err();
        assert_eq!(error, EditError::FretOutOfRange { fret: 18, max: 17 });
    }

    #[test]
    fn las_tecnicas_se_activan_y_se_deshacen() {
        let mut score = score();
        let mut history = EditHistory::new();
        let at = addr(0, 0);

        history
            .apply(
                &mut score,
                &EditCommand::SetNote {
                    addr: at,
                    string: 3,
                    fret: 7,
                },
            )
            .unwrap();
        history
            .apply(
                &mut score,
                &EditCommand::SetTechnique {
                    addr: at,
                    string: 3,
                    technique: NoteTechniques::HAMMER_PULL.bits(),
                    on: true,
                },
            )
            .unwrap();

        assert!(score.beat_at(at).unwrap().notes[0]
            .techniques
            .contains(NoteTechniques::HAMMER_PULL));

        history.undo(&mut score).unwrap();
        assert!(!score.beat_at(at).unwrap().notes[0]
            .techniques
            .contains(NoteTechniques::HAMMER_PULL));
    }

    #[test]
    fn quitar_un_pulso_y_deshacerlo_devuelve_sus_notas() {
        let mut score = score();
        let mut history = EditHistory::new();
        let at = addr(0, 0);

        // Un acorde de tres notas con una figura que no es la de por defecto.
        for (string, fret) in [(3_u8, 0_u8), (2, 1), (1, 0)] {
            history
                .apply(
                    &mut score,
                    &EditCommand::SetNote {
                        addr: at,
                        string,
                        fret,
                    },
                )
                .unwrap();
        }
        history
            .apply(
                &mut score,
                &EditCommand::SetDuration {
                    addr: at,
                    duration: Duration::Half,
                    dots: 1,
                },
            )
            .unwrap();

        history
            .apply(&mut score, &EditCommand::RemoveBeat { addr: at })
            .unwrap();
        history.undo(&mut score).unwrap();

        let beat = score.beat_at(at).expect("el pulso debe volver");
        assert_eq!(beat.notes.len(), 3, "vuelven las tres notas");
        assert_eq!(beat.duration, Duration::Half);
        assert_eq!(beat.dots, 1, "vuelve el puntillo");
    }

    #[test]
    fn insertar_un_pulso_desplaza_los_siguientes() {
        let mut score = score();
        let at = addr(0, 0);
        apply(
            &mut score,
            &EditCommand::SetNote {
                addr: at,
                string: 1,
                fret: 1,
            },
        )
        .unwrap();
        apply(
            &mut score,
            &EditCommand::SetNote {
                addr: addr(0, 1),
                string: 1,
                fret: 2,
            },
        )
        .unwrap();

        apply(
            &mut score,
            &EditCommand::InsertBeat {
                addr: at,
                duration: Duration::Eighth,
            },
        )
        .unwrap();

        assert!(
            score.beat_at(at).unwrap().is_rest,
            "el hueco nuevo va al principio"
        );
        assert_eq!(
            score.beat_at(addr(0, 1)).unwrap().notes[0].fret,
            1,
            "lo anterior se corrió"
        );
        assert_eq!(score.beat_at(addr(0, 2)).unwrap().notes[0].fret, 2);
    }

    #[test]
    fn un_lote_se_deshace_de_una_vez_y_en_orden_inverso() {
        let mut score = score();
        let mut history = EditHistory::new();
        let at = addr(0, 0);

        history
            .apply(
                &mut score,
                &EditCommand::Batch {
                    commands: vec![
                        EditCommand::SetNote {
                            addr: at,
                            string: 3,
                            fret: 0,
                        },
                        EditCommand::SetNote {
                            addr: at,
                            string: 2,
                            fret: 1,
                        },
                        EditCommand::SetNote {
                            addr: at,
                            string: 1,
                            fret: 0,
                        },
                    ],
                },
            )
            .unwrap();
        assert_eq!(score.beat_at(at).unwrap().notes.len(), 3);

        history.undo(&mut score).unwrap();
        assert!(
            score.beat_at(at).unwrap().notes.is_empty(),
            "el acorde entero se deshace junto"
        );
    }

    #[test]
    fn las_operaciones_se_deserializan_desde_el_json_que_manda_la_interfaz() {
        // Regresión: `Duration` se serializaba como "Quarter" y las técnicas como texto,
        // así que la interfaz mandaba números y el lote entero fallaba antes de aplicarse.
        // El síntoma era desconcertante: escribir no hacía absolutamente nada.
        let json = r#"[
            {"kind":"set_note","addr":{"track":0,"staff":0,"bar":0,"voice":0,"beat":0},
             "string":3,"fret":5},
            {"kind":"set_duration","addr":{"track":0,"staff":0,"bar":0,"voice":0,"beat":0},
             "duration":8,"dots":1},
            {"kind":"set_technique","addr":{"track":0,"staff":0,"bar":0,"voice":0,"beat":0},
             "string":3,"technique":1,"on":true}
        ]"#;

        let commands: Vec<EditCommand> =
            serde_json::from_str(json).expect("la interfaz manda números, no nombres");

        let mut score = score();
        for command in &commands {
            apply(&mut score, command).unwrap();
        }

        let beat = score.beat_at(addr(0, 0)).unwrap();
        assert_eq!(
            beat.duration,
            Duration::Eighth,
            "la figura llegó como número"
        );
        assert_eq!(beat.dots, 1);
        assert!(beat.notes[0]
            .techniques
            .contains(NoteTechniques::HAMMER_PULL));
    }

    #[test]
    fn una_figura_invalida_se_rechaza_al_deserializar() {
        let json = r#"{"kind":"set_duration","addr":{"track":0,"staff":0,"bar":0,"voice":0,
                       "beat":0},"duration":5,"dots":0}"#;
        assert!(
            serde_json::from_str::<EditCommand>(json).is_err(),
            "1/5 no es una figura rítmica"
        );
    }

    #[test]
    fn una_direccion_invalida_da_error_y_no_toca_la_partitura() {
        let mut score = score();
        let bad = BeatAddr {
            track: 9,
            staff: 0,
            bar: 0,
            voice: 0,
            beat: 0,
        };
        let error = apply(
            &mut score,
            &EditCommand::SetNote {
                addr: bad,
                string: 1,
                fret: 0,
            },
        )
        .unwrap_err();
        assert_eq!(error, EditError::InvalidAddress(bad));
    }
}
