//! Puntuación de dificultad de una tablatura.
//!
//! Da un número de 0 a 100 para poder decir «un 15 % más difícil» y que signifique algo.
//!
//! Los rasgos se aplastan con `sat(x) = x / (x + k)` antes de combinarlos, para que
//! ninguno pueda dominar el resultado por sí solo: sin eso, un pasaje muy rápido taparía
//! por completo lo demás.
//!
//! La puntuación de la canción **no es la media** de sus compases, sino una media
//! potencial: una pieza es tan difícil como su compás más duro que no puedes saltarte, y
//! promediar disolvería justo eso.
//!
//! Los pesos de aquí son un punto de partida razonable, no una verdad medida. La
//! calibración de verdad —comparaciones por pares contra el criterio de quien toca— es
//! trabajo pendiente; hasta entonces el número sirve para comparar versiones de la misma
//! canción, que es para lo que lo usa el motor de arreglos.

use crate::model::{Beat, NoteTechniques, Score};

/// Exponente de la media potencial. Cuanto más alto, más pesan los compases duros.
const HARDNESS_EXPONENT: f32 = 4.0;

/// Dificultad de una partitura y su desglose por compás.
#[derive(Debug, Clone)]
pub struct Difficulty {
    /// Puntuación global, de 0 a 100.
    pub score: f32,
    /// Puntuación de cada compás, en orden.
    pub per_bar: Vec<f32>,
}

/// Peso de cada técnica. Un vibrato cuesta más que un apagado con la palma.
fn technique_weight(techniques: NoteTechniques) -> f32 {
    let mut weight = 0.0;
    if techniques.contains(NoteTechniques::HAMMER_PULL) {
        weight += 0.15;
    }
    if techniques.contains(NoteTechniques::VIBRATO) {
        weight += 0.20;
    }
    if techniques.contains(NoteTechniques::VIBRATO_WIDE) {
        weight += 0.30;
    }
    if techniques.contains(NoteTechniques::PALM_MUTE) {
        weight += 0.10;
    }
    if techniques.contains(NoteTechniques::GHOST) {
        weight += 0.15;
    }
    if techniques.contains(NoteTechniques::DEAD) {
        weight += 0.10;
    }
    if techniques.contains(NoteTechniques::STACCATO) {
        weight += 0.10;
    }
    weight
}

/// Aplasta un rasgo al rango [0, 1). `k` es el valor que se considera «medianamente duro».
fn sat(value: f32, k: f32) -> f32 {
    value / (value + k)
}

/// Traste ancla de un pulso: el más grave de los pisados.
///
/// Las cuerdas al aire no cuentan, porque no ocupan ningún dedo ni obligan a mover la mano.
fn anchor(beat: &Beat) -> Option<u8> {
    beat.notes
        .iter()
        .filter(|n| n.fret > 0)
        .map(|n| n.fret)
        .min()
}

/// Rasgos de un compás.
struct BarFeatures {
    /// Notas por segundo.
    density: f32,
    /// Apertura de mano en trastes.
    span: f32,
    /// Cambios de posición por segundo.
    shifts: f32,
    /// Peso de técnicas por nota.
    techniques: f32,
    /// Complejidad de los acordes.
    chords: f32,
}

impl BarFeatures {
    /// Combina los rasgos en una puntuación de 0 a 100.
    fn score(&self) -> f32 {
        // Los pesos suman 1. Reparto pensado en qué frena de verdad a quien toca.
        let value = 0.26 * sat(self.density, 6.0)
            + 0.20 * sat(self.span, 4.0)
            + 0.18 * sat(self.shifts, 3.0)
            + 0.20 * sat(self.techniques, 0.8)
            + 0.16 * sat(self.chords, 2.5);
        (value * 100.0).clamp(0.0, 100.0)
    }
}

