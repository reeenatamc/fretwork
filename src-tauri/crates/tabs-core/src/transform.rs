//! Motor de arreglos: hace una tablatura un poco más difícil sin desfigurarla.
//!
//! Resuelve el problema de partida: las tablaturas que circulan son o triviales o
//! imposibles, y falta el punto medio.
//!
//! Tres reglas protegen la identidad de la canción, y son lo que hace que «un poco»
//! signifique algo:
//!
//! 1. **Se reparten a lo ancho antes de insistir en un compás.** Se trabaja en pasadas:
//!    primero un arreglo por compás en los mejores sitios, y sólo si falta para el
//!    objetivo se permite un segundo. Sin esto la canción queda con un trozo recargado y
//!    el resto igual que estaba.
//! 2. **Al menos el 40 % de los pulsos queda intacto.** Sin este suelo no se obtiene la
//!    misma canción más difícil, se obtiene otra canción.
//! 3. **Nunca se quita ni se cambia una nota original.** Se adorna y se añade; reescribir
//!    la melodía sería otra cosa.
//!
//! Los arreglos activos son los de bajo riesgo: ligados, arrastres, vibrato,
//! subdivisiones y notas de paso. Los que cambian el carácter del arreglo —doblar
//! octavas, dobles cuerdas, terceras— no entran aquí todavía.

use serde::{Deserialize, Serialize};

use crate::difficulty::{evaluate, Difficulty};
use crate::model::{Beat, Duration, Note, NoteTechniques, Score, SlideOut};

/// Un arreglo aplicado, tal como se le enseña a quien toca.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppliedMove {
    /// Identificador interno del tipo de arreglo.
    pub move_id: String,
    /// Compás donde se aplicó, empezando en 1 para mostrarlo.
    pub bar: u32,
    /// Descripción en español, para la interfaz.
    pub description: String,
    /// Cuánto subió la dificultad global al aplicarlo.
    pub delta: f32,
}

/// Resultado de una transformación.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Arrangement {
    /// Dificultad antes.
    pub before: f32,
    /// Dificultad después.
    pub after: f32,
    /// Arreglos aplicados, en orden.
    pub moves: Vec<AppliedMove>,
    /// Proporción de pulsos que quedaron sin tocar, de 0 a 1.
    pub untouched_ratio: f32,
}

/// Un sitio donde cabe un arreglo concreto.
#[derive(Clone, Copy, Debug)]
struct Candidate {
    kind: MoveKind,
    bar: usize,
    beat: usize,
    /// Prioridad musical: cuanto más alta, mejor sitio. Ver [`musical_prior`].
    prior: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveKind {
    /// Dos notas contiguas en la misma cuerda, a uno o dos trastes: se ligan.
    Legato,
    /// Dos notas en la misma cuerda algo más separadas: se conectan con un arrastre.
    Slide,
    /// Una nota larga y sostenida: se le pone vibrato.
    Vibrato,
    /// Una nota larga: se parte en dos repeticiones de la misma altura.
    Subdivide,
    /// Hueco de dos o más trastes en la misma cuerda: se rellena con una nota de paso.
    PassingNote,
}

impl MoveKind {
    fn id(self) -> &'static str {
        match self {
            Self::Legato => "legato",
            Self::Slide => "slide",
            Self::Vibrato => "vibrato",
            Self::Subdivide => "subdivide",
            Self::PassingNote => "passing_note",
        }
    }

    fn describe(self) -> &'static str {
        match self {
            Self::Legato => "Ligado entre notas contiguas",
            Self::Slide => "Arrastre para conectar el cambio de posición",
            Self::Vibrato => "Vibrato sobre la nota larga",
            Self::Subdivide => "Nota larga partida en dos",
            Self::PassingNote => "Nota de paso rellenando el salto",
        }
    }
}

/// Opciones de la transformación.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Cuánto subir la dificultad, en tanto por uno. `0.15` es «un poco más difícil».
    pub target_delta: f32,
    /// Proporción mínima de pulsos que debe quedar intacta.
    pub min_untouched: f32,
    /// Cuántos arreglos puede acumular un mismo compás como mucho.
    pub max_moves_per_bar: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            target_delta: 0.15,
            min_untouched: 0.40,
            // Tres arreglos por compás como techo, y sólo si las primeras pasadas no
            // alcanzaron el objetivo. Lo que de verdad protege la canción es el suelo de
            // pulsos intactos, no dejar compases enteros en blanco.
            max_moves_per_bar: 3,
        }
    }
}

