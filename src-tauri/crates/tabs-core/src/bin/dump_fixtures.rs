//! Genera los casos de prueba del viaje de ida y vuelta a AlphaTex.
//!
//! Emite por la salida estándar un JSON con pares `{tex, expected}`. El script
//! `scripts/verify-alphatex.mjs` los parsea con el propio alphaTab y comprueba que lo
//! que entiende coincide con lo que quisimos escribir.
//!
//! Es la única forma honesta de validar el serializador: comprobarlo contra el
//! programa que de verdad va a leerlo, no contra nuestras propias suposiciones.

use serde::Serialize;
use tabs_core::alphatex::to_alphatex;
use tabs_core::model::{
    Beat, BeatId, Duration, Note, NoteId, NoteTechniques, Score, TimeSignature, Tuplet,
};

/// Un caso de prueba: el AlphaTex generado y lo que alphaTab debería entender.
#[derive(Serialize)]
struct Fixture {
    name: String,
    tex: String,
    expected: Expected,
}

/// Proyección del modelo con lo que se puede comprobar tras el viaje de ida y vuelta.
#[derive(Serialize)]
struct Expected {
    title: String,
    tempo: f32,
    tuning: Vec<u8>,
    capo: u8,
    bars: Vec<ExpectedBar>,
}

#[derive(Serialize)]
struct ExpectedBar {
    time_signature: [u8; 2],
    beats: Vec<ExpectedBeat>,
}

#[derive(Serialize)]
struct ExpectedBeat {
    /// Divisor de la redonda: 4 es negra, 8 corchea.
    duration: u16,
    dots: u8,
    is_rest: bool,
    notes: Vec<ExpectedNote>,
}

#[derive(Serialize)]
struct ExpectedNote {
    /// Cuerda con nuestra convención: 1 es la más aguda.
    string: u8,
    fret: u8,
    /// Altura que debe sonar. Es la comprobación que no depende de convenciones.
    sounding_midi: u8,
}

/// Construye la proyección esperada a partir de la propia partitura.
fn project(score: &Score) -> Expected {
    let track = &score.tracks[0];
    let staff = &track.staves[0];

    let bars = staff
        .bars
        .iter()
        .enumerate()
        .map(|(index, bar)| {
            let master = &score.master_bars[index];
            let beats = bar.voices[0]
                .beats
                .iter()
                .map(|beat| {
                    // Las notas se ordenan por cuerda: dentro de un acorde el orden no
                    // significa nada musicalmente, y así la comparación es estable.
                    let mut notes: Vec<ExpectedNote> = beat
                        .notes
                        .iter()
                        .map(|note| ExpectedNote {
                            string: note.string,
                            fret: note.fret,
                            sounding_midi: track
                                .tuning
                                .sounding_pitch(note.string, note.fret, track.capo)
                                .unwrap_or(0),
                        })
                        .collect();
                    notes.sort_by_key(|note| note.string);

                    ExpectedBeat {
                        duration: beat.duration as u16,
                        dots: beat.dots,
                        is_rest: beat.is_rest,
                        notes,
                    }
                })
                .collect();

            ExpectedBar {
                time_signature: [
                    master.time_signature.numerator,
                    master.time_signature.denominator,
                ],
                beats,
            }
        })
        .collect();

    Expected {
        title: score.meta.title.clone(),
        tempo: score.meta.tempo_bpm,
        tuning: track.tuning.midi_notes.clone(),
        capo: track.capo,
        bars,
    }
}

fn fixture(name: &str, score: &Score) -> Fixture {
    Fixture {
        name: name.to_owned(),
        tex: to_alphatex(score),
        expected: project(score),
    }
}

/// Melodía simple de una nota por pulso, en cuerdas distintas.
fn melodia_simple() -> Score {
    let mut score = Score::new("Melodía simple", 1);
    score.meta.tempo_bpm = 90.0;
    let mut beats = Vec::new();
    // Cuerda 1 (mi agudo) traste 0, cuerda 2 traste 1, cuerda 3 traste 2, cuerda 6 traste 3.
    for (string, fret) in [(1_u8, 0_u8), (2, 1), (3, 2), (6, 3)] {
        let mut beat = Beat::new(BeatId(0), Duration::Quarter);
        beat.notes.push(Note::new(NoteId(0), string, fret));
        beats.push(beat);
    }
    score.tracks[0].staves[0].bars[0].voices[0].beats = beats;
    score.assign_missing_ids();
    score
}

/// Acorde de mi menor al aire, para comprobar el orden de las cuerdas en un acorde.
fn acorde_em() -> Score {
    let mut score = Score::new("Acorde Em", 1);
    score.meta.tempo_bpm = 80.0;
    let mut beat = Beat::new(BeatId(0), Duration::Whole);
    // Em: 6ª=0, 5ª=2, 4ª=2, 3ª=0, 2ª=0, 1ª=0.
    for (string, fret) in [(6_u8, 0_u8), (5, 2), (4, 2), (3, 0), (2, 0), (1, 0)] {
        beat.notes.push(Note::new(NoteId(0), string, fret));
    }
    score.tracks[0].staves[0].bars[0].voices[0].beats = vec![beat];
    score.assign_missing_ids();
    score
}