/// Extrae los rasgos de un compás.
fn features(beats: &[Beat], seconds: f32) -> BarFeatures {
    let sounding: Vec<&Beat> = beats
        .iter()
        .filter(|b| !b.is_rest && !b.notes.is_empty())
        .collect();

    if sounding.is_empty() || seconds <= 0.0 {
        return BarFeatures {
            density: 0.0,
            span: 0.0,
            shifts: 0.0,
            techniques: 0.0,
            chords: 0.0,
        };
    }

    // Un acorde es un solo ataque, pero cada nota extra añade algo de trabajo.
    let onsets = sounding.len() as f32;
    let extra_notes: f32 = sounding
        .iter()
        .map(|b| (b.notes.len().saturating_sub(1)) as f32 * 0.35)
        .sum();
    let density = (onsets + extra_notes) / seconds;

    // Apertura: la mayor distancia entre trastes pisados dentro de un mismo pulso.
    let span = sounding
        .iter()
        .filter_map(|beat| {
            let frets: Vec<u8> = beat
                .notes
                .iter()
                .filter(|n| n.fret > 0)
                .map(|n| n.fret)
                .collect();
            let max = frets.iter().copied().max()?;
            let min = frets.iter().copied().min()?;
            Some(f32::from(max - min))
        })
        .fold(0.0_f32, f32::max);

    // Cambios de posición: sólo cuentan los saltos de más de un traste.
    let anchors: Vec<u8> = sounding.iter().filter_map(|b| anchor(b)).collect();
    let shift_total: f32 = anchors
        .windows(2)
        .map(|pair| {
            let distance = f32::from(pair[0].abs_diff(pair[1]));
            (distance - 1.0).max(0.0)
        })
        .sum();
    let shifts = shift_total / seconds;

    let technique_total: f32 = sounding
        .iter()
        .flat_map(|b| b.notes.iter())
        .map(|n| technique_weight(n.techniques))
        .sum();
    let techniques = technique_total / onsets;

    // Acordes: cada nota pisada de más cuesta un dedo más.
    let chord_total: f32 = sounding
        .iter()
        .map(|beat| {
            let fretted = beat.notes.iter().filter(|n| n.fret > 0).count();
            0.6 * (fretted.saturating_sub(1)) as f32
        })
        .sum();
    let chords = chord_total / onsets;

    BarFeatures {
        density,
        span,
        shifts,
        techniques,
        chords,
    }
}

/// Calcula la dificultad de una partitura.
#[must_use]
pub fn evaluate(score: &Score) -> Difficulty {
    let bpm = if score.meta.tempo_bpm > 0.0 {
        score.meta.tempo_bpm
    } else {
        90.0
    };

    let Some(track) = score.tracks.first() else {
        return Difficulty {
            score: 0.0,
            per_bar: Vec::new(),
        };
    };
    let Some(staff) = track.staves.first() else {
        return Difficulty {
            score: 0.0,
            per_bar: Vec::new(),
        };
    };

    let mut per_bar = Vec::with_capacity(staff.bars.len());
    let mut weights = Vec::with_capacity(staff.bars.len());

    for (index, bar) in staff.bars.iter().enumerate() {
        let signature = score
            .master_bars
            .get(index)
            .map_or_else(Default::default, |master| master.time_signature);

        // Duración del compás en segundos, según su indicación y el tempo.
        let beats_per_bar = f32::from(signature.numerator) * 4.0 / f32::from(signature.denominator);
        let seconds = beats_per_bar * 60.0 / bpm;

        let beats = bar
            .voices
            .first()
            .map_or(&[][..], |voice| voice.beats.as_slice());
        per_bar.push(features(beats, seconds).score());
        // Un compás que dura más pesa más en el total.
        weights.push(seconds.max(0.01));
    }

    Difficulty {
        score: aggregate(&per_bar, &weights),
        per_bar,
    }
}