/// Hace la partitura un poco más difícil.
///
/// Devuelve la partitura modificada junto con el detalle de lo que se hizo, para que se
/// pueda revisar arreglo por arreglo antes de quedársela.
#[must_use]
pub fn embellish(score: &Score, options: Options) -> (Score, Arrangement) {
    let baseline: Difficulty = evaluate(score);
    let total_beats = count_beats(score);

    let mut working = score.clone();
    let mut applied: Vec<AppliedMove> = Vec::new();
    let mut touched_beats = 0_usize;
    // Cuántos arreglos lleva cada compás, para repartirlos en vez de amontonarlos.
    let bar_slots = score.bar_count() as usize + 1;
    let mut moves_per_bar: Vec<usize> = vec![0; bar_slots];
    // Qué tipos de arreglo lleva ya cada compás. Un compás con cuatro ligados seguidos
    // no suena a arreglo, suena a tic: la variedad es parte de que quede bien.
    let mut kinds_per_bar: Vec<Vec<MoveKind>> = vec![Vec::new(); bar_slots];

    let target = baseline.score * (1.0 + options.target_delta);
    let max_touched = ((1.0 - options.min_untouched) * total_beats as f32).floor() as usize;

    // Se trabaja en pasadas: en la primera, un arreglo por compás y sólo en los mejores
    // sitios. Si aún falta para el objetivo, otra vuelta permitiendo un segundo arreglo
    // por compás, y así.
    //
    // Repartir a lo ancho antes que insistir en un compás es lo que evita que la canción
    // quede con un trozo recargado y el resto igual que estaba.
    'passes: for pass in 0..options.max_moves_per_bar {
        if evaluate(&working).score >= target || touched_beats >= max_touched {
            break;
        }

        // Se vuelven a buscar los sitios en cada pasada: los arreglos que añaden notas
        // corren los pulsos siguientes, así que las posiciones de la pasada anterior ya
        // no valdrían.
        let mut candidates = collect_candidates(&working);
        // El orden es por criterio musical, no por cuánta dificultad añade: el objetivo
        // es que el arreglo suene bien, no que el número suba rápido.
        candidates.sort_by(|a, b| b.prior.total_cmp(&a.prior));

        for candidate in candidates {
            if touched_beats >= max_touched {
                break 'passes;
            }
            if evaluate(&working).score >= target {
                break 'passes;
            }
            // En la pasada N cada compás admite como mucho N+1 arreglos.
            if moves_per_bar.get(candidate.bar).copied().unwrap_or(0) > pass {
                continue;
            }
            // Cada tipo de arreglo, una vez por compás. Sin esto gana siempre el mismo
            // —el de mayor prioridad— y el compás acaba con cuatro ligados idénticos.
            if kinds_per_bar
                .get(candidate.bar)
                .is_some_and(|kinds| kinds.contains(&candidate.kind))
            {
                continue;
            }

            let current = evaluate(&working);
            let mut attempt = working.clone();
            if !apply_move(&mut attempt, candidate) {
                continue;
            }
            let result = evaluate(&attempt);

            // Un arreglo se juzga por lo que le hace a SU compás, no al total.
            //
            // Medirlo contra la puntuación global lo diluye entre todos los compases
            // hasta volverlo indistinguible del ruido, y entonces se descartan uno tras
            // otro arreglos que sí estaban haciendo su trabajo.
            let bar_before = current.per_bar.get(candidate.bar).copied().unwrap_or(0.0);
            let bar_after = result.per_bar.get(candidate.bar).copied().unwrap_or(0.0);
            if bar_after <= bar_before + 0.05 {
                continue;
            }

            working = attempt;
            touched_beats += 1;
            if let Some(count) = moves_per_bar.get_mut(candidate.bar) {
                *count += 1;
            }
            if let Some(kinds) = kinds_per_bar.get_mut(candidate.bar) {
                kinds.push(candidate.kind);
            }

            applied.push(AppliedMove {
                move_id: candidate.kind.id().to_owned(),
                bar: candidate.bar as u32 + 1,
                description: candidate.kind.describe().to_owned(),
                delta: result.score - current.score,
            });
        }
    }

    let after = evaluate(&working).score;
    let untouched_ratio = if total_beats == 0 {
        1.0
    } else {
        1.0 - (touched_beats as f32 / total_beats as f32)
    };

    (
        working,
        Arrangement {
            before: baseline.score,
            after,
            moves: applied,
            untouched_ratio,
        },
    )
}