/// Mezcla de figuras, puntillos, tresillos y silencios.
fn ritmos_variados() -> Score {
    let mut score = Score::new("Ritmos variados", 2);
    score.meta.tempo_bpm = 120.0;

    // Compás 1: negra con puntillo, corchea, dos negras.
    let mut compas1 = Vec::new();
    let mut dotted = Beat::new(BeatId(0), Duration::Quarter);
    dotted.dots = 1;
    dotted.notes.push(Note::new(NoteId(0), 3, 5));
    compas1.push(dotted);

    let mut eighth = Beat::new(BeatId(0), Duration::Eighth);
    eighth.notes.push(Note::new(NoteId(0), 3, 7));
    compas1.push(eighth);

    for fret in [5_u8, 3] {
        let mut beat = Beat::new(BeatId(0), Duration::Quarter);
        beat.notes.push(Note::new(NoteId(0), 4, fret));
        compas1.push(beat);
    }

    // Compás 2: tresillo de corcheas, silencio de negra, dos corcheas, negra.
    let mut compas2 = Vec::new();
    for fret in [0_u8, 2, 3] {
        let mut beat = Beat::new(BeatId(0), Duration::Eighth);
        beat.tuplet = Some(Tuplet {
            numerator: 3,
            denominator: 2,
        });
        beat.notes.push(Note::new(NoteId(0), 2, fret));
        compas2.push(beat);
    }
    compas2.push(Beat::rest(BeatId(0), Duration::Quarter));
    for fret in [1_u8, 0] {
        let mut beat = Beat::new(BeatId(0), Duration::Eighth);
        beat.notes.push(Note::new(NoteId(0), 1, fret));
        compas2.push(beat);
    }
    let mut last = Beat::new(BeatId(0), Duration::Quarter);
    last.notes.push(Note::new(NoteId(0), 1, 3));
    compas2.push(last);

    score.tracks[0].staves[0].bars[0].voices[0].beats = compas1;
    score.tracks[0].staves[0].bars[1].voices[0].beats = compas2;
    score.assign_missing_ids();
    score
}

/// Técnicas de mano izquierda sobre notas sueltas.
fn tecnicas() -> Score {
    let mut score = Score::new("Técnicas", 1);
    score.meta.tempo_bpm = 100.0;
    let mut beats = Vec::new();

    let mut plain = Beat::new(BeatId(0), Duration::Eighth);
    plain.notes.push(Note::new(NoteId(0), 3, 5));
    beats.push(plain);

    let mut hammer = Beat::new(BeatId(0), Duration::Eighth);
    let mut note = Note::new(NoteId(0), 3, 7);
    note.techniques = NoteTechniques::HAMMER_PULL;
    hammer.notes.push(note);
    beats.push(hammer);

    let mut vibrato = Beat::new(BeatId(0), Duration::Quarter);
    let mut note = Note::new(NoteId(0), 2, 8);
    note.techniques = NoteTechniques::VIBRATO;
    vibrato.notes.push(note);
    beats.push(vibrato);

    let mut muted = Beat::new(BeatId(0), Duration::Quarter);
    let mut note = Note::new(NoteId(0), 6, 3);
    note.techniques = NoteTechniques::PALM_MUTE;
    muted.notes.push(note);
    beats.push(muted);

    score.tracks[0].staves[0].bars[0].voices[0].beats = beats;
    score.assign_missing_ids();
    score
}

/// Cejilla puesta: comprueba que el traste sigue siendo relativo a ella.
fn con_cejilla() -> Score {
    let mut score = Score::new("Con cejilla", 1);
    score.meta.tempo_bpm = 70.0;
    score.tracks[0].capo = 3;

    let mut beats = Vec::new();
    for (string, fret) in [(6_u8, 0_u8), (5, 2), (1, 3)] {
        let mut beat = Beat::new(BeatId(0), Duration::Quarter);
        beat.notes.push(Note::new(NoteId(0), string, fret));
        beats.push(beat);
    }
    let mut rest = Beat::rest(BeatId(0), Duration::Quarter);
    rest.is_rest = true;
    beats.push(rest);

    score.tracks[0].staves[0].bars[0].voices[0].beats = beats;
    score.assign_missing_ids();
    score
}

/// Cambio de indicación de compás a mitad de la pieza.
fn cambio_de_compas() -> Score {
    let mut score = Score::new("Cambio de compás", 2);
    score.meta.tempo_bpm = 110.0;
    score.master_bars[1].time_signature = TimeSignature {
        numerator: 3,
        denominator: 4,
    };

    let mut compas1 = Vec::new();
    for _ in 0..4 {
        let mut beat = Beat::new(BeatId(0), Duration::Quarter);
        beat.notes.push(Note::new(NoteId(0), 4, 5));
        compas1.push(beat);
    }
    let mut compas2 = Vec::new();
    for _ in 0..3 {
        let mut beat = Beat::new(BeatId(0), Duration::Quarter);
        beat.notes.push(Note::new(NoteId(0), 5, 3));
        compas2.push(beat);
    }

    score.tracks[0].staves[0].bars[0].voices[0].beats = compas1;
    score.tracks[0].staves[0].bars[1].voices[0].beats = compas2;
    score.assign_missing_ids();
    score
}

fn main() {
    let fixtures = vec![
        fixture("melodía simple", &melodia_simple()),
        fixture("acorde Em", &acorde_em()),
        fixture("ritmos variados", &ritmos_variados()),
        fixture("técnicas", &tecnicas()),
        fixture("con cejilla", &con_cejilla()),
        fixture("cambio de compás", &cambio_de_compas()),
    ];

    match serde_json::to_string_pretty(&fixtures) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("no se pudieron serializar los casos de prueba: {error}");
            std::process::exit(1);
        }
    }
}