/// Combina las puntuaciones por compás con una media potencial.
///
/// Los pasajes duros pesan mucho más que los fáciles, sin que un único compás se lleve
/// todo el resultado. Se mezcla con algo de media aritmética para que la longitud y la
/// densidad general sigan contando.
fn aggregate(per_bar: &[f32], weights: &[f32]) -> f32 {
    if per_bar.is_empty() {
        return 0.0;
    }

    let total_weight: f32 = weights.iter().sum();
    if total_weight <= 0.0 {
        return 0.0;
    }

    let soft: f32 = (per_bar
        .iter()
        .zip(weights)
        .map(|(value, weight)| weight * (value / 100.0).powf(HARDNESS_EXPONENT))
        .sum::<f32>()
        / total_weight)
        .powf(1.0 / HARDNESS_EXPONENT);

    let mean: f32 = per_bar
        .iter()
        .zip(weights)
        .map(|(value, weight)| weight * value / 100.0)
        .sum::<f32>()
        / total_weight;

    (100.0 * (0.75 * soft + 0.25 * mean)).clamp(0.0, 100.0)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::evaluate;
    use crate::model::{Beat, BeatId, Duration, Note, NoteId, NoteTechniques, Score};

    /// Partitura de un compás con los pulsos indicados, a 90 bpm.
    fn score_with(beats: Vec<Beat>) -> Score {
        let mut score = Score::new("Prueba", 1);
        score.meta.tempo_bpm = 90.0;
        score.tracks[0].staves[0].bars[0].voices[0].beats = beats;
        score
    }

    fn note_beat(duration: Duration, string: u8, fret: u8) -> Beat {
        let mut beat = Beat::new(BeatId(1), duration);
        beat.notes.push(Note::new(NoteId(1), string, fret));
        beat
    }

    #[test]
    fn un_compas_vacio_no_tiene_dificultad() {
        assert!(evaluate(&Score::new("Prueba", 4)).score < f32::EPSILON);
    }

    #[test]
    fn una_redonda_sola_es_muy_facil() {
        let score = score_with(vec![note_beat(Duration::Whole, 1, 0)]);
        assert!(
            evaluate(&score).score < 15.0,
            "salió {}",
            evaluate(&score).score
        );
    }

    #[test]
    fn las_semicorcheas_rapidas_son_mucho_mas_dificiles_que_las_negras() {
        let lentas = score_with((0..4).map(|_| note_beat(Duration::Quarter, 3, 5)).collect());
        let rapidas = score_with(
            (0..16)
                .map(|_| note_beat(Duration::Sixteenth, 3, 5))
                .collect(),
        );
        assert!(
            evaluate(&rapidas).score > evaluate(&lentas).score * 1.5,
            "lentas {} vs rápidas {}",
            evaluate(&lentas).score,
            evaluate(&rapidas).score
        );
    }

    #[test]
    fn abrir_la_mano_sube_la_dificultad() {
        let cerrada = score_with(vec![{
            let mut beat = Beat::new(BeatId(1), Duration::Quarter);
            beat.notes.push(Note::new(NoteId(1), 1, 5));
            beat.notes.push(Note::new(NoteId(2), 2, 6));
            beat
        }]);
        let abierta = score_with(vec![{
            let mut beat = Beat::new(BeatId(1), Duration::Quarter);
            beat.notes.push(Note::new(NoteId(1), 1, 5));
            beat.notes.push(Note::new(NoteId(2), 2, 12));
            beat
        }]);
        assert!(evaluate(&abierta).score > evaluate(&cerrada).score);
    }

    #[test]
    fn cambiar_de_posicion_sube_la_dificultad() {
        let quieta = score_with((0..4).map(|_| note_beat(Duration::Quarter, 3, 5)).collect());
        let saltarina = score_with(
            [5_u8, 15, 3, 17]
                .iter()
                .map(|&fret| note_beat(Duration::Quarter, 3, fret))
                .collect(),
        );
        assert!(evaluate(&saltarina).score > evaluate(&quieta).score);
    }

    #[test]
    fn las_tecnicas_suben_la_dificultad() {
        let limpia = score_with((0..4).map(|_| note_beat(Duration::Eighth, 3, 5)).collect());
        let adornada = score_with(
            (0..4)
                .map(|_| {
                    let mut beat = note_beat(Duration::Eighth, 3, 5);
                    beat.notes[0].techniques =
                        NoteTechniques::HAMMER_PULL | NoteTechniques::VIBRATO;
                    beat
                })
                .collect(),
        );
        assert!(evaluate(&adornada).score > evaluate(&limpia).score);
    }

    #[test]
    fn las_cuerdas_al_aire_no_cuentan_como_apertura() {
        // Un mi menor al aire no exige abrir la mano, aunque suenen seis cuerdas.
        let mut beat = Beat::new(BeatId(1), Duration::Whole);
        for (string, fret) in [(6_u8, 0_u8), (5, 2), (4, 2), (3, 0), (2, 0), (1, 0)] {
            beat.notes.push(Note::new(NoteId(1), string, fret));
        }
        let score = score_with(vec![beat]);
        assert!(
            evaluate(&score).score < 30.0,
            "salió {}",
            evaluate(&score).score
        );
    }

    #[test]
    fn el_compas_mas_duro_pesa_mas_que_el_promedio() {
        // Una pieza fácil con un compás imposible no es una pieza fácil.
        let mut score = Score::new("Prueba", 4);
        score.meta.tempo_bpm = 120.0;
        for bar in 0..3 {
            score.tracks[0].staves[0].bars[bar].voices[0].beats =
                (0..4).map(|_| note_beat(Duration::Quarter, 3, 5)).collect();
        }
        score.tracks[0].staves[0].bars[3].voices[0].beats = (0..16)
            .map(|i| note_beat(Duration::Sixteenth, 3, if i % 2 == 0 { 3 } else { 15 }))
            .collect();

        let result = evaluate(&score);
        let media: f32 = result.per_bar.iter().sum::<f32>() / result.per_bar.len() as f32;
        assert!(
            result.score > media,
            "global {} debería superar la media {media}",
            result.score
        );
    }
}