/// Cuenta los pulsos con sonido de la partitura.
fn count_beats(score: &Score) -> usize {
    score
        .iter_beats()
        .filter(|(_, beat)| !beat.is_rest && !beat.notes.is_empty())
        .count()
}

/// Prioridad musical de un sitio.
///
/// Codifica dónde un guitarrista metería el adorno de forma natural: al final de una
/// frase, en tiempo débil, y nunca de entrada en el primer ni en el último compás, que
/// son los que fijan la identidad de la pieza.
fn musical_prior(score: &Score, bar: usize, beat: usize, beats_in_bar: usize) -> f32 {
    let mut prior = 0.5;

    let bar_count = score.bar_count() as usize;
    if bar == 0 || (bar_count > 1 && bar + 1 == bar_count) {
        prior -= 0.35;
    }
    // Final de frase: los últimos pulsos del compás son donde cae bien un adorno.
    if beats_in_bar > 1 && beat + 1 >= beats_in_bar {
        prior += 0.25;
    }
    // Los tiempos débiles admiten adornos mejor que los fuertes.
    if beat % 2 == 1 {
        prior += 0.15;
    }
    prior
}

/// Busca todos los sitios donde cabe algún arreglo.
fn collect_candidates(score: &Score) -> Vec<Candidate> {
    let mut candidates = Vec::new();

    let Some(staff) = score.tracks.first().and_then(|track| track.staves.first()) else {
        return candidates;
    };

    for (bar_index, bar) in staff.bars.iter().enumerate() {
        let Some(voice) = bar.voices.first() else {
            continue;
        };
        let beats = &voice.beats;

        for (beat_index, beat) in beats.iter().enumerate() {
            if beat.is_rest || beat.notes.is_empty() {
                continue;
            }

            let prior = musical_prior(score, bar_index, beat_index, beats.len());
            let mut push = |kind: MoveKind, bonus: f32| {
                candidates.push(Candidate {
                    kind,
                    bar: bar_index,
                    beat: beat_index,
                    prior: prior + bonus,
                });
            };

            // Nota larga y sola: admite vibrato o partirse en dos.
            //
            // `Duration` se ordena de larga a corta, así que «al menos una negra» se
            // escribe `<= Quarter`. Escribirlo al revés dejaba fuera justamente a las
            // negras, que son la figura más común, y el motor se quedaba sin sus arreglos
            // de más impacto.
            if beat.notes.len() == 1 && beat.duration <= Duration::Quarter {
                push(MoveKind::Vibrato, 0.10);
                push(MoveKind::Subdivide, 0.0);
            }

            // Relación con la nota siguiente en la misma cuerda.
            let Some(next) = beats.get(beat_index + 1) else {
                continue;
            };
            if next.is_rest {
                continue;
            }

            for note in &beat.notes {
                let Some(next_note) = next.notes.iter().find(|n| n.string == note.string) else {
                    continue;
                };
                let distance = note.fret.abs_diff(next_note.fret);

                match distance {
                    1..=2 if note.fret > 0 && next_note.fret > 0 => push(MoveKind::Legato, 0.20),
                    3..=7 => push(MoveKind::Slide, 0.10),
                    _ => {}
                }

                // Un salto con sitio de sobra pide una nota de paso.
                if (2..=5).contains(&distance) && beat.duration <= Duration::Quarter {
                    push(MoveKind::PassingNote, 0.05);
                }
            }
        }
    }

    candidates
}

/// Aplica un arreglo. Devuelve `false` si al final no era posible.
fn apply_move(score: &mut Score, candidate: Candidate) -> bool {
    match candidate.kind {
        MoveKind::Legato => set_technique_on_next(score, candidate, NoteTechniques::HAMMER_PULL),
        MoveKind::Vibrato => set_technique_here(score, candidate, NoteTechniques::VIBRATO),
        MoveKind::Slide => add_slide(score, candidate),
        MoveKind::Subdivide => subdivide(score, candidate),
        MoveKind::PassingNote => add_passing_note(score, candidate),
    }
}

/// Acceso a los pulsos de un compás.
fn beats_mut(score: &mut Score, bar: usize) -> Option<&mut Vec<Beat>> {
    score
        .tracks
        .first_mut()?
        .staves
        .first_mut()?
        .bars
        .get_mut(bar)?
        .voices
        .first_mut()
        .map(|voice| &mut voice.beats)
}

/// El ligado se marca en la nota de **destino**, que es la que se toca sin púa.
fn set_technique_on_next(
    score: &mut Score,
    candidate: Candidate,
    technique: NoteTechniques,
) -> bool {
    let Some(beats) = beats_mut(score, candidate.bar) else {
        return false;
    };
    let Some(next) = beats.get_mut(candidate.beat + 1) else {
        return false;
    };

    let mut changed = false;
    for note in &mut next.notes {
        if !note.techniques.contains(technique) {
            note.techniques.insert(technique);
            changed = true;
        }
    }
    changed
}

fn set_technique_here(score: &mut Score, candidate: Candidate, technique: NoteTechniques) -> bool {
    let Some(beats) = beats_mut(score, candidate.bar) else {
        return false;
    };
    let Some(beat) = beats.get_mut(candidate.beat) else {
        return false;
    };

    let mut changed = false;
    for note in &mut beat.notes {
        if !note.techniques.contains(technique) {
            note.techniques.insert(technique);
            changed = true;
        }
    }
    changed
}

fn add_slide(score: &mut Score, candidate: Candidate) -> bool {
    let Some(beats) = beats_mut(score, candidate.bar) else {
        return false;
    };
    let Some(beat) = beats.get_mut(candidate.beat) else {
        return false;
    };

    let mut changed = false;
    for note in &mut beat.notes {
        if note.slide_out.is_none() && note.fret > 0 {
            note.slide_out = Some(SlideOut::Legato);
            changed = true;
        }
    }
    changed
}

/// Parte una nota larga en dos repeticiones de la misma altura.
///
/// No cambia ninguna altura ni la duración total del compás: sólo añade un ataque, que es
/// lo que sube la dificultad.
fn subdivide(score: &mut Score, candidate: Candidate) -> bool {
    let halved = {
        let Some(beats) = beats_mut(score, candidate.bar) else {
            return false;
        };
        let Some(beat) = beats.get(candidate.beat) else {
            return false;
        };
        // Con puntillo la mitad no es exacta, así que ese caso se deja en paz.
        if beat.dots > 0 {
            return false;
        }
        match beat.duration {
            Duration::Whole => Duration::Half,
            Duration::Half => Duration::Quarter,
            Duration::Quarter => Duration::Eighth,
            Duration::Eighth => Duration::Sixteenth,
            _ => return false,
        }
    };

    let copy_id = score.next_beat_id();
    let note_ids: Vec<_> = (0..8).map(|_| score.next_note_id()).collect();

    let Some(beats) = beats_mut(score, candidate.bar) else {
        return false;
    };
    let Some(beat) = beats.get_mut(candidate.beat) else {
        return false;
    };

    beat.duration = halved;
    let mut copy = beat.clone();
    copy.id = copy_id;
    for (note, id) in copy.notes.iter_mut().zip(note_ids) {
        note.id = id;
    }

    beats.insert(candidate.beat + 1, copy);
    true
}

/// Rellena el salto hacia la nota siguiente con la nota intermedia.
///
/// La nota añadida sale de partir la actual por la mitad, así que el compás sigue
/// cuadrando exactamente y no hay que recolocar nada más.
fn add_passing_note(score: &mut Score, candidate: Candidate) -> bool {
    let (halved, string, passing_fret) = {
        let Some(beats) = beats_mut(score, candidate.bar) else {
            return false;
        };
        let Some(beat) = beats.get(candidate.beat) else {
            return false;
        };
        let Some(next) = beats.get(candidate.beat + 1) else {
            return false;
        };

        if beat.dots > 0 || beat.notes.len() != 1 || next.is_rest {
            return false;
        }
        let note = &beat.notes[0];
        let Some(next_note) = next.notes.iter().find(|n| n.string == note.string) else {
            return false;
        };

        // La nota de paso va justo en medio de las dos.
        let low = note.fret.min(next_note.fret);
        let high = note.fret.max(next_note.fret);
        if high - low < 2 {
            return false;
        }
        let middle = low + (high - low) / 2;
        if middle == note.fret || middle == next_note.fret {
            return false;
        }

        let halved = match beat.duration {
            Duration::Half => Duration::Quarter,
            Duration::Quarter => Duration::Eighth,
            Duration::Eighth => Duration::Sixteenth,
            _ => return false,
        };
        (halved, note.string, middle)
    };

    let beat_id = score.next_beat_id();
    let note_id = score.next_note_id();

    let Some(beats) = beats_mut(score, candidate.bar) else {
        return false;
    };
    let Some(beat) = beats.get_mut(candidate.beat) else {
        return false;
    };
    beat.duration = halved;

    let mut passing = Beat::new(beat_id, halved);
    let mut note = Note::new(note_id, string, passing_fret);
    // La nota de paso se liga: es lo que la hace sonar como adorno y no como otra nota.
    note.techniques.insert(NoteTechniques::HAMMER_PULL);
    passing.notes.push(note);

    beats.insert(candidate.beat + 1, passing);
    true
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{embellish, Options};
    use crate::difficulty::evaluate;
    use crate::model::{Beat, BeatId, Duration, Note, NoteId, Score};

    /// Número de compases de la pieza de prueba.
    const BARS: usize = 8;

    /// Una escala que sube y baja por la tercera cuerda, ocho compases de negras.
    ///
    /// Ocho compases y no cuatro porque las reglas de reparto necesitan sitio para poder
    /// elegir; con una pieza diminuta no se distingue el criterio de la falta de opciones.
    fn scale() -> Score {
        let mut score = Score::new("Escala", BARS as u32);
        score.meta.tempo_bpm = 100.0;
        let frets = [0_u8, 2, 4, 5, 7, 9, 11, 12];

        for bar in 0..BARS {
            let beats: Vec<Beat> = (0..4)
                .map(|i| {
                    // Sube en los compases pares y baja en los impares.
                    let step = if bar % 2 == 0 { i } else { 3 - i };
                    let mut beat = Beat::new(BeatId(0), Duration::Quarter);
                    beat.notes.push(Note::new(
                        NoteId(0),
                        3,
                        frets[(bar / 2 * 2 + step) % frets.len()],
                    ));
                    beat
                })
                .collect();
            score.tracks[0].staves[0].bars[bar].voices[0].beats = beats;
        }
        score.assign_missing_ids();
        score
    }

    #[test]
    fn la_version_adornada_se_acerca_al_objetivo_pedido() {
        // Regresión: comprobar sólo `después > antes` dejaba pasar un único arreglo que
        // movía la dificultad en milésimas. La versión adornada tiene que notarse.
        let original = scale();
        let options = Options::default();
        let (_, result) = embellish(&original, options);

        let achieved = (result.after - result.before) / result.before;
        assert!(
            achieved >= options.target_delta * 0.5,
            "se pidió +{:.0} % y sólo se logró +{:.1} % ({} → {})",
            options.target_delta * 100.0,
            achieved * 100.0,
            result.before,
            result.after
        );
        assert!(
            result.moves.len() >= 3,
            "con ocho compases deberían caber varios arreglos, salieron {}",
            result.moves.len()
        );
    }

    #[test]
    fn no_se_pasa_mucho_del_objetivo() {
        // «Un poco más difícil» no puede acabar siendo el doble de difícil.
        let options = Options {
            target_delta: 0.15,
            ..Options::default()
        };
        let (_, result) = embellish(&scale(), options);
        let achieved = (result.after - result.before) / result.before;
        assert!(
            achieved <= options.target_delta * 2.0,
            "se pidió +15 % y salió +{:.0} %",
            achieved * 100.0
        );
    }

    #[test]
    fn los_arreglos_no_son_todos_del_mismo_tipo() {
        // Cuatro ligados seguidos no suenan a arreglo, suenan a tic.
        let (_, result) = embellish(&scale(), Options::default());
        let tipos: std::collections::HashSet<&str> =
            result.moves.iter().map(|m| m.move_id.as_str()).collect();
        assert!(
            tipos.len() >= 2,
            "todos los arreglos fueron del mismo tipo: {tipos:?}"
        );
    }

    #[test]
    fn ningun_compas_repite_el_mismo_tipo_de_arreglo() {
        let (_, result) = embellish(&scale(), Options::default());
        let mut vistos = std::collections::HashSet::new();
        for aplicado in &result.moves {
            assert!(
                vistos.insert((aplicado.bar, aplicado.move_id.clone())),
                "el compás {} repitió «{}»",
                aplicado.bar,
                aplicado.move_id
            );
        }
    }

    #[test]
    fn se_respeta_el_suelo_de_pulsos_intactos() {
        let (_, result) = embellish(&scale(), Options::default());
        assert!(
            result.untouched_ratio >= 0.40,
            "sólo quedó intacto el {:.0} %",
            result.untouched_ratio * 100.0
        );
    }

    #[test]
    fn los_arreglos_se_reparten_por_toda_la_pieza() {
        let (_, result) = embellish(&scale(), Options::default());

        let mut por_compas = std::collections::HashMap::new();
        for aplicado in &result.moves {
            *por_compas.entry(aplicado.bar).or_insert(0_usize) += 1;
        }

        // Ningún compás debe llevarse la mayor parte: si uno los concentra, la canción
        // queda con un trozo recargado y el resto igual que estaba.
        let maximo = por_compas.values().copied().max().unwrap_or(0);
        assert!(
            maximo <= 3,
            "un compás se llevó {maximo} arreglos: {por_compas:?}"
        );
        assert!(
            por_compas.len() >= 3,
            "los arreglos se concentraron en sólo {} compases",
            por_compas.len()
        );
    }

    #[test]
    fn el_primer_arreglo_no_cae_en_un_extremo() {
        let (_, result) = embellish(&scale(), Options::default());
        // El primer y el último compás fijan la identidad de la pieza. Con suficientes
        // arreglos acaban tocándose, pero el criterio musical debe dejarlos para después.
        let Some(primero) = result.moves.first() else {
            panic!("debería haber arreglos");
        };
        assert!(
            primero.bar != 1 && primero.bar != BARS as u32,
            "el primer arreglo cayó en un extremo: compás {}",
            primero.bar
        );
    }

    #[test]
    fn no_se_pierde_ninguna_nota_original() {
        // Adornar añade; nunca quita ni cambia lo que ya estaba.
        let original = scale();
        let (arranged, _) = embellish(&original, Options::default());

        let resultantes: Vec<(u8, u8)> = arranged
            .iter_beats()
            .flat_map(|(_, beat)| beat.notes.iter().map(|n| (n.string, n.fret)))
            .collect();

        for (_, beat) in original.iter_beats() {
            for note in &beat.notes {
                assert!(
                    resultantes.contains(&(note.string, note.fret)),
                    "se perdió la nota {}.{} de la versión original",
                    note.fret,
                    note.string
                );
            }
        }
    }

    #[test]
    fn pedir_mas_dificultad_produce_mas_arreglos() {
        let (_, poco) = embellish(
            &scale(),
            Options {
                target_delta: 0.05,
                ..Options::default()
            },
        );
        let (_, bastante) = embellish(
            &scale(),
            Options {
                target_delta: 0.30,
                ..Options::default()
            },
        );
        assert!(
            bastante.moves.len() >= poco.moves.len(),
            "un pelín: {} arreglos; bastante: {}",
            poco.moves.len(),
            bastante.moves.len()
        );
    }

    #[test]
    fn una_partitura_vacia_no_revienta() {
        let (result, arrangement) = embellish(&Score::new("Vacía", 4), Options::default());
        assert_eq!(arrangement.moves.len(), 0);
        assert!(evaluate(&result).score < f32::EPSILON);
    }

    #[test]
    fn los_compases_siguen_cuadrando_tras_adornar() {
        // Partir una nota en dos mantiene la duración total: es lo que permite que el
        // compás siga sumando exactamente lo que debe.
        let original = scale();
        let (arranged, _) = embellish(&original, Options::default());

        for bar in 0..BARS {
            let total: crate::model::Fraction = arranged.tracks[0].staves[0].bars[bar].voices[0]
                .beats
                .iter()
                .fold(crate::model::Fraction::zero(), |acc, beat| {
                    acc + beat.duration_in_whole_notes()
                });
            assert_eq!(
                total,
                crate::model::Fraction::new(4, 4),
                "el compás {} dejó de cuadrar",
                bar + 1
            );
        }
    }
}
